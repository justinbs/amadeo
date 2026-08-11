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
use amadeo_camera::FollowCamera;
use amadeo_character::MOVE_FORWARD;
use amadeo_core::Tick;
use amadeo_ecs::World;
use amadeo_input::{InputDriver, ScriptedSource};
use amadeo_render::Camera;
use amadeo_transform::{Parent, Transform};
use scarp::build_simulation;

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

/// The player's yaw, in degrees.
fn player_yaw(world: &World) -> f32 {
    world
        .query::<(&amadeo_terrain::TerrainViewer, &Transform)>()
        .map(|(_, (_, transform))| transform.rotation[1])
        .next()
        .expect("the scene authors one player")
}

/// The camera's pitch, in degrees.
fn camera_pitch(world: &World) -> f32 {
    world
        .query::<(&FollowCamera, &Transform)>()
        .map(|(_, (_, transform))| transform.rotation[0])
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
fn the_camera_does_not_jump_between_distances_from_tick_to_tick() {
    // **The flicker Justin reported**, which the first version of this system had and which its
    // three tests all passed through.
    //
    // Two causes, both fixed. `move_shape` *slides* along what it hits, so measuring the
    // straight-line distance travelled counted a sideways slide as progress — a camera brushing a
    // slope got a distance with little to do with the axis it was pointed along, and it swung as the
    // player moved. And the distance snapped in both directions, so noise in the sweep near an edge
    // showed up directly as movement.
    //
    // Asserting on the *change* per tick rather than on any particular distance, because what makes
    // this a bug is the jumping rather than the value. The ease-out rate is the cap in the outward
    // direction; inward is allowed to snap, so the bound is one-sided.
    // **The swing is forced rather than waited for**, and that is deliberate. Walking the player
    // across open hillside was tried first and did not reproduce it: the camera is simply not
    // obstructed often enough out there, so the target sat at its authored distance and the test
    // passed with the easing removed — proving nothing.
    //
    // Alternating the sweep radius between "everything is in the way" and "nothing is" drives the
    // target from the minimum to the maximum and back on every single tick, which is a harsher
    // version of what noisy geometry does and needs no particular terrain to arrange. What is being
    // tested is the response to a swinging target, and this swings it.
    let mut app = build_simulation().expect("the game builds");
    let mut source = ScriptedSource::new();
    source.axis(Tick(120), MOVE_FORWARD, 1.0);
    amadeo_input::install(&mut app.world, InputDriver::new(Box::new(source)));

    app.run_ticks(240).expect("the world advances");

    let wanted = follow(&app.world);
    let mut previous = camera_distance(&app.world);
    let mut worst_jump_outward = 0.0f32;

    for tick in 0..120 {
        set_follow(
            &mut app.world,
            FollowCamera {
                radius: if tick % 2 == 0 { 4.0 } else { 0.05 },
                ..wanted
            },
        );
        app.run_ticks(1).expect("the world advances");

        let now = camera_distance(&app.world);
        worst_jump_outward = worst_jump_outward.max(now - previous);
        previous = now;
    }

    // One tick of easing, plus a little slack for floating point.
    let allowed = wanted.return_speed * amadeo_core::FIXED_DT + 1e-3;
    assert!(
        worst_jump_outward <= allowed,
        "the camera moved outward by {worst_jump_outward} in one tick, which is more than the \
         {allowed} the ease-out allows — it is snapping rather than easing, which is what the \
         flicker was"
    );
}

#[test]
fn the_mouse_turns_the_player_and_tilts_only_the_camera() {
    // **The split that makes a third-person view feel right**, and the thing most likely to be wired
    // the wrong way round. Yaw belongs on the player, so walking forward goes where you are looking
    // and the camera comes round for free as a child entity. Pitch belongs on the camera alone — a
    // character that pitched would lean over and walk into the ground.
    //
    // Driven through the scripted source rather than by writing `InputState`, because `sample_input`
    // rebuilds that resource every tick in `PreSimulation` and a poked value never survives to be
    // read. The same path a real mouse takes.
    let mut app = build_simulation().expect("the game builds");
    let mut source = ScriptedSource::new();
    source.press(Tick(2), amadeo_camera::LOOK, true);
    source.axis(Tick(2), amadeo_camera::LOOK_X, 100.0);
    source.axis(Tick(2), amadeo_camera::LOOK_Y, 50.0);
    amadeo_input::install(&mut app.world, InputDriver::new(Box::new(source)));

    let yaw_before = player_yaw(&app.world);
    let pitch_before = camera_pitch(&app.world);

    app.run_ticks(4).expect("the world advances");

    let yaw_after = player_yaw(&app.world);
    let pitch_after = camera_pitch(&app.world);

    // Moving the mouse right turns the view right, which is *negative* yaw — the character's own
    // turn axis is positive for left.
    assert!(
        yaw_after < yaw_before - 1.0,
        "dragging right should turn the player right: yaw went from {yaw_before} to {yaw_after}"
    );
    // And down should tilt the view down, which is negative pitch.
    assert!(
        pitch_after < pitch_before - 1.0,
        "dragging down should tilt the camera down: pitch went from {pitch_before} to {pitch_after}"
    );
}

#[test]
fn the_camera_cannot_be_tilted_past_vertical() {
    // Clamped short of straight up and straight down, because at exactly vertical the camera's
    // forward direction is parallel to the world up its basis is built from, the basis collapses,
    // and the view rolls unpredictably. Driven with an absurd drag so the clamp is what stops it
    // rather than the drag running out.
    let mut app = build_simulation().expect("the game builds");
    let mut source = ScriptedSource::new();
    source.press(Tick(2), amadeo_camera::LOOK, true);
    source.axis(Tick(2), amadeo_camera::LOOK_Y, 4000.0);
    amadeo_input::install(&mut app.world, InputDriver::new(Box::new(source)));

    app.run_ticks(30).expect("the world advances");

    let pitch = camera_pitch(&app.world);
    assert!(
        (-90.0..=90.0).contains(&pitch),
        "the camera must never tip past vertical, got {pitch}"
    );
    assert!(
        pitch > -89.0,
        "and should stop short of it rather than sitting exactly there, got {pitch}"
    );
}

/// The camera's height above the player — its local `y`.
fn camera_height(world: &World) -> f32 {
    world
        .query::<(&Camera, &Parent, &Transform)>()
        .map(|(_, (_, _, transform))| transform.translation[1])
        .next()
        .expect("the scene authors one follow camera")
}

#[test]
fn the_pivot_is_swept_for_too_rather_than_assumed_clear() {
    // **The gap the other five tests all pass straight through**, because every one of them has the
    // player standing in the open where the point three metres above their head is fresh air.
    //
    // In a tunnel it is rock. A shape cast that *starts* embedded has no good answer — solvers
    // differ on whether they report an immediate hit, no hit, or push out — so the distance coming
    // back is arbitrary, and arbitrary per tick is exactly the flicker this system exists to remove.
    //
    // Forced the same way the flicker test forces its swing: a sweep radius large enough that
    // nothing above the player is clear. The pivot must then come down rather than staying at its
    // authored height.
    let mut app = game();
    app.run_ticks(240).expect("the world advances");

    let wanted = follow(&app.world);
    let open = camera_height(&app.world);
    assert!(
        (open - wanted.height).abs() < 0.5,
        "in the open the pivot should reach its authored height of {}, got {open}",
        wanted.height
    );

    set_follow(
        &mut app.world,
        FollowCamera {
            radius: 4.0,
            ..wanted
        },
    );
    app.run_ticks(4).expect("the world advances");

    let blocked = camera_height(&app.world);
    assert!(
        blocked < open,
        "with nothing above the player clear, the pivot must come down from {open}, got {blocked}"
    );
    assert!(
        blocked >= -0.01,
        "but never below the player it is following, got {blocked}"
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
