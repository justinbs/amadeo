//! **M2.5's exit gate 4**: what a frame of an open world costs, with GPU time measured this time.
//!
//! `docs/10-frame-budget.md` recorded M2's numbers and named GPU execution time as the first thing
//! that gate could not answer: *"Needs timestamp queries in the wgpu backend."* This is that,
//! against a streamed terrain world rather than eleven boxes.
//!
//! # Why this reports rather than asserts, mostly
//!
//! A timing on a shared CI runner is a number about that runner, not about the engine. So the hard
//! assertions here are the ones that stay true on any machine — that the numbers exist, that they
//! are attributed to the right passes, and that they are not absurd — and the *values* are printed
//! for `docs/10` to record. That is the same split `docs/10` already documents under "What is
//! asserted, and what is only reported".
//!
//! Skips itself on a machine with no GPU, and on one whose adapter does not advertise
//! `TIMESTAMP_QUERY`, because neither is a failure of the engine.

use amadeo_render::{Renderer, WgpuBackend, describe_frame, render_quads};
use amadeo_terrain::Terrain;
use scarp::build_simulation;

/// Advances until the streamer has stopped producing geometry.
///
/// The same barrier `culling_reduces_draw_calls` uses, and for the same reason: how much of the
/// world has arrived after a fixed number of ticks is how fast the machine is, and a frame budget
/// measured over a half-built world is a number about nothing.
fn settle(app: &mut amadeo_app::App) {
    let mut previous = usize::MAX;
    for _ in 0..200 {
        app.run_ticks(1).expect("the world advances");
        app.world
            .service::<Terrain>()
            .expect("terrain is installed")
            .streamer
            .wait_for_idle();
        app.run_ticks(1).expect("the world advances");

        let cached = describe_frame(&app.world).drawn.len();
        if cached == previous {
            return;
        }
        previous = cached;
    }
}

#[test]
fn a_frame_of_open_world_is_measured_on_the_gpu() {
    let mut app = build_simulation().expect("the world builds");
    app.run_ticks(200).expect("the world advances");
    settle(&mut app);

    let description = describe_frame(&app.world);
    let (in_world, in_view) = (description.drawn.len(), description.visible_count());

    // How many meshes actually reach a backend, counted through the null one before the GPU is
    // involved at all — the same number `culling_reduces_draw_calls` measures.
    app.world
        .insert_service(Renderer::new(Box::new(amadeo_render::NullBackend::new(
            640, 360,
        ))));
    render_quads(&mut app.world);
    let submitted: usize = app
        .world
        .service::<Renderer>()
        .and_then(Renderer::null_backend)
        .and_then(|backend| backend.last_frame())
        .map_or(0, |frame| {
            frame.views.iter().map(|view| view.meshes.len()).sum()
        });

    // 640x360 rather than a full window: this is measuring geometry and draw submission, and a
    // bigger target measures the fill rate of whatever GPU the runner has.
    let Ok(mut backend) = WgpuBackend::offscreen(640, 360) else {
        eprintln!("no GPU adapter here; skipping the frame budget measurement");
        return;
    };
    if !backend.supports_gpu_timing() {
        eprintln!("this adapter does not advertise TIMESTAMP_QUERY; skipping");
        return;
    }
    backend.set_gpu_timing(true);
    app.world.insert_service(Renderer::new(Box::new(backend)));

    // Two frames: the first uploads every mesh and texture, so its timing is about loading rather
    // than about drawing. Everything after it is a steady-state frame, which is the number the
    // budget is about.
    render_quads(&mut app.world);
    render_quads(&mut app.world);

    let renderer = app.world.service::<Renderer>().expect("installed");
    let backend = renderer
        .backend_as::<WgpuBackend>()
        .expect("the wgpu backend was just installed");
    let timing = backend
        .last_gpu_timing()
        .expect("timing was on for that frame")
        .clone();

    println!("--- exit gate 4: the Scarp, 640x360 ---");
    println!("meshes in world {in_world}, in view {in_view}, submitted {submitted}");
    println!("gpu total       {:?}", timing.total);
    for (label, cost) in &timing.passes {
        println!("  {label:<28} {cost:?}");
    }

    // What holds on any machine.
    assert!(
        !timing.passes.is_empty(),
        "a frame drew no timed passes at all"
    );
    assert!(
        timing.total > std::time::Duration::ZERO,
        "the GPU reported zero time for a frame that drew {submitted} meshes"
    );
    assert!(
        timing.total < std::time::Duration::from_millis(500),
        "a frame took {:?} on the GPU, which is not a measurement, it is a hang",
        timing.total
    );
    // The whole frame cannot be shorter than any one pass inside it.
    for (label, cost) in &timing.passes {
        assert!(
            *cost <= timing.total,
            "pass {label} reports {cost:?}, longer than the whole frame's {:?}",
            timing.total
        );
    }
}
