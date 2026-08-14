//! **M2's exit gate 4**: frame time within a declared budget, at a declared scene complexity.
//!
//! ```text
//! cargo test -p atrium --test frame_budget -- --nocapture
//! ```
//!
//! # What is asserted, and what is only reported
//!
//! The same split `crates/amadeo-render/tests/sprite_throughput.rs` established, and for the same
//! reasons — `CLAUDE.md` §6 forbids tests that depend on wall-clock, and CI runners are shared and
//! variable:
//!
//! - **Scene complexity is asserted.** How many meshes, bodies and shadow casters are in the room is
//!   a pure function of the scene file, with no clock involved. A budget quoted against a complexity
//!   nobody checked is a number about an unknown scene, so this is the half that makes the numbers
//!   mean something.
//! - **Times are printed, plus one deliberately enormous ceiling.** The ceiling catches an
//!   algorithmic collapse — an accidental O(n²), a solver that stopped sleeping — and nothing
//!   subtler. A tight timing assertion would be flaky, and a flaky performance gate is one people
//!   learn to ignore, which is worse than not having one.
//!
//! The recorded numbers live in `docs/10-frame-budget.md`. This test is how they are regenerated.

use amadeo_app::{App, Profiler};
use amadeo_character::CharacterController;
use amadeo_physics::{Collider, RigidBody};
use amadeo_render::{DirectionalLight, Mesh, ShadowMode};

/// One 60 Hz frame. The whole budget, for everything — simulation, rendering and presentation — so
/// the *simulation* wanting a fraction of it is the actual bar.
const FRAME_BUDGET_US: f64 = 16_666.7;

/// The share of a frame this test will let the simulation take before failing.
///
/// **Deliberately enormous.** Simulation is one part of a frame and rendering is usually the larger
/// one, so a healthy number here is a few percent. Half a frame is not a budget — it is a tripwire
/// for something having gone structurally wrong, which is the only timing claim a shared CI runner
/// can support.
const CEILING_FRACTION: f64 = 0.5;

/// Ticks run before measuring, to get past cold caches, lazily allocated storage and rapier building
/// its broad phase for the first time.
const WARM_UP: u64 = 120;

/// Ticks measured.
const MEASURED: u64 = 600;

fn room() -> App {
    atrium::build_headless().expect("the room builds")
}

#[test]
fn the_scene_complexity_the_budget_is_quoted_against() {
    // **Asserted, because a budget without a complexity is a number about an unknown scene.** If
    // someone halves the room and the frame time halves, this is what says the comparison moved.
    let app = room();

    let meshes = app.world.query::<(&Mesh,)>().count();
    let bodies = app.world.query::<(&RigidBody, &Collider)>().count();
    let characters = app.world.query::<(&CharacterController,)>().count();
    let casters = app
        .world
        .query::<(&DirectionalLight,)>()
        .filter(|(_, (light,))| light.shadows != ShadowMode::Off)
        .count();

    // Twelve since the watcher joined (ADR 0068). It has a mesh and no collider — it walks through
    // the pillars, which is a real limitation of moving it by writing a `Transform` rather than
    // through `move_shape`, and is noted in `move_the_watcher` rather than hidden here.
    //
    // **Thirteen since the brass key** (ADR 0070), which has both a mesh and a collider. Worth
    // knowing that this number *falls back to twelve once somebody picks the key up*: storing an
    // item removes its `Transform`, and every pass that draws or simulates a thing requires one.
    // That is the module's whole mechanism showing up in a measurement taken for another reason.
    assert_eq!(meshes, 13, "drawn meshes");
    assert_eq!(bodies, 12, "bodies with a collider");
    assert_eq!(characters, 1, "characters");
    assert_eq!(casters, 1, "shadow-casting lights");
}

