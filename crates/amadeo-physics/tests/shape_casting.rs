//! A cast is not a move, and the difference is the whole reason it exists — ADR 0054.
//!
//! `move_shape` answers *"where does this body end up?"* and **slides** to do it. `cast_shape`
//! answers *"how far along this line before something blocks it?"* and does not. **Q34** recorded
//! that borrowing the first to answer the second was a workaround; session 15 watched it fail twice,
//! the second time by putting a follow camera underneath the terrain.
//!
//! The tests below are mostly about that distinction, because a `cast_shape` that quietly behaved
//! like `move_shape` would pass every "does it stop at the wall" test and reintroduce the bug.

#![cfg(feature = "rapier")]

use amadeo_ecs::World;
use amadeo_physics::{
    Collider, Gravity, Physics, RapierPhysics, RigidBody, Shape, ShapeCast, ShapeMove, step_physics,
};

/// A world with one wide static floor at y = 0, and physics stepped once so the index exists.
fn world_with_a_floor() -> World {
    let mut world = World::new();
    world.insert_service(Physics::new(Box::new(RapierPhysics::new())));
    world.insert_resource(Gravity::earth());

    let floor = world.spawn();
    let mut placement = amadeo_transform::Transform::at(0.0, 0.0);
    placement.translation = [0.0, -0.5, 0.0];
    world.insert(floor, placement);
    world.insert(floor, RigidBody::default());
    world.insert(floor, Collider::cuboid(50.0, 1.0, 50.0));

    // **Once**, because both queries answer from an index the step builds and an unstepped world
    // reports everything clear — the failure mode `move_shape`'s docs warn about.
    step_physics(&mut world);
    world
}

#[test]
fn a_clear_sweep_reports_nothing_in_the_way() {
    // The control. Without it, a `cast_shape` that always reported a hit at fraction zero would pass
    // every other test in this file.
    let world = world_with_a_floor();
    let physics = world.service::<Physics>().expect("physics");

    let hit = physics.cast_shape(&ShapeCast::new(
        Shape::Sphere { radius: 0.25 },
        [0.0, 10.0, 0.0],
        [0.0, 4.0, 0.0],
    ));
    assert!(
        hit.is_none(),
        "sweeping upward from ten metres up hits nothing, got {hit:?}"
    );
}

#[test]
fn a_sweep_into_the_floor_stops_at_the_surface() {
    // A sphere of radius 0.25 dropped from y = 4 onto a floor whose top face is y = 0 should be
    // stopped with its *centre* a radius above the surface, so it travels about 3.75 of its 4.
    let world = world_with_a_floor();
    let physics = world.service::<Physics>().expect("physics");

    let hit = physics
        .cast_shape(&ShapeCast::new(
            Shape::Sphere { radius: 0.25 },
            [0.0, 4.0, 0.0],
            [0.0, -4.0, 0.0],
        ))
        .expect("the floor is in the way");

    assert!(
        (hit.translation[1] - 0.25).abs() < 0.05,
        "the sphere's centre should rest a radius above the floor at y = 0.25, got {}",
        hit.translation[1]
    );
    // The fraction and the position have to agree, or a caller using one and reasoning about the
    // other is being lied to.
    assert!(
        (hit.fraction - 0.9375).abs() < 0.02,
        "3.75 of a 4-metre sweep is a fraction of 0.9375, got {}",
        hit.fraction
    );
    // Pointing out of the floor, which is up.
    assert!(
        hit.normal[1] > 0.9,
        "the floor's outward normal points up, got {:?}",
        hit.normal
    );
}

#[test]
fn the_reported_position_is_always_on_the_line_that_was_asked_about() {
    // **The property `move_shape` cannot offer, and the one this whole operation exists for.**
    //
    // A sweep angled down into the floor is the exact geometry that broke the follow camera: the
    // character move hits the floor and *slides* horizontally along it, so where it ends up is
    // nowhere near the line it was given. A cast has to stay on that line.
    let world = world_with_a_floor();
    let physics = world.service::<Physics>().expect("physics");

    // **This is the follow camera's geometry**, at the scale that broke it: a shallow line, mostly
    // sideways and a little downward, starting close above the floor. Steeper angles hide the
    // problem, which is why it survived one round of fixing.
    let start = [0.0f32, 1.0, 0.0];
    let motion = [6.0f32, -3.464, 0.0];
    let shape = Shape::Sphere { radius: 0.25 };
    let length = (motion[0] * motion[0] + motion[1] * motion[1] + motion[2] * motion[2]).sqrt();
    let direction = [motion[0] / length, motion[1] / length, motion[2] / length];

    let hit = physics
        .cast_shape(&ShapeCast::new(shape, start, motion))
        .expect("the floor is in the way");

    // On the line: the position must be the start plus exactly `fraction` of the motion.
    for axis in 0..3 {
        let expected = start[axis] + motion[axis] * hit.fraction;
        assert!(
            (hit.translation[axis] - expected).abs() < 1e-4,
            "axis {axis} is off the line: got {} rather than {expected}",
            hit.translation[axis]
        );
    }
    let clear = hit.fraction * length;

    // And now the same question asked the old way, which is what makes this evidence rather than a
    // restatement of the implementation. `move_shape` descends to the floor and then **slides** the
    // rest of its horizontal motion along it; projecting that travel onto the direction asked for —
    // the exact arithmetic the camera used — counts the slide as progress along the line.
    let mut moving = world;
    let physics = moving.service_mut::<Physics>().expect("physics");
    let slid = physics.move_shape(&ShapeMove {
        step_height: 0.0,
        snap_distance: 0.0,
        ..ShapeMove::new(shape, start, motion)
    });
    let travelled = [
        slid.translation[0] - start[0],
        slid.translation[1] - start[1],
        slid.translation[2] - start[2],
    ];
    let projected =
        travelled[0] * direction[0] + travelled[1] * direction[1] + travelled[2] * direction[2];

    assert!(
        projected > clear * 2.0,
        "the projection of a sliding move should wildly over-report how far the line is clear \
         (got {projected} against a true {clear}) — if it does not, this test is not exercising \
         the failure it exists to pin"
    );
}

