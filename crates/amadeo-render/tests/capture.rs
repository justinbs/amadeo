//! The first automated coverage the GPU path has ever had.
//!
//! # Why this file matters more than its size suggests
//!
//! `render.describe` checks what *should* be drawn, computed from the world. Until now **nothing
//! checked what the GPU actually produced** — every claim about the wgpu backend rested on somebody
//! opening a window and looking. `STATUS.md` carried that as a known gap through three milestones.
//!
//! ADR 0021 named `render.capture` as the agent's eyes. This is the mechanism behind it: an offscreen
//! backend renders a frame into a texture it owns and hands the pixels back, so a test can assert on
//! them. The offscreen and windowed backends differ **only** in where the frame lands — same shaders,
//! same pipelines, same passes — so these assertions are about the renderer that actually ships.
//!
//! # Skipping is honest here, and only here
//!
//! Creating a device can genuinely fail: a CI runner with no GPU and no software fallback has
//! nothing to render with. These tests report that and pass rather than failing, because a missing
//! adapter is a fact about the machine and not about the engine. Every other assertion is real.
//!
//! `capture_reports_why_it_cannot_when_it_cannot` needs no GPU at all and always runs.

#![cfg(feature = "gpu")]

use amadeo_ecs::World;
use amadeo_render::{
    ArchMesh, BoxMesh, Camera, CylinderMesh, DirectionalLight, Environment, EnvironmentCache, Fog,
    Material, MaterialCache, Mesh, MeshCache, MeshData, NullBackend, PointLight, Quad,
    RenderBackend, Renderer, ShadowMode, SphereMesh, SpotLight, StairMesh, TextureData, Vignette,
    WedgeMesh, WgpuBackend, render_quads,
};
use amadeo_transform::Transform;

/// Only one GPU device exists at a time, across every test in this file.
///
/// # This is working around a real wgpu bug, not being cautious
///
/// Each test here creates an offscreen device and drops it at the end. Dropping one **while another
/// is still alive** is `gfx-rs/wgpu#6571` — a `STATUS_ACCESS_VIOLATION` on Windows, reported
/// specifically against parallel tests using headless adapters. Cargo runs tests in parallel by
/// default, so without this the whole binary is a race.
///
/// **It reached CI before it reached anyone's machine**, which is worth knowing: the determinism
/// job's three debug runs pass because they pass `--test-threads=1`, and its release run did not —
/// so the same code crashed in one step and passed in another, three steps earlier in the same job.
///
/// A lock rather than `--test-threads=1` in CI, because the bug is in the tests rather than in the
/// runner: a developer running `cargo test --all-features` should not hit it either.
static ONE_DEVICE_AT_A_TIME: std::sync::Mutex<()> = std::sync::Mutex::new(());

/// Takes the GPU lock for the rest of the test.
///
/// Poisoning is deliberately ignored. A poisoned lock means some *other* test panicked, which is a
/// failure that test already reports — turning it into a cascade of failures here would bury the one
/// message worth reading.
fn one_device_at_a_time() -> std::sync::MutexGuard<'static, ()> {
    ONE_DEVICE_AT_A_TIME
        .lock()
        .unwrap_or_else(std::sync::PoisonError::into_inner)
}

/// Renders one world offscreen and hands back the pixels, or `None` if this machine has no GPU.
fn capture(world: &mut World, width: u32, height: u32) -> Option<TextureData> {
    // Held for this whole function, which is exactly the device's lifetime: it is created below and
    // dropped when `renderer` falls out of scope at the end. That is why one lock here covers all
    // thirteen tests rather than needing a line in each — and why it must stay here rather than
    // moving into `WgpuBackend::offscreen`, where it would not cover the drop.
    let _gpu = one_device_at_a_time();

    let backend = match WgpuBackend::offscreen(width, height) {
        Ok(backend) => backend,
        Err(error) => {
            println!("skipping: no offscreen device on this machine ({error})");
            return None;
        }
    };
    world.insert_service(Renderer::new(Box::new(backend)));
    render_quads(world);

    let mut renderer = world
        .remove_service::<Renderer>()
        .expect("just installed it");
    let image = renderer
        .capture()
        .expect("an offscreen backend can capture");
    Some(image)
}

/// The colour at a pixel, as `[r, g, b, a]`.
fn pixel_at(image: &TextureData, x: u32, y: u32) -> [u8; 4] {
    let index = ((y * image.width + x) * 4) as usize;
    [
        image.pixels[index],
        image.pixels[index + 1],
        image.pixels[index + 2],
        image.pixels[index + 3],
    ]
}

fn add_camera(world: &mut World, height: f32) {
    let eye = world.spawn();
    world.insert(eye, Transform::at(0.0, 0.0));
    world.insert(eye, Camera::orthographic(height));
}

/// The same camera, naming an environment asset id.
fn add_camera_named(world: &mut World, height: f32, environment: &str) {
    let eye = world.spawn();
    world.insert(eye, Transform::at(0.0, 0.0));
    world.insert(
        eye,
        Camera {
            environment: environment.to_string(),
            ..Camera::orthographic(height)
        },
    );
}

#[test]
fn an_empty_world_captures_the_clear_colour() {
    let mut world = World::new();
    add_camera(&mut world, 10.0);

    let Some(image) = capture(&mut world, 64, 64) else {
        return;
    };

    assert_eq!(image.width, 64);
    assert_eq!(image.height, 64);
    assert_eq!(image.pixels.len(), 64 * 64 * 4);

    // The default clear is a dark neutral that is deliberately *not* black, so "nothing rendered"
    // and "cleared but empty" are distinguishable at a glance — including here.
    let corner = pixel_at(&image, 0, 0);
    assert!(
        corner[0] > 0 && corner[0] < 128,
        "expected a dark non-black clear, got {corner:?}"
    );
    assert_eq!(corner[3], 255, "the background must be opaque");
}

#[test]
fn a_quad_actually_reaches_the_pixels() {
    // The claim nothing has ever checked: a `Quad` in the world becomes coloured pixels on the
    // target. A red quad two world units across, in a ten-unit view, centred.
    let mut world = World::new();
    add_camera(&mut world, 10.0);

    let entity = world.spawn();
    world.insert(entity, Transform::at(0.0, 0.0));
    world.insert(entity, Quad::new(2.0, 2.0, [1.0, 0.0, 0.0, 1.0]));

    let Some(image) = capture(&mut world, 64, 64) else {
        return;
    };

    let centre = pixel_at(&image, 32, 32);
    assert!(
        centre[0] > 200 && centre[1] < 60 && centre[2] < 60,
        "the middle of the screen should be red, got {centre:?}"
    );

    // And the corner is still the background, so the quad has a *size* rather than filling
    // everything — which a wrong projection would produce and a colour check alone would miss.
    let corner = pixel_at(&image, 1, 1);
    assert_ne!(corner, centre, "the quad should not fill the whole target");
}

#[test]
fn the_camera_decides_what_is_on_screen() {
    // Two captures of one world through different cameras must differ. This is the assertion that
    // catches a projection wired up wrongly — the sort of bug `render.describe` cannot see, because
    // `describe` computes the same projection rather than observing it.
    let quad = |world: &mut World| {
        let entity = world.spawn();
        world.insert(entity, Transform::at(0.0, 0.0));
        world.insert(entity, Quad::new(2.0, 2.0, [1.0, 0.0, 0.0, 1.0]));
    };

    let mut near = World::new();
    add_camera(&mut near, 4.0);
    quad(&mut near);

    let mut far = World::new();
    add_camera(&mut far, 40.0);
    quad(&mut far);

    let (Some(near), Some(far)) = (capture(&mut near, 64, 64), capture(&mut far, 64, 64)) else {
        return;
    };

    // A quad two units across fills half a four-unit view and a twentieth of a forty-unit one, so a
    // point well off centre is inside the first and outside the second.
    let sample = (24, 32);
    let close = pixel_at(&near, sample.0, sample.1);
    let distant = pixel_at(&far, sample.0, sample.1);

    assert!(
        close[0] > 200,
        "the near camera should show the quad here, got {close:?}"
    );
    assert!(
        distant[0] < 128,
        "the far camera should show background here, got {distant:?}"
    );
}

#[test]
fn the_present_pass_does_not_turn_the_screen_upside_down() {
    // Since the render graph landed, every camera draws into an off-screen image and a full-screen
    // triangle copies it onto the destination. That triangle maps texture space (v = 0 on the top
    // row) onto clip space (y = +1 at the top), so it flips y — and getting that backwards inverts
    // the entire screen while every existing test still passes, because a centred quad, a clear
    // colour and a symmetrical view all look identical either way.
    //
    // STATUS.md carries the same warning about `sprite.wgsl`'s vertical flip, which is still
    // unexercised. This is the equivalent check for the pass that now runs on every frame.
    let mut world = World::new();
    add_camera(&mut world, 10.0);

    // Two world units above the middle of a ten-unit view, so it sits in the upper half and nowhere
    // near the centre line.
    let entity = world.spawn();
    world.insert(entity, Transform::at(0.0, 2.0));
    world.insert(entity, Quad::new(2.0, 2.0, [1.0, 0.0, 0.0, 1.0]));

    let Some(image) = capture(&mut world, 64, 64) else {
        return;
    };

    let above = pixel_at(&image, 32, 19);
    let below = pixel_at(&image, 32, 45);
    assert!(
        above[0] > 200,
        "world +Y is up, so the quad belongs in the top half of the image, got {above:?}"
    );
    assert!(
        below[0] < 128,
        "the bottom half should still be background, got {below:?}"
    );
}

/// The same capture, but with a look installed on the camera.
fn capture_with(
    world: &mut World,
    look: Environment,
    width: u32,
    height: u32,
) -> Option<TextureData> {
    let mut cache = EnvironmentCache::new();
    cache.insert("test_look", look);
    world.insert_service(cache);
    capture(world, width, height)
}

