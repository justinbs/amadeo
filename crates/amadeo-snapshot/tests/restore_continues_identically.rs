//! A restored world must not merely *look* right — it must carry on identically.
//!
//! # Why this file exists, and why hash comparison is not enough
//!
//! `World::state_hash` deliberately excludes the entity allocator's free list. That is correct — the
//! free list is bookkeeping, not simulation state — but it means **two worlds can hash identically
//! and then hand out different entity handles on the very next `spawn`**: one has a slot to reuse,
//! the other does not. Since an entity's index and generation are both hashed, those two worlds
//! diverge a few ticks later, and the divergence looks like a simulation bug rather than a restore
//! bug.
//!
//! So the tests here are built around one shape:
//!
//! 1. run a world forward, snapshot it mid-run;
//! 2. keep running the original;
//! 3. restore the snapshot into a fresh world and run it the *same* number of ticks;
//! 4. require the two to agree at the end.
//!
//! Step 4 is what a hash comparison at step 3 cannot tell you. `a_restored_world_spawns_the_same
//! _handles` is the sharpest case: delete its `free_slots` line and every other assertion in this
//! file still passes.

use amadeo_core::StableHash;
use amadeo_ecs::{Component, ComponentRegistry, World};
use amadeo_reflect::Reflect;
use amadeo_snapshot::{capture, restore};

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

/// A counter, so a resource takes part too.
#[derive(Debug, Clone, Copy, PartialEq, StableHash, Reflect)]
struct Score {
    /// Points so far.
    points: u32,
}
impl amadeo_ecs::Resource for Score {}

fn registry() -> ComponentRegistry {
    let mut registry = ComponentRegistry::new();
    registry.register::<Position>().expect("registers");
    registry.register::<Velocity>().expect("registers");
    registry
}

/// A world with a few moving entities and a resource.
fn starting_world() -> World {
    let mut world = World::new();
    world.insert_resource(Score { points: 0 });

    for index in 0..5 {
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
                x: 1.0,
                y: index as f32,
            },
        );
    }
    world
}

/// One tick: move everything, and bump the score.
fn step(world: &mut World) {
    world.for_each_pair_mut::<Position, Velocity>(|_entity, position, velocity| {
        position.x += velocity.x;
        position.y += velocity.y;
    });
    if let Some(score) = world.resource_mut::<Score>() {
        score.points += 1;
    }
    world.advance_tick();
}

/// Restores a snapshot into a world set up the way a game's own startup would leave it.
///
/// Resources are inserted at their defaults first, because that is what registers how to rebuild
/// them — a snapshot overwrites a resource, it does not invent one.
fn restored_from(text: &str) -> World {
    let document = amadeo_snapshot::parse(text).expect("the snapshot parses");
    let mut world = World::new();
    world.insert_resource(Score { points: 0 });
    restore(&mut world, &registry(), &document).expect("the snapshot restores");
    world
}

#[test]
fn a_restored_world_matches_the_one_it_came_from() {
    let mut world = starting_world();
    for _ in 0..10 {
        step(&mut world);
    }

    let text = amadeo_snapshot::to_text(&capture(&world, &registry()));
    let restored = restored_from(&text);

    assert_eq!(restored.state_hash(), world.state_hash());
    assert_eq!(restored.tick(), world.tick());
}

#[test]
fn a_restored_world_carries_on_identically() {
    // The property that matters. Matching at the moment of restore is necessary and not sufficient.
    let mut world = starting_world();
    for _ in 0..10 {
        step(&mut world);
    }

    let text = amadeo_snapshot::to_text(&capture(&world, &registry()));
    let mut restored = restored_from(&text);

    for _ in 0..50 {
        step(&mut world);
        step(&mut restored);
    }

    assert_eq!(restored.state_hash(), world.state_hash());
}

