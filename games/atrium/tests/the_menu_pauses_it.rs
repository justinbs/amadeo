//! Pressing Escape stops the room, and the menu still works while it is stopped — ADR 0065.
//!
//! # What this covers that the engine's own tests cannot
//!
//! `crates/amadeo-app/tests/pausing.rs` proves the mechanism against counter systems. This proves
//! the **wiring**, in the real game, through the same `build_simulation` the windowed binary runs:
//! a named action reaching a screen, a screen reaching the engine's `Paused`, a hidden menu becoming
//! visible, focus walking three authored buttons, and a choice reaching a component the scene file
//! put there.
//!
//! Every one of those is a place the pieces could each be right and the join wrong, and the symptom
//! would be a menu that opens over a room that carries on moving underneath it.

use amadeo_app::{App, Paused};
use amadeo_character::{CharacterController, MOVE_FORWARD};
use amadeo_core::Tick;
use amadeo_ecs::Entity;
use amadeo_input::{ActionId, InputDriver, InputState, ScriptedSource};
use amadeo_transform::Transform;
use amadeo_ui::{Focus, Focusable, UI_CONFIRM, UI_NEXT, UiNode};
use atrium::{MenuButton, MenuChoice, PAUSE, PauseMenu, Screen};

fn room() -> App {
    let mut app = atrium::build_simulation().expect("the room builds");
    amadeo_input::install(
        &mut app.world,
        InputDriver::new(Box::new(ScriptedSource::new())),
    );
    app
}

/// Presses an action for exactly one tick, then releases it for one.
///
/// # Why this goes through the scripted *source* rather than writing `InputState`
///
/// Both the pause toggle and menu navigation are edge-triggered, and `sample_input` rolls current
/// values into previous ones **before** applying the source. So a value written straight onto the
/// resource arrives already looking like a held key — `just_pressed` is false on the very first
/// tick, and the press is never seen at all.
///
/// Written the other way first, and five tests failed with a game that simply never paused. That is
/// the same trap `focus.rs`'s own `press` helper documents, met from the other end.
///
/// The release matters as much as the press: without it the key stays down forever and every later
/// `tap` in the test is a *held* key rather than a new press.
fn tap(app: &mut App, action: &str) {
    let now = app.tick();
    let release = Tick(now.0 + 1);
    app.world
        .with_service_taken::<InputDriver, ()>(|_world, driver| {
            if let Some(scripted) = driver.source.as_any_mut().downcast_mut::<ScriptedSource>() {
                scripted.press(now, action, true);
                scripted.press(release, action, false);
            }
        });
    app.run_ticks(2).expect("ticks run");
}

fn menu_root(app: &App) -> Entity {
    app.world
        .query::<(&PauseMenu,)>()
        .map(|(entity, _)| entity)
        .next()
        .expect("the scene authored a pause menu")
}

fn menu_is_up(app: &App) -> bool {
    app.world
        .get::<UiNode>(menu_root(app))
        .expect("the root is a node")
        .visible
}

fn screen(app: &App) -> Screen {
    *app.world.resource::<Screen>().expect("installed")
}

fn player(app: &App) -> Entity {
    app.world
        .query::<(&CharacterController,)>()
        .map(|(entity, _)| entity)
        .next()
        .expect("one character")
}

fn position(app: &App, entity: Entity) -> [f32; 3] {
    app.world
        .get::<Transform>(entity)
        .expect("still there")
        .translation
}

#[test]
fn the_menu_starts_hidden_and_the_game_starts_running() {
    let app = room();
    assert_eq!(screen(&app), Screen::Playing);
    assert!(!menu_is_up(&app));
    assert_eq!(
        app.world.resource::<Paused>().map(|state| state.paused),
        Some(false)
    );
}

#[test]
fn escape_puts_the_menu_up_and_the_engine_into_a_pause() {
    let mut app = room();
    tap(&mut app, PAUSE);

    assert_eq!(screen(&app), Screen::Paused);
    assert!(menu_is_up(&app), "the menu root should be visible");
    assert_eq!(
        app.world.resource::<Paused>().map(|state| state.paused),
        Some(true),
        "the game's screen has to reach the engine's pause, or the room keeps moving"
    );
}

#[test]
fn escape_again_puts_it_away() {
    let mut app = room();
    tap(&mut app, PAUSE);
    tap(&mut app, PAUSE);

    assert_eq!(screen(&app), Screen::Playing);
    assert!(!menu_is_up(&app));
}

#[test]
fn the_room_stops_moving() {
    // **The assertion the whole feature is for.** Held forward, paused, and held forward some more:
    // the character must not have travelled during the pause.
    let mut app = room();
    let player = player(&app);

    if let Some(state) = app.world.resource_mut::<InputState>() {
        state.set_axis(ActionId::new(MOVE_FORWARD), 1.0);
    }
    app.run_ticks(20).expect("ticks run");
    let walked = position(&app, player);
    assert!(
        (walked[2] - 2.0).abs() > 0.1,
        "the control case: the character should have walked, got {walked:?}"
    );

    tap(&mut app, PAUSE);
    let at_pause = position(&app, player);
    app.run_ticks(60).expect("ticks run");

    assert_eq!(
        position(&app, player),
        at_pause,
        "sixty paused ticks with forward held should move nobody"
    );
}