#[test]
fn an_environment_actually_reaches_the_pixels() {
    // The end of ADR 0034's chain, and the only assertion that covers all of it: a `.environment`
    // asset id on a camera, through the cache, into the frame, into the post pass's uniform, into
    // the shader, out as different pixels. Every earlier check in `tests/environment.rs` stops at
    // the frame.
    //
    // A vignette is the effect worth testing here because it varies *across* the image — so a
    // uniform that never reached the shader, or reached it as zeros, produces a picture this can
    // tell apart from the real one. A pure colour change could be faked by almost any bug.
    let quad = |world: &mut World| {
        let entity = world.spawn();
        world.insert(entity, Transform::at(0.0, 0.0));
        // Filling the whole ten-unit view, so both sample points below are on the quad and any
        // difference between them is the effect rather than the background.
        world.insert(entity, Quad::new(20.0, 20.0, [1.0, 1.0, 1.0, 1.0]));
    };

    let mut plain = World::new();
    add_camera_named(&mut plain, 10.0, "test_look");
    quad(&mut plain);

    let mut darkened = World::new();
    add_camera_named(&mut darkened, 10.0, "test_look");
    quad(&mut darkened);

    let (Some(plain), Some(darkened)) = (
        capture_with(&mut plain, Environment::default(), 64, 64),
        capture_with(
            &mut darkened,
            Environment {
                vignette: Vignette {
                    intensity: 1.0,
                    radius: 0.0,
                },
                ..Environment::default()
            },
            64,
            64,
        ),
    ) else {
        return;
    };

    // Widened to `i32` before comparing: these are `u8`s at the top of their range, and a tolerance
    // added to 255 overflows.
    let red = |pixel: [u8; 4]| i32::from(pixel[0]);

    // The middle is essentially untouched: a vignette darkens the edges, not the centre.
    let centre_before = pixel_at(&plain, 32, 32);
    let centre_after = pixel_at(&darkened, 32, 32);
    assert!(
        red(centre_after) + 12 >= red(centre_before),
        "the centre should survive the vignette, {centre_before:?} -> {centre_after:?}"
    );

    // The corner is markedly darker, which is the effect doing its job.
    let corner_before = pixel_at(&plain, 2, 2);
    let corner_after = pixel_at(&darkened, 2, 2);
    assert!(
        red(corner_after) + 40 < red(corner_before),
        "the corner should be darkened, {corner_before:?} -> {corner_after:?}"
    );
}

#[test]
fn the_default_environment_leaves_the_picture_alone() {
    // The claim that let post-processing ship without changing either game's confirmed appearance:
    // a camera naming no environment must produce the same pixels as one naming a default. If the
    // post pass ever stopped being a no-op by default, every existing capture would shift and this
    // is what would say so.
    let build = |world: &mut World| {
        let entity = world.spawn();
        world.insert(entity, Transform::at(0.0, 0.0));
        world.insert(entity, Quad::new(2.0, 2.0, [0.8, 0.3, 0.1, 1.0]));
    };

    let mut bare = World::new();
    add_camera(&mut bare, 10.0);
    build(&mut bare);

    let mut defaulted = World::new();
    add_camera_named(&mut defaulted, 10.0, "test_look");
    build(&mut defaulted);

    let (Some(bare), Some(defaulted)) = (
        capture(&mut bare, 64, 64),
        capture_with(&mut defaulted, Environment::default(), 64, 64),
    ) else {
        return;
    };

    assert_eq!(
        bare.pixels, defaulted.pixels,
        "a default environment must be indistinguishable from none at all"
    );
}

#[test]
fn a_3d_camera_allocates_and_attaches_a_depth_buffer_without_complaint() {
    // The depth buffer's format, its usages and its attachment are all things wgpu validates on a
    // real device and nowhere else — a depth texture asked for `TEXTURE_BINDING`, or attached with
    // the colour bind-group layout, fails at creation with a message about the wrong thing. The
    // graph tests prove the *plan*; only this proves the device accepts it.
    //
    // Nothing draws yet (the mesh pipeline is next), so the assertion is that a 3D frame renders and
    // captures at all. That is a low bar and exactly the right one for prerequisite machinery.
    let mut world = World::new();
    let eye = world.spawn();
    world.insert(eye, Transform::at(0.0, 0.0));
    world.insert(eye, Camera::perspective(60.0));

    let Some(image) = capture(&mut world, 64, 64) else {
        return;
    };

    assert_eq!(image.pixels.len(), 64 * 64 * 4);
    // Cleared, because a 3D camera with nothing in front of it is an empty room rather than a
    // failure — and the clear colour is deliberately not black, so this tells them apart.
    let pixel = pixel_at(&image, 32, 32);
    assert!(
        pixel[0] > 0 && pixel[0] < 128,
        "expected the clear, got {pixel:?}"
    );
}

#[test]
fn a_2d_capture_is_unchanged_by_the_depth_machinery() {
    // The claim that made it safe to add: a 2D frame declares no depth buffer at all, so nothing
    // about the sprite path can have moved. Asserted against the clear colour rather than trusting
    // that "no depth transient" means "no difference".
    let mut world = World::new();
    add_camera(&mut world, 10.0);
    let entity = world.spawn();
    world.insert(entity, Transform::at(0.0, 0.0));
    world.insert(entity, Quad::new(2.0, 2.0, [1.0, 0.0, 0.0, 1.0]));

    let Some(image) = capture(&mut world, 64, 64) else {
        return;
    };
    let centre = pixel_at(&image, 32, 32);
    assert!(centre[0] > 200 && centre[1] < 60, "got {centre:?}");
}

/// A world with a 3D camera looking down −Z at the origin, a light, and one box.
///
/// The camera sits at +Z looking back at the origin, which is what an unrotated camera does — it
/// looks along its own negative Z, the same axis a light travels along.
fn a_lit_box(colour: [f32; 4], size: [f32; 3]) -> World {
    let mut world = World::new();

    let eye = world.spawn();
    let mut placement = Transform::at(0.0, 0.0);
    // Five units back along +Z. An unrotated camera looks along its own negative Z, so this puts the
    // origin — and the box — in front of it.
    placement.translation = [0.0, 0.0, 5.0];
    world.insert(eye, placement);
    world.insert(eye, Camera::perspective(60.0));

    let sun = world.spawn();
    world.insert(sun, Transform::at(0.0, 0.0));
    world.insert(sun, DirectionalLight::default());

    let mut meshes = MeshCache::new();
    meshes.insert("cube", BoxMesh { size }.tessellate());
    world.insert_service(meshes);

    let mut materials = MaterialCache::new();
    materials.insert(
        "paint",
        Material {
            base_colour: colour,
            ..Material::default()
        },
    );
    world.insert_service(materials);

    let thing = world.spawn();
    world.insert(thing, Transform::at(0.0, 0.0));
    world.insert(thing, Mesh::new("cube", "paint"));
    world
}

#[test]
fn a_mesh_actually_reaches_the_pixels() {
    // **The first 3D geometry this engine has ever drawn.** Everything before it — the graph, the
    // depth buffer, the matrices, the collection pass — was scaffolding for exactly this.
    //
    // A red box two units across, five units from the camera. The light shines straight down −Z,
    // which is directly at the box's front face, so that face is fully lit.
    let mut world = a_lit_box([1.0, 0.0, 0.0, 1.0], [2.0, 2.0, 2.0]);

    let Some(image) = capture(&mut world, 64, 64) else {
        return;
    };

    // **The green and blue bounds were loosened when PBR landed, and the reason is not a
    // regression.** Before ADR 0048 a lit surface was pure Lambert: a red box reflected only red,
    // so the other two channels sat near zero. A real surface also has a *specular* highlight, and
    // a dielectric's highlight is white rather than tinted — which is what makes plastic look like
    // plastic. Facing the light head-on at the default roughness of 0.5, that highlight measures
    // about 0.15 in linear light, and it lifts green and blue to roughly 111.
    //
    // So the assertion keeps what it was actually testing — geometry reaches the pixels, and the
    // face reads as *red* — while no longer encoding "there is no specular" as though that were a
    // property of the renderer rather than a missing feature.
    let centre = pixel_at(&image, 32, 32);
    assert!(
        centre[0] > 100 && centre[1] < 160 && centre[2] < 160 && centre[0] > centre[1] + 80,
        "the middle of the screen should be a lit red face with a white highlight on it, \
         got {centre:?}"
    );

    // And the corner is still the background, so the box has a *size* rather than filling
    // everything — which a wrong projection would produce and a colour check alone would miss.
    // Compared against the clear colour's own bound rather than an absolute, because the clear is a
    // dark *grey* whose red channel is 69: a tighter threshold would be asserting the background is
    // black, which it deliberately is not.
    let corner = pixel_at(&image, 2, 2);
    assert!(
        corner[0] < 128 && corner[0] == corner[1].saturating_sub(6),
        "the corner should still be the neutral clear colour, got {corner:?}"
    );
}

#[test]
fn a_materials_texture_actually_reaches_the_pixels() {
    // **What `base_colour_texture` was for.** The field had existed since ADR 0033 and was read by
    // nothing — not the frame, not the shader — so every 3D surface in the engine was one flat
    // colour and no test could tell.
    //
    // A white box wearing a texture that is pure blue on its left half and pure red on its right.
    // Sampling at two points is what makes this a texture test rather than a tint test: a shader
    // that ignored the image and used `base_colour` would give one colour in both places, and a
    // shader that sampled with a broken UV would give the same colour twice as well.
    let mut world = a_lit_box([1.0, 1.0, 1.0, 1.0], [2.0, 2.0, 2.0]);

    // Two by one pixels: blue, then red. Uploaded through the same path a sprite's would be.
    let mut textures = amadeo_render::TextureCache::new();
    textures.insert_decoded(
        "halves",
        TextureData {
            width: 2,
            height: 1,
            format: amadeo_image::PixelFormat::Rgba8UnormSrgb,
            pixels: vec![0, 0, 255, 255, 255, 0, 0, 255],
        },
    );
    world.insert_service(textures);

    if let Some(materials) = world.service_mut::<MaterialCache>() {
        materials.insert(
            "paint",
            Material {
                base_colour: [1.0, 1.0, 1.0, 1.0],
                base_colour_texture: "halves".to_string(),
                ..Material::default()
            },
        );
    }

    let Some(image) = capture(&mut world, 64, 64) else {
        return;
    };

    // `BoxMesh` gives each face UVs running the full 0..1 across it, so the box's front face wears
    // the whole image: blue on the left, red on the right.
    let left = pixel_at(&image, 22, 32);
    let right = pixel_at(&image, 42, 32);

    assert!(
        left[2] > left[0],
        "the left of the box should be the texture's blue half, got {left:?}"
    );
    assert!(
        right[0] > right[2],
        "the right of the box should be the texture's red half, got {right:?}"
    );
}

