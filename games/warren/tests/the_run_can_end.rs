//! The Warren's playable loop: find the key, reach the door, get out — or get caught.
//!
//! # What this covers of M3's exit gate
//!
//! Item 1's middle clause ("playable loop → lose state (caught) and win state (escape)") and item 3
//! ("at least one pursuing entity with distinct AI states, driven by `mod-behaviour`"). Not the
//! title screen, not the procedural interiors, and not the save loop — `games/atrium` proves that
//! one and this game has not needed it yet.
//!
//! # The endings are driven, not asserted into place
//!
//! Every test here reaches its ending through the systems that would run in a real frame: the
//! warden is moved by its own machine, and the door is opened by an `Interacted` event. Writing
//! `Outcome::Escaped` directly and checking it came back would test nothing.

use amadeo_app::App;
use amadeo_behaviour::Behaviour;
use amadeo_core::Tick;
use amadeo_ecs::Entity;
use amadeo_input::{InputDriver, ScriptedSource};
use amadeo_interaction::USE;
use amadeo_inventory::Item;
use amadeo_transform::Transform;
use warren::{KEY, Outcome, WARDEN_SIGHT, Warden, outcome, player};

fn room() -> App {
    let mut app = warren::build_simulation().expect("the room builds");
    amadeo_input::install(
        &mut app.world,
        InputDriver::new(Box::new(ScriptedSource::new())),
    );
    app
}

fn thing_of_kind(app: &App, kind: &str) -> Entity {
    app.world
        .query::<(&Item,)>()
        .filter(|(_, (item,))| item.kind == kind)
        .map(|(entity, _)| entity)
        .next()
        .unwrap_or_else(|| panic!("the scene should hold a `{kind}`"))
}

fn warden(app: &App) -> Entity {
    app.world
        .query::<(&Warden,)>()
        .map(|(entity, _)| entity)
        .next()
        .expect("the scene puts a warden in the room")
}

fn warden_state(app: &App) -> String {
    app.world
        .get::<Behaviour>(warden(app))
        .map(|mind| mind.state.clone())
        .unwrap_or_default()
}

/// Moves the player somewhere.
///
/// **Q30 nearly bites here.** The character controller rewrites its own `Transform` from
/// `CharacterMotion` every tick, so a write between ticks can be undone — it survives for a tick,
/// which is enough for a test that acts immediately and not enough for one that waits. Where a test
/// needs the *distance* to hold for several ticks, move the warden instead: it has no controller,
/// so nothing argues with its transform.
fn stand_at(app: &mut App, at: [f32; 3]) {
    let player = player(&app.world).expect("a character");
    if let Some(transform) = app.world.get_mut::<Transform>(player) {
        transform.translation = at;
    }
    app.run_ticks(1).expect("a tick runs");
}

/// Puts the warden somewhere and lets the world settle for a few ticks.
fn warden_stands_at(app: &mut App, at: [f32; 3]) {
    let warden = warden(app);
    if let Some(transform) = app.world.get_mut::<Transform>(warden) {
        transform.translation = at;
    }
    app.run_ticks(4).expect("ticks run");
}

/// Presses "use" for one tick. Edge-triggered, so it goes through the source.
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

/// Puts the warden out of the way, so a test about the door is not also a test about being caught.
fn banish_the_warden(app: &mut App) {
    let warden = warden(app);
    if let Some(transform) = app.world.get_mut::<Transform>(warden) {
        transform.translation = [0.0, 0.9, 100.0];
    }
}

#[test]
fn a_fresh_run_has_not_ended() {
    let app = room();
    assert_eq!(outcome(&app.world), Outcome::Playing);
}

// --- The win ------------------------------------------------------------------------------------

#[test]
fn the_door_is_locked_until_you_have_the_key() {
    let mut app = room();
    banish_the_warden(&mut app);
    let player = player(&app.world).expect("a character");

    // In front of the door, which sits in the north wall at x = -4.
    stand_at(&mut app, [-4.0, 1.0, -6.4]);
    tap_use(&mut app);

    assert_eq!(
        outcome(&app.world),
        Outcome::Playing,
        "using a locked door must not open it"
    );

    // And the prompt says so, which is the only thing the player has to go on.
    assert_eq!(
        warren::prompt(&app.world).as_deref(),
        Some("The door is locked")
    );
    assert_eq!(amadeo_inventory::count_of(&app.world, player, KEY), 0);
}

