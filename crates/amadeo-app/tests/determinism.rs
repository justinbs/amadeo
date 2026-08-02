//! Determinism tests for the fixed-timestep loop.
//!
//! These are the tests invariant I3 exists for, and they are the precursor to the golden replay
//! harness. Everything the project claims about verifiability — replay-as-regression-test, headless
//! verification, snapshots, reproducible bug reports — rests on the properties asserted here.
//!
//! If something in this file starts failing, do not adjust the expected values until you know why.
//! A changed state hash means the simulation behaves differently, which is either a real regression
//! or a deliberate change that invalidates every recorded replay.

use amadeo_app::{App, SimRng, Stage, system};
use amadeo_core::{FIXED_DT, FIXED_DT_NANOS, StableHash, StableHasher};
use amadeo_ecs::{Commands, Component, Service, World};
use amadeo_events::{Event, WorldEvents};
use amadeo_reflect::Reflect;

// --- Test world definitions ---

#[derive(Debug, Clone, Copy, PartialEq, StableHash, Reflect)]
struct Position {
    x: f32,
    y: f32,
}
impl Component for Position {}

#[derive(Debug, Clone, Copy, PartialEq, StableHash, Reflect)]
struct Velocity {
    x: f32,
    y: f32,
}
impl Component for Velocity {}

/// Counts how many times the render stage ran, to prove it does not touch simulation state.
///
/// A `Service`, not a `Resource`, and that distinction is exactly what this test file was written to
/// exercise. Render-side bookkeeping must not enter the state hash, or a windowed run could never
/// agree with a headless one (invariant I7). Filing it as a `Resource` makes
/// `rendering_cannot_change_simulation_state` fail — which is how this gap was found.
#[derive(Debug, Default)]
struct RenderCount(u32);

impl Service for RenderCount {}

#[derive(Debug, Clone, PartialEq, Eq, Reflect)]
struct Bounced {
    /// Which entity bounced.
    entity_index: u32,
}

impl StableHash for Bounced {
    fn stable_hash(&self, hasher: &mut StableHasher) {
        hasher.write_u32(self.entity_index);
    }
}
impl Event for Bounced {}

// --- Systems ---

/// Moves everything by its velocity, using the fixed timestep rather than a measured delta.
fn integrate(world: &mut World) {
    world.for_each_pair_mut::<Position, Velocity>(|_entity, position, velocity| {
        position.x += velocity.x * FIXED_DT;
        position.y += velocity.y * FIXED_DT;
    });
}

/// Reflects anything that leaves a 10-unit box, and announces it.
fn bounce(world: &mut World) {
    const LIMIT: f32 = 10.0;

    // Collect first, then send: `for_each_pair_mut` holds a borrow of the world, so events cannot be
    // sent from inside the closure. Collecting is the straightforward way around it.
    let mut bounced = Vec::new();
    world.for_each_pair_mut::<Velocity, Position>(|entity, velocity, position| {
        if position.x.abs() > LIMIT {
            velocity.x = -velocity.x;
            bounced.push(entity.index());
        }
        if position.y.abs() > LIMIT {
            velocity.y = -velocity.y;
            bounced.push(entity.index());
        }
    });

    for entity_index in bounced {
        world.send_event(Bounced { entity_index });
    }
}

/// Applies a small random nudge, drawing from the simulation RNG.
fn jitter(world: &mut World) {
    world.with_resource_taken::<SimRng, ()>(|world, rng| {
        world.for_each_mut::<Velocity>(|_entity, velocity| {
            velocity.x += rng.0.range_f32(-0.01, 0.01);
        });
    });
}

/// A render-stage system. Writes only to a render-side service, never to simulation state.
fn count_renders(world: &mut World) {
    if let Some(count) = world.service_mut::<RenderCount>() {
        count.0 += 1;
    }
}

// --- Fixtures ---

