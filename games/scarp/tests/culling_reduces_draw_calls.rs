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
use amadeo_terrain::Terrain;
use scarp::build_simulation;

/// Advances until the terrain streamer has stopped producing geometry.
///
/// # Why this exists, and it is the fourth time
///
/// **How many chunks have geometry after a fixed number of ticks is machine speed.** Meshes are
/// built on a job pool and `TerrainUpdate::meshes` is documented as timing-dependent by design
/// (ADR 0041 §2) — only colliders and residency are required to arrive on a definite tick.
///
/// The first version of this test ran 200 ticks and asserted `meshes > 20`. On this machine 50 had
/// arrived; on a CI runner, 17. **The three assertions that actually measure culling all passed
/// there** — 8 of 17 in view, 8 submitted — and the *guard clause*, checking the world was big
/// enough to be worth measuring, was the thing asserting on how fast the machine was.
///
/// That is the same defect this session has now recorded three times in `docs/07`, arriving a fourth
/// time in the one place nobody thinks to look: the setup of the test that proves the gate.
///
/// Waiting until the pool is idle *and* the count has stopped moving makes the number a pure
/// function of residency and the terrain source, which is identical everywhere. `in_flight` is
/// documented as diagnostics-only and must never reach gameplay; a test deciding when to stop
/// waiting is exactly the use it is for.
fn settle(app: &mut amadeo_app::App) {
    let mut previous = usize::MAX;
    let mut quiet_ticks = 0;

    for _ in 0..200 {
        app.run_ticks(1).expect("the world advances");

        // **Wait at the barrier rather than running more ticks and hoping.** Without this the main
        // thread simply outruns the workers: at one worker, six hundred ticks went by before the
        // pool had finished, and the loop gave up. A barrier is ADR 0041's blessed shape and cannot
        // change what comes out, only when.
        app.world
            .service::<Terrain>()
            .expect("terrain is installed")
            .streamer
            .wait_for_idle();

        // One more tick drains what the barrier just finished into the cache, so the count below is
        // of geometry that has actually arrived rather than of jobs that have merely completed.
        app.run_ticks(1).expect("the world advances");

        let cached = describe_frame(&app.world).drawn.len();
        if cached == previous {
            quiet_ticks += 1;
            if quiet_ticks >= 2 {
                return;
            }
        } else {
            quiet_ticks = 0;
        }
        previous = cached;
    }
    panic!("the terrain streamer never went quiet; something is re-meshing every tick");
}

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
    // Long enough for the player to fall and settle, so residency stops moving.
    app.run_ticks(200).expect("the world advances");
    // **And then until the meshing pool is actually quiet** — see `settle` for why the first line is
    // not enough, and why assuming it was is a mistake this repository has now made four times.
    settle(&mut app);

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

#[test]
fn once_the_pool_is_quiet_the_count_is_the_same_at_every_thread_count() {
    // **The corrected form of a test that was wrong twice.**
    //
    // The first version asserted the two worlds submitted the same number of meshes *at a fixed
    // tick*. That fails and should: `TerrainUpdate::meshes` is timing-dependent by design (ADR 0041
    // §2), so at tick 200 a one-worker world has simply done less. Deleting it left the impression
    // there was no claim to make here — and then CI failed the *other* test for exactly the same
    // reason, which showed there was.
    //
    // The claim that does hold is about the **settled** state: once nothing is in flight, which
    // chunks have geometry is a pure function of residency and the terrain source, and neither knows
    // how many threads there are. A single worker is the closest thing to a loaded CI runner this
    // machine can produce, and it is what would have caught the failure before pushing.
    let mut slow = scarp::build_with_workers(1).expect("builds");
    let mut fast = scarp::build_with_workers(8).expect("builds");

    for app in [&mut slow, &mut fast] {
        app.run_ticks(200).expect("the world advances");
        settle(app);
    }

    let count = |app: &mut amadeo_app::App| -> (usize, usize) {
        let described = describe_frame(&app.world).drawn.len();
        let submitted = frame(&mut app.world)
            .views
            .iter()
            .map(|view| view.meshes.len())
            .sum();
        (described, submitted)
    };
    assert_eq!(count(&mut slow), count(&mut fast));
}
