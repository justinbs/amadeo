//! **M2.5's exit gate 3**, measured rather than believed.
//!
//! The gate asks that frustum culling "demonstrably reduces draw calls, measured through
//! `render.describe` rather than believed". Two numbers, from one running world:
//!
//! - **`render.describe`** walks the *world* and says how many meshes exist and how many of them
//!   fall inside the view. That is the ground truth, and it is deliberately independent of whether
//!   culling is implemented at all — it would report the same split with culling turned off.
//! - **`FrameData`** is what the renderer actually submits. With culling, it should carry the
//!   visible ones and not the rest.
//!
//! The gate is met when the second follows the first. Checking only the frame would prove the
//! renderer is self-consistent and nothing else; checking only `describe` would prove nothing at
//! all, since it does not do the culling.

use amadeo_ecs::World;
use amadeo_render::{FrameData, NullBackend, Renderer, describe_frame, render_quads};
use scarp::build_simulation;

/// The frame this world would draw, through a backend that records rather than draws.
fn frame(world: &mut World) -> FrameData {
    world.insert_service(Renderer::new(Box::new(NullBackend::new(1280, 720))));
    render_quads(world);
    world
        .service::<Renderer>()
        .expect("installed")
        .null_backend()
        .expect("a null backend")
        .last_frame()
        .expect("a frame was drawn")
        .clone()
}

#[test]
fn culling_submits_only_what_the_camera_can_see() {
    let mut app = build_simulation().expect("the world builds");
    // Long enough for the player to settle and the streamer to fill the region around them.
    app.run_ticks(200).expect("the world advances");

    // What exists, and how much of it is in view. Independent of culling: `describe` reads the
    // world, not the frame.
    let description = describe_frame(&app.world);
    let in_world = description.drawn.len();
    let in_view = description.visible_count();

    let frame = frame(&mut app.world);
    let submitted: usize = frame.views.iter().map(|view| view.meshes.len()).sum();
    println!("exists {in_world}, in view {in_view}, submitted {submitted}");

    assert!(
        in_world > 20,
        "only {in_world} meshes exist; this world is too small to be measuring culling with"
    );
    assert!(
        in_view < in_world,
        "every one of {in_world} meshes is in view, so there is nothing to cull and this test \
         proves nothing — move the camera or widen the streamed region"
    );

    assert!(
        submitted < in_world,
        "culling submitted all {in_world} meshes; nothing was culled"
    );
    // Never fewer than are visible: culling something on screen makes geometry vanish as the camera
    // turns, which is far worse than submitting a few extra.
    assert!(
        submitted >= in_view,
        "culling dropped {} meshes that are in view",
        in_view - submitted
    );
}

// **A test that belongs here and cannot exist**, recorded because writing it and watching it fail
// was the clearest possible statement of ADR 0041 §2.
//
// The obvious companion to the above is "two worlds streamed at 1 and 8 workers submit the same
// number of meshes". It fails, and it *should*: how many chunks have geometry at tick 200 depends on
// what the job pool finished, which is machine speed. `TerrainUpdate::meshes` is documented as
// timing-dependent by design, and a mesh arriving a frame late is explicitly allowed — only
// colliders and residency are required to be identical.
//
// So there is no assertion to make about submitted mesh *counts* across thread counts. The claim
// that does hold is the one `a_walk_reproduces_at_every_thread_count` already makes: the simulation
// is identical, and rendering cannot reach it because `FrameData` is built from a Service (ADR
// 0009). Culling is downstream of that and cannot change it.