/// Builds an app with a known starting state. Deliberately not alphabetical in registration order,
/// to prove that does not matter.
fn build_app(seed: u64) -> App {
    let mut app = App::with_seed(seed);
    app.register_event::<Bounced>();
    app.insert_service(RenderCount::default());

    app.add_system(Stage::Simulation, system("jitter", jitter).after("bounce"));
    app.add_system(Stage::Simulation, system("integrate", integrate));
    app.add_system(
        Stage::Simulation,
        system("bounce", jitter_free_bounce()).after("integrate"),
    );
    app.add_system(Stage::Render, system("count_renders", count_renders));

    for i in 0..12u32 {
        let entity = app.world.spawn();
        app.world.insert(
            entity,
            Position {
                x: i as f32 - 6.0,
                y: (i % 5) as f32,
            },
        );
        app.world.insert(
            entity,
            Velocity {
                x: if i % 2 == 0 { 30.0 } else { -30.0 },
                y: 15.0,
            },
        );
    }
    app
}

/// Wrapper so `bounce` can be registered under a label that differs from its function name.
fn jitter_free_bounce() -> impl FnMut(&mut World) {
    bounce
}

// --- The tests ---

#[test]
fn identical_runs_produce_identical_state() {
    let mut first = build_app(1234);
    let mut second = build_app(1234);

    first.run_ticks(600).expect("schedule resolves");
    second.run_ticks(600).expect("schedule resolves");

    assert_eq!(
        first.state_hash(),
        second.state_hash(),
        "two identical runs diverged, which means something in the simulation is not deterministic"
    );
}

#[test]
fn state_matches_at_every_checkpoint() {
    // Checking only the final state would hide a divergence that happens to reconverge. Golden
    // replays therefore assert at checkpoints, and so does this.
    let mut first = build_app(99);
    let mut second = build_app(99);

    for checkpoint in [1u64, 10, 100, 300, 600] {
        let target = checkpoint - first.tick().0;
        first.run_ticks(target).expect("schedule resolves");
        second.run_ticks(target).expect("schedule resolves");

        assert_eq!(first.tick().0, checkpoint);
        assert_eq!(
            first.state_hash(),
            second.state_hash(),
            "diverged by tick {checkpoint}"
        );
    }
}

#[test]
fn different_seeds_diverge() {
    // If they agreed, the RNG would not be feeding into simulation at all and the test above would
    // be vacuous.
    let mut first = build_app(1);
    let mut second = build_app(2);

    first.run_ticks(120).expect("schedule resolves");
    second.run_ticks(120).expect("schedule resolves");

    assert_ne!(first.state_hash(), second.state_hash());
}

#[test]
fn rendering_cannot_change_simulation_state() {
    // Invariant I7: a headless run and a windowed run must agree exactly. The windowed one renders.
    let mut headless = build_app(7);
    let mut windowed = build_app(7);

    for _ in 0..120 {
        headless.step().expect("schedule resolves");

        windowed.step().expect("schedule resolves");
        windowed.render().expect("schedule resolves");
    }

    assert_eq!(
        windowed.world.service::<RenderCount>().expect("present").0,
        120,
        "the render stage should have run"
    );
    assert_eq!(
        headless.state_hash(),
        windowed.state_hash(),
        "rendering altered simulation state"
    );
}

#[test]
fn real_time_stepping_matches_exact_tick_stepping() {
    // The two entry points must agree, or a bug would reproduce in one mode and not the other.
    let mut by_ticks = build_app(42);
    let mut by_time = build_app(42);

    by_ticks.run_ticks(60).expect("schedule resolves");

    // Feed exactly 60 ticks' worth of nanoseconds, a frame at a time.
    let mut total_ticks = 0;
    for _ in 0..60 {
        total_ticks += by_time
            .advance_real_time(FIXED_DT_NANOS)
            .expect("schedule resolves");
    }

    assert_eq!(total_ticks, 60);
    assert_eq!(by_ticks.tick(), by_time.tick());
    assert_eq!(by_ticks.state_hash(), by_time.state_hash());
}

