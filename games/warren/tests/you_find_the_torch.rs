//! The Warren's spine: stand in a room, look at a thing, take it, and the beam lights.
//!
//! # What is actually being proved
//!
//! Not that interaction works — `modules/amadeo-interaction` has its own tests for that. This is the
//! claim one level up, and it is the reason this game exists before its level design does: that the
//! **first-person** arrangement composes. The `Interactor` is on the camera, the camera is a child
//! of the character, the pitch comes from the mouse, and the bag is on the character. Every one of
//! those is a seam, and `games/atrium` exercises none of them because it is third person.
//!
//! # Against the handcrafted room, deliberately
//!
//! The game boots into a generated level and `the_level_is_a_level.rs` plays that one. What is being
//! proved here is an *arrangement*, not a level, and the handcrafted room is the one whose
//! coordinates are written down — "walk north for four hundred ticks and hit a wall" is a sentence
//! about a seam only when the room has a wall to the north. In a generated interior it is a sentence
//! about which side the generator happened to put a doorway on.

use amadeo_app::App;
use amadeo_camera::{FirstPersonCamera, LOOK, LOOK_Y};
use amadeo_character::MOVE_FORWARD;
use amadeo_core::Tick;
use amadeo_ecs::Entity;
use amadeo_input::{ActionId, InputDriver, InputState, ScriptedSource};
use amadeo_interaction::{Interactor, Looking, USE};
use amadeo_inventory::Item;
use amadeo_render::SpotLight;
use amadeo_transform::Transform;
use warren::{BEAM_INTENSITY, TORCH, eyes, holding_torch, is_stored, player, prompt};

fn room() -> App {
    let mut app = warren::build_handcrafted().expect("the room builds");
    amadeo_input::install(
        &mut app.world,
        InputDriver::new(Box::new(ScriptedSource::new())),
    );
    // Past the title screen; see `the_run_can_end.rs` for why every builder here does this.
    if let Some(screen) = app.world.resource_mut::<warren::Screen>() {
        *screen = warren::Screen::Playing;
    }
    app
}

fn torch(app: &App) -> Entity {
    app.world
        .query::<(&Item,)>()
        .filter(|(_, (item,))| item.kind == TORCH)
        .map(|(entity, _)| entity)
        .next()
        .expect("the scene puts a torch in the room")
}

/// The torch beam, found by **what it is attached to** rather than by being the only spot light.
///
/// It used to be `the first SpotLight in the world`, which was true while the beam was the only one.
/// Session 23 made the emergency fittings spots too — a point light 0.3 m off its own mounting wall
/// clips it to white whatever its intensity, and a spot aimed down and away pools instead — and this
/// silently started answering about a fitting. The symptom was `the_beam_starts_dark` reporting an
/// intensity of 7.0, which reads as "the torch is on before you pick it up" and is not what broke.
fn beam(app: &App) -> Entity {
    let eyes = eyes(&app.world).expect("an interactor");
    app.world
        .query::<(&SpotLight, &amadeo_transform::Parent)>()
        .filter(|(_, (_, parent))| parent.0 == eyes)
        .map(|(entity, _)| entity)
        .next()
        .expect("the scene puts a beam on the camera")
}

fn beam_intensity(app: &App) -> f32 {
    app.world
        .get::<SpotLight>(beam(app))
        .expect("still there")
        .intensity
}

/// Walks forward for `ticks` ticks. An axis, so writing the resource is enough.
fn walk(app: &mut App, ticks: u64) {
    for _ in 0..ticks {
        if let Some(state) = app.world.resource_mut::<InputState>() {
            state.set_axis(ActionId::new(MOVE_FORWARD), 1.0);
        }
        app.run_ticks(1).expect("a tick runs");
    }
}

/// Aims the view down by driving the same named axis a mouse would.
///
/// **This is the thing an authored pitch cannot do**, and the reason the interactor is on the
/// camera: the sweep follows the camera's forward, so looking down aims it down.
fn look_down(app: &mut App, amount: f32, ticks: u64) {
    for _ in 0..ticks {
        if let Some(state) = app.world.resource_mut::<InputState>() {
            // **`LOOK` has to be held**, and a first-person game holds it permanently — the module
            // gates on it so that a third-person game can keep a free cursor until you grab the
            // view. Forgetting it is silent: the axes arrive and nothing turns.
            state.set_button(ActionId::new(LOOK), true);
            state.set_axis(ActionId::new(LOOK_Y), amount);
        }
        app.run_ticks(1).expect("a tick runs");
    }
    if let Some(state) = app.world.resource_mut::<InputState>() {
        state.set_axis(ActionId::new(LOOK_Y), 0.0);
    }
}

/// Presses "use" for one tick, through the source — it is edge-triggered.
fn tap_use(app: &mut App) {
    let now = app.tick();
    let release = Tick(now.0 + 1);
    app.world
        .with_service_taken::<InputDriver, ()>(|_world, driver| {
            if let Some(scripted) = driver.source.as_any_mut().downcast_mut::<ScriptedSource>() {
                scripted.press(now, USE, true);
                scripted.press(release, USE, false);
            }
        });
    app.run_ticks(2).expect("ticks run");
}

