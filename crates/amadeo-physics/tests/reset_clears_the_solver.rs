//! What `PhysicsBackend::reset` is actually for, checked rather than asserted — ADR 0036.
//!
//! # The claim being tested
//!
//! ADR 0036 says the solver's world is a **cache rebuilt from the components**, and ADR 0028's
//! lesson says hash equality after a restore is necessary and not sufficient. Put together they
//! predict that a solver which has been simulating one world, then handed another world's
//! components, does not behave like a fresh one — because contact caches, warm-start impulses and
//! sleeping islands are not components and do not come back with them.
//!
//! That prediction is worth checking rather than repeating. `Physics::reset` was added the day this
//! file was written, and a line of code with a strong comment and no demonstration behind it is
//! exactly what this project keeps finding it does not want.
//!
//! # And the measured answer is not the predicted one
//!
//! **A warm solver matches a cold one exactly**, even with a settled, sleeping stack of six dynamic
//! boxes. The reset changes nothing about contacts.
//!
//! That is ADR 0036's own contract paying off rather than a surprise: `PhysicsBackend::step` is
//! handed the complete input and returns the complete output, and the trait requires that a backend
//! **not keep state which cannot be rebuilt from the bodies it is given**. A solver honouring that
//! has nothing to go stale — so the same decision that makes physics deterministic is what makes the
//! contact-cache half of `reset` unnecessary.
//!
//! What the reset is still unambiguously for is **dropping static geometry**, which is derived data
//! belonging to a level rather than to a body, travels by id rather than through `step`, and would
//! otherwise leave the previous world's ground standing in the next one.
//!
//! # Why it lives here and not beside the save/load tests
//!
//! `games/atrium` restores snapshots and is the obvious home, and it cannot show this: its bodies
//! are static geometry plus one kinematic character, and neither accumulates the state that would
//! go stale. **Sleeping dynamic bodies are the case**, and building a stack of them needs nothing
//! above `amadeo-physics`.

#![cfg(feature = "rapier")]

use amadeo_ecs::{Entity, World};
use amadeo_physics::{
    Collider, Gravity, Physics, RapierPhysics, RigidBody, Velocity, step_physics,
};
use amadeo_transform::Transform;

/// How many boxes go in the stack.
const BOXES: usize = 6;

/// A stack of boxes on a floor — bodies that settle, rest on each other, and go to sleep.
///
/// The shape that accumulates solver state: resting contacts persist between steps, warm-start
/// impulses build up, and an island that stops moving is put to sleep. None of it is a component.
fn stack() -> World {
    let mut world = World::new();
    world.insert_service(Physics::new(Box::new(RapierPhysics::new())));
    world.insert_resource(Gravity::earth());

    let floor = world.spawn();
    let mut ground = Transform::at(0.0, 0.0);
    ground.translation = [0.0, -0.5, 0.0];
    world.insert(floor, ground);
    world.insert(floor, RigidBody::default());
    world.insert(floor, Collider::cuboid(50.0, 1.0, 50.0));

    for index in 0..BOXES {
        let box_ = world.spawn();
        let mut placement = Transform::at(0.0, 0.0);
        // Slightly offset each one, so the stack settles into something with real contacts rather
        // than a perfectly symmetric tower that resolves in one step.
        placement.translation = [
            0.02 * index as f32,
            1.0 + 1.05 * index as f32,
            -0.02 * index as f32,
        ];
        world.insert(box_, placement);
        world.insert(box_, RigidBody::dynamic(1.0));
        world.insert(box_, Collider::cuboid(0.5, 0.5, 0.5));
        world.insert(box_, Velocity::default());
    }
    world
}

/// Every dynamic body's starting placement, in entity order.
fn starting_places(world: &World) -> Vec<(Entity, [f32; 3])> {
    let mut places: Vec<(Entity, [f32; 3])> = world
        .entities()
        .into_iter()
        .filter(|entity| world.get::<Velocity>(*entity).is_some())
        .filter_map(|entity| {
            world
                .get::<Transform>(entity)
                .map(|transform| (entity, transform.translation))
        })
        .collect();
    places.sort_by_key(|(entity, _)| (entity.index(), entity.generation()));
    places
}

/// Puts the bodies back where they started and stops them dead — what restoring a snapshot does to
/// the *components*, and only to the components.
fn rewind(world: &mut World, places: &[(Entity, [f32; 3])]) {
    for (entity, place) in places {
        if let Some(transform) = world.get_mut::<Transform>(*entity) {
            transform.translation = *place;
            transform.rotation = [0.0; 3];
        }
        if let Some(velocity) = world.get_mut::<Velocity>(*entity) {
            *velocity = Velocity::default();
        }
    }
}

