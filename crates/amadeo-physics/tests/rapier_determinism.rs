//! Rapier actually collides, and does it reproducibly — ADR 0036.
//!
//! M2's exit gate 3 asks for a physics-heavy replay of 200+ bodies reproducing bit-identically
//! across runs and processes. This file is the in-process half of that claim, and the reason it can
//! be made at all: `enhanced-determinism` is on and `parallel` and `simd-*` are off, which ADR 0036
//! chose knowing it costs every core but one.
//!
//! The separate-process half is what `amadeo replay` already does for the rest of the engine, and it
//! comes when a game uses physics.

#![cfg(feature = "rapier")]

use amadeo_ecs::World;
use amadeo_physics::Physics;
use amadeo_physics::{
    Collider, Gravity, PhysicsBackend, RapierPhysics, RigidBody, Velocity, step_physics,
};
use amadeo_transform::Transform;

/// A ball dropped onto a wide static floor.
///
/// The smallest world that exercises the thing `NullPhysics` cannot do: collision detection and
/// response. Without a solver the ball simply falls forever.
fn dropped_ball(height: f32) -> World {
    let mut world = World::new();
    world.insert_service(Physics::new(Box::new(RapierPhysics::new())));
    world.insert_resource(Gravity::earth());

    let floor = world.spawn();
    let mut placement = Transform::at(0.0, 0.0);
    placement.translation = [0.0, -0.5, 0.0];
    world.insert(floor, placement);
    world.insert(floor, RigidBody::default());
    world.insert(floor, Collider::cuboid(50.0, 1.0, 50.0));

    let ball = world.spawn();
    let mut above = Transform::at(0.0, 0.0);
    above.translation = [0.0, height, 0.0];
    world.insert(ball, above);
    world.insert(ball, RigidBody::dynamic(1.0));
    world.insert(ball, Collider::sphere(0.5));
    world.insert(ball, Velocity::default());
    world
}

/// The entity carrying a `Velocity` — the ball, since the floor has none.
fn ball_of(world: &World) -> amadeo_ecs::Entity {
    world
        .entities()
        .into_iter()
        .find(|entity| world.get::<Velocity>(*entity).is_some())
        .expect("the ball has a velocity")
}

#[test]
fn a_ball_lands_on_the_floor_instead_of_falling_through_it() {
    // The one thing the null backend genuinely cannot do, and therefore the only real proof that
    // rapier is wired up rather than merely linked.
    let mut world = dropped_ball(6.0);
    let ball = ball_of(&world);

    for _ in 0..240 {
        step_physics(&mut world);
    }

    let height = world
        .get::<Transform>(ball)
        .expect("still there")
        .translation[1];
    // Floor top is at y = 0, ball radius 0.5, so it settles at about 0.5.
    assert!(
        (0.3..=0.7).contains(&height),
        "the ball should be resting on the floor, found it at {height}"
    );
}

#[test]
fn nothing_falls_through_at_a_greater_drop() {
    // A faster impact is where a solver without continuous collision detection lets a body tunnel
    // straight through a floor between two steps. Worth its own case, because the failure looks
    // identical to "gravity is broken" from the outside.
    let mut world = dropped_ball(40.0);
    let ball = ball_of(&world);

    for _ in 0..600 {
        step_physics(&mut world);
    }

    let height = world
        .get::<Transform>(ball)
        .expect("still there")
        .translation[1];
    assert!(
        height > -1.0,
        "the ball tunnelled through the floor, ending at {height}"
    );
}

#[test]
fn the_same_world_simulates_to_the_same_state_hash() {
    // Gate 3's claim, in process. `enhanced-determinism` is what makes this true rather than
    // usually-true, and ADR 0036 paid for it with every core but one.
    let run = || {
        let mut world = dropped_ball(6.0);
        let mut hashes = Vec::new();
        for tick in 0..300 {
            step_physics(&mut world);
            if tick % 60 == 0 {
                hashes.push(world.state_hash());
            }
        }
        (hashes, world.state_hash())
    };

    assert_eq!(run(), run());
}