#[test]
fn the_room_loads_and_the_player_is_first_person() {
    let app = room();
    let player = player(&app.world).expect("a character");
    let eyes = eyes(&app.world).expect("an interactor");

    assert_ne!(
        eyes, player,
        "the interactor is on the camera, not the body"
    );
    assert!(
        app.world.get::<FirstPersonCamera>(eyes).is_some(),
        "and the camera is the first-person rig, which no game had used before this one"
    );
    // Two bore sections (deck + crown), four side walls, two bulkheads, a cross-passage and its
    // blind cap, two fittings (shade + tube each), a section plate in four parts (surround, plate,
    // rule, letter), two bunk frames with two mattresses on one of them, two crates, the torch, the
    // key, the door, the warden and the lamp it carries. The lights themselves are not geometry, and
    // the player has no body mesh -- in first person you would be standing inside it.
    assert_eq!(app.world.query::<(&amadeo_render::Mesh,)>().count(), 31);
}

#[test]
fn the_beam_starts_dark() {
    // Authored at zero in the scene rather than absent, so there is one light that switches rather
    // than two lights that have to agree.
    let app = room();
    assert_eq!(beam_intensity(&app), 0.0);
    assert!(!holding_torch(&app.world));
}

#[test]
fn you_can_stand_in_the_room_without_falling_through_it() {
    let mut app = room();
    app.run_ticks(30).expect("ticks run");

    let player = player(&app.world).expect("a character");
    let at = app
        .world
        .get::<Transform>(player)
        .expect("still there")
        .translation;
    assert!(
        at[1] > 0.5,
        "the character should be standing on the floor, not falling: {at:?}"
    );
}

#[test]
fn the_walls_keep_you_in_the_room() {
    // Cheap and worth having in a game that is *made of* corridors: against `NullPhysics` the
    // character walks through everything, so this is also what says the solver is actually wired up.
    let mut app = room();
    walk(&mut app, 400);

    let player = player(&app.world).expect("a character");
    let at = app
        .world
        .get::<Transform>(player)
        .expect("still there")
        .translation;

    // The north wall's inner face is at z = -7.85, and the capsule's radius is 0.35.
    assert!(
        at[2] > -7.9,
        "four hundred ticks of walking forward should have been stopped by the north wall, \
         not passed through it: {at:?}"
    );
    assert!(
        at[2] < 0.0,
        "and it should have got most of the way: {at:?}"
    );
}

/// Stands the player just short of the crate the torch sits on, facing it.
///
/// Placed rather than walked. Where the crate sits is a level-design number that will change as the
/// room does, and a test that walked there would fail every time somebody moved the furniture — for
/// a reason that has nothing to do with what it is checking.
fn stand_before_the_torch(app: &mut App) {
    let player = player(&app.world).expect("a character");
    if let Some(transform) = app.world.get_mut::<Transform>(player) {
        // The crate is at x = -1.6, z = 1.0. Facing -Z, which is forward at yaw zero.
        transform.translation = [-1.6, 1.0, 2.6];
    }
    app.run_ticks(1).expect("a tick runs");
}

#[test]
fn looking_down_at_the_torch_offers_the_prompt_the_scene_authored() {
    let mut app = room();
    stand_before_the_torch(&mut app);

    // The torch sits at eye level minus about 0.6 m, a metre and a half ahead, so a level sweep
    // passes over it — the whole reason the interactor is on a camera that can pitch.
    look_down(&mut app, 20.0, 6);

    assert_eq!(
        prompt(&app.world).as_deref(),
        Some("Take the torch"),
        "the prompt has to come from the scene file rather than from code, and the view has to be \
         able to aim down at all"
    );
}

#[test]
fn pressing_use_while_looking_at_it_takes_it_through_the_real_input_path() {
    // The whole chain, driven the way a player drives it: a named action, sampled from a source,
    // read as an edge, turned into an event, turned into a pickup, turned into light. Every other
    // test here shortcuts one of those.
    let mut app = room();
    let torch = torch(&app);
    stand_before_the_torch(&mut app);
    look_down(&mut app, 20.0, 6);

    assert!(!holding_torch(&app.world), "not yet");
    tap_use(&mut app);

    assert!(holding_torch(&app.world), "F should have taken it");
    assert!(is_stored(&app.world, torch));
    assert_eq!(beam_intensity(&app), BEAM_INTENSITY);
}

#[test]
fn taking_the_torch_lights_the_beam_and_takes_it_out_of_the_room() {
    // Driven directly rather than by walking the player into position: where the crate sits is a
    // level-design number that will change, and a test that depends on it would break every time
    // somebody moved a crate. What is being proved is the chain from `Interacted` to a lit beam.
    let mut app = room();
    let player = player(&app.world).expect("a character");
    let torch = torch(&app);

    amadeo_inventory::store(&mut app.world, torch, player).expect("the bag has room");
    app.run_ticks(1).expect("a tick runs");

    assert!(holding_torch(&app.world));
    assert!(
        is_stored(&app.world, torch),
        "and it is out of the world -- ADR 0070's mechanism, in a second game"
    );
    assert_eq!(
        beam_intensity(&app),
        BEAM_INTENSITY,
        "the beam should light on the tick the torch arrives, not the one after"
    );
}