#[test]
fn with_the_key_the_same_door_lets_you_out() {
    let mut app = room();
    banish_the_warden(&mut app);
    let player = player(&app.world).expect("a character");
    let key = thing_of_kind(&app, KEY);

    amadeo_inventory::store(&mut app.world, key, player).expect("the bag has room");
    stand_at(&mut app, [-4.0, 1.0, -6.4]);

    // The prompt has to change, or a player holding the key has no way to know it worked.
    assert_eq!(
        warren::prompt(&app.world).as_deref(),
        Some("Unlock the door and leave")
    );

    tap_use(&mut app);
    assert_eq!(outcome(&app.world), Outcome::Escaped);
}

#[test]
fn escaping_twice_is_still_escaping() {
    // A run that has ended stays ended. Cheap to assert and the kind of thing that only shows up
    // when somebody keeps playing after the win.
    let mut app = room();
    let player = player(&app.world).expect("a character");
    let key = thing_of_kind(&app, KEY);
    amadeo_inventory::store(&mut app.world, key, player).expect("room");
    stand_at(&mut app, [-4.0, 1.0, -6.4]);
    tap_use(&mut app);
    assert_eq!(outcome(&app.world), Outcome::Escaped);

    // Now stand in the warden's arms. Being caught after escaping would be a memorable bug.
    let warden = warden(&app);
    let at = app
        .world
        .get::<Transform>(warden)
        .expect("placed")
        .translation;
    stand_at(&mut app, [at[0], 1.0, at[2]]);
    app.run_ticks(10).expect("ticks run");

    assert_eq!(outcome(&app.world), Outcome::Escaped);
}

// --- The lose -----------------------------------------------------------------------------------

#[test]
fn the_warden_starts_idle_and_notices_you() {
    // ADR 0068's boundary from the game's side: this game supplies `"sees_you"`, the module decides
    // the state, and neither knows the other's business.
    let mut app = room();
    let you = app
        .world
        .get::<Transform>(player(&app.world).expect("a character"))
        .expect("placed")
        .translation;

    // Well outside its sight. Moving the warden rather than the player, for the reason `stand_at`
    // documents: only one of the two has something rewriting its transform every tick.
    warden_stands_at(&mut app, [you[0], 0.93, you[2] - (WARDEN_SIGHT + 6.0)]);
    assert_eq!(warden_state(&app), "idle");

    // Now within sight.
    warden_stands_at(&mut app, [you[0], 0.93, you[2] - 4.0]);
    assert_eq!(warden_state(&app), "pursue");
}

#[test]
fn it_gives_up_and_goes_back_to_idle() {
    // The gate asks for *distinct* states, and "search" is the one that is easy to author and never
    // reach: it needs the player to have been seen and then not be.
    let mut app = room();
    let you = app
        .world
        .get::<Transform>(player(&app.world).expect("a character"))
        .expect("placed")
        .translation;

    // Seen. Far enough that it does not reach you and end the run before the state can change.
    warden_stands_at(&mut app, [you[0], 0.93, you[2] - 8.0]);
    assert_eq!(warden_state(&app), "pursue");

    // Gone. It searches for a while, then loses interest — the machine's `after 5.0`.
    warden_stands_at(&mut app, [you[0], 0.93, you[2] - (WARDEN_SIGHT + 30.0)]);
    assert_eq!(warden_state(&app), "search");

    app.run_ticks(60 * 6).expect("six seconds run");
    assert_eq!(
        warden_state(&app),
        "idle",
        "it should have given up after the authored five seconds"
    );
}

#[test]
fn standing_still_while_it_pursues_gets_you_caught() {
    let mut app = room();
    let post = app
        .world
        .get::<Transform>(warden(&app))
        .expect("placed")
        .translation;

    // Close enough to be seen, far enough that it has to walk. Nothing is asserted into place: the
    // machine decides to pursue and `move_the_warden` closes the distance a tick at a time.
    stand_at(&mut app, [post[0], 1.0, post[2] + 5.0]);
    app.run_ticks(60 * 6).expect("six seconds run");

    assert_eq!(outcome(&app.world), Outcome::Caught);
}

#[test]
fn it_is_slower_than_you_are() {
    // The property that makes a chase winnable, and the reason `WARDEN_SPEED` is what it is. Checked
    // as a *number* rather than by running away, because outrunning it in a 16 m room is a test of
    // the room rather than of the pursuit.
    // Read out of the character controller the scene authored rather than written down here, so
    // that changing the player's speed and forgetting the warden's fails this rather than shipping
    // a chase nobody can win.
    let walk = app_speed();
    assert!(
        warren::WARDEN_SPEED < walk,
        "the warden ({}) must be slower than the player ({walk}), or a chase is a cutscene",
        warren::WARDEN_SPEED
    );
}

