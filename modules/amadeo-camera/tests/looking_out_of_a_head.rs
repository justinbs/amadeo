//! The first-person rig, headless.
//!
//! # Why these are worth having separately from the third-person ones
//!
//! Both rigs share `look_with_mouse`, and sharing it is the point — two copies of "yaw goes on the
//! parent, pitch goes on the camera, and both are subtracted" would be two chances to get a sign
//! backwards, in a way that ships because a view turning the wrong way is *plausible*.
//!
//! What that sharing means is that a change made for one rig can silently break the other. These
//! assert the first-person half of the contract, so it cannot be broken by a third-person fix.

use amadeo_camera::{FirstPersonCamera, LOOK, LOOK_X, LOOK_Y, look_with_mouse, place_first_person};
use amadeo_ecs::{Entity, World};
use amadeo_input::{ActionId, InputState};
use amadeo_transform::{Parent, Transform};

/// A body with a camera in its head, and the two entities.
fn head() -> (World, Entity, Entity) {
    let mut world = World::new();
    world.insert_resource(InputState::new());

    let body = world.spawn();
    world.insert(body, Transform::default());

    let eyes = world.spawn();
    world.insert(eyes, Parent(body));
    world.insert(eyes, Transform::default());
    world.insert(eyes, FirstPersonCamera::default());

    (world, body, eyes)
}

/// Moves the pointer by `dx`, `dy` with the look button held, then runs the rig.
fn look(world: &mut World, dx: f32, dy: f32) {
    if let Some(input) = world.resource_mut::<InputState>() {
        input.set_button(ActionId::new(LOOK), true);
        input.set_axis(ActionId::new(LOOK_X), dx);
        input.set_axis(ActionId::new(LOOK_Y), dy);
    }
    look_with_mouse(world);
    place_first_person(world);
}

fn rotation(world: &World, entity: Entity) -> [f32; 3] {
    world
        .get::<Transform>(entity)
        .expect("still there")
        .rotation
}

#[test]
fn the_eyes_sit_at_the_authored_height() {
    // The component is the authority and the transform is derived from it, so a scene that authored
    // neither still gets a camera at eye level rather than at the parent's feet.
    let (mut world, _, eyes) = head();
    place_first_person(&mut world);

    assert_eq!(
        world
            .get::<Transform>(eyes)
            .expect("still there")
            .translation,
        [0.0, FirstPersonCamera::default().height, 0.0]
    );
}

#[test]
fn changing_the_height_moves_the_eyes() {
    // What a crouch would be, and the reason the height is not authored on the transform: one place
    // decides, so nothing has to fight anything else about it.
    let (mut world, _, eyes) = head();
    world.insert(
        eyes,
        FirstPersonCamera {
            height: 0.2,
            ..FirstPersonCamera::default()
        },
    );
    place_first_person(&mut world);

    assert_eq!(
        world
            .get::<Transform>(eyes)
            .expect("still there")
            .translation[1],
        0.2
    );
}

#[test]
fn turning_rotates_the_body_and_tilting_rotates_only_the_head() {
    // **The split that makes first person work.** Yaw has to go on the parent or moving forward
    // would not go where you are looking; pitch has to stay on the camera or the character leans
    // over and walks into the floor.
    let (mut world, body, eyes) = head();
    look(&mut world, 10.0, 4.0);

    assert!(
        rotation(&world, body)[1] < 0.0,
        "the body should have turned"
    );
    assert_eq!(rotation(&world, body)[0], 0.0, "the body must not pitch");

    assert!(
        rotation(&world, eyes)[0] < 0.0,
        "the head should have tilted"
    );
    assert_eq!(rotation(&world, eyes)[1], 0.0, "the head must not yaw");
}

#[test]
fn the_pointer_turns_the_view_the_way_it_moved() {
    // Two sign conventions, both easy to invert and neither of which crashes when wrong. Moving the
    // pointer right looks right; pushing it away from you — which is *negative* in a window, where y
    // grows downward — raises the view.
    let (mut world, body, eyes) = head();

    look(&mut world, 5.0, 0.0);
    let turned_right = rotation(&world, body)[1];
    assert!(turned_right < 0.0, "got {turned_right}");

    look(&mut world, -10.0, 0.0);
    assert!(
        rotation(&world, body)[1] > turned_right,
        "moving the pointer back should turn back the other way"
    );

    look(&mut world, 0.0, -5.0);
    assert!(
        rotation(&world, eyes)[0] > 0.0,
        "pushing the pointer away should look up, got {}",
        rotation(&world, eyes)[0]
    );
}

#[test]
fn the_view_cannot_be_tilted_past_vertical() {
    // ADR 0018's gimbal problem arriving somewhere concrete, and it bites harder here than in third
    // person: a first-person view is *expected* to look nearly straight up, so this clamp is the
    // only thing between a player and a rolling horizon.
    let (mut world, _, eyes) = head();
    let limits = FirstPersonCamera::default();

    for _ in 0..40 {
        look(&mut world, 0.0, 100.0);
    }
    assert_eq!(rotation(&world, eyes)[0], limits.min_pitch);

    for _ in 0..80 {
        look(&mut world, 0.0, -100.0);
    }
    assert_eq!(rotation(&world, eyes)[0], limits.max_pitch);

    assert!(
        limits.min_pitch > -90.0 && limits.max_pitch < 90.0,
        "both limits must stop short of vertical"
    );
}

#[test]
fn nothing_moves_without_the_look_button() {
    // The same gate the third-person rig uses. A first-person game holds it permanently — see the
    // module docs — which is a decision the *game* makes rather than one this rig makes for it.
    let (mut world, body, eyes) = head();
    if let Some(input) = world.resource_mut::<InputState>() {
        input.set_axis(ActionId::new(LOOK_X), 50.0);
        input.set_axis(ActionId::new(LOOK_Y), 50.0);
    }
    look_with_mouse(&mut world);

    assert_eq!(rotation(&world, body), [0.0; 3]);
    assert_eq!(rotation(&world, eyes), [0.0; 3]);
}

#[test]
fn placing_the_eyes_does_not_undo_the_tilt() {
    // The two systems write the same `Transform` — one its rotation, the other its translation — and
    // the obvious implementation of the second overwrites the whole thing. That would leave a view
    // that snaps level every tick, which looks like the mouse not working rather than like an
    // ordering bug.
    let (mut world, _, eyes) = head();
    look(&mut world, 0.0, 20.0);

    let tilted = rotation(&world, eyes)[0];
    assert!(tilted < 0.0, "the control case");

    place_first_person(&mut world);
    assert_eq!(rotation(&world, eyes)[0], tilted);
}

#[test]
fn a_world_with_no_first_person_camera_is_left_alone() {
    // The headless and third-person cases, which is every game that does not use this rig.
    let mut world = World::new();
    world.insert_resource(InputState::new());
    let lonely = world.spawn();
    world.insert(lonely, Transform::at(3.0, 4.0));

    place_first_person(&mut world);
    look_with_mouse(&mut world);

    assert_eq!(
        world
            .get::<Transform>(lonely)
            .expect("still there")
            .translation,
        [3.0, 4.0, 0.0]
    );
}