/// A world from [`a_lit_box`] whose material wears `normal_texture` at a given strength.
///
/// The map is one pixel leaning hard along the surface's +u axis: `(255, 128, 255)` decodes to
/// `(1, 0, 1)`, which normalises to 45° off the surface normal. A flat map would be `(128, 128, 255)`.
fn a_box_wearing_a_normal_map(strength: f32) -> World {
    let mut world = a_lit_box([1.0, 1.0, 1.0, 1.0], [2.0, 2.0, 2.0]);

    let mut textures = amadeo_render::TextureCache::new();
    textures.insert_decoded(
        "leaning",
        TextureData {
            width: 1,
            height: 1,
            // **Linear, not sRGB**, which is the whole point of the format tag: these bytes are a
            // direction. Tagged sRGB, 255 and 128 would decode to different numbers entirely and
            // the lean would not be 45°.
            format: amadeo_image::PixelFormat::Rgba8Unorm,
            pixels: vec![255, 128, 255, 255],
        },
    );
    world.insert_service(textures);

    if let Some(materials) = world.service_mut::<MaterialCache>() {
        materials.insert(
            "paint",
            Material {
                base_colour: [1.0, 1.0, 1.0, 1.0],
                normal_texture: "leaning".to_string(),
                normal_strength: strength,
                ..Material::default()
            },
        );
    }
    world
}

#[test]
fn a_normal_map_actually_changes_the_shading() {
    // **The thing that makes normal mapping real rather than plumbed.** Every piece of it can be
    // wired up correctly — the field set, the texture decoded, the tangents generated, the bind
    // group built — and the shader can still ignore the map entirely. Nothing on the CPU side can
    // tell; only a pixel can.
    //
    // The light shines straight down −Z at the box's front face, so that face is fully lit and its
    // lambert term is 1.0 — the brightest a surface can be. Leaning the normal 45° away can
    // therefore only make it *darker*, which is an unambiguous direction to assert in.
    let mut flat = a_lit_box([1.0, 1.0, 1.0, 1.0], [2.0, 2.0, 2.0]);
    let Some(without) = capture(&mut flat, 64, 64) else {
        return;
    };

    let mut leaning = a_box_wearing_a_normal_map(1.0);
    let Some(with) = capture(&mut leaning, 64, 64) else {
        return;
    };

    let plain = pixel_at(&without, 32, 32);
    let bumped = pixel_at(&with, 32, 32);

    assert!(
        bumped[0] < plain[0].saturating_sub(20),
        "a normal leaning 45° off the light should be visibly darker than one facing it square: \
         got {bumped:?} with the map against {plain:?} without it. Equal values mean the shader \
         never read the map"
    );
    assert!(
        bumped[0] > 30,
        "and darker rather than black — a black surface here means the tangent frame produced a \
         NaN or pointed backwards, got {bumped:?}"
    );
}

#[test]
fn normal_strength_zero_is_exactly_the_unmapped_surface() {
    // Two things at once, and the second is why this is worth its own test.
    //
    // That the dial works — and that the darkening above is attributable to *the normal map* rather
    // than to anything else the textured path does differently. At strength zero the sideways lean
    // is scaled to nothing and the frame collapses back to the geometric normal, so the pixels must
    // match the untextured box exactly. If they did not, something in binding a second texture
    // would be changing the picture on its own.
    let mut flat = a_lit_box([1.0, 1.0, 1.0, 1.0], [2.0, 2.0, 2.0]);
    let Some(without) = capture(&mut flat, 64, 64) else {
        return;
    };

    let mut zeroed = a_box_wearing_a_normal_map(0.0);
    let Some(with) = capture(&mut zeroed, 64, 64) else {
        return;
    };

    assert_eq!(
        pixel_at(&without, 32, 32),
        pixel_at(&with, 32, 32),
        "normal_strength 0.0 must be byte-identical to naming no normal map at all"
    );
}

/// A capture of a box with the given surface parameters, optionally turned about Y.
///
/// Turning matters for these tests. Straight on, the camera, the light and the surface normal all
/// line up, which is exactly where a specular highlight is at its most extreme — bright enough to
/// saturate and stop being a measurement. A turned box puts the visible face off the highlight,
/// where the numbers still have room to move.
fn a_surface(colour: [f32; 4], metallic: f32, roughness: f32, degrees: f32) -> Option<TextureData> {
    let mut world = a_lit_box(colour, [2.0, 2.0, 2.0]);
    if let Some(materials) = world.service_mut::<MaterialCache>() {
        materials.insert(
            "paint",
            Material {
                base_colour: colour,
                metallic,
                roughness,
                ..Material::default()
            },
        );
    }
    for entity in world.entities() {
        if world.get::<Mesh>(entity).is_some() {
            let mut transform = Transform::at(0.0, 0.0);
            transform.rotation = [0.0, degrees, 0.0];
            world.insert(entity, transform);
        }
    }
    capture(&mut world, 64, 64)
}

#[test]
fn roughness_changes_how_a_surface_shades() {
    // **What `roughness` was for.** It has been on `Material` since ADR 0033 and the shader read it
    // for the first time in ADR 0048 — so before this these two captures were identical, and no
    // test in the engine could have told.
    //
    // A **dark** box, so that the diffuse term is small and what is being compared is the highlight
    // rather than the paint. The camera and the light both look down −Z, so a face square to them
    // reflects light straight back at the viewer: a smooth surface concentrates that into a narrow
    // blaze, a rough one spreads it into almost nothing.
    let (Some(smooth), Some(rough)) = (
        a_surface([0.15, 0.15, 0.15, 1.0], 0.0, 0.15, 0.0),
        a_surface([0.15, 0.15, 0.15, 1.0], 0.0, 1.0, 0.0),
    ) else {
        return;
    };

    let shiny_centre = pixel_at(&smooth, 32, 32);
    let dull_centre = pixel_at(&rough, 32, 32);

    assert!(
        shiny_centre[0] > dull_centre[0].saturating_add(40),
        "a smooth surface concentrates its highlight and must be far brighter head-on than a rough \
         one: smooth {shiny_centre:?} against rough {dull_centre:?}. Equal values mean the shader \
         never read `roughness`"
    );

    // The smooth case **saturates**, and that is the finding rather than a flaw in the test. A
    // near-mirror pointed at a light genuinely is far brighter than white, which is what the HDR
    // target exists to carry and what a tonemapper exists to bring back down — and the default
    // `Environment` deliberately does nothing (ADR 0034). See ADR 0048's consequences.
    assert_eq!(
        shiny_centre[0], 255,
        "a near-mirror facing the light heads into HDR, so this is expected to clip at 255 until a \
         tonemap is on; got {shiny_centre:?}"
    );
}

#[test]
fn a_metal_has_no_diffuse_colour() {
    // **The half of `metallic` that surprises people.** It is not a shininess dial: a metal has no
    // diffuse colour *at all*. Light either reflects off it or is absorbed, and nothing scatters
    // back out — which is why a gold bar has no "gold-coloured matte" to it, and why the difference
    // between gold and yellow paint is this one property.
    //
    // A red box turned 50°, so the visible face is off the highlight and the reading is about the
    // diffuse rather than about a saturated specular. The dielectric shows its red plainly; the
    // metal has only a red-tinted reflection, which away from the highlight is very little.
    let (Some(dielectric), Some(metal)) = (
        a_surface([1.0, 0.0, 0.0, 1.0], 0.0, 0.5, 50.0),
        a_surface([1.0, 0.0, 0.0, 1.0], 1.0, 0.5, 50.0),
    ) else {
        return;
    };

    let painted = pixel_at(&dielectric, 32, 32);
    let metallic = pixel_at(&metal, 32, 32);

    assert!(
        i32::from(metallic[0]) < i32::from(painted[0]) - 60,
        "a metal has no diffuse, so off the highlight it must be far darker than the same colour as \
         paint: metal {metallic:?} against dielectric {painted:?}"
    );
}

#[test]
fn a_metal_reflects_the_sky_rather_than_going_black() {
    // **This test used to be called `a_metal_is_black_under_ambient_because_there_is_no_sky_yet`,
    // and it was written so that closing Q28 would break it.** Closing Q28 did not break it — it
    // kept passing, for a reason that had stopped being true. Worth recording, because a test that
    // outlives its rationale is worse than no test: it reads as evidence for something it no longer
    // checks.
    //
    // What it now checks is the actual claim of image-based lighting: a metal has no diffuse, so
    // *all* of its ambient light is reflection, and under a blue sky it must come out blue. Before
    // ADR 0049 there was nothing to reflect and the answer was black.
    //
    // Turned 130°, so the visible face points away from the sun and what reaches it is the
    // environment rather than the direct light.
    let sky = |colour: [f32; 4]| -> Option<TextureData> {
        let mut world = a_lit_box([0.8, 0.8, 0.8, 1.0], [2.0, 2.0, 2.0]);
        if let Some(materials) = world.service_mut::<MaterialCache>() {
            materials.insert(
                "paint",
                Material {
                    base_colour: [0.8, 0.8, 0.8, 1.0],
                    metallic: 1.0,
                    roughness: 0.25,
                    ..Material::default()
                },
            );
        }
        let mut skies = amadeo_render::SkyCache::new();
        skies.insert("overhead", amadeo_render::EnvironmentMap::solid(colour));
        world.insert_service(skies);

        let mut looks = EnvironmentCache::new();
        looks.insert(
            "outdoors",
            Environment {
                sky: "overhead".to_string(),
                ..Environment::default()
            },
        );
        world.insert_service(looks);

        for entity in world.entities() {
            if world.get::<Camera>(entity).is_some() {
                let mut camera = Camera::perspective(60.0);
                camera.environment = "outdoors".to_string();
                world.insert(entity, camera);
            }
            if world.get::<Mesh>(entity).is_some() {
                let mut transform = Transform::at(0.0, 0.0);
                transform.rotation = [0.0, 130.0, 0.0];
                world.insert(entity, transform);
            }
        }
        capture(&mut world, 64, 64)
    };

    let (Some(blue), Some(red)) = (sky([0.1, 0.2, 1.2, 1.0]), sky([1.2, 0.2, 0.1, 1.0])) else {
        return;
    };

    let under_blue = pixel_at(&blue, 32, 32);
    let under_red = pixel_at(&red, 32, 32);

    assert!(
        under_blue[2] > under_blue[0],
        "a metal under a blue sky must read blue, got {under_blue:?}"
    );
    assert!(
        under_red[0] > under_red[2],
        "and the same metal under a red sky must read red, got {under_red:?}"
    );
    // And it is genuinely lit rather than merely tinted-dark, which is what "no longer black" means.
    assert!(
        under_blue[2] > 60,
        "the reflection should be visible, not a hint: {under_blue:?}"
    );
}

