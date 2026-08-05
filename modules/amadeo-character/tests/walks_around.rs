//! What a character controller has to actually do: stand on a floor, be stopped by a wall, fall,
//! jump, and do all of it identically every time.
//!
//! # These tests are paired on purpose
//!
//! Every collision claim here is made twice — once against `RapierPhysics`, where it must hold, and
//! once against `NullPhysics`, where it must **fail**. ADR 0037 section 5 explains why: a backend
//! with no solver applies the requested motion unmodified, so a test that passes against it is not
//! testing collision at all.
//!
//! This project learned that the expensive way in session 9. Two test suites looked green and were
//! measuring nothing — a capture path that had silently stopped covering the present pass, and
//! collision tests that would have passed with no solver behind them. **A test is not evidence
//! until you have watched it fail.** Here, watching it fail is part of the suite rather than
//! something someone did once by hand.

use amadeo_character::{CharacterController, CharacterMotion, MOVE_RIGHT, drive_characters};
use amadeo_ecs::{Entity, World};
use amadeo_input::{ActionId, InputState};
use amadeo_physics::{
    Collider, Gravity, NullPhysics, Physics, PhysicsBackend, RigidBody, step_physics,
};
use amadeo_transform::Transform;

/// Where a resting character's centre ends up: half its total capsule height above the floor.
///
/// A capsule's total height is its straight section plus a cap at each end, so 1.2 + 2 * 0.4 = 2.0
/// and the centre rests at 1.0. Written out because getting it wrong makes every assertion here
/// off by a constant and look like a physics bug.
const RESTING_HEIGHT: f32 = 1.0;

/// How far along +X the wall's near face is.
const WALL_FACE: f32 = 2.5;

fn character_world(backend: Box<dyn PhysicsBackend>) -> (World, Entity) {
    let mut world = World::new();
    world.insert_service(Physics::new(backend));
    world.insert_resource(Gravity::earth());
    world.insert_resource(InputState::new());

    // A floor whose top surface is exactly y = 0, so RESTING_HEIGHT is the whole story.
    let floor = world.spawn();
    world.insert(floor, Transform::at_xyz(0.0, -0.5, 0.0));
    world.insert(floor, RigidBody::default());
    world.insert(floor, Collider::cuboid(40.0, 1.0, 40.0));

    // A wall spanning x = 2.5 to 3.5, tall enough that nothing steps or jumps over it.
    let wall = world.spawn();
    world.insert(wall, Transform::at_xyz(3.0, 2.0, 0.0));
    world.insert(wall, RigidBody::default());
    world.insert(wall, Collider::cuboid(1.0, 8.0, 40.0));

    // The character. Kinematic, because gameplay decides where it goes and the solver does not
    // argue -- which is exactly what ADR 0037 chose and why the move-and-slide query exists.
    let player = world.spawn();
    world.insert(player, Transform::at_xyz(0.0, RESTING_HEIGHT, 0.0));
    world.insert(player, RigidBody::kinematic());
    world.insert(player, Collider::capsule(0.4, 1.2));
    world.insert(player, CharacterController::default());
    world.insert(player, CharacterMotion::default());

    (world, player)
}

/// One tick, in the order [`amadeo_character::install`] registers: physics first, then the character.
///
/// Spelled out rather than driven through an `App` so the ordering ADR 0037 section 3 calls
/// load-bearing is visible in the test rather than buried in a schedule.
fn tick(world: &mut World) {
    step_physics(world);
    drive_characters(world);
}

/// Holds an axis for `ticks` ticks.
fn hold_axis(world: &mut World, action: &str, value: f32, ticks: u32) {
    for _ in 0..ticks {
        if let Some(input) = world.resource_mut::<InputState>() {
            input.begin_tick();
            input.set_axis(ActionId::new(action), value);
        }
        tick(world);
    }
}

fn position(world: &World, entity: Entity) -> [f32; 3] {
    world
        .get::<Transform>(entity)
        .expect("still there")
        .translation
}

// --- Standing on a floor ---

#[test]
#[cfg(feature = "rapier")]
fn a_character_rests_on_the_floor_instead_of_falling_through_it() {
    let (mut world, player) = character_world(Box::new(amadeo_physics::RapierPhysics::new()));
    for _ in 0..120 {
        tick(&mut world);
    }

    let height = position(&world, player)[1];
    assert!(
        (height - RESTING_HEIGHT).abs() < 0.1,
        "should be resting at about {RESTING_HEIGHT}, got {height}"
    );
    assert!(
        world
            .get::<CharacterMotion>(player)
            .expect("there")
            .grounded,
        "and should know it is standing on something"
    );
}

#[test]
fn without_a_solver_a_character_falls_through_the_floor() {
    // The control case. `NullPhysics` applies the requested motion unmodified, so two seconds of
    // gravity puts the character well below the floor -- which is what proves the test above is
    // measuring collision response rather than a coincidence.
    let (mut world, player) = character_world(Box::new(NullPhysics::new()));
    for _ in 0..120 {
        tick(&mut world);
    }

    let height = position(&world, player)[1];
    assert!(
        height < -5.0,
        "with no solver it should have fallen far below the floor, got {height}"
    );
    assert!(
        !world
            .get::<CharacterMotion>(player)
            .expect("there")
            .grounded
    );
}