#[test]
fn partial_frames_accumulate_instead_of_being_lost() {
    let mut app = build_app(3);

    // A third of a tick, three times over, should produce exactly one tick.
    assert_eq!(
        app.advance_real_time(FIXED_DT_NANOS / 3).expect("resolves"),
        0
    );
    assert_eq!(
        app.advance_real_time(FIXED_DT_NANOS / 3).expect("resolves"),
        0
    );
    let ran = app
        .advance_real_time(FIXED_DT_NANOS / 3 + 2)
        .expect("resolves");
    assert_eq!(ran, 1);
    assert_eq!(app.tick().0, 1);
}

#[test]
fn a_long_stall_does_not_cause_a_catch_up_spiral() {
    let mut app = build_app(3);

    // Ten seconds of stalled real time. Without a cap this would run 600 ticks in one frame.
    let ran = app
        .advance_real_time(FIXED_DT_NANOS * 600)
        .expect("resolves");

    assert!(
        ran <= 8,
        "ran {ran} ticks in a single frame; the per-frame cap is not holding"
    );

    // The backlog must be discarded, not carried, or the next frame runs the cap again forever.
    let next = app.advance_real_time(0).expect("resolves");
    assert_eq!(next, 0, "backlog survived the cap and will spiral");
}

#[test]
fn render_interpolation_stays_in_range() {
    let mut app = build_app(3);
    for fraction in [0u64, 1, 2, 3, 7] {
        app.advance_real_time(FIXED_DT_NANOS / 10 * fraction)
            .expect("resolves");
        let alpha = app.render_interpolation();
        assert!((0.0..1.0).contains(&alpha), "interpolation was {alpha}");
    }
}

#[test]
fn events_become_readable_one_tick_after_being_sent() {
    let mut app = build_app(5);

    // Run until a bounce happens, then confirm the event surfaced.
    let mut saw_event = false;
    for _ in 0..200 {
        app.step().expect("schedule resolves");
        if !app.world.read_events::<Bounced>().is_empty() {
            saw_event = true;
            break;
        }
    }
    assert!(
        saw_event,
        "expected at least one bounce in 200 ticks; the fixture is not exercising events"
    );
}

#[test]
fn event_traffic_is_part_of_the_determinism_guarantee() {
    let mut first = build_app(11);
    let mut second = build_app(11);

    first.run_ticks(200).expect("schedule resolves");
    second.run_ticks(200).expect("schedule resolves");

    let first_events = first.read_bounced_count();
    let second_events = second.read_bounced_count();
    assert_eq!(first_events, second_events);
    assert_eq!(first.state_hash(), second.state_hash());
}

/// Small helper so the test above reads clearly.
trait BouncedCount {
    fn read_bounced_count(&self) -> usize;
}

impl BouncedCount for App {
    fn read_bounced_count(&self) -> usize {
        self.world.read_events::<Bounced>().len()
    }
}

#[test]
fn system_order_is_independent_of_registration_order() {
    let mut app = build_app(1);
    let order = app
        .resolved_order(Stage::Simulation)
        .expect("schedule resolves");

    // Constraints say integrate -> bounce -> jitter, despite registration being jitter first.
    assert_eq!(order, vec!["integrate", "bounce", "jitter"]);
}

#[test]
fn registered_events_are_reported() {
    let app = build_app(1);
    let events = app.registered_events();
    assert_eq!(events.len(), 1);
    assert!(events[0].contains("Bounced"), "{events:?}");
}

#[test]
fn rng_state_is_part_of_the_state_hash() {
    // Two worlds identical except for how many random values have been drawn have diverged, and
    // that must be visible -- otherwise a replay could silently desync.
    let mut untouched = App::with_seed(5);
    let mut drawn = App::with_seed(5);
    drawn
        .world
        .resource_mut::<SimRng>()
        .expect("seeded")
        .0
        .next_u32();

    assert_ne!(untouched.state_hash(), drawn.state_hash());

    // And advancing both identically keeps them in step.
    untouched
        .world
        .resource_mut::<SimRng>()
        .expect("seeded")
        .0
        .next_u32();
    assert_eq!(untouched.state_hash(), drawn.state_hash());
}