#[test]
fn geometry_is_visible_from_the_inside_rather_than_transparent() {
    // **What "digging down showed the sky" actually was** (ADR 0052).
    //
    // Terrain from surface nets is an open surface — the boundary between rock and air — with no
    // underside. With back faces culled it vanished when seen from beneath, so a camera under the
    // ground looked straight through the world to the skybox. It reads as the terrain having failed
    // to stream, which is what made it hard to attribute.
    //
    // Tested with a box big enough that the camera is inside it, which is the same geometry
    // question without needing a world: from within, every surface is a back face. Before the fix
    // this capture was the clear colour.
    let mut world = a_lit_box([0.9, 0.9, 0.9, 1.0], [20.0, 20.0, 20.0]);

    let Some(image) = capture(&mut world, 64, 64) else {
        return;
    };

    // The clear colour is a dark neutral around 69; a lit interior wall is far brighter.
    let centre = pixel_at(&image, 32, 32);
    assert!(
        centre[0] > 120,
        "from inside a box the far wall should be visible, got {centre:?} — which is the clear \
         colour, meaning the camera is seeing straight through the geometry"
    );

    // And the corners too: being inside means being *surrounded*, so there should be no gap
    // anywhere. A single-sided box would leak the background at every one of them.
    for (x, y) in [(2u32, 2u32), (61, 2), (2, 61), (61, 61)] {
        let corner = pixel_at(&image, x, y);
        assert!(
            corner[0] > 120,
            "({x},{y}) shows {corner:?}, so the box is not enclosing the camera"
        );
    }
}

#[test]
fn a_slanted_edge_is_anti_aliased_rather_than_a_staircase() {
    // **What ADR 0051 bought, and the only way to see it is to look at an edge.** Every other test
    // in this file samples the middle of something, where anti-aliasing changes nothing — so all of
    // them would pass just as happily with it switched off.
    //
    // A box turned 30° about Y puts a near-vertical silhouette edge across the picture at an angle.
    // Without multisampling every pixel along it is either fully box or fully background, so a
    // vertical scan across the edge finds only two values. With it, the pixels the edge passes
    // through are mixtures, and those in-between values are the anti-aliasing.
    let Some(image) = a_surface([0.9, 0.9, 0.9, 1.0], 0.0, 0.9, 30.0) else {
        return;
    };

    // Scan a row across where the box's left silhouette falls, counting values that are neither
    // background nor surface.
    let row = 32;
    let mut partials = 0;
    let mut seen_box = false;
    let mut seen_background = false;

    for x in 0..64u32 {
        let pixel = pixel_at(&image, x, row);
        // The clear colour is a dark neutral; the lit box is bright. Anything comfortably between
        // the two is a pixel the edge passed through.
        if pixel[0] < 90 {
            seen_background = true;
        } else if pixel[0] > 170 {
            seen_box = true;
        } else {
            partials += 1;
        }
    }

    assert!(
        seen_background && seen_box,
        "the scan must cross an actual silhouette, or it proves nothing"
    );
    assert!(
        partials > 0,
        "a slanted edge with multisampling on must produce partially-covered pixels; finding none \
         means every pixel is fully one thing or the other, which is a staircase"
    );
}

#[test]
fn a_nearer_face_hides_a_further_one() {
    // What the depth buffer is *for*, and the one thing no amount of graph testing could show. A
    // box's back faces are behind its front ones; without depth testing whichever drew last would
    // win, and the tessellation order is not the view order.
    //
    // Checked by making the box long in Z: if depth is broken, the far face — which is smaller on
    // screen because it is further away — shows through the near one.
    let mut world = a_lit_box([1.0, 1.0, 1.0, 1.0], [2.0, 2.0, 6.0]);

    let Some(image) = capture(&mut world, 64, 64) else {
        return;
    };

    // The near face is lit head-on and the far one faces away, so a depth failure reads as the
    // centre going dark. It is bright, so the nearer surface won.
    let centre = pixel_at(&image, 32, 32);
    assert!(
        centre[0] > 100,
        "the near face should win the depth test, got {centre:?}"
    );
}

#[test]
fn a_face_turned_away_from_the_light_is_darker_than_one_facing_it() {
    // Proves the lighting is actually reading normals rather than painting every face flat, which
    // is what a box tessellated with averaged corner normals would look like — the exact mistake
    // ADR 0035's `a_box_has_flat_faces_rather_than_averaged_corners` guards on the CPU side.
    //
    // Two captures of the same box, one turned 50° about Y. Comparing the *centre pixel* of each
    // rather than two points within one image, so the test does not depend on where the boundary
    // between two faces happens to land on a 64-pixel target.
    //
    // Straight on, the front face is square to the light and fully lit. Turned, the surface at the
    // centre is at an angle to it, so `N·L` is smaller and the pixel is darker.
    //
    // **A mid-grey box rather than a white one**, and that is not cosmetic. A white box square to
    // the light comes out at 255 — *clipped* — so the comparison was against a value that had run
    // out of room to be brighter, and any change that lifted the darker side shrank the gap for no
    // reason to do with normals. Raising the ambient term when shadows landed did exactly that and
    // this test failed at a difference of 13. Grey keeps both readings inside the range, which makes
    // the assertion about the lighting rather than about where the clip happens to be.
    let turn = |degrees: f32| {
        let mut world = a_lit_box([0.5, 0.5, 0.5, 1.0], [2.0, 2.0, 2.0]);
        for entity in world.entities() {
            if world.get::<Mesh>(entity).is_some() {
                let mut transform = Transform::at(0.0, 0.0);
                transform.rotation = [0.0, degrees, 0.0];
                world.insert(entity, transform);
            }
        }
        capture(&mut world, 64, 64)
    };

    let (Some(square_on), Some(turned)) = (turn(0.0), turn(50.0)) else {
        return;
    };

    let lit = i32::from(pixel_at(&square_on, 32, 32)[0]);
    let angled = i32::from(pixel_at(&turned, 32, 32)[0]);
    assert!(
        lit > angled + 15,
        "a surface angled away from the light must be darker: {lit} square on vs {angled} turned"
    );
}

/// Writes a 3D scene to a PNG when `AMADEO_SHOT` names a path.
///
/// Not an assertion — a way to *look* at what the mesh pass draws, which is how the sprite path was
/// confirmed too. Skipped silently in CI, where nobody is looking.
#[test]
fn a_scene_can_be_looked_at() {
    let Ok(path) = std::env::var("AMADEO_SHOT") else {
        return;
    };

    let mut world = World::new();

    let eye = world.spawn();
    let mut placement = Transform::at(0.0, 0.0);
    placement.translation = [3.5, 3.0, 7.0];
    placement.rotation = [-18.0, 24.0, 0.0];
    world.insert(eye, placement);
    world.insert(eye, Camera::perspective(55.0));

    let sun = world.spawn();
    let mut sun_placement = Transform::at(0.0, 0.0);
    sun_placement.rotation = [-50.0, -30.0, 0.0];
    world.insert(sun, sun_placement);
    world.insert(sun, DirectionalLight::default());

    let mut meshes = MeshCache::new();
    meshes.insert(
        "ground",
        amadeo_render::PlaneMesh { size: [14.0, 14.0] }.tessellate(),
    );
    meshes.insert(
        "block",
        BoxMesh {
            size: [1.6, 1.6, 1.6],
        }
        .tessellate(),
    );
    meshes.insert(
        "pillar",
        BoxMesh {
            size: [0.8, 3.4, 0.8],
        }
        .tessellate(),
    );
    world.insert_service(meshes);

    let mut materials = MaterialCache::new();
    let paint = |r: f32, g: f32, b: f32| Material {
        base_colour: [r, g, b, 1.0],
        ..Material::default()
    };
    materials.insert("slate", paint(0.30, 0.33, 0.40));
    materials.insert("rust", paint(0.72, 0.32, 0.18));
    materials.insert("moss", paint(0.28, 0.48, 0.30));
    world.insert_service(materials);

    let mut place = |mesh: &str, material: &str, at: [f32; 3]| {
        let entity = world.spawn();
        let mut transform = Transform::at(0.0, 0.0);
        transform.translation = at;
        world.insert(entity, transform);
        world.insert(entity, Mesh::new(mesh, material));
    };

    place("ground", "slate", [0.0, 0.0, 0.0]);
    place("block", "rust", [0.0, 0.8, 0.0]);
    place("block", "moss", [2.2, 0.8, -1.4]);
    place("block", "rust", [-2.0, 0.8, 1.2]);
    place("pillar", "slate", [-3.0, 1.7, -2.6]);
    place("pillar", "moss", [3.2, 1.7, 2.0]);

    let Some(image) = capture(&mut world, 960, 540) else {
        return;
    };
    let png = amadeo_render::encode_png(&image).expect("encodes");
    std::fs::write(&path, png).expect("writes");
    println!("wrote {path}");
}

#[test]
fn capture_reports_why_it_cannot_when_it_cannot() {
    // Needs no GPU, so this one always runs. A backend that cannot read its own output must say so
    // rather than return a blank image a caller would have to know not to trust — and the message
    // has to name what answers the same question instead.
    let mut backend = NullBackend::new(32, 32);
    let error = backend
        .capture()
        .expect_err("the null backend draws nothing");

    let message = error.to_string();
    assert!(message.contains("null"), "{message}");
    assert!(
        message.contains("render.describe"),
        "the message should point at what does work: {message}"
    );
}