#[test]
fn opening_the_menu_highlights_something() {
    // The engine will not do this — `navigate_focus` deliberately focuses nothing until asked, so
    // that it cannot override what a game wanted focused (ADR 0063). A menu you have to press down
    // on before you can confirm anything is a menu that looks broken, so the *game* supplies it.
    let mut app = room();
    tap(&mut app, PAUSE);

    let focused = app.world.resource::<Focus>().and_then(|focus| focus.entity);
    let choice = focused.and_then(|entity| app.world.get::<MenuButton>(entity));
    assert_eq!(
        choice.map(|button| button.choice),
        Some(MenuChoice::Resume),
        "the first item in the authored order"
    );
}

#[test]
fn the_menu_still_navigates_while_everything_else_is_stopped() {
    // The other half, and the one a coarse pause gets wrong: `navigate_focus` is a `Simulation`
    // system, so it is inside exactly the stage that stops. `.while_paused()` is what keeps it.
    let mut app = room();
    tap(&mut app, PAUSE);

    let first = app.world.resource::<Focus>().and_then(|focus| focus.entity);
    tap(&mut app, UI_NEXT);
    let second = app.world.resource::<Focus>().and_then(|focus| focus.entity);
    assert_ne!(first, second, "the focus should have moved on");

    // And it is walking the order the scene file authored, rather than whatever the storage
    // happened to iterate.
    let order = |entity: Option<Entity>| {
        entity
            .and_then(|entity| app.world.get::<Focusable>(entity))
            .map(|focusable| focusable.order)
    };
    assert_eq!(order(first), Some(0));
    assert_eq!(order(second), Some(1));
}

#[test]
fn the_focus_cannot_land_on_a_menu_that_is_not_up() {
    // The pause-menu bug ADR 0063 names: a stale focus is how "confirm" activates a button that is
    // no longer on screen. Hiding the menu hides its items from navigation, so there is nothing to
    // confirm.
    let mut app = room();
    tap(&mut app, UI_NEXT);
    assert_eq!(
        app.world.resource::<Focus>().and_then(|f| f.entity),
        None,
        "nothing is focusable while the menu is down"
    );
}

#[test]
fn choosing_resume_closes_the_menu() {
    let mut app = room();
    tap(&mut app, PAUSE);
    // Already on "Resume", because opening the menu highlighted it.
    tap(&mut app, UI_CONFIRM);
    // One more tick for `apply_screen` to project the new screen — the menu is closed by the
    // projector, not by the button, so that there is one writer for everything derived.
    app.run_ticks(1).expect("a tick runs");

    assert_eq!(screen(&app), Screen::Playing);
    assert!(!menu_is_up(&app));
}

#[test]
fn choosing_return_to_start_walks_the_character_back() {
    let mut app = room();
    let player = player(&app);

    if let Some(state) = app.world.resource_mut::<InputState>() {
        state.set_axis(ActionId::new(MOVE_FORWARD), 1.0);
    }
    app.run_ticks(30).expect("ticks run");
    if let Some(state) = app.world.resource_mut::<InputState>() {
        state.set_axis(ActionId::new(MOVE_FORWARD), 0.0);
    }
    assert!((position(&app, player)[2] - 2.0).abs() > 0.5, "walked away");

    tap(&mut app, PAUSE);
    // Opening lands on the first item, so one step reaches the second.
    tap(&mut app, UI_NEXT);
    tap(&mut app, UI_CONFIRM);
    app.run_ticks(1).expect("a tick runs");

    let home = position(&app, player);
    assert!(
        (home[0]).abs() < 0.01 && (home[2] - 2.0).abs() < 0.01,
        "should be back at the spawn point, got {home:?}"
    );
    assert_eq!(screen(&app), Screen::Playing, "and playing again");
}

#[test]
fn choosing_quit_asks_the_platform_to_close() {
    // The simulation cannot close a window, so the menu records the decision and `main.rs` acts on
    // it. Recording it in a hashed resource rather than calling out is what keeps the decision
    // inside the replay.
    let mut app = room();
    tap(&mut app, PAUSE);
    for _ in 0..2 {
        tap(&mut app, UI_NEXT);
    }
    tap(&mut app, UI_CONFIRM);

    assert_eq!(screen(&app), Screen::Quitting);

    // Terminal: Escape does not talk it out of quitting.
    tap(&mut app, PAUSE);
    assert_eq!(screen(&app), Screen::Quitting);
}

#[test]
fn every_button_says_what_it_does_in_the_scene_file() {
    // ADR 0063's split, checked rather than described: the engine says which entity was chosen, and
    // the *scene* says what that means. A button with no `MenuButton` would be one that highlights,
    // clicks, and does nothing — which is a silent failure and exactly the shape worth pinning.
    let app = room();
    let mut buttons: Vec<(i32, MenuChoice)> = app
        .world
        .query::<(&Focusable, &MenuButton)>()
        .map(|(_, (focusable, button))| (focusable.order, button.choice))
        .collect();
    buttons.sort_by_key(|(order, _)| *order);

    assert_eq!(
        buttons,
        vec![
            (0, MenuChoice::Resume),
            (1, MenuChoice::ReturnToStart),
            (2, MenuChoice::Quit),
        ]
    );
}

#[test]
fn a_pause_reproduces() {
    // Invariant I3 over the whole thing. Pausing, walking a menu and resuming is input like any
    // other, and two runs of the same input must agree bit for bit — which is what makes a replay
    // containing a pause a replay rather than an approximation.
    let run = || {
        let mut app = room();
        app.run_ticks(10).expect("ticks run");
        tap(&mut app, PAUSE);
        tap(&mut app, UI_NEXT);
        tap(&mut app, UI_CONFIRM);
        app.run_ticks(10).expect("ticks run");
        (app.state_hash(), app.tick())
    };

    assert_eq!(run(), run());
}
