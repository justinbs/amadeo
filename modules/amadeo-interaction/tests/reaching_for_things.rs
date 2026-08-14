//! Looking at a thing and using it, against a real solver.
//!
//! # Why the whole file is behind `rapier`
//!
//! The mechanism *is* a shape cast, and `NullPhysics` reports every cast clear. A headless run of
//! this module therefore finds nothing in reach ever — which is the honest null-backend answer and
//! is asserted once, at the bottom, as the control case. Everything else needs a solver.
//!
//! That is the same trade `modules/amadeo-character` makes, where the null backend walks through
//! walls and the test says so on purpose.

use amadeo_ecs::{Entity, World};
use amadeo_events::WorldEvents;
use amadeo_input::{ActionId, InputState};
use amadeo_interaction::{Interactable, Interacted, Interactor, Looking, USE, update_interactions};
use amadeo_physics::{Collider, Gravity, Physics, RigidBody, step_physics};
use amadeo_transform::{Parent, Transform, propagate_transforms};

/// A world with an interactor at the origin looking down −Z, and physics stepped once.
///
/// **Stepped**, because a cast answers from an index the step builds and an unstepped world reports
/// everything clear (ADR 0054) — which would make every test here pass for the wrong reason.
#[cfg(feature = "rapier")]
fn room() -> (World, Entity) {
    let mut world = World::new();
    world.insert_service(Physics::new(Box::new(amadeo_physics::RapierPhysics::new())));
    world.insert_resource(Gravity::default());
    world.insert_resource(InputState::new());
    world.register_event::<Interacted>();

    let looker = world.spawn();
    world.insert(looker, Transform::default());
    world.insert(looker, Interactor::default());

    (world, looker)
}

/// Puts a usable box `distance` in front of the interactor, and hands back its entity.
#[cfg(feature = "rapier")]
fn thing_ahead(world: &mut World, distance: f32, prompt: &str) -> Entity {
    let thing = world.spawn();
    let mut placement = Transform::at(0.0, 0.0);
    // −Z is forward (ADR 0018).
    placement.translation = [0.0, 0.0, -distance];
    world.insert(thing, placement);
    world.insert(thing, RigidBody::default());
    world.insert(thing, Collider::cuboid(1.0, 1.0, 1.0));
    world.insert(thing, Interactable::new(prompt));
    step_physics(world);
    thing
}

#[cfg(feature = "rapier")]
fn looking_at(world: &World, looker: Entity) -> Option<Entity> {
    world.get::<Looking>(looker).and_then(|looking| looking.at)
}

/// Presses the use action for one tick. Edge-triggered, so the release matters.
#[cfg(feature = "rapier")]
fn press_use(world: &mut World) {
    if let Some(input) = world.resource_mut::<InputState>() {
        input.begin_tick();
        input.set_button(ActionId::new(USE), false);
        input.begin_tick();
        input.set_button(ActionId::new(USE), true);
    }
}

#[cfg(feature = "rapier")]
fn activations(world: &mut World) -> Vec<(Entity, Entity)> {
    world.swap_events::<Interacted>();
    world
        .read_events::<Interacted>()
        .iter()
        .map(|record| (record.event.interactor, record.event.target))
        .collect()
}

#[cfg(feature = "rapier")]
mod with_a_solver {
    use super::*;

    #[test]
    fn something_in_front_is_looked_at() {
        let (mut world, looker) = room();
        let door = thing_ahead(&mut world, 1.5, "Open the door");
        update_interactions(&mut world);

        assert_eq!(looking_at(&world, looker), Some(door));
        let distance = world.get::<Looking>(looker).expect("written").distance;
        assert!((0.0..=1.5).contains(&distance), "got {distance}");
    }

    #[test]
    fn something_behind_is_not() {
        // **The sign that would make interaction silently never work.** ADR 0018 puts forward at
        // −Z, and a cast built the other way reaches behind the player — which reads as the feature
        // being broken rather than as one minus sign.
        let (mut world, looker) = room();
        let behind = world.spawn();
        let mut placement = Transform::at(0.0, 0.0);
        placement.translation = [0.0, 0.0, 1.5];
        world.insert(behind, placement);
        world.insert(behind, RigidBody::default());
        world.insert(behind, Collider::cuboid(1.0, 1.0, 1.0));
        world.insert(behind, Interactable::new("Behind you"));
        step_physics(&mut world);

        update_interactions(&mut world);
        assert_eq!(looking_at(&world, looker), None);
    }