/// A floor with a box hanging above it, lit by a sun at an angle so the shadow lands beside the box
/// rather than under it — ADR 0038.
///
/// # The geometry is arranged so the two samples cannot be confused
///
/// The sun travels along `(-1, -2, 0)` normalised, so anything drops half a unit sideways for every
/// unit it is above the floor. The box sits at x = 2 and y = 4, which puts its shadow centred on
/// x = 0 — well clear of the box itself, which is what lets the camera look straight down and see
/// both at once without the box hiding its own shadow.
fn a_floor_under_a_floating_box(shadows: ShadowMode) -> World {
    let mut world = World::new();

    // Straight down from fourteen units up. At a 60 degree vertical field of view that shows about
    // eight world units either side, so the twenty-unit floor fills the frame.
    let eye = world.spawn();
    world.insert(
        eye,
        Transform {
            translation: [0.0, 14.0, 0.0],
            rotation: [-90.0, 0.0, 0.0],
            ..Transform::default()
        },
    );
    world.insert(eye, Camera::perspective(60.0));

    // Pitch and yaw chosen so the light travels along (-1, -2, 0) normalised. A light aims like a
    // camera -- along its own negative Z -- so this is "rotate it until it points there" rather than
    // a direction typed in directly, which is the vocabulary a scene file has.
    let sun = world.spawn();
    world.insert(
        sun,
        Transform {
            rotation: [-63.435, 90.0, 0.0],
            ..Transform::default()
        },
    );
    world.insert(
        sun,
        DirectionalLight {
            shadows,
            ..DirectionalLight::default()
        },
    );

    let mut meshes = MeshCache::new();
    meshes.insert(
        "floor",
        BoxMesh {
            size: [20.0, 0.2, 20.0],
        }
        .tessellate(),
    );
    meshes.insert(
        "block",
        BoxMesh {
            size: [2.0, 2.0, 2.0],
        }
        .tessellate(),
    );
    world.insert_service(meshes);

    let mut materials = MaterialCache::new();
    materials.insert(
        "pale",
        Material {
            base_colour: [0.9, 0.9, 0.9, 1.0],
            ..Material::default()
        },
    );
    world.insert_service(materials);

    let floor = world.spawn();
    world.insert(floor, Transform::at_xyz(0.0, 0.0, 0.0));
    world.insert(floor, Mesh::new("floor", "pale"));

    let block = world.spawn();
    world.insert(block, Transform::at_xyz(2.0, 4.0, 0.0));
    world.insert(block, Mesh::new("block", "pale"));
    world
}

#[test]
fn a_shadow_actually_reaches_the_pixels() {
    // **The whole of ADR 0038 in one assertion.** Every other shadow test checks a part -- where the
    // box is fitted, that the pass is ordered before the view -- and this is the only one that can
    // prove the shadow map is drawn, sampled, and applied to the light.
    //
    // The floor at screen centre is where the block's shadow lands; the floor at the left edge is
    // five world units away from it and lit by the same light at the same angle, so the two pixels
    // differ in exactly one thing.
    let mut world = a_floor_under_a_floating_box(ShadowMode::Orthogonal);

    let Some(image) = capture(&mut world, 64, 64) else {
        return;
    };

    let shadowed = pixel_at(&image, 32, 32);
    let lit = pixel_at(&image, 10, 32);

    assert!(
        lit[0] > 60,
        "the far floor should be lit by the sun, got {lit:?}"
    );
    assert!(
        shadowed[0] + 20 < lit[0],
        "the floor under the block should be clearly darker than floor beside it; \
         shadowed {shadowed:?}, lit {lit:?}"
    );
}

#[test]
fn the_same_scene_without_shadows_is_evenly_lit() {
    // The control, and the reason the test above is evidence rather than decoration. Both floor
    // pixels face the light identically, so with shadows off they must match -- if they differ here,
    // the test above was measuring something other than a shadow and would have passed anyway.
    //
    // Session 9's lesson, applied: a test is not evidence until you have watched it fail.
    let mut world = a_floor_under_a_floating_box(ShadowMode::Off);

    let Some(image) = capture(&mut world, 64, 64) else {
        return;
    };

    let centre = pixel_at(&image, 32, 32);
    let edge = pixel_at(&image, 10, 32);
    assert!(
        centre[0].abs_diff(edge[0]) < 12,
        "with shadows off, one flat floor should be evenly lit; centre {centre:?}, edge {edge:?}"
    );
}

#[test]
fn a_cascaded_shadow_reaches_the_pixels_too() {
    // **The GPU half of ADR 0055**, and the reason it needs its own test rather than trusting the
    // Orthogonal one: cascades changed the shadow map from a plain texture to a texture *array*,
    // the shadow pass from one to four, and the mesh shader from sampling one layer to selecting
    // one. Every one of those can be wrong in a way that still compiles, binds and draws.
    //
    // The failure mode worth naming is the quiet one: a cascade selection that always picks layer
    // zero looks perfect in a scene small enough to fit inside the near cascade, which is exactly
    // what a unit-sized test scene is. So this asserts the same thing the single-map test does —
    // the shadow is there — and the *arithmetic* differences between cascades are pinned headlessly
    // in `shadows.rs`, where they can be checked rather than squinted at.
    let mut world = a_floor_under_a_floating_box(ShadowMode::Cascaded { blend: 0.5 });

    let Some(image) = capture(&mut world, 64, 64) else {
        return;
    };

    let shadowed = pixel_at(&image, 32, 32);
    let lit = pixel_at(&image, 10, 32);

    assert!(
        lit[0] > 60,
        "the far floor should be lit by the sun, got {lit:?}"
    );
    assert!(
        shadowed[0] + 20 < lit[0],
        "the floor under the block should be clearly darker with cascades on as well; \
         shadowed {shadowed:?}, lit {lit:?}"
    );
}

#[test]
fn the_sky_still_faces_the_right_way_with_cascades_on() {
    // **The bug that cost this feature its first capture**, pinned so it cannot come back.
    //
    // `sky.wgsl` kept its own copy of the per-view uniform struct. Cascades turned one matrix in it
    // into an array of four, and the copy was not updated — so the three vectors that turn a screen
    // position into a world direction were read 192 bytes early. Nothing failed to compile and
    // nothing failed validation: the sky was simply drawn facing somewhere else, which showed as a
    // vast dark wedge across the horizon.
    //
    // Both shaders now share one declaration (`view.wgsl`). This checks the property that broke:
    // with the camera level, the top of the frame is sky and the sky above the horizon is *bright*.
    // A misread direction sends it somewhere dark, which is what made the bug visible in the first
    // place.
    let mut world = a_floor_under_a_floating_box(ShadowMode::Cascaded { blend: 0.5 });

    let Some(image) = capture(&mut world, 64, 64) else {
        return;
    };

    // Well above the horizon, where nothing but sky can be.
    let sky = pixel_at(&image, 32, 2);
    assert!(
        sky[2] > 40,
        "the sky above the horizon should be visibly blue; got {sky:?}. A dark value here is the \
         sky shader reading its direction vectors from the wrong offset — see view.wgsl"
    );
}

#[test]
fn bloom_actually_reaches_the_pixels() {
    // **`Bloom` was authorable and read by nothing.** It has been a reflected field on `Environment`
    // since ADR 0034 — spellable in a `.environment` file, visible to `describe`, validated by
    // `amadeo check` — and the renderer ignored it completely. A scene could ask for it and silently
    // get nothing, which is the same class of defect as the asset that failed to build in silence.
    //
    // The property that says bloom happened is that light appears **outside** the thing emitting it.
    // A small bright quad on a dark background bleeds into the surrounding pixels; without bloom
    // those pixels are exactly the background, because nothing else in the pipeline moves light
    // sideways.
    let build = |world: &mut World| {
        let entity = world.spawn();
        world.insert(entity, Transform::at(0.0, 0.0));
        // Brighter than white, which is what the HDR scene target exists to carry and what gives
        // the bright pass something above the threshold to find.
        world.insert(entity, Quad::new(1.0, 1.0, [6.0, 6.0, 6.0, 1.0]));
    };

    let mut off = World::new();
    add_camera_named(&mut off, 10.0, "test_look");
    build(&mut off);

    let mut on = World::new();
    add_camera_named(&mut on, 10.0, "test_look");
    build(&mut on);

    let glowing = Environment {
        bloom: amadeo_render::Bloom {
            threshold: 1.0,
            intensity: 1.5,
        },
        ..Environment::default()
    };

    let (Some(off), Some(on)) = (
        capture_with(&mut off, Environment::default(), 64, 64),
        capture_with(&mut on, glowing, 64, 64),
    ) else {
        return;
    };

    // Just outside the quad: it covers one world unit in a ten-unit view, so about six pixels across
    // the middle of a 64-pixel image, with its edge near x = 35.
    //
    // **Eight pixels from centre, because the glow is deliberately tight**: nine taps on a half-
    // resolution target reach four half-res texels, which is eight full-resolution pixels. Measured
    // across this row it runs 251, 172, 109, 73 and is back to the background by x = 44. Widening it
    // means a downsample chain rather than a bigger kernel — see `bloom.wgsl`.
    let beside_off = pixel_at(&off, 40, 32);
    let beside_on = pixel_at(&on, 40, 32);

    assert!(
        beside_on[0] > beside_off[0] + 8,
        "with bloom on, the background beside a bright quad should pick up light from it: \
         {beside_off:?} without, {beside_on:?} with"
    );

    // And the far corner is beyond the blur, so it must be untouched — otherwise this is measuring
    // something that brightened the whole image rather than a glow with a *radius*.
    let corner_off = pixel_at(&off, 1, 1);
    let corner_on = pixel_at(&on, 1, 1);
    assert!(
        corner_on[0].abs_diff(corner_off[0]) < 4,
        "bloom must have a radius rather than lifting the whole picture: corner went \
         {corner_off:?} to {corner_on:?}"
    );
}

#[test]
fn bloom_off_is_byte_identical_to_before_it_existed() {
    // The control, and the reason the test above is evidence. `Bloom::intensity` defaults to zero,
    // so every existing scene must render *exactly* as it did — not nearly. If bloom's passes ran
    // regardless, or the black placeholder were not exactly black, this is what would say so.
    //
    // Byte-identical rather than close, because "close" is what an accidental extra full-screen pass
    // would also be.
    let build = |world: &mut World| {
        let entity = world.spawn();
        world.insert(entity, Transform::at(0.0, 0.0));
        world.insert(entity, Quad::new(1.0, 1.0, [6.0, 6.0, 6.0, 1.0]));
    };

    let mut bare = World::new();
    add_camera(&mut bare, 10.0);
    build(&mut bare);

    let mut defaulted = World::new();
    add_camera_named(&mut defaulted, 10.0, "test_look");
    build(&mut defaulted);

    let (Some(bare), Some(defaulted)) = (
        capture(&mut bare, 64, 64),
        capture_with(&mut defaulted, Environment::default(), 64, 64),
    ) else {
        return;
    };

    assert_eq!(
        bare.pixels, defaulted.pixels,
        "the default environment must still be a no-op now that bloom exists"
    );
}