#[test]
fn one_simulation_tick_fits_well_inside_a_frame() {
    let mut app = room();

    // Warm up, then throw the warm-up away: the first ticks pay for storage that is allocated on
    // first use and for rapier building its broad phase, and leaving those in the average would
    // report a number the game never actually runs at.
    app.run_ticks(WARM_UP).expect("warm-up runs");
    if let Some(profiler) = app.world.service_mut::<Profiler>() {
        profiler.reset();
    }

    app.run_ticks(MEASURED).expect("measured ticks run");

    let profiler = app.world.service::<Profiler>().expect("installed by App");
    let mean = profiler.mean_tick().as_secs_f64() * 1e6;

    println!("\n--- games/atrium, {MEASURED} ticks after {WARM_UP} warm-up ---");
    println!("{}", profiler.report());
    println!(
        "simulation is {:.2}% of a 60 Hz frame ({FRAME_BUDGET_US:.0} µs)\n",
        (mean / FRAME_BUDGET_US) * 100.0
    );

    assert_eq!(profiler.ticks(), MEASURED);
    assert!(
        mean < FRAME_BUDGET_US * CEILING_FRACTION,
        "one simulation tick took {mean:.1} µs, past the {:.0} µs tripwire. This is not a tuning \
         threshold — it is set at half a frame to catch something structural, so hitting it means \
         a system has changed complexity class rather than got slower",
        FRAME_BUDGET_US * CEILING_FRACTION
    );
}

#[test]
fn every_system_is_accounted_for() {
    // The profiler is only useful if it sees the whole tick. A system that never reported would make
    // the total quietly optimistic, and the number this gate writes down would be about a subset of
    // the frame — which is the failure mode that makes a budget worthless rather than wrong.
    let mut app = room();
    app.run_ticks(10).expect("runs");

    let profiler = app.world.service::<Profiler>().expect("installed by App");
    for label in [
        amadeo_input::SAMPLE_INPUT,
        amadeo_physics::STEP_PHYSICS,
        amadeo_character::DRIVE_CHARACTERS,
        amadeo_transform::PROPAGATE_TRANSFORMS,
    ] {
        let timing = profiler
            .system(label)
            .unwrap_or_else(|| panic!("`{label}` ran but was not measured"));
        assert_eq!(timing.runs, 10, "`{label}` should have run once per tick");
    }
}

#[test]
fn profiling_does_not_move_the_state_hash() {
    // **The claim ADR 0040 rests on**, checked rather than argued: the profiler reads a wall clock
    // inside the tick, and that is only safe because it is a `Service` and ADR 0009 keeps services
    // structurally out of the hash.
    //
    // Two worlds, one of them profiled and one with the profiler removed, must agree exactly — and
    // must keep agreeing, which is why this runs them on rather than comparing a single hash.
    let hash_after = |profiled: bool| {
        let mut app = room();
        if !profiled {
            app.world.remove_service::<Profiler>();
        }
        app.run_ticks(180).expect("runs");
        app.world.state_hash()
    };

    assert_eq!(
        hash_after(true),
        hash_after(false),
        "timing must not be able to reach the state hash"
    );
}

/// What the CPU spends preparing and submitting one frame, with a real GPU device.
///
/// # This is not "GPU frame time", and the difference matters
///
/// It measures the engine's own work: walking the world into a `FrameData`, compiling the render
/// graph, uploading what changed, and recording command buffers. **How long the GPU then takes to
/// execute those commands is not measured here** — that needs timestamp queries, which the backend
/// does not have.
///
/// So this is a real number about a real half of the frame, and it is explicitly half. Saying which
/// half is the difference between a budget and a reassuring figure.
#[test]
fn preparing_a_frame_on_the_cpu_fits_inside_one_too() {
    use amadeo_render::{Renderer, WgpuBackend, render_quads};

    let mut app = room();
    let backend = match WgpuBackend::offscreen(1280, 720) {
        Ok(backend) => backend,
        Err(error) => {
            // A missing GPU is a fact about the machine, not about the engine — the same posture
            // `capture.rs` takes, so this can never be a flaky failure, only a quiet one.
            println!("skipping: no offscreen device on this machine ({error})");
            return;
        }
    };
    // **Whether this number means anything depends on what answered.** `offscreen` asks for an
    // adapter with no compatible surface, which is what lets a *software* rasteriser answer on a
    // machine with no GPU — and that is how CI captures images at all. But it is dozens of times
    // slower than hardware, not slightly, so a time measured on one says nothing about the game.
    let on_real_hardware = !backend.adapter().software;
    let adapter = backend.adapter().name.clone();
    app.world.insert_service(Renderer::new(Box::new(backend)));

    app.run_ticks(WARM_UP).expect("warm-up runs");
    for _ in 0..30 {
        render_quads(&mut app.world);
    }

    const FRAMES: u32 = 200;
    let started = std::time::Instant::now();
    for _ in 0..FRAMES {
        render_quads(&mut app.world);
    }
    let mean = started.elapsed().as_secs_f64() * 1e6 / f64::from(FRAMES);

    println!(
        "\n--- games/atrium, CPU-side frame preparation, 1280x720, {FRAMES} frames ---\n\
         adapter                             {adapter}{}\n\
         collect + graph + upload + submit   {mean:.3}µs   ({:.2}% of a 60 Hz frame)\n\
         GPU execution time is NOT included -- that needs timestamp queries.\n",
        if on_real_hardware {
            ""
        } else {
            "   (SOFTWARE -- timings below are not meaningful)"
        },
        (mean / FRAME_BUDGET_US) * 100.0
    );

    // The tripwire only applies where the measurement means something. This is the same posture the
    // missing-device branch above already takes: a fact about the machine, reported rather than
    // failed. `docs/10-frame-budget.md`'s numbers are regenerated on real hardware, and that is the
    // only place a budget claim can honestly be made.
    if !on_real_hardware {
        println!(
            "not asserting the {:.0} µs tripwire: a software adapter is dozens of times slower \
             than hardware, so this number measures the runner rather than the engine.",
            FRAME_BUDGET_US * CEILING_FRACTION
        );
        return;
    }

    assert!(
        mean < FRAME_BUDGET_US * CEILING_FRACTION,
        "preparing a frame took {mean:.1} µs on {adapter}, past the {:.0} µs tripwire",
        FRAME_BUDGET_US * CEILING_FRACTION
    );
}