#[test]
fn a_restored_world_spawns_the_same_handles() {
    // **The sharpest test here.** Despawning leaves slots on the allocator's free stack, and the
    // free stack is excluded from the state hash — so a snapshot that dropped it would restore a
    // world that hashed identically and then handed out different entity handles on the next spawn.
    //
    // Removing `free_slots` from the format breaks this test and nothing else in the file.
    let mut world = starting_world();
    let entities = world.entities();
    // Two holes, in a specific order, so the stack has something to say.
    world.despawn(entities[3]);
    world.despawn(entities[1]);

    let text = amadeo_snapshot::to_text(&capture(&world, &registry()));
    let mut restored = restored_from(&text);

    assert_eq!(restored.state_hash(), world.state_hash(), "before spawning");

    // Now spawn on both sides. The handles must match, one for one.
    let original: Vec<_> = (0..4).map(|_| world.spawn()).collect();
    let replayed: Vec<_> = (0..4).map(|_| restored.spawn()).collect();

    assert_eq!(
        replayed, original,
        "a restored allocator must reuse slots in the same order"
    );
    assert_eq!(restored.state_hash(), world.state_hash(), "after spawning");
}

#[test]
fn a_generation_survives_the_round_trip() {
    // A reused slot carries a higher generation, and both halves of a handle are hashed. Restoring
    // the index and forgetting the generation would produce a world that is wrong in a way only the
    // hash notices.
    let mut world = starting_world();
    let entities = world.entities();
    world.despawn(entities[0]);
    let reused = world.spawn();
    assert_eq!(reused.generation(), 1, "the slot was reused");

    let text = amadeo_snapshot::to_text(&capture(&world, &registry()));
    let restored = restored_from(&text);

    assert!(
        restored
            .entities()
            .iter()
            .any(|entity| entity.generation() == 1),
        "the reused slot's generation came back"
    );
    assert_eq!(restored.state_hash(), world.state_hash());
}

#[test]
fn an_entity_with_no_components_survives() {
    // It is in the state hash like any other, and it lives in the empty archetype where a naive
    // capture that walked only populated archetypes would miss it.
    let mut world = World::new();
    world.spawn();
    world.spawn();

    let text = amadeo_snapshot::to_text(&capture(&world, &registry()));
    let document = amadeo_snapshot::parse(&text).expect("parses");
    assert_eq!(document.entities.len(), 2);

    let mut restored = World::new();
    restore(&mut restored, &registry(), &document).expect("restores");
    assert_eq!(restored.state_hash(), world.state_hash());
}

#[test]
fn a_resource_comes_back_with_its_value() {
    let mut world = starting_world();
    for _ in 0..7 {
        step(&mut world);
    }

    let text = amadeo_snapshot::to_text(&capture(&world, &registry()));
    let restored = restored_from(&text);

    assert_eq!(
        restored.resource::<Score>().expect("present").points,
        7,
        "the recorded value, not the default it was seeded with"
    );
}

#[test]
fn capturing_does_not_perturb_the_world() {
    // An agent taking a snapshot to look at something must not change what it is looking at.
    let mut world = starting_world();
    for _ in 0..3 {
        step(&mut world);
    }

    let before = world.state_hash();
    for _ in 0..5 {
        let _ = capture(&world, &registry());
    }
    assert_eq!(world.state_hash(), before);
}

#[test]
fn a_snapshot_is_byte_stable() {
    // Invariant I2: writing an unchanged snapshot produces an identical file. Round-tripped through
    // text, not just through the struct, because the file is what gets diffed.
    let mut world = starting_world();
    for _ in 0..4 {
        step(&mut world);
    }

    let once = amadeo_snapshot::to_text(&capture(&world, &registry()));
    let twice = amadeo_snapshot::to_text(&amadeo_snapshot::parse(&once).expect("parses"));
    assert_eq!(once, twice);
}