// --- Being stopped by a wall ---

#[test]
#[cfg(feature = "rapier")]
fn a_character_walking_into_a_wall_stops_at_it() {
    let (mut world, player) = character_world(Box::new(amadeo_physics::RapierPhysics::new()));
    // At yaw zero the character's right hand points +X, so a full-right axis walks it at the wall.
    // Two seconds at the default 5 units/s is ten units of travel into a wall 2.5 units away.
    hold_axis(&mut world, MOVE_RIGHT, 1.0, 120);

    let x = position(&world, player)[0];
    assert!(
        x < WALL_FACE,
        "the wall's near face is at {WALL_FACE}; the character reached {x}"
    );
    assert!(
        x > 1.5,
        "but it should have walked most of the way there, not been stuck at the start; got {x}"
    );
}

#[test]
fn without_a_solver_a_character_walks_straight_through_the_wall() {
    // The same control, for the wall. ADR 0037 section 5 is the reason this test exists and is
    // expected to "pass" by doing the wrong thing.
    let (mut world, player) = character_world(Box::new(NullPhysics::new()));
    hold_axis(&mut world, MOVE_RIGHT, 1.0, 120);

    let x = position(&world, player)[0];
    assert!(
        x > WALL_FACE + 1.0,
        "with no solver it should be well past the wall, got {x}"
    );
}

// --- Jumping ---

#[test]
#[cfg(feature = "rapier")]
fn a_grounded_character_can_jump_and_comes_back_down() {
    let (mut world, player) = character_world(Box::new(amadeo_physics::RapierPhysics::new()));
    // Settle onto the floor first: a jump is only allowed when grounded, which is the whole point
    // of tracking it.
    for _ in 0..30 {
        tick(&mut world);
    }
    assert!(
        world
            .get::<CharacterMotion>(player)
            .expect("there")
            .grounded
    );

    if let Some(input) = world.resource_mut::<InputState>() {
        input.begin_tick();
        input.set_button(ActionId::new(amadeo_character::JUMP), true);
    }
    tick(&mut world);

    let mut peak = position(&world, player)[1];
    for _ in 0..90 {
        // The button is released, so `just_pressed` is false and nothing re-triggers -- a character
        // that kept jumping while airborne would climb forever.
        if let Some(input) = world.resource_mut::<InputState>() {
            input.begin_tick();
            input.set_button(ActionId::new(amadeo_character::JUMP), false);
        }
        tick(&mut world);
        peak = peak.max(position(&world, player)[1]);
    }

    assert!(
        peak > RESTING_HEIGHT + 0.5,
        "the jump should have left the ground, peaked at {peak}"
    );
    let height = position(&world, player)[1];
    assert!(
        (height - RESTING_HEIGHT).abs() < 0.1,
        "and landed back on the floor, got {height}"
    );
}

// --- Determinism, which is the invariant everything else rests on ---

#[test]
fn driving_a_character_is_reproducible() {
    // I3, at the level a replay cares about. Runs against the null backend so it costs milliseconds
    // and needs no rapier; the rapier equivalent is
    // `crates/amadeo-physics/tests/rapier_determinism.rs`, which pins a literal hash across
    // platforms.
    let run = || {
        let (mut world, player) = character_world(Box::new(NullPhysics::new()));
        hold_axis(&mut world, MOVE_RIGHT, 1.0, 60);
        hold_axis(&mut world, MOVE_RIGHT, -0.5, 60);
        (world.state_hash(), position(&world, player))
    };
    assert_eq!(run(), run());
}

#[test]
fn a_character_is_part_of_the_state_hash() {
    // ADR 0037's last consequence, asserted rather than claimed: a moved character is a different
    // world, which is what makes a character-driven game replayable for nothing.
    let (mut world, _) = character_world(Box::new(NullPhysics::new()));
    let before = world.state_hash();
    hold_axis(&mut world, MOVE_RIGHT, 1.0, 10);
    assert_ne!(before, world.state_hash());
}

#[test]
fn a_world_with_no_physics_service_leaves_characters_alone() {
    // A game that installed the module but no backend should get a character that does nothing,
    // rather than one that falls through an empty world.
    let (mut world, player) = character_world(Box::new(NullPhysics::new()));
    world.remove_service::<Physics>();

    let before = position(&world, player);
    hold_axis(&mut world, MOVE_RIGHT, 1.0, 30);
    assert_eq!(before, position(&world, player));
}

#[test]
#[cfg(feature = "rapier")]
fn a_resting_character_does_not_sink_into_the_floor() {
    // The regression this module's first bug produced, pinned at the scale it happened.
    //
    // Pressing gently downward to "stay attached" moved the character further in one tick than the
    // skin gap it was being held at, so the next cast started from a touching position and it
    // ratcheted down about 0.07 units per second -- slowly enough to look like tuning and fast
    // enough to fall through a level. Ground detection does not need help; snapping does that job.
    let (mut world, player) = character_world(Box::new(amadeo_physics::RapierPhysics::new()));
    let mut lowest = f32::MAX;
    for _ in 0..600 {
        tick(&mut world);
        lowest = lowest.min(position(&world, player)[1]);
    }
    assert!(
        lowest > RESTING_HEIGHT - 0.01,
        "ten seconds of standing still should not move the character down; lowest was {lowest}"
    );
}
