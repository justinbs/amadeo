//! **Q27**, driven headlessly: a third-person camera must not end up inside the world.
//!
//! # Why the bug looked like something else entirely
//!
//! It was reported as *"digging down shows the sky"*, and that is exactly what it looks like — but
//! the hole is not the cause. Surface nets meshes only the boundary between solid and air, so solid
//! rock contains **no geometry at all**, and the boundary's faces point outward so from underneath
//! they are backface-culled and vanish. Put the camera under the terrain and it looks straight
//! through the world to the skybox.
//!
//! So this asserts on where the camera *is*, not on what it draws. That is the honest level: no
//! amount of pixel checking would distinguish "the camera is underground" from "the terrain failed
//! to stream", which is precisely the confusion the bug caused.

use amadeo_app::App;
use amadeo_ecs::World;
use amadeo_input::{InputDriver, ScriptedSource};
use amadeo_render::Camera;
use amadeo_transform::{Parent, Transform};
use scarp::{FollowCamera, build_simulation};

fn game() -> App {
    let mut app = build_simulation().expect("the game builds");
    amadeo_input::install(
        &mut app.world,
        InputDriver::new(Box::new(ScriptedSource::new())),
    );
    app
}

/// The camera's distance behind the player — its local `z`, since it is a child entity.
fn camera_distance(world: &World) -> f32 {
    world
        .query::<(&Camera, &Parent, &Transform)>()
        .map(|(_, (_, _, transform))| transform.translation[2])
        .next()
        .expect("the scene authors one follow camera")
}

/// What the camera asked for, as authored.
fn follow(world: &World) -> FollowCamera {
    world
        .query::<(&FollowCamera,)>()
        .map(|(_, (follow,))| *follow)
        .next()
        .expect("the scene authors one follow camera")
}

/// Replaces the follow settings on whichever entity carries them.
fn set_follow(world: &mut World, settings: FollowCamera) {
    let entities: Vec<_> = world
        .query::<(&FollowCamera,)>()
        .map(|(entity, _)| entity)
        .collect();
    for entity in entities {
        world.insert(entity, settings);
    }
}

#[test]
fn the_camera_keeps_its_authored_distance_in_the_open() {
    // The control case, and it has to hold or the fix would be "pull the camera in always", which
    // would look like a bug of its own — a third-person camera glued to the player's back.
    let mut app = game();
    let wanted = follow(&app.world);

    // Long enough for the ground to stream in and the player to land on it.
    app.run_ticks(240).expect("the world advances");

    let distance = camera_distance(&app.world);
    assert!(
        (distance - wanted.distance).abs() < 0.5,
        "standing in the open on a hillside, the camera should sit at about its authored {} units \
         back, got {distance}",
        wanted.distance
    );
}

#[test]
fn the_camera_is_pulled_in_when_the_sweep_hits_something() {
    // **The test that proves the sweep is consulted at all**, which the one above cannot: if
    // `move_shape` were never called, or were called before `step_physics` and so queried an empty
    // world, the camera would keep its authored distance and that test would still pass.
    //
    // A sphere wide enough that the ground is unavoidably in the way. There is no camera placement
    // seven units behind a character standing on terrain that a four-metre sphere does not
    // intersect, so the only correct answer is the minimum.
    let mut app = game();
    app.run_ticks(240).expect("the world advances");

    let open = camera_distance(&app.world);

    let wanted = follow(&app.world);
    set_follow(
        &mut app.world,
        FollowCamera {
            radius: 4.0,
            ..wanted
        },
    );
    app.run_ticks(4).expect("the world advances");

    let blocked = camera_distance(&app.world);
    assert!(
        blocked < open,
        "a four-metre sweep must find the ground and pull the camera in from {open}, got {blocked}"
    );
    assert!(
        (blocked - wanted.min_distance).abs() < 0.01,
        "and pull it all the way to the authored minimum of {}, got {blocked}",
        wanted.min_distance
    );
}

#[test]
fn the_camera_never_goes_closer_than_its_minimum() {
    // The other end of the clamp. A camera pulled to the pivot sits inside the thing it follows,
    // which is its own kind of wrong — and with the near plane at 0.1 it would also start clipping
    // through the character.
    let mut app = game();
    let wanted = follow(&app.world);
    set_follow(
        &mut app.world,
        FollowCamera {
            // Absurd, so the sweep is blocked from the very first millimetre.
            radius: 20.0,
            ..wanted
        },
    );
    app.run_ticks(240).expect("the world advances");

    let distance = camera_distance(&app.world);
    assert!(
        distance >= wanted.min_distance - 0.01,
        "the camera must never come closer than {}, got {distance}",
        wanted.min_distance
    );
}