/// A dark floor with no sun at all, so the only light is whatever the test adds.
///
/// **The directional light has zero intensity rather than being absent**, which is what makes these
/// tests measure a point or spot light rather than a change in the sun. A world with no light entity
/// at all takes a different path through the collection pass, and testing that path here would be
/// testing two things at once.
fn an_unlit_floor() -> World {
    let mut world = World::new();

    let eye = world.spawn();
    world.insert(
        eye,
        Transform {
            translation: [0.0, 14.0, 0.0],
            rotation: [-90.0, 0.0, 0.0],
            ..Transform::default()
        },
    );
    world.insert(eye, Camera::perspective(60.0));

    let sun = world.spawn();
    world.insert(sun, Transform::default());
    world.insert(
        sun,
        DirectionalLight {
            intensity: 0.0,
            ..DirectionalLight::default()
        },
    );

    let mut meshes = MeshCache::new();
    meshes.insert(
        "floor",
        BoxMesh {
            size: [20.0, 0.2, 20.0],
        }
        .tessellate(),
    );
    world.insert_service(meshes);

    let mut materials = MaterialCache::new();
    materials.insert(
        "pale",
        Material {
            base_colour: [0.9, 0.9, 0.9, 1.0],
            ..Material::default()
        },
    );
    world.insert_service(materials);

    let floor = world.spawn();
    world.insert(floor, Transform::at_xyz(0.0, 0.0, 0.0));
    world.insert(floor, Mesh::new("floor", "pale"));
    world
}

#[test]
fn a_point_light_actually_reaches_the_pixels() {
    // **ADR 0057's whole claim in one assertion**: the engine had exactly one light and it had no
    // position, so nothing in a scene could be lit *from somewhere*. This is a bulb over a floor.
    //
    // The property that says a point light happened is that brightness falls off **with distance**.
    // A directional light of any colour lights a flat floor evenly, so a test that only checked
    // "the floor got brighter" would pass against one wired up wrongly as directional.
    let mut world = an_unlit_floor();
    let lamp = world.spawn();
    world.insert(lamp, Transform::at_xyz(0.0, 3.0, 0.0));
    world.insert(
        lamp,
        PointLight {
            colour: [1.0, 1.0, 1.0],
            intensity: 40.0,
            range: 12.0,
        },
    );

    // **The baseline is the same floor with no lamp at all**, and it matters more than it looks.
    // A camera naming no environment still gets the neutral cube map, which lights every surface
    // to about 94 — so "the floor is bright" is true before a light is added, and an assertion on an
    // absolute value would pass against a `PointLight` the renderer ignored completely.
    let mut unlit = an_unlit_floor();

    let (Some(unlit), Some(image)) = (capture(&mut unlit, 64, 64), capture(&mut world, 64, 64))
    else {
        return;
    };

    // The camera looks straight down from above, so screen centre is the floor directly under the
    // lamp and the edges are the floor several units away from it.
    let under = pixel_at(&image, 32, 32);
    let away = pixel_at(&image, 4, 32);
    let baseline = pixel_at(&unlit, 32, 32);

    assert!(
        under[0] > baseline[0] + 40,
        "the floor under the lamp should be much brighter than the same floor with no lamp: \
         {under:?} against {baseline:?}"
    );
    assert!(
        under[0] > away[0] + 40,
        "a point light falls off with distance, so under it must be clearly brighter than the \
         floor to the side: {under:?} under, {away:?} away. Equal brightness means it was wired \
         up as a directional light"
    );
}

#[test]
fn a_spot_light_lights_a_cone_and_not_the_rest() {
    // The property that distinguishes a spot from a point light, and the one a flashlight is: light
    // *inside* the cone and none outside it.
    //
    // Aimed straight down from three units up with a narrow cone, so the lit patch is a small disc
    // under it. A spot whose cone was ignored would light the whole floor exactly as the point-light
    // test does — which is why that test alone cannot cover this.
    let mut world = an_unlit_floor();
    let torch = world.spawn();
    world.insert(
        torch,
        Transform {
            translation: [0.0, 3.0, 0.0],
            // Aimed like a camera: pitched down 90 degrees puts its negative Z on the floor.
            rotation: [-90.0, 0.0, 0.0],
            ..Transform::default()
        },
    );
    world.insert(
        torch,
        SpotLight {
            colour: [1.0, 1.0, 1.0],
            intensity: 40.0,
            range: 12.0,
            inner_angle: 15.0,
            outer_angle: 20.0,
            ..SpotLight::default()
        },
    );

    let mut unlit = an_unlit_floor();

    let (Some(unlit), Some(image)) = (capture(&mut unlit, 64, 64), capture(&mut world, 64, 64))
    else {
        return;
    };

    let inside = pixel_at(&image, 32, 32);
    let outside = pixel_at(&image, 4, 32);
    let baseline = pixel_at(&unlit, 4, 32);

    assert!(
        inside[0] > baseline[0] + 40,
        "the floor inside the cone should be much brighter than unlit: {inside:?} against \
         {baseline:?}"
    );
    // **Against the unlit baseline rather than against a fixed number.** Ambient light already puts
    // this pixel near 94 with no lamp in the world at all, so an absolute threshold would either be
    // unsatisfiable or trivially true — the first version of this test asserted `< 40` and failed
    // for that reason rather than because the cone was wrong.
    assert!(
        outside[0].abs_diff(baseline[0]) <= 2,
        "the floor outside the cone should be exactly as dark as it is with no lamp at all: \
         {outside:?} against {baseline:?}. A brighter value means the cone was not applied and \
         this is behaving as a point light"
    );
}

#[test]
fn a_scene_with_no_punctual_lights_is_unchanged_by_them_existing() {
    // The control. Every existing scene has no `PointLight` and no `SpotLight`, so every existing
    // capture must be **byte-identical** — not close. A loop that ran once over a zeroed light, or a
    // count read from the wrong place, would show up here and nowhere else.
    let mut before = a_floor_under_a_floating_box(ShadowMode::Orthogonal);
    let mut after = a_floor_under_a_floating_box(ShadowMode::Orthogonal);
    // The second world gets a light with zero intensity, which the collection pass drops — so this
    // also pins that "authored but off" costs nothing, which is what `intensity` defaulting to a
    // usable value would otherwise quietly break.
    let dark = after.spawn();
    after.insert(dark, Transform::at_xyz(0.0, 3.0, 0.0));
    after.insert(
        dark,
        PointLight {
            intensity: 0.0,
            ..PointLight::default()
        },
    );

    let (Some(before), Some(after)) = (capture(&mut before, 64, 64), capture(&mut after, 64, 64))
    else {
        return;
    };

    assert_eq!(
        before.pixels, after.pixels,
        "a light with no intensity must contribute nothing at all"
    );
}

#[test]
fn a_spot_light_casts_a_shadow() {
    // **ADR 0058, and the second half of M3's renderer exam.** A flashlight that lights a corridor
    // but shines *through* the crate in front of it is exactly the moment a horror scene stops
    // working, and until now that is what every point and spot light did.
    //
    // A blocker between the torch and the floor. The floor directly beneath it must be darker than
    // the same floor with the torch not casting — and the comparison is against **the same scene
    // with `shadows: false`**, not against a number, so the only difference between the two captures
    // is the shadow itself.
    let build = |shadows: bool| {
        let mut world = an_unlit_floor();

        let torch = world.spawn();
        world.insert(
            torch,
            Transform {
                translation: [0.0, 6.0, 0.0],
                rotation: [-90.0, 0.0, 0.0],
                ..Transform::default()
            },
        );
        world.insert(
            torch,
            SpotLight {
                colour: [1.0, 1.0, 1.0],
                intensity: 8.0,
                range: 20.0,
                inner_angle: 30.0,
                outer_angle: 38.0,
                shadows,
                shadow_resolution: 1024,
                shadow_bias: 0.02,
            },
        );

        // Between the torch and the floor, and small enough that its shadow lands well inside the
        // cone rather than at the soft edge where the falloff is doing the darkening.
        let mut meshes = MeshCache::new();
        meshes.insert(
            "floor",
            BoxMesh {
                size: [20.0, 0.2, 20.0],
            }
            .tessellate(),
        );
        meshes.insert(
            "blocker",
            BoxMesh {
                size: [2.0, 0.4, 2.0],
            }
            .tessellate(),
        );
        world.insert_service(meshes);

        let blocker = world.spawn();
        world.insert(blocker, Transform::at_xyz(0.0, 5.0, 0.0));
        world.insert(blocker, Mesh::new("blocker", "pale"));
        world
    };

    // **Three captures, because two cannot say what "shadowed" means.** The unlit floor is the
    // value a fully shadowed pixel must return *to* — see the note in `docs/07` about absolute
    // thresholds measuring the ambient.
    let (Some(unlit), Some(off), Some(on)) = (
        capture(&mut an_unlit_floor(), 64, 64),
        capture(&mut build(false), 64, 64),
        capture(&mut build(true), 64, 64),
    ) else {
        return;
    };

    // The camera looks straight down and the blocker is centred, so it covers screen centre in both.
    // The floor to the side is inside the cone and inside the blocker's shadow.
    let baseline = pixel_at(&unlit, 16, 32);
    let unshadowed = pixel_at(&off, 16, 32);
    let shadowed = pixel_at(&on, 16, 32);

    // The torch reaches this pixel at all, which is what makes the next assertion mean something.
    assert!(
        i32::from(unshadowed[0]) > i32::from(baseline[0]) + 10,
        "the torch should light this pixel when it is not casting: {unshadowed:?} against an \
         unlit {baseline:?}"
    );
    // And casting takes it **all the way back** to unlit — the blocker is directly between, so the
    // right answer is not "darker" but "none of this light arrives".
    assert!(
        shadowed[0].abs_diff(baseline[0]) <= 2,
        "with the torch casting, this pixel is behind the blocker and should get none of its \
         light: {shadowed:?} against an unlit {baseline:?}"
    );

    // And well outside the blocker's shadow the two must agree, or this is measuring the light
    // getting dimmer overall rather than a shadow with a *shape*.
    let clear_on = pixel_at(&on, 32, 60);
    let clear_off = pixel_at(&off, 32, 60);
    assert!(
        clear_on[0].abs_diff(clear_off[0]) < 8,
        "outside the blocker's shadow the two should match: {clear_on:?} against {clear_off:?}"
    );
}

