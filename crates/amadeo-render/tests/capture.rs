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
    BoxMesh, Camera, DirectionalLight, Environment, EnvironmentCache, Material, MaterialCache,
    Mesh, MeshCache, NullBackend, Quad, RenderBackend, Renderer, ShadowMode, TextureData, Vignette,
    WgpuBackend, render_quads,
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

    let centre = pixel_at(&image, 32, 32);
    assert!(
        centre[0] > 100 && centre[1] < 60 && centre[2] < 60,
        "the middle of the screen should be a lit red face, got {centre:?}"
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
    let turn = |degrees: f32| {
        let mut world = a_lit_box([1.0, 1.0, 1.0, 1.0], [2.0, 2.0, 2.0]);
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
