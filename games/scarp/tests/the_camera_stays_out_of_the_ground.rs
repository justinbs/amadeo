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

use amadeo_app::{App, Stage};
use amadeo_camera::{CameraArm, FollowCamera};
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

/// A game whose view is being dragged vertically for the whole run: positive tilts down.
///
/// The drag is deliberately far larger than the pitch limits, so the view saturates against
/// [`FollowCamera::min_pitch`] or [`FollowCamera::max_pitch`] within a few ticks and then holds
/// there. That makes "looking as far down as this camera can" a fixed, authored angle rather than
/// something that depends on how long the test runs.
///
/// Scripted from tick 2 and never afterwards, because [`ScriptedSource`] schedules against
/// **absolute** ticks — installing a source at tick 0 into a world that has already run is a script
/// whose events are all in the past, and the symptom is a test where nothing moves.
fn looking(drag: f32) -> App {
    let mut app = build_simulation().expect("the game builds");
    let mut source = ScriptedSource::new();
    source.press(Tick(2), amadeo_camera::LOOK, true);
    source.axis(Tick(2), amadeo_camera::LOOK_Y, drag);
    amadeo_input::install(&mut app.world, InputDriver::new(Box::new(source)));
    app
}

/// How far the camera is from its pivot, along the arm.
///
/// **Read from [`CameraArm`] rather than from the transform's local `z`, and that distinction is new
/// in session 15.** Before the camera orbited, the two were the same number and the coordinate was a
/// fair proxy. Now the arm leans with pitch, so local `z` is `distance × cos(pitch)` — at the Scarp's
/// authored −16° tilt it reads 6.73 for an arm of 7.0, which is close enough to pass a tolerance and
/// wrong enough to make the test mean something else.
fn camera_distance(world: &World) -> f32 {
    world
        .query::<(&CameraArm,)>()
        .map(|(_, (arm,))| arm.distance)
        .next()
        .expect("the scene authors one follow camera")
}