/// A shadow-casting spot light in a scene with **no directional light at all**.
///
/// # Why this scene rather than another
///
/// Every other capture test here has a `DirectionalLight` in it, and so does every game in the
/// repository. That left one configuration completely uncovered — and it is exactly the one M3's
/// exit gate asks for, a dark corridor lit by a moving flashlight. A spot here takes **layer 0** of
/// the shadow array, where the cascades normally sit, so it is the case where the directional path
/// and the spot path are most likely to disagree about an index.
///
/// The control is the *same light with shadows off*. Nothing occludes this floor, so asking for a
/// shadow map must not change the picture — and comparing against that rather than against an
/// absolute value is what stops this passing against a spot the renderer ignored entirely.
///
/// # It was written to reproduce a bug that turned out not to exist
///
/// Session 18 filed Q39 against a black screen in `games/warren`. This test is what disproved it:
/// the configuration works. The screen was black because a capture at **tick 0** renders child
/// transforms that `propagate_transforms` has never composed, so the game's beam — a grandchild of
/// the player — sat at its local `y = -0.1`, inside the floor slab, and quite correctly shadowed
/// everything with the floor it was buried in.
///
/// The test is worth keeping anyway. It is precisely the coverage gap that made the wrong diagnosis
/// plausible for as long as it was.
#[test]
fn a_shadow_casting_spot_lights_a_scene_that_has_no_sun() {
    let mut lit = an_unlit_floor();
    let torch = lit.spawn();
    // Above the floor and aimed straight down, so the cone lands in the middle of the picture.
    let mut placement = Transform::at_xyz(0.0, 4.0, 0.0);
    placement.rotation = [-90.0, 0.0, 0.0];
    lit.insert(torch, placement);
    lit.insert(
        torch,
        SpotLight {
            colour: [1.0, 1.0, 1.0],
            intensity: 60.0,
            range: 16.0,
            inner_angle: 14.0,
            outer_angle: 30.0,
            shadows: true,
            ..SpotLight::default()
        },
    );

    // The same light with shadows off. It is the control, and it is what says the geometry, the
    // angles and the intensity are all fine — the *only* difference is the flag.
    let mut without = an_unlit_floor();
    let plain = without.spawn();
    let mut placement = Transform::at_xyz(0.0, 4.0, 0.0);
    placement.rotation = [-90.0, 0.0, 0.0];
    without.insert(plain, placement);
    without.insert(
        plain,
        SpotLight {
            colour: [1.0, 1.0, 1.0],
            intensity: 60.0,
            range: 16.0,
            inner_angle: 14.0,
            outer_angle: 30.0,
            shadows: false,
            ..SpotLight::default()
        },
    );

    let (Some(with_shadows), Some(no_shadows)) =
        (capture(&mut lit, 64, 64), capture(&mut without, 64, 64))
    else {
        return;
    };

    let shadowed = pixel_at(&with_shadows, 32, 32);
    let plain_pixel = pixel_at(&no_shadows, 32, 32);

    // Nothing occludes this floor, so asking for a shadow map must not change the picture at all.
    assert!(
        shadowed[0] as i32 > plain_pixel[0] as i32 - 8,
        "a spot light with nothing between it and the floor should light that floor whether or not \
         it casts. With shadows it reads {shadowed:?}; with the same light and shadows off it reads \
         {plain_pixel:?}"
    );
}

/// A lit red box with a named environment on the camera, so a look can be varied.
fn a_lit_box_under(look: Environment) -> World {
    let mut world = a_lit_box([1.0, 0.0, 0.0, 1.0], [2.0, 2.0, 2.0]);
    let mut looks = EnvironmentCache::new();
    looks.insert("look", look);
    world.insert_service(looks);

    let eye = world
        .query::<(&Camera,)>()
        .map(|(entity, _)| entity)
        .next()
        .expect("a_lit_box makes one");
    if let Some(camera) = world.get_mut::<Camera>(eye) {
        camera.environment = "look".to_string();
    }
    world
}

#[test]
fn fog_off_is_byte_identical() {
    // **ADR 0073's central claim, and the one that decides whether adding fog was safe.** Every
    // `.environment` in the repository defaults to `density 0.0`, so every existing picture — the
    // Vault's captured pixels, the Atrium's, the Scarp's — has to come out exactly as it did.
    //
    // The shader returns early at zero density rather than computing `mix(colour, fog, 0.0)`, which
    // would also be exact but would depend on that being true of every driver's `mix`.
    let mut plain = a_lit_box([1.0, 0.0, 0.0, 1.0], [2.0, 2.0, 2.0]);
    let Some(before) = capture(&mut plain, 64, 64) else {
        return;
    };

    let mut with_the_field = a_lit_box_under(Environment::default());
    let after = capture(&mut with_the_field, 64, 64).expect("the device worked a moment ago");

    assert_eq!(
        before.pixels, after.pixels,
        "a scene that authors no fog must render exactly as it did before fog existed"
    );
}

#[test]
fn fog_actually_reaches_the_pixels() {
    // The control for the test above: a renderer that ignored the field entirely would pass that
    // one perfectly. The box is five units away and the fog is dense enough to have closed most of
    // the way by then, so its lit red face should read as the fog's blue instead.
    let mut world = a_lit_box_under(Environment {
        fog: Fog {
            colour: [0.0, 0.0, 1.0],
            density: 0.4,
            start: 0.0,
        },
        ..Environment::default()
    });
    let Some(image) = capture(&mut world, 64, 64) else {
        return;
    };

    let centre = pixel_at(&image, 32, 32);
    assert!(
        centre[2] > centre[0],
        "four units of dense blue fog should have taken a red face blue, got {centre:?}"
    );
}

#[test]
fn fog_thickens_with_distance() {
    // **The property that makes it fog rather than a tint**, and the one a wrong sign or a missing
    // `distance` would break while still passing the test above. The same box twice, at two
    // distances, under the same fog: the far one has to be further along towards the fog colour.
    let redness_at = |depth: f32| -> Option<u8> {
        let mut world = a_lit_box_under(Environment {
            fog: Fog {
                colour: [0.0, 0.0, 1.0],
                density: 0.12,
                start: 0.0,
            },
            ..Environment::default()
        });
        // Push the box away from the camera, which sits at +5 looking down −Z.
        let thing = world
            .query::<(&Mesh,)>()
            .map(|(entity, _)| entity)
            .next()
            .expect("a_lit_box makes one");
        if let Some(transform) = world.get_mut::<Transform>(thing) {
            transform.translation = [0.0, 0.0, -depth];
        }
        capture(&mut world, 64, 64).map(|image| pixel_at(&image, 32, 32)[0])
    };

    let Some(near) = redness_at(0.0) else {
        return;
    };
    let far = redness_at(12.0).expect("the device worked a moment ago");
    assert!(
        far < near,
        "the further box should be more fogged: near {near}, far {far}"
    );
}

#[test]
fn an_arch_draws_as_a_vault_rather_than_a_box() {
    // **The engine's first curved primitive, checked on a screen and not only in arithmetic.**
    // Its unit tests pin the geometry; this pins that the geometry becomes pixels, and it is worth
    // having separately because a normal pointing the wrong way is arithmetically fine and renders
    // black (ADR 0052 means it still *draws* — it is just lit from behind).
    //
    // The camera sits inside a section looking down its length with a lamp overhead. A vault lights
    // brightly at the crown and falls off towards the springing, so the test is that the top of the
    // frame is lit and the lower corners are not — which a cuboid room would fail, since a flat
    // ceiling under a point light has no falloff across the frame's width.
    let mut world = World::new();

    let eye = world.spawn();
    let mut place = Transform::at(0.0, 0.0);
    place.translation = [0.0, 1.6, 3.0];
    world.insert(eye, place);
    world.insert(eye, Camera::perspective(70.0));

    let mut meshes = MeshCache::new();
    meshes.insert(
        "tunnel",
        ArchMesh {
            width: 5.0,
            height: 3.4,
            length: 16.0,
            segments: 16,
            floor: true,
        }
        .tessellate(),
    );
    world.insert_service(meshes);

    let mut materials = MaterialCache::new();
    materials.insert(
        "plaster",
        Material {
            base_colour: [0.55, 0.53, 0.47, 1.0],
            roughness: 0.95,
            ..Material::default()
        },
    );
    world.insert_service(materials);

    let tunnel = world.spawn();
    world.insert(tunnel, Transform::at(0.0, 0.0));
    world.insert(tunnel, Mesh::new("tunnel", "plaster"));

    let lamp = world.spawn();
    let mut lit = Transform::at(0.0, 0.0);
    lit.translation = [0.0, 2.6, -1.0];
    world.insert(lamp, lit);
    world.insert(
        lamp,
        PointLight {
            colour: [0.9, 0.85, 0.7],
            intensity: 12.0,
            range: 9.0,
        },
    );

    let Some(image) = capture(&mut world, 128, 96) else {
        return;
    };

    let crown = pixel_at(&image, 64, 10);
    let corner = pixel_at(&image, 4, 90);
    assert!(
        crown[0] > 40,
        "the crown of the vault should catch the lamp, got {crown:?}"
    );
    assert!(
        crown[0] > corner[0] + 20,
        "a vault falls off from crown to springing; crown {crown:?} corner {corner:?}"
    );
}

/// A world holding one shape at the origin, lit from one side, with the camera looking at it.
///
/// Shared by the ADR 0074 capture tests below so they differ only in the shape and in what they
/// assert about the pixels. The light is deliberately **off to one side** rather than behind the
/// camera: a head-on light flattens everything and would make a faceted and a smooth shape look
/// nearly the same, which is the one thing these tests have to tell apart.
fn world_showing(mesh: MeshData, eye_at: [f32; 3], pitch: f32) -> World {
    world_showing_from(mesh, eye_at, pitch, 0.0)
}