    #[test]
    fn something_out_of_reach_is_not() {
        let (mut world, looker) = room();
        let far = thing_ahead(&mut world, 40.0, "Too far");
        update_interactions(&mut world);

        assert_eq!(looking_at(&world, looker), None);
        assert_ne!(far, looker, "the control: it does exist");
    }

    #[test]
    fn turning_away_stops_looking_at_it() {
        // The half that catches a `Looking` which is written once and never cleared — an
        // interaction prompt that stays on screen after you look away, and a "use" that still works.
        let (mut world, looker) = room();
        thing_ahead(&mut world, 1.5, "Open the door");
        update_interactions(&mut world);
        assert!(looking_at(&world, looker).is_some());

        if let Some(transform) = world.get_mut::<Transform>(looker) {
            transform.rotation[1] = 180.0;
        }
        update_interactions(&mut world);
        assert_eq!(looking_at(&world, looker), None);
    }

    #[test]
    fn a_wall_in_the_way_blocks_it() {
        // The whole reason this is a cast rather than a distance check. Reaching through a wall to
        // open a door is the classic interaction bug, and it is invisible until somebody tries it.
        let (mut world, looker) = room();
        let door = thing_ahead(&mut world, 2.0, "Open the door");

        let wall = world.spawn();
        let mut placement = Transform::at(0.0, 0.0);
        placement.translation = [0.0, 0.0, -1.0];
        world.insert(wall, placement);
        world.insert(wall, RigidBody::default());
        world.insert(wall, Collider::cuboid(4.0, 4.0, 0.2));
        step_physics(&mut world);

        update_interactions(&mut world);
        assert_ne!(
            looking_at(&world, looker),
            Some(door),
            "the door is behind a wall"
        );
    }

    #[test]
    fn using_raises_an_event_naming_both_ends() {
        // Both ends, because a game with two players in the same room needs to know which of them
        // opened the door.
        let (mut world, looker) = room();
        let door = thing_ahead(&mut world, 1.5, "Open the door");

        press_use(&mut world);
        update_interactions(&mut world);

        assert_eq!(activations(&mut world), vec![(looker, door)]);
    }

    #[test]
    fn using_with_nothing_in_reach_raises_nothing() {
        let (mut world, _) = room();
        press_use(&mut world);
        update_interactions(&mut world);

        assert!(activations(&mut world).is_empty());
    }

    #[test]
    fn a_disabled_thing_is_looked_past() {
        // A locked door. It is still solid — the cast still stops on it — but it is not something
        // that can be used, which is why `Looking` is `None` rather than pointing at it.
        let (mut world, looker) = room();
        let door = thing_ahead(&mut world, 1.5, "Open the door");
        world.insert(
            door,
            Interactable {
                enabled: false,
                ..Interactable::new("Locked")
            },
        );

        press_use(&mut world);
        update_interactions(&mut world);

        assert_eq!(looking_at(&world, looker), None);
        assert!(activations(&mut world).is_empty());
    }

    #[test]
    fn scenery_is_not_something_you_can_use() {
        // A hit is not an interaction. Without this check a player could "use" a wall, and with
        // static geometry — which reports no entity at all — the answer has to be `None` rather
        // than a panic.
        let (mut world, looker) = room();
        let wall = world.spawn();
        let mut placement = Transform::at(0.0, 0.0);
        placement.translation = [0.0, 0.0, -1.5];
        world.insert(wall, placement);
        world.insert(wall, RigidBody::default());
        world.insert(wall, Collider::cuboid(4.0, 4.0, 0.2));
        step_physics(&mut world);

        press_use(&mut world);
        update_interactions(&mut world);

        assert_eq!(looking_at(&world, looker), None);
        assert!(activations(&mut world).is_empty());
    }

    #[test]
    fn what_is_being_looked_at_is_not_in_the_state_hash() {
        // `Looking` is derived (see its docs), and this is the assertion behind that call: it is
        // recomputed every tick from transforms and the physics index, both already hashed, so
        // hashing it too would only hash the same facts twice.
        let (mut world, _) = room();
        thing_ahead(&mut world, 1.5, "Open the door");

        let before = world.state_hash();
        update_interactions(&mut world);
        update_interactions(&mut world);
        assert_eq!(before, world.state_hash());
    }
}