/// The camera's local position relative to the player it hangs off.
fn camera_local(world: &World) -> [f32; 3] {
    world
        .query::<(&Camera, &Parent, &Transform)>()
        .map(|(_, (_, _, transform))| transform.translation)
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
    // # This test used to force the obstruction with a four-metre sphere, and it was measuring the
    // # wrong thing
    //
    // A sphere that wide, centred on the pivot three metres up, **contains the player's own capsule**
    // — so what it hit first was always the player, never the ground, and it reported an immediate
    // block. It asserted the right answer for the wrong reason for as long as it existed, and the
    // reason it stopped was session 15 telling both sweeps to ignore the followed body. That
    // penetration *was* the flicker (see `walking_over_open_ground_does_not_disturb_the_arm`), so a
    // test relying on it was a test relying on the bug.
    //
    // What forces it now is geometry rather than a fat probe: **raise the upward pitch limit and
    // look all the way up**. The arm swings down as it swings back, so at 89° a seven-metre arm ends
    // up very nearly seven metres *below* the pivot and a few centimetres behind it — which is under
    // the player's feet and inside solid ground on any terrain and any seed. An ordinary 0.35 m
    // probe finds that.
    //
    // The Scarp authors a 30° limit precisely so this cannot happen in play; raising it here is
    // asking the rig what it does when a game authors a generous one, which is a thing a game may
    // legitimately do.
    let level = {
        let mut app = game();
        app.run_ticks(300).expect("the world advances");
        camera_distance(&app.world)
    };

    let mut app = looking(-300.0);
    let wanted = follow(&app.world);
    set_follow(
        &mut app.world,
        FollowCamera {
            max_pitch: 89.0,
            ..wanted
        },
    );
    app.run_ticks(300).expect("the world advances");

    let blocked = camera_distance(&app.world);

    assert!(
        camera_pitch(&app.world) > 80.0,
        "the view has to have actually tilted up and over for this to be testing anything, got a \
         pitch of {}",
        camera_pitch(&app.world)
    );
    assert!(
        blocked < level - 0.5,
        "swinging the arm down into the ground must pull the camera in from the {level} it has in \
         the open, got {blocked}"
    );
    assert!(
        blocked >= wanted.min_distance - 0.01,
        "but never past the authored minimum of {}, got {blocked}",
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

/// Where the pivot ended up, as a height above the player.
///
/// Recovered rather than read: the camera's local `y` is the pivot **plus** the arm's vertical lean,
/// so subtracting the lean back off is what isolates the thing the sweep decided. Doing this in the
/// test rather than exposing a field keeps the pivot an implementation detail of one system.
fn pivot_height(world: &World) -> f32 {
    let local = camera_local(world);
    let distance = camera_distance(world);
    let pitch = camera_pitch(world);
    let (sin_pitch, _) = amadeo_core::sin_cos_degrees(pitch);
    local[1] - (-sin_pitch) * distance
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
    let open = pivot_height(&app.world);
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

    let blocked = pivot_height(&app.world);
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
fn tilting_the_view_swings_the_camera_round_its_pivot() {
    // **The second thing Justin reported in session 15**, and the one that was a design gap rather
    // than a bug: *"pointing the camera downwards I end up looking at the ground, upwards I end up
    // looking upwards from where the camera is"*.
    //
    // The camera's position was the constant `[0, height, distance]` and pitch reached it nowhere,
    // so tilting spun the camera on the spot and the player left the frame. What a third-person
    // camera has to do instead is **orbit**: tilt down and it rises so it looks down *at* what it
    // follows; tilt up and it drops so it looks up past it.
    //
    // Asserting on the direction of travel rather than on a position, because the exact height
    // depends on the arm length and the arm length depends on what the sweep found — and the claim
    // being made here is about the shape of the motion, not about a number.
    // Three worlds from the same seed, differing only in what the mouse did — so any difference in
    // where the camera ends up is attributable to the tilt and to nothing else.
    let mut level_app = game();
    let mut down = looking(300.0);
    let mut up = looking(-300.0);
    for app in [&mut level_app, &mut down, &mut up] {
        app.run_ticks(300).expect("the world advances");
    }

    let level = camera_local(&level_app.world);
    let looking_down = camera_local(&down.world);
    let looking_up = camera_local(&up.world);

    assert!(
        camera_pitch(&down.world) < camera_pitch(&up.world),
        "the two cases must actually have tilted in opposite directions"
    );
    assert!(
        looking_down[1] > level[1] + 1.0,
        "tilting down must lift the camera above where it was ({} to {}), so it looks down at the \
         player rather than at the ground under itself",
        level[1],
        looking_down[1]
    );
    assert!(
        looking_up[1] < level[1] - 1.0,
        "and tilting up must drop it ({} to {}), so it looks up past the player",
        level[1],
        looking_up[1]
    );
    // The other half of an orbit: swinging up and over shortens the *horizontal* reach, because the
    // arm is a fixed length rather than a fixed offset. Without this the test would also pass for a
    // camera that simply slid up and down a pole.
    assert!(
        looking_down[2] < level[2] - 1.0,
        "swinging overhead must bring the camera closer in plan view ({} to {}), which is what \
         makes it an orbit rather than a lift",
        level[2],
        looking_down[2]
    );
}

#[test]
fn the_camera_still_looks_at_what_it_follows_at_every_tilt() {
    // The property that makes the orbit worth having: wherever the camera ends up, the thing it
    // follows is in front of it. That is what "the player stays in frame at every angle" *is*, and
    // nothing in the two position assertions above actually checks it.
    //
    // # The version of this test that proves nothing
    //
    // Deriving the arm direction and the forward direction from the same pitch value and dotting
    // them is a check on trigonometry, not on the rig — it passes against a camera left at the
    // origin. What makes it real is taking the arm from **where the system actually put the camera**
    // (its position, minus the pivot) and the forward from **the rotation nothing but
    // `look_with_mouse` writes**, so the two come from opposite ends of the system and can disagree.
    for drag in [0.0f32, 300.0, -300.0, 90.0] {
        let mut app = looking(drag);
        app.run_ticks(300).expect("the world advances");

        let pitch = camera_pitch(&app.world);
        let local = camera_local(&app.world);
        let pivot = pivot_height(&app.world);

        // Measured: from the pivot out to wherever the camera was placed.
        let mut arm = [local[0], local[1] - pivot, local[2]];
        let length = (arm[0] * arm[0] + arm[1] * arm[1] + arm[2] * arm[2]).sqrt();
        assert!(
            length > 0.5,
            "the camera has to be somewhere for this to mean anything, arm length was {length}"
        );
        for axis in &mut arm {
            *axis /= length;
        }

        // Declared: the camera's own forward, which is its local −Z turned by its pitch.
        let (sin_pitch, cos_pitch) = amadeo_core::sin_cos_degrees(pitch);
        let forward = [0.0, sin_pitch, -cos_pitch];

        let facing = arm[0] * forward[0] + arm[1] * forward[1] + arm[2] * forward[2];
        assert!(
            (facing + 1.0).abs() < 1e-4,
            "at a pitch of {pitch} the camera sits along {arm:?} from its pivot while facing \
             {forward:?}. Those must be exactly opposed for the pivot to be in the middle of the \
             frame; the dot product was {facing} rather than -1"
        );
    }
}

#[test]
fn walking_over_open_ground_does_not_disturb_the_arm() {
    // **The flicker, reproduced the way Justin met it** — by walking — rather than the way the
    // previous regression test forced it.
    //
    // That test alternates the sweep radius between "everything is in the way" and "nothing is" to
    // swing the target, and it is still here and still worth having, because it tests the response
    // to a swinging target. But it passed throughout the entire time the bug was present, because
    // the bug was not in the response. It was in the *input*: the upward sweep to the pivot started
    // inside the player's own capsule, and rapier read that penetration as a slope too steep to
    // stand on, reported `sliding_down_slope`, and cancelled the motion — on about one tick in ten,
    // depending on the exact contact normal, which moves as the player walks.
    //
    // So the pivot collapsed to the player's feet, the camera snapped to its minimum, eased out at
    // 0.1 m per tick, and was knocked down again long before covering the 5.8 m back. What that
    // looks like is a camera that never settles.
    //
    // The assertion is that walking across ordinary open ground disturbs the arm **not at all**.
    // Watched failing against the old code, where it reported a swing of 5.8 metres in one tick.
    let mut app = build_simulation().expect("the game builds");
    let mut source = ScriptedSource::new();
    source.axis(Tick(120), MOVE_FORWARD, 1.0);
    amadeo_input::install(&mut app.world, InputDriver::new(Box::new(source)));

    // Long enough to land, and then long enough for the arm to have reached its full extent.
    app.run_ticks(300).expect("the world advances");

    let wanted = follow(&app.world);
    let settled = camera_distance(&app.world);
    assert!(
        (settled - wanted.distance).abs() < 0.01,
        "the camera should have reached its authored {} units before this test starts measuring, \
         got {settled} — if it has not, the arm is being knocked down faster than it can ease out",
        wanted.distance
    );

    let mut previous = settled;
    let mut worst = 0.0f32;
    for _ in 0..120 {
        app.run_ticks(1).expect("the world advances");
        let now = camera_distance(&app.world);
        worst = worst.max((now - previous).abs());
        previous = now;
    }

    assert!(
        worst < 0.05,
        "walking over open ground moved the camera arm by {worst} in a single tick. Nothing is in \
         the way out here, so the arm should not move at all"
    );
}

#[test]
fn the_camera_reads_the_parent_after_it_has_moved() {
    // **An ordering that is correct today and declared nowhere**, which `amadeo_camera::install`
    // explains and deliberately does not fix.
    //
    // `keep_camera_clear` reads the player's transform to place the camera, and `drive_characters`
    // is what writes it. Both are `.after(step_physics)` and neither is ordered against the other,
    // so the schedule falls back to alphabetical — which happens to put them the right way round.
    //
    // Declaring it would mean `modules/amadeo-camera` naming a system in `modules/amadeo-character`,
    // and trap 10 is explicit that a camera rig must not assume a character exists at all. So the
    // constraint lives here instead, in the one place that knows both modules are installed: the
    // game. A rename on either side turns this red rather than producing a camera that trails the
    // player by a tick, which is a symptom nobody would attribute to a schedule.
    let mut app = build_simulation().expect("the game builds");
    let order = app
        .resolved_order(Stage::Simulation)
        .expect("the schedule resolves");

    let mover = order
        .iter()
        .position(|label| *label == amadeo_character::DRIVE_CHARACTERS)
        .expect("the character module is installed");
    let camera = order
        .iter()
        .position(|label| *label == amadeo_camera::KEEP_CAMERA_CLEAR)
        .expect("the camera module is installed");

    assert!(
        mover < camera,
        "'{}' must run before '{}' or the camera follows where the player was last tick. \
         Resolved order was {order:?}",
        amadeo_character::DRIVE_CHARACTERS,
        amadeo_camera::KEEP_CAMERA_CLEAR
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