fn run(world: &mut World, ticks: usize) {
    for _ in 0..ticks {
        step_physics(world);
    }
}

fn places(world: &World) -> Vec<[f32; 3]> {
    starting_places(world)
        .into_iter()
        .map(|(_, place)| place)
        .collect()
}

/// Where a stack ends up after `ticks` in a solver that has never seen anything else.
fn from_cold(ticks: usize) -> Vec<[f32; 3]> {
    let mut world = stack();
    run(&mut world, ticks);
    places(&world)
}

/// The same, in a solver that has already settled the stack once — optionally reset in between.
fn from_warm(ticks: usize, reset: bool) -> Vec<[f32; 3]> {
    let mut world = stack();
    let start = starting_places(&world);

    // Long enough for the stack to come to rest and for rapier to put the island to sleep, which is
    // the state that has no component to travel in.
    run(&mut world, 240);

    rewind(&mut world, &start);
    if reset && let Some(physics) = world.service_mut::<Physics>() {
        physics.reset();
    }

    run(&mut world, ticks);
    places(&world)
}

#[test]
fn a_reset_solver_behaves_like_a_fresh_one() {
    // **The property `Physics::reset` exists to provide.** Restoring a world into a process that has
    // been simulating must land in the same place as loading it into a fresh one, or a save resumes
    // into a different game depending on what the player did before loading it.
    assert_eq!(from_warm(60, true), from_cold(60));
}

#[test]
fn what_the_reset_is_worth_here() {
    // **Reported rather than asserted, because the honest answer is not the expected one.**
    //
    // The prediction was that a warm solver would diverge from a cold one: contact caches,
    // warm-start impulses and sleeping islands are not components, so restoring components alone
    // should leave them behind. `rewind` does exactly what a snapshot restore does — puts the
    // transforms and velocities back, and nothing else.
    //
    // Whether it actually diverges is a fact about rapier's internals under
    // `enhanced-determinism`, and it is allowed to change between versions — the version is pinned
    // (ADR 0036) precisely because such things move. So this prints the answer instead of demanding
    // one, in the same spirit as the frame-budget tests: a claim about somebody else's solver is not
    // a thing to fail a build over.
    //
    // What is *not* optional either way is the reset itself. It is the contract for replacing a
    // world (ADR 0036), and it is also what drops static geometry — so a game that streams terrain
    // and skips it keeps the ground of the level it just left, which is not subtle at all.
    let cold = from_cold(60);
    let warm = from_warm(60, false);

    if warm == cold {
        println!(
            "rapier rebuilt everything from the components: a warm solver matched a cold one \
             exactly, so the reset changed nothing measurable here."
        );
        println!(
            "That is ADR 0036's own contract paying off rather than a surprise. \
             `PhysicsBackend::step` is handed the complete input and returns the complete output, \
             and the trait says a backend `must not keep state that cannot be rebuilt from the \
             bodies it is given`. A solver honouring that has nothing to go stale."
        );
    } else {
        let worst = warm
            .iter()
            .zip(&cold)
            .map(|(a, b)| {
                (0..3)
                    .map(|axis| (a[axis] - b[axis]).abs())
                    .fold(0.0_f32, f32::max)
            })
            .fold(0.0_f32, f32::max);
        println!(
            "a warm solver diverged from a cold one by up to {worst} m over 60 ticks — this is \
             what `Physics::reset` prevents."
        );
    }

    // The one thing that is asserted: the reset does not make things *worse*. Whatever rapier does,
    // resetting must land on the cold answer.
    assert_eq!(from_warm(60, true), cold);
}

#[test]
fn resetting_drops_the_static_geometry() {
    // The half that is unambiguous and has nothing to do with contact caches: static meshes are
    // derived data belonging to a level, so replacing the world must not leave the old one's ground
    // standing. A game that streams terrain rebuilds it by noticing it is gone (ADR 0043).
    let mut world = stack();
    let mesh = amadeo_physics::StaticMesh {
        id: amadeo_physics::StaticMeshId(7),
        translation: [0.0, 0.0, 0.0],
        vertices: vec![
            [0.0, 0.0, 0.0],
            [1.0, 0.0, 0.0],
            [0.0, 0.0, 1.0],
            [1.0, 0.0, 1.0],
        ],
        indices: vec![[0, 1, 2], [1, 3, 2]],
        friction: 0.8,
    };

    if let Some(physics) = world.service_mut::<Physics>() {
        physics.insert_static_mesh(mesh).expect("a valid mesh");
        assert_eq!(physics.static_mesh_count(), 1);

        physics.reset();
        assert_eq!(
            physics.static_mesh_count(),
            0,
            "the previous level's ground must not survive into the next one"
        );
    }
}