#[test]
fn dropping_it_puts_the_room_back_in_the_dark() {
    // The half that is easy to leave out: a state written every tick from the inventory cannot get
    // stuck on, and this is what says so.
    let mut app = room();
    let player = player(&app.world).expect("a character");
    let torch = torch(&app);

    amadeo_inventory::store(&mut app.world, torch, player).expect("room");
    app.run_ticks(1).expect("a tick runs");
    assert_eq!(beam_intensity(&app), BEAM_INTENSITY);

    amadeo_inventory::drop_at(&mut app.world, torch, [0.0, 0.5, 4.0]).expect("it was stored");
    app.run_ticks(1).expect("a tick runs");

    assert_eq!(beam_intensity(&app), 0.0);
    assert!(!holding_torch(&app.world));
    assert!(
        app.world.get::<Transform>(torch).is_some(),
        "and it is lying in the room again, with its mesh and collider untouched"
    );
}

#[test]
fn the_view_pitches_when_the_look_axis_moves() {
    // The seam that makes the interactor-on-a-camera arrangement worth anything: a runtime pitch,
    // which an authored angle cannot give. If this stops working, interaction still "works" and
    // quietly only reaches things at eye height.
    let mut app = room();
    let eyes = eyes(&app.world).expect("an interactor");
    let before = app.world.get::<Transform>(eyes).expect("placed").rotation[0];

    look_down(&mut app, 1.0, 10);

    let after = app.world.get::<Transform>(eyes).expect("placed").rotation[0];
    assert_ne!(
        before, after,
        "the mouse axis should have tilted the view; it is what aims the sweep"
    );
}

#[test]
fn the_interactor_is_the_camera_so_the_sweep_follows_the_view() {
    // Stated as a structural assertion rather than a geometric one, because the geometry is what
    // the module's own tests cover. What this game adds is *where* the interactor lives.
    let app = room();
    let eyes = eyes(&app.world).expect("an interactor");

    assert!(app.world.get::<Interactor>(eyes).is_some());
    assert!(app.world.get::<FirstPersonCamera>(eyes).is_some());
    assert!(
        app.world.get::<Looking>(eyes).is_some() || app.tick() == Tick(0),
        "the module writes `Looking` onto the interactor every tick once one has run"
    );
}

/// Whether each piece of the reticle is currently drawn, as `(opens, visible)`.
fn reticle(app: &App) -> Vec<(bool, bool)> {
    let mut pieces: Vec<(bool, bool)> = app
        .world
        .query::<(&warren::Reticle, &amadeo_ui::UiNode)>()
        .map(|(_, (reticle, node))| (reticle.opens, node.visible))
        .collect();
    pieces.sort_unstable();
    pieces
}

#[test]
fn the_reticle_opens_only_when_something_is_in_reach() {
    // **`docs/11` §8's "usability failure at the core verb".** Interaction is a sphere swept along
    // the camera's forward and nothing said where that pointed, so failing to reach a thing and
    // failing to aim at it looked identical — which means the player cannot learn the verb by using
    // it.
    //
    // Three states in one test, because the interesting claim is the *difference* between them and a
    // test of any one alone passes for an implementation that never changes. **Mutated once**:
    // dropping the write in `write_the_hud` leaves the authored defaults in place, which satisfies
    // the closed state and fails the other two.
    let mut app = room();
    stand_before_the_torch(&mut app);

    // Level, over the top of the crate: the dot is up, the ticks are not.
    look_down(&mut app, 0.0, 6);
    assert_eq!(
        prompt(&app.world),
        None,
        "the setup is wrong if something is already in reach here"
    );
    let closed = reticle(&app);
    assert!(
        !closed.is_empty(),
        "the HUD authors no reticle at all — five nodes carrying `Reticle` were expected"
    );
    assert!(
        closed.iter().any(|(opens, visible)| !opens && *visible),
        "the dot is always up while playing, so the player can see where they are aimed: {closed:?}"
    );
    assert!(
        closed.iter().all(|(opens, visible)| !opens || !visible),
        "nothing is in reach, so no tick may be showing: {closed:?}"
    );

    // Aimed down at the torch: the ticks open.
    look_down(&mut app, 20.0, 6);
    assert!(
        prompt(&app.world).is_some(),
        "the aim-down setup stopped working; see the test above"
    );
    let open = reticle(&app);
    assert!(
        open.iter().all(|(_, visible)| *visible),
        "with something in reach every piece is up — that is the 'opening' the design asks for: \
         {open:?}"
    );

    // And it goes away with the game. A reticle over a pause menu or an ending is a game that looks
    // like it is still running.
    if let Some(screen) = app.world.resource_mut::<warren::Screen>() {
        *screen = warren::Screen::Paused;
    }
    app.run_ticks(1).expect("a tick runs");
    let paused = reticle(&app);
    assert!(
        paused.iter().all(|(_, visible)| !*visible),
        "nothing of the reticle survives leaving play: {paused:?}"
    );
}
