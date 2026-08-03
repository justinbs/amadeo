//! The acceptance test `docs/05-roadmap.md` set for snapshots, run rather than assumed.
//!
//! > Acceptance test: restoring to tick N beats re-simulating to tick N at N = 18 000.
//!
//! 18,000 ticks is five simulated minutes at 60 Hz, which is the figure ADR 0011's spike used when
//! it found that **re-simulation, not compilation**, is what degrades the agent's edit→observe loop.
//! The whole point of the feature is that this test passes; if it did not, snapshots would be
//! machinery that costs more than it saves.
//!
//! # The measured answer
//!
//! Both profiles, on the machine in `STATUS.md` § Environment:
//!
//! | | re-simulate 18,000 ticks | restore | |
//! |---|---|---|---|
//! | **debug** | 37.98 ms | 1.74 ms | **22× faster** |
//! | release | 1.86 ms | 0.32 ms | 6× faster |
//!
//! **Debug is the number that matters**, because `amadeo` launches a game with `cargo run` and no
//! `--release` (ADR 0016) — so the agent's loop is a debug loop.
//!
//! **Both are a floor rather than a typical figure.** The `step` below is a position add; ADR 0011's
//! benchmark tick was a three-state enemy AI and cost 4.6 µs against this one's ~2. A real game's
//! tick does far more work per entity, and every bit of it widens the gap, because restoring is a
//! function of *world size* while re-simulating is a function of world size times ticks times how
//! much each tick does.
//!
//! # What is asserted, and what is only printed
//!
//! Following `sprite_throughput.rs`: **the comparison is asserted, the absolute times are not.**
//! CI runners are shared and variable, and `CLAUDE.md` §6 forbids tests that depend on wall-clock
//! timing. But "restoring is faster than re-simulating" is a ratio, and it is not a close call — so
//! the assertion is a deliberately generous one that a real regression would still trip.
//!
//! Run with `--nocapture` to see the numbers.
//!
//! ```text
//! cargo test -p amadeo-snapshot --test restore_beats_resimulation -- --nocapture
//! ```

use amadeo_core::StableHash;
use amadeo_ecs::{Component, ComponentRegistry, World};
use amadeo_reflect::Reflect;
use amadeo_snapshot::{capture, restore};
use std::time::Instant;

/// Five simulated minutes at 60 Hz — the figure the roadmap named.
const TICKS: u64 = 18_000;

/// How many entities the world carries. Small on purpose: this measures the *loop*, and ADR 0011's
/// benchmark was 64 entities.
const ENTITIES: usize = 64;

/// Somewhere to be.
#[derive(Debug, Clone, Copy, PartialEq, StableHash, Reflect)]
struct Position {
    /// Across.
    x: f32,
    /// Up.
    y: f32,
}
impl Component for Position {}

/// How fast, and in which direction.
#[derive(Debug, Clone, Copy, PartialEq, StableHash, Reflect)]
struct Velocity {
    /// Across, per tick.
    x: f32,
    /// Up, per tick.
    y: f32,
}
impl Component for Velocity {}

fn registry() -> ComponentRegistry {
    let mut registry = ComponentRegistry::new();
    registry.register::<Position>().expect("registers");
    registry.register::<Velocity>().expect("registers");
    registry
}

fn starting_world() -> World {
    let mut world = World::new();
    for index in 0..ENTITIES {
        let entity = world.spawn();
        world.insert(
            entity,
            Position {
                x: index as f32,
                y: 0.0,
            },
        );
        world.insert(
            entity,
            Velocity {
                x: 0.01,
                y: (index % 7) as f32 * 0.01,
            },
        );
    }
    world
}

/// One tick of a plausible simulation: move everything, then wrap it.
fn step(world: &mut World) {
    world.for_each_pair_mut::<Position, Velocity>(|_entity, position, velocity| {
        position.x += velocity.x;
        position.y += velocity.y;
        if position.x > 100.0 {
            position.x -= 200.0;
        }
    });
    world.advance_tick();
}

#[test]
fn restoring_beats_re_simulating_at_five_simulated_minutes() {
    let registry = registry();

    // --- The cost being replaced: getting to tick 18,000 the only way there used to be. ---
    let mut world = starting_world();
    let simulate_start = Instant::now();
    for _ in 0..TICKS {
        step(&mut world);
    }
    let simulating = simulate_start.elapsed();

    let text = amadeo_snapshot::to_text(&capture(&world, &registry));

    // --- The cost that replaces it: parse a file and rebuild a world. ---
    let restore_start = Instant::now();
    let document = amadeo_snapshot::parse(&text).expect("parses");
    let mut restored = World::new();
    restore(&mut restored, &registry, &document).expect("restores");
    let restoring = restore_start.elapsed();

    // Correctness first: a faster wrong answer is not an answer.
    assert_eq!(restored.state_hash(), world.state_hash());
    assert_eq!(restored.tick(), world.tick());

    let ratio = simulating.as_secs_f64() / restoring.as_secs_f64();
    println!("\n  restoring to tick {TICKS} versus re-simulating to it, {ENTITIES} entities\n");
    println!("    re-simulate  {:>9.2?}", simulating);
    println!(
        "    restore      {:>9.2?}   ({ratio:.0}x faster)",
        restoring
    );
    println!("    snapshot     {:>9} bytes\n", text.len());

    // The roadmap's acceptance test. Deliberately generous -- the measured ratio is far larger, and
    // a threshold set near it would fail on a loaded CI runner for no useful reason. What this
    // catches is the feature ceasing to be worth having.
    assert!(
        restoring < simulating,
        "restoring took {restoring:?} and re-simulating took {simulating:?}; \
         snapshots exist to be the cheaper of the two"
    );
}

#[test]
fn a_restored_world_at_five_minutes_carries_on_identically() {
    // Speed is only interesting if the world is right. The same running-on check the other test file
    // makes, at the scale the roadmap actually named.
    let registry = registry();

    let mut world = starting_world();
    for _ in 0..TICKS {
        step(&mut world);
    }

    let text = amadeo_snapshot::to_text(&capture(&world, &registry));
    let document = amadeo_snapshot::parse(&text).expect("parses");
    let mut restored = World::new();
    restore(&mut restored, &registry, &document).expect("restores");

    for _ in 0..500 {
        step(&mut world);
        step(&mut restored);
    }

    assert_eq!(restored.state_hash(), world.state_hash());
}
