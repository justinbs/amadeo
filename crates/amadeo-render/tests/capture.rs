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
    Camera, Environment, EnvironmentCache, NullBackend, Quad, RenderBackend, Renderer, TextureData,
    Vignette, WgpuBackend, render_quads,
};
use amadeo_transform::Transform;

/// Renders one world offscreen and hands back the pixels, or `None` if this machine has no GPU.
fn capture(world: &mut World, width: u32, height: u32) -> Option<TextureData> {
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