#[test]
fn a_sweep_can_ignore_the_body_it_starts_inside() {
    // The filter a follow camera needs, because its pivot sweep starts at the middle of whatever it
    // is following. Without it, rapier reports the body it starts in.
    let mut world = world_with_a_floor();

    let blocker = world.spawn();
    let mut placement = amadeo_transform::Transform::at(0.0, 0.0);
    placement.translation = [0.0, 5.0, 0.0];
    world.insert(blocker, placement);
    world.insert(blocker, RigidBody::default());
    world.insert(blocker, Collider::cuboid(1.0, 1.0, 1.0));
    step_physics(&mut world);

    let physics = world.service::<Physics>().expect("physics");
    let shape = Shape::Sphere { radius: 0.25 };
    // Starting inside the blocker and sweeping up out of it.
    let cast = ShapeCast::new(shape, [0.0, 5.0, 0.0], [0.0, 6.0, 0.0]);

    let ignored = physics.cast_shape(&cast.ignoring(blocker));
    assert!(
        ignored.is_none(),
        "with the blocker excluded there is nothing above, got {ignored:?}"
    );
}

#[test]
fn a_zero_length_sweep_has_no_answer_rather_than_a_wrong_one() {
    // There is no line to ask about, so there is no direction a normal could point along. Reporting
    // "clear" is the only honest option, and it keeps a caller from dividing by a length of zero.
    let world = world_with_a_floor();
    let physics = world.service::<Physics>().expect("physics");

    let hit = physics.cast_shape(&ShapeCast::new(
        Shape::Sphere { radius: 0.25 },
        [0.0, 0.25, 0.0],
        [0.0, 0.0, 0.0],
    ));
    assert!(
        hit.is_none(),
        "a sweep of no length hits nothing, got {hit:?}"
    );
}

#[test]
fn a_sweep_starting_against_a_surface_and_pointing_away_is_not_blocked() {
    // **`stop_at_penetration: false`, and why.** A sphere resting exactly on the floor and asked to
    // go *up* is not obstructed, but the default shape-cast setting reports an immediate hit for
    // anything that starts in contact whatever direction it was going.
    //
    // This is the follow camera's pivot in a low corridor: squeezed down until it touches the
    // ceiling, with the arm pointing down and back into open air. Reporting a block there is a
    // camera glued to its minimum distance for as long as you stand under something.
    let world = world_with_a_floor();
    let physics = world.service::<Physics>().expect("physics");

    let hit = physics.cast_shape(&ShapeCast::new(
        Shape::Sphere { radius: 0.25 },
        // Exactly touching: centre one radius above the floor's top face.
        [0.0, 0.25, 0.0],
        [0.0, 5.0, 0.0],
    ));
    assert!(
        hit.is_none(),
        "resting on the floor and sweeping upward is unobstructed, got {hit:?}"
    );
}

#[test]
fn the_null_backend_finds_nothing_which_is_what_makes_the_others_evidence() {
    // ADR 0037 §5's posture, applied to the third operation. Every assertion above would also pass
    // against a backend that simply reported the floor by coincidence; this is what says they are
    // measuring real geometry.
    let mut world = World::new();
    world.insert_service(Physics::new(Box::new(amadeo_physics::NullPhysics::new())));
    world.insert_resource(Gravity::earth());

    let floor = world.spawn();
    let mut placement = amadeo_transform::Transform::at(0.0, 0.0);
    placement.translation = [0.0, -0.5, 0.0];
    world.insert(floor, placement);
    world.insert(floor, RigidBody::default());
    world.insert(floor, Collider::cuboid(50.0, 1.0, 50.0));
    step_physics(&mut world);

    let physics = world.service::<Physics>().expect("physics");
    let hit = physics.cast_shape(&ShapeCast::new(
        Shape::Sphere { radius: 0.25 },
        [0.0, 4.0, 0.0],
        [0.0, -4.0, 0.0],
    ));
    assert!(
        hit.is_none(),
        "the null backend detects nothing, by design — got {hit:?}"
    );
}