#[test]
fn restoring_twice_is_the_same_as_restoring_once() {
    // A restore has to be idempotent, or "go back to the checkpoint" would drift each time it is
    // used — which is exactly the workflow snapshots exist for.
    let mut world = starting_world();
    for _ in 0..6 {
        step(&mut world);
    }
    let text = amadeo_snapshot::to_text(&capture(&world, &registry()));

    let mut restored = restored_from(&text);
    let after_first = restored.state_hash();

    let document = amadeo_snapshot::parse(&text).expect("parses");
    restore(&mut restored, &registry(), &document).expect("restores again");

    assert_eq!(restored.state_hash(), after_first);
}

#[test]
fn restoring_over_a_dirty_world_replaces_it_rather_than_merging() {
    // Restoring into a world that already has entities must not leave any of them behind. A merge
    // would produce a world that is a mixture of two moments, which is the worst possible outcome:
    // plausible, and wrong.
    let mut world = starting_world();
    for _ in 0..2 {
        step(&mut world);
    }
    let text = amadeo_snapshot::to_text(&capture(&world, &registry()));

    let mut dirty = starting_world();
    for _ in 0..40 {
        step(&mut dirty);
    }
    for _ in 0..9 {
        let entity = dirty.spawn();
        dirty.insert(entity, Position { x: 99.0, y: 99.0 });
    }

    let document = amadeo_snapshot::parse(&text).expect("parses");
    restore(&mut dirty, &registry(), &document).expect("restores");

    assert_eq!(dirty.state_hash(), world.state_hash());
    assert_eq!(dirty.entities().len(), world.entities().len());
}

#[test]
fn a_snapshot_naming_an_unknown_component_says_so() {
    // The likely failure when a snapshot outlives the build that took it.
    let text = concat!(
        "amadeo-snapshot 2\n",
        "tick 0\n",
        "state-hash 0000000000000000\n",
        "schema-hash 0000000000000000\n",
        "\n",
        "entities\n",
        "  0:0\n",
        "    Nonexistent\n",
        "      x 1.0\n",
    );

    let document = amadeo_snapshot::parse(text).expect("parses");
    let error = restore(&mut World::new(), &registry(), &document).expect_err("unknown component");
    let message = error.to_string();

    assert!(message.contains("Nonexistent"), "{message}");
    assert!(message.contains("0:0"), "{message}");
    assert!(message.contains("amadeo describe"), "{message}");
}

#[test]
fn a_hash_that_does_not_match_is_refused() {
    // The format's integrity check. If a restore silently produced a slightly different world, every
    // assertion made afterwards would be measuring the wrong thing.
    let mut world = starting_world();
    step(&mut world);
    let text = amadeo_snapshot::to_text(&capture(&world, &registry()));

    // Flip one digit rather than adding one: a seventeenth hex digit would overflow `u64` and fail
    // at *parse* time, which is a different check from the one under test here.
    let hash_line = text
        .lines()
        .find(|line| line.starts_with("state-hash "))
        .expect("every snapshot has one");
    let flipped = format!(
        "state-hash {}{}",
        if hash_line.ends_with('0') { '1' } else { '0' },
        &hash_line["state-hash ".len() + 1..]
    );
    let corrupted = text.replace(hash_line, &flipped);

    let document = amadeo_snapshot::parse(&corrupted).expect("still parses");
    let error = restore(&mut World::new(), &registry(), &document).expect_err("hash mismatch");

    assert!(error.to_string().contains("Do not trust"), "{error}");
}

#[test]
fn a_slot_that_is_neither_live_nor_free_is_refused() {
    // Cannot come from a captured world; can come from a hand-edited file, which is a supported
    // thing to do. A gap would leave a slot that could never be allocated again.
    let text = concat!(
        "amadeo-snapshot 2\n",
        "tick 0\n",
        "state-hash 0000000000000000\n",
        "schema-hash 0000000000000000\n",
        "\n",
        "entities\n",
        "  0:0\n",
        "  2:0\n",
    );

    let document = amadeo_snapshot::parse(text).expect("parses");
    let error = restore(&mut World::new(), &registry(), &document).expect_err("gap");

    assert!(error.to_string().contains("do not add up"), "{error}");
}