#[test]
fn tick_count_advances_exactly_once_per_step() {
    let mut app = build_app(1);
    assert_eq!(app.tick().0, 0);
    app.step().expect("resolves");
    assert_eq!(app.tick().0, 1);
    app.run_ticks(10).expect("resolves");
    assert_eq!(app.tick().0, 11);
}

// --- Deferred commands inside the loop ---
//
// Spawning and despawning churns entity slots, archetype rows, and the free list. All three feed
// into the state hash, so if command ordering or slot reuse were nondeterministic these tests would
// catch it. Doing this in the running loop rather than in isolation is the point: the flush happens
// once per stage, and a per-stage flush is exactly where an ordering mistake would hide.

/// Counts down, and despawns itself at zero.
#[derive(Debug, Clone, Copy, PartialEq, StableHash, Reflect)]
struct Lifetime(i32);
impl Component for Lifetime {}

/// Ages every entity and queues a despawn for the expired ones.
fn expire(world: &mut World) {
    world.with_service_taken::<Commands, ()>(|world, commands| {
        world.for_each_mut::<Lifetime>(|entity, lifetime| {
            lifetime.0 -= 1;
            if lifetime.0 <= 0 {
                commands.despawn(entity);
            }
        });
    });
}

/// Spawns entities periodically, with a lifetime and position derived from the tick.
///
/// Two per spawn tick, and only one of them gets a `Position`, so entities land in two different
/// archetypes — that is what makes the churn exercise archetype migration rather than one flat table.
fn spawn_periodically(world: &mut World) {
    let tick = world.tick();
    if !tick.is_multiple_of(7) {
        return;
    }
    let lifetime = 5 + (tick.0 % 11) as i32;
    let x = tick.0 as f32;

    world.with_service_taken::<Commands, ()>(|_world, commands| {
        commands.spawn_with(move |world, entity| {
            world.insert(entity, Lifetime(lifetime));
        });
        commands.spawn_with(move |world, entity| {
            world.insert(entity, Lifetime(lifetime + 2));
            world.insert(entity, Position { x, y: 0.0 });
        });
    });
}

fn build_churn_app(seed: u64) -> App {
    let mut app = App::with_seed(seed);
    app.add_system(Stage::Simulation, system("expire", expire));
    app.add_system(
        Stage::Simulation,
        system("spawn_periodically", spawn_periodically).after("expire"),
    );
    for i in 0..4i32 {
        let entity = app.world.spawn();
        app.world.insert(entity, Lifetime(3 + i * 2));
    }
    app
}

#[test]
fn spawn_and_despawn_churn_stays_deterministic() {
    let mut first = build_churn_app(5);
    let mut second = build_churn_app(5);

    first.run_ticks(300).expect("schedule resolves");
    second.run_ticks(300).expect("schedule resolves");

    assert_eq!(
        first.state_hash(),
        second.state_hash(),
        "entity slot reuse or command ordering is not deterministic"
    );
}

#[test]
fn churn_agrees_at_every_checkpoint() {
    let mut first = build_churn_app(9);
    let mut second = build_churn_app(9);

    for _ in 0..30 {
        first.run_ticks(10).expect("resolves");
        second.run_ticks(10).expect("resolves");
        assert_eq!(
            first.state_hash(),
            second.state_hash(),
            "diverged by tick {}",
            first.tick().0
        );
    }
}

#[test]
fn the_churn_fixture_actually_churns() {
    // Guards against the two tests above passing because nothing is ever spawned or removed.
    let mut app = build_churn_app(5);
    let start = app.world.entity_count();

    app.run_ticks(100).expect("resolves");
    let after = app.world.entity_count();

    assert!(
        app.world.archetype_count() >= 2,
        "expected archetype churn, saw {} archetypes",
        app.world.archetype_count()
    );
    assert!(
        after != start || after > 0,
        "expected the entity population to move: {start} -> {after}"
    );
}