/// As [`world_showing`], but the camera may also be yawed — which the stair needs.
///
/// A `StairMesh` climbs along **+Z**, so its tallest step is its **+Z end**. A camera on the +Z axis
/// is therefore looking at the back of the flight, where the top step occludes every step behind it
/// and the whole thing renders as a truncated slab. Seen from −Z, with the camera yawed round, each
/// tread and riser is visible in turn. This was found by rendering it, and it is the second framing
/// mistake in this file that an assertion happily passed.
fn world_showing_from(mesh: MeshData, eye_at: [f32; 3], pitch: f32, yaw: f32) -> World {
    let mut world = World::new();

    // **The caller places the camera, and that is not over-parameterisation.** These shapes share
    // neither an origin nor a footprint: a cylinder and a sphere are centred on their origin; a wedge
    // sits *on* y = 0 because it is a thing you put on a floor; and a default stair is 2.2 deep and
    // 1.4 tall, climbing along +Z straight at a camera that sits on the +Z axis.
    //
    // Two framings were tried and both were found by rendering the shape and looking at it rather
    // than by any assertion. At eye level on z = 3 the stair filled the top half of the frame with a
    // single flat band, because the camera was nearly inside the top step — and the assertion passed.
    // Then one formula for every shape put the camera high enough that a cone, which is widest at its
    // base, had its base below the bottom of the frame.
    //
    // A stair also needs its own *angle*, not just its own distance: seen level-on, every riser faces
    // the camera and shades identically, so a flight of steps looks exactly like a slab. It has to be
    // looked down on before the treads and risers alternate.
    let eye = world.spawn();
    world.insert(
        eye,
        Transform {
            translation: eye_at,
            rotation: [pitch, yaw, 0.0],
            ..Transform::default()
        },
    );
    world.insert(eye, Camera::perspective(45.0));

    let mut meshes = MeshCache::new();
    meshes.insert("subject", mesh);
    world.insert_service(meshes);

    let mut materials = MaterialCache::new();
    materials.insert(
        "chalk",
        Material {
            base_colour: [0.55, 0.53, 0.5, 1.0],
            roughness: 0.9,
            ..Material::default()
        },
    );
    world.insert_service(materials);

    let subject = world.spawn();
    world.insert(subject, Transform::at(0.0, 0.0));
    world.insert(subject, Mesh::new("subject", "chalk"));

    // A light aims like a camera — along its own negative Z — so its direction is a rotation rather
    // than a vector, which is the vocabulary a scene file has. Pitched down and yawed round to the
    // left, so the subject is lit from off to one side.
    let sun = world.spawn();
    world.insert(
        sun,
        Transform {
            rotation: [-25.0, 40.0, 0.0],
            ..Transform::default()
        },
    );
    world.insert(
        sun,
        DirectionalLight {
            colour: [1.0, 0.97, 0.92],
            // Deliberately modest. At 3.0 against a bright material every lit pixel clipped to 255,
            // so a smooth sphere and a faceted one produced byte-identical flat white — the shading
            // these tests exist to compare was entirely above the top of the range.
            intensity: 1.0,
            ..DirectionalLight::default()
        },
    );

    world
}

#[test]
fn every_parametric_shape_draws_something_lit() {
    // ADR 0074's set, on a screen. Its unit tests pin the geometry; this pins that the geometry
    // becomes pixels — a separate claim, and the one the session that added these shapes made in a
    // commit message rather than in the repository.
    //
    // Two failures, and the second is the one this originally missed. A shape can fail to draw at
    // all; or it can draw **black**, which is what an inverted normal does under ADR 0052 since
    // nothing is culled. The comment here used to name that second case while the assertion could not
    // see it: the clear colour is deliberately *not* black (`backend.rs`), so a black shape differs
    // from the background and counted as drawn. Hence a brightness floor as well as a difference.
    //
    // The camera is per shape — see `world_showing`. A default stair is twice the extent of a default
    // sphere, and both of the framings tried before this one hid a shape that was drawing perfectly
    // well.
    for (name, mesh, eye_at, pitch) in [
        (
            "cylinder",
            CylinderMesh::default().tessellate(),
            [0.0, 0.35, 2.0],
            -8.0,
        ),
        (
            "cone",
            CylinderMesh {
                top_radius: 0.0,
                ..CylinderMesh::default()
            }
            .tessellate(),
            [0.0, 0.35, 2.0],
            -8.0,
        ),
        (
            "sphere",
            SphereMesh::default().tessellate(),
            [0.0, 0.3, 1.9],
            -6.0,
        ),
        (
            "wedge",
            WedgeMesh::default().tessellate(),
            [0.0, 0.9, 2.4],
            -14.0,
        ),
        (
            "stair",
            StairMesh::default().tessellate(),
            [0.0, 2.6, -4.4],
            -26.0,
        ),
    ] {
        let yaw = if eye_at[2] < 0.0 { 180.0 } else { 0.0 };
        let mut world = world_showing_from(mesh, eye_at, pitch, yaw);
        let Some(image) = capture(&mut world, 96, 96) else {
            return;
        };

        // A grid over the middle of the frame rather than one centre pixel, because these shapes do
        // not share an origin: a cylinder and a sphere are centred on theirs, while a wedge and a
        // stair sit *on* y = 0 because they are things you put on a floor. So the exact centre of the
        // frame is the middle of a sphere and the bottom edge of a wedge, and sampling it alone
        // reported the wedge as missing when it was merely below the crosshair.
        let background = pixel_at(&image, 2, 2);
        let samples: Vec<[u8; 4]> = (24..72)
            .step_by(8)
            .flat_map(|x| (24..72).step_by(8).map(move |y| (x, y)))
            .map(|(x, y)| pixel_at(&image, x, y))
            .collect();

        let drawn = samples.iter().filter(|p| **p != background).count();
        assert!(
            drawn >= 6,
            "a `{name}` covered only {drawn} of {} sample points in the middle of the frame, so it \
             either did not draw or is the same colour as the background {background:?}",
            samples.len()
        );

        // And it is *lit*, not merely present. `background` is a deliberately non-black clear colour,
        // so without this a shape rendering pure black — an inverted normal — would count as drawn.
        let brightest = samples.iter().map(|p| p[0]).max().unwrap_or(0);
        assert!(
            brightest > 60,
            "a `{name}` drew, but its brightest sample is {brightest}, which is dark enough to be a \
             surface lit from behind rather than in front"
        );
    }
}

#[test]
fn a_stair_draws_as_a_flight_rather_than_as_a_box() {
    // **The assertion `docs/13` item 5 actually asks for: one that a `BoxMesh` would fail.** The
    // previous version of this file asserted only that a stair covered the frame, which a box of the
    // same bounds does equally well — a useless test in a repository whose founding measurement is
    // "23 of 23 meshes are boxes".
    //
    // A flight of steps lit from one side is a *run of alternating brightnesses* down its climb: each
    // tread faces up and each riser faces the camera, so they catch different amounts of light. A box
    // has one front face and one top face, so a vertical line down it crosses at most one boundary.
    // That is the difference, and it is a property of what a stair *is* rather than of where any
    // particular step lands.
    let bands_down_the_middle = |mesh: MeshData| -> Option<usize> {
        // From the flight's **low** end, looking up it. See `world_showing_from`.
        let mut world = world_showing_from(mesh, [0.0, 3.0, -4.2], -32.0, 180.0);
        let image = capture(&mut world, 96, 96)?;

        let mut bands = 0;
        let mut previous = pixel_at(&image, 48, 6)[0];
        for y in 7..64 {
            let here = pixel_at(&image, 48, y)[0];
            if here.abs_diff(previous) > 3 {
                bands += 1;
            }
            previous = here;
        }
        Some(bands)
    };

    let stair = StairMesh {
        width: 2.0,
        steps: 6,
        rise: 0.28,
        run: 0.34,
    };
    // A box of the stair's own bounding volume — the control, and the thing the old assertion could
    // not tell apart from the real mesh.
    let box_of_the_same_size = BoxMesh {
        size: [2.0, stair.total_rise(), stair.total_run()],
    };

    let (Some(steps), Some(slab)) = (
        bands_down_the_middle(stair.tessellate()),
        bands_down_the_middle(box_of_the_same_size.tessellate()),
    ) else {
        return;
    };

    assert!(
        steps > slab,
        "a flight of steps should cross more brightness bands down the frame than a box of the same \
         bounds: stair {steps}, box {slab}"
    );
    assert!(
        steps >= 3,
        "a six-step flight crossed only {steps} brightness bands, which is not a staircase"
    );
}

#[test]
fn a_faceted_sphere_reads_as_faceted_and_a_smooth_one_does_not() {
    // **The art-direction claim, checked as pixels rather than as normals.** `docs/12-the-bar.md` §3
    // makes low poly first-class, and the whole of what makes a low-poly shape read as deliberate is
    // that its facets catch light in flat steps while its silhouette stays angular. A smooth sphere
    // at the same triangle count has a polygonal outline and perfectly continuous shading, which is
    // what reads as unfinished.
    //
    // Measured as **how many times the brightness steps by more than one level** along a horizontal
    // line across the ball. The two scanlines this produces are worth writing down, because they are
    // the feature:
    //
    //   flat  [144 ×28, 183 ×28]                 — facets, each one constant value
    //   smooth[145, 147, 149, 150, 152, … , 194] — a continuous ramp
    //
    // **Read the counts, not the appearance of those rows.** The smooth line changes at nearly every
    // pixel, but by *one* level at a time, and `abs_diff > 1` deliberately does not count that — a
    // one-level ramp is exactly what dithering or rounding noise looks like, and counting it would
    // make a smooth surface's score depend on the exposure. So the measured numbers are far smaller
    // than the rows suggest: **smooth 7, faceted 1**. An earlier version of this comment implied
    // about fifty, which is the number a future session would have checked against when this went
    // flaky.
    //
    // Both a ratio and a floor are asserted, because seven against one is a comfortable ratio built
    // out of small absolute numbers, and a change that quietly drove both to zero — a blank frame, a
    // clipped exposure — would satisfy a ratio test and nothing else.
    let levels = |flat: bool| -> Option<usize> {
        let mut world = world_showing(
            SphereMesh {
                radius: 1.2,
                segments: 10,
                rings: 8,
                flat,
            }
            .tessellate(),
            [0.0, 0.3, 3.3],
            -5.0,
        );
        let image = capture(&mut world, 96, 96)?;

        let mut steps = 0;
        let mut previous = pixel_at(&image, 20, 48)[0];
        for x in 21..76 {
            let here = pixel_at(&image, x, 48)[0];
            // A step of more than one level, so sampling noise in a smooth gradient is not counted
            // as a facet edge while a real facet boundary is.
            if here.abs_diff(previous) > 1 {
                steps += 1;
            }
            previous = here;
        }
        Some(steps)
    };

    let (Some(faceted), Some(smooth)) = (levels(true), levels(false)) else {
        return;
    };

    assert!(
        smooth >= 4,
        "a smooth sphere's scanline stepped only {smooth} times, so there is no gradient to compare \
         against — the frame is probably blank, or the exposure is clipping"
    );
    assert!(
        smooth > faceted * 2,
        "a smooth sphere should change brightness far more often across a scanline than a faceted \
         one: smooth {smooth} steps, faceted {faceted}"
    );
}