/// How the simulation cost grows with the number of dynamic bodies.
///
/// # Why a scaling curve rather than one number
///
/// The Atrium is eleven bodies, which is a *room* rather than a stress test. A budget quoted only
/// against it would answer "does the demo run" — which was never in doubt — instead of "what can
/// this engine carry", which is what a budget is for.
///
/// It also turns the ceiling into a real check. A single measurement cannot tell a slow constant
/// from a bad complexity class; four measurements an order of magnitude apart can, and gate 3
/// already establishes 200 bodies as the number this engine cares about.
#[test]
fn simulation_cost_against_body_count() {
    use amadeo_physics::{RigidBody, Velocity};
    use amadeo_transform::Transform;

    println!("\n--- games/atrium plus N falling bodies, {MEASURED} ticks ---");
    println!("{:>8} {:>14} {:>12}", "bodies", "mean/tick", "% of frame");

    let mut previous: Option<(usize, f64)> = None;
    for extra in [0_usize, 50, 200, 800] {
        let mut app = room();
        for index in 0..extra {
            let entity = app.world.spawn();
            // Spread across a grid above the floor and dropped. Deliberately *not* stacked in one
            // column: a column settles into one contact island and measures the sleeping path,
            // which is the easy case rather than the representative one.
            let across = (index % 20) as f32 * 0.9 - 9.0;
            let along = ((index / 20) % 20) as f32 * 0.9 - 9.0;
            let height = 3.0 + (index / 400) as f32 * 1.5;
            app.world
                .insert(entity, Transform::at_xyz(across, height, along));
            app.world.insert(entity, RigidBody::dynamic(5.0));
            app.world.insert(entity, Collider::cuboid(0.4, 0.4, 0.4));
            app.world.insert(entity, Velocity::default());
        }

        app.run_ticks(WARM_UP).expect("warm-up runs");
        if let Some(profiler) = app.world.service_mut::<Profiler>() {
            profiler.reset();
        }
        app.run_ticks(MEASURED).expect("measured ticks run");

        let profiler = app.world.service::<Profiler>().expect("installed");
        let mean = profiler.mean_tick().as_secs_f64() * 1e6;
        let bodies = 11 + extra;
        println!(
            "{bodies:>8} {mean:>13.2}µs {:>11.2}%",
            (mean / FRAME_BUDGET_US) * 100.0
        );

        // The complexity check: physics is expected to be roughly linear in body count, so a
        // sixteen-fold increase in bodies must not cost far more than sixteen times the time.
        // Generous by a factor of four, because this is looking for a changed complexity class and
        // not for a regression — a tight ratio here would be the flaky assertion this file avoids.
        if let Some((previous_bodies, previous_mean)) = previous
            && previous_mean > 1.0
        {
            let body_ratio = bodies as f64 / previous_bodies as f64;
            let time_ratio = mean / previous_mean;
            assert!(
                time_ratio < body_ratio * 4.0,
                "going from {previous_bodies} to {bodies} bodies ({body_ratio:.1}x) cost \
                 {time_ratio:.1}x the time. Physics should be roughly linear in body count; this \
                 looks like a complexity change rather than a slowdown"
            );
        }
        previous = Some((bodies, mean));
    }
    println!();
}