/// The player's authored walking speed, from the scene.
fn app_speed() -> f32 {
    let app = room();
    let player = player(&app.world).expect("a character");
    app.world
        .get::<amadeo_character::CharacterController>(player)
        .expect("the player has one")
        .speed
}

// --- The HUD ------------------------------------------------------------------------------------

/// What a HUD line currently says.
fn line_says<T: amadeo_ecs::Component>(app: &App) -> String {
    app.world
        .query::<(&T,)>()
        .map(|(entity, _)| entity)
        .next()
        .and_then(|entity| app.world.get::<amadeo_ui::Text>(entity))
        .map(|text| text.content.clone())
        .unwrap_or_default()
}

#[test]
fn the_prompt_line_says_what_you_are_looking_at() {
    // The door told you it was locked only in a test until this existed. A prompt the player cannot
    // see is a prompt that is not there.
    let mut app = room();
    banish_the_warden(&mut app);

    assert_eq!(
        line_says::<warren::PromptLine>(&app),
        "",
        "nothing in reach, nothing to say"
    );

    stand_at(&mut app, [-4.0, 1.0, -6.4]);
    assert_eq!(line_says::<warren::PromptLine>(&app), "The door is locked");
}

#[test]
fn the_ending_line_stays_empty_until_the_run_ends() {
    let mut app = room();
    banish_the_warden(&mut app);
    assert_eq!(line_says::<warren::EndingLine>(&app), "");

    let player = player(&app.world).expect("a character");
    let key = thing_of_kind(&app, KEY);
    amadeo_inventory::store(&mut app.world, key, player).expect("room");
    stand_at(&mut app, [-4.0, 1.0, -6.4]);
    tap_use(&mut app);

    assert_eq!(line_says::<warren::EndingLine>(&app), "YOU GOT OUT");
    // And the prompt gets out of the way: a stale "unlock the door" under a win screen reads as the
    // game still running.
    assert_eq!(line_says::<warren::PromptLine>(&app), "");
}

#[test]
fn an_unchanged_hud_is_not_rewritten() {
    // `Text` is an ordinary component, so its content is **hashed**. `write_the_hud` therefore
    // compares before writing, and this is what says so.
    //
    // **Not asserted against `state_hash`**, which was the first attempt and cannot work: the hash
    // includes the tick number by construction, so it changes every tick no matter what. What can be
    // observed is the thing the guard actually protects — that the string does not churn.
    let mut app = room();
    banish_the_warden(&mut app);
    app.run_ticks(5).expect("ticks run");

    let settled = line_says::<warren::PromptLine>(&app);
    app.run_ticks(30).expect("half a second runs");

    assert_eq!(
        settled,
        line_says::<warren::PromptLine>(&app),
        "half a second of standing still must not rewrite the HUD"
    );
}

// --- Room pieces (ADR 0071) ---------------------------------------------------------------------

#[test]
fn a_socket_is_a_component_a_scene_can_author() {
    // The first concrete piece of ADR 0071. Nothing generates yet; what this pins is that a socket
    // is *authored data* — a registered component with a kind and an open flag — rather than
    // something a generator infers from a bounding box.
    let mut app = room();

    // The Warren is one handcrafted room and authors none yet, which is the honest starting state.
    assert!(warren::open_sockets(&app.world).is_empty());

    // A piece declares a doorway by putting one on a child entity. Placed and aimed by that
    // entity's own `Transform`, which is why `Socket` carries neither.
    let doorway = app.world.spawn();
    app.world.insert(doorway, Transform::default());
    app.world.insert(doorway, warren::Socket::new("corridor"));

    let found = warren::open_sockets(&app.world);
    assert_eq!(found.len(), 1);
    assert_eq!(found[0].1.kind, "corridor");
    assert!(found[0].1.open);

    // A used socket stays visible rather than being removed, so a layout that came out wrong can
    // still be read back with `amadeo query`.
    if let Some(socket) = app.world.get_mut::<warren::Socket>(doorway) {
        socket.open = false;
    }
    assert!(warren::open_sockets(&app.world).is_empty());
    assert!(app.world.get::<warren::Socket>(doorway).is_some());
}

#[test]
fn open_sockets_come_back_in_a_reproducible_order() {
    // The seeded-RNG half of determinism is worthless if the *sequence* it feeds is not
    // reproducible. Two worlds built the same way must offer a generator the same sockets in the
    // same order, or two machines lay out different levels from one seed.
    let first = room();
    let second = room();

    let left: Vec<_> = warren::open_sockets(&first.world);
    let right: Vec<_> = warren::open_sockets(&second.world);
    assert_eq!(left, right);
}