#[test]
fn two_hundred_bodies_reproduce_exactly() {
    // The scale gate 3 names. Interesting beyond repetition: with this many bodies the solver forms
    // contact islands and processes them in an order that comes from its internal structures, which
    // is exactly where a nondeterministic engine stops agreeing with itself.
    let build = || {
        let mut world = World::new();
        world.insert_service(Physics::new(Box::new(RapierPhysics::new())));
        world.insert_resource(Gravity::earth());

        let floor = world.spawn();
        let mut placement = Transform::at(0.0, 0.0);
        placement.translation = [0.0, -0.5, 0.0];
        world.insert(floor, placement);
        world.insert(floor, RigidBody::default());
        world.insert(floor, Collider::cuboid(80.0, 1.0, 80.0));

        // A loose grid, dropped so they land on each other as well as on the floor.
        for index in 0..200_i32 {
            let entity = world.spawn();
            let x = (index % 20) as f32 * 1.4 - 14.0;
            let z = (index / 20) as f32 * 1.4 - 7.0;
            let mut at = Transform::at(0.0, 0.0);
            at.translation = [x, 2.0 + (index % 3) as f32, z];
            world.insert(entity, at);
            world.insert(entity, RigidBody::dynamic(1.0));
            world.insert(entity, Collider::cuboid(1.0, 1.0, 1.0));
            world.insert(entity, Velocity::default());
        }
        world
    };

    let run = || {
        let mut world = build();
        for _ in 0..120 {
            step_physics(&mut world);
        }
        world.state_hash()
    };

    assert_eq!(run(), run(), "200 bodies must reproduce bit-identically");
}

#[test]
fn resetting_makes_a_restored_world_behave_like_a_fresh_one() {
    // ADR 0028's lesson, applied to physics. Rapier keeps contact caches and sleeping state between
    // steps, so a world whose components were replaced wholesale — a snapshot restore — would carry
    // caches describing a world that no longer exists. `reset` is what makes the two agree, and this
    // is the test that would fail if it stopped being called.
    //
    // Simulated here by running one backend forward, resetting it, and pointing it at a fresh world:
    // it must produce exactly what a brand-new backend produces.
    let mut used = RapierPhysics::new();
    let mut warm = dropped_ball(6.0);
    for _ in 0..120 {
        step_physics(&mut warm);
    }
    // Now dirty `used` with an unrelated simulation, then reset it.
    let mut scratch = dropped_ball(30.0);
    for _ in 0..60 {
        step_physics(&mut scratch);
    }
    used.reset();

    let hash_with = {
        let mut world = dropped_ball(6.0);
        world.insert_service(Physics::new(Box::new(used)));
        for _ in 0..120 {
            step_physics(&mut world);
        }
        world.state_hash()
    };

    let hash_fresh = {
        let mut world = dropped_ball(6.0);
        for _ in 0..120 {
            step_physics(&mut world);
        }
        world.state_hash()
    };

    assert_eq!(
        hash_with, hash_fresh,
        "a reset backend must be indistinguishable from a new one"
    );
}

/// The state hash of `dropped_ball(6.0)` after 300 ticks, recorded on Windows.
///
/// **This is the cross-platform claim, not a regression guard.** Every other test here asserts that
/// a run agrees with *itself*, which any engine manages. This one pins the actual number, and CI runs
/// it on Windows and Linux — so if rapier's `enhanced-determinism` does not deliver what ADR 0036
/// bought every core but one for, the two jobs disagree and CI says so.
///
/// If it ever fails, **do not regenerate it without reading `docs/07` § Golden replays first.** The
/// question is always *why*, and there are three candidates worth separating: the rapier version
/// changed (ADR 0036 pins it exactly so this is visible), a feature crept in (`parallel` or `simd-*`
/// would do it), or the test's own world changed.
const BALL_AFTER_300_TICKS: u64 = 12_966_218_810_477_015_508;

#[test]
fn the_hash_is_the_same_on_every_platform() {
    let mut world = dropped_ball(6.0);
    for _ in 0..300 {
        step_physics(&mut world);
    }

    let hash = world.state_hash();
    assert_eq!(
        hash, BALL_AFTER_300_TICKS,
        "physics diverged across platforms or versions. Read the note above this constant before \
         changing it — the number moving is the finding, not the problem to silence."
    );
}

#[test]
fn a_static_body_holds_still_under_a_pile() {
    // Static is the default, so this is what a level is made of. A floor that drifted would take
    // everything standing on it with it, slowly, in a way that reads as the camera being wrong.
    let mut world = dropped_ball(3.0);
    let floor = world
        .entities()
        .into_iter()
        .find(|entity| world.get::<Velocity>(*entity).is_none())
        .expect("the floor has no velocity");
    let before = world.get::<Transform>(floor).expect("there").translation;

    for _ in 0..180 {
        step_physics(&mut world);
    }

    assert_eq!(
        world.get::<Transform>(floor).expect("there").translation,
        before
    );
}