#[test]
fn without_a_solver_nothing_is_ever_in_reach() {
    // **The control case, and it is honest rather than sad.** The mechanism is a shape cast, and
    // `NullPhysics` reports every cast clear — so a headless build of a game using this module has
    // no interaction at all. Stating it here is what stops a future test passing against the null
    // backend and being believed.
    let mut world = World::new();
    world.insert_service(Physics::new(Box::new(amadeo_physics::NullPhysics::new())));
    world.insert_resource(InputState::new());
    world.register_event::<Interacted>();

    let looker = world.spawn();
    world.insert(looker, Transform::default());
    world.insert(looker, Interactor::default());

    let thing = world.spawn();
    let mut placement = Transform::at(0.0, 0.0);
    placement.translation = [0.0, 0.0, -1.0];
    world.insert(thing, placement);
    world.insert(thing, Interactable::new("Right there"));

    update_interactions(&mut world);
    assert_eq!(
        world.get::<Looking>(looker).and_then(|looking| looking.at),
        None
    );
}

/// The defect `games/atrium` found the first time a game put an `Interactor` on a child.
///
/// # Why this is a whole module of its own
///
/// The module's docs have said since it was written that an interactor is **usually a child** — a
/// camera or a reaching point on a character. Every test above puts it on a lone entity with no
/// collider anywhere, so the arrangement the docs call usual was the one arrangement nothing
/// covered.
///
/// In that arrangement the sweep starts inside the *parent's* collider. Ignoring the interactor
/// ignored nothing, every cast came back at `fraction: 0.0` against the body, and `Looking::at`
/// stayed `None` for ever — which is indistinguishable from standing too far away, so there is no
/// symptom to notice.
#[cfg(feature = "rapier")]
mod on_a_child {
    use super::*;

    /// A body with a collider, and a reaching point parented to it.
    fn body_with_a_hand() -> (World, Entity, Entity) {
        let mut world = World::new();
        world.insert_service(Physics::new(Box::new(amadeo_physics::RapierPhysics::new())));
        world.insert_resource(Gravity::default());
        world.insert_resource(InputState::new());
        world.register_event::<Interacted>();

        let body = world.spawn();
        world.insert(body, Transform::default());
        world.insert(body, RigidBody::default());
        // Wide enough that a sweep starting at the centre begins inside it, which is the whole
        // point: this is a player capsule in every game that has one.
        world.insert(body, Collider::cuboid(1.0, 2.0, 1.0));

        let hand = world.spawn();
        world.insert(hand, Transform::default());
        world.insert(hand, Interactor::default());
        world.insert(hand, Parent(body));

        (world, body, hand)
    }

    #[test]
    fn a_sweep_from_a_child_ignores_the_body_it_is_attached_to() {
        let (mut world, _body, hand) = body_with_a_hand();
        let thing = thing_ahead(&mut world, 1.5, "Open it");
        propagate_transforms(&mut world);
        update_interactions(&mut world);

        assert_eq!(
            looking_at(&world, hand),
            Some(thing),
            "the sweep begins inside the parent's collider, so ignoring only the interactor -- \
             which has no collider of its own -- leaves the body in the way and reports nothing \
             for ever"
        );
    }

    #[test]
    fn and_using_it_still_raises_the_event() {
        let (mut world, _body, hand) = body_with_a_hand();
        let thing = thing_ahead(&mut world, 1.5, "Open it");
        propagate_transforms(&mut world);
        press_use(&mut world);
        update_interactions(&mut world);

        assert_eq!(activations(&mut world), vec![(hand, thing)]);
    }

    #[test]
    fn a_grandchild_reaches_past_the_body_too() {
        // Walking one link would fix the case above and not this one, and a camera boom on a
        // character is two links in most games.
        let (mut world, body, _hand) = body_with_a_hand();
        let arm = world.spawn();
        world.insert(arm, Transform::default());
        world.insert(arm, Parent(body));

        let tip = world.spawn();
        world.insert(tip, Transform::default());
        world.insert(tip, Interactor::default());
        world.insert(tip, Parent(arm));

        let thing = thing_ahead(&mut world, 1.5, "Open it");
        propagate_transforms(&mut world);
        update_interactions(&mut world);

        assert_eq!(looking_at(&world, tip), Some(thing));
    }
}
