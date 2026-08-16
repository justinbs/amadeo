//! Title screen, pause, save, quit, resume — M3 exit gate item 1's shell.
//!
//! # Everything here goes through the buttons
//!
//! The other test files set `Screen::Playing` directly and get on with what they are about. This one
//! never does. Every transition below is reached by moving the highlight with `ui_next` and pressing
//! `ui_confirm`, because what is being proved is that **the menus work** — and a test that wrote the
//! screen it wanted would pass against a menu whose buttons did nothing at all.
//!
//! # What "complete, not a demo" turns out to mean
//!
//! `docs/05`'s exit gate item 1 is a list of six transitions: title, play, lose, win, pause, save,
//! quit, resume. Each one is a test here, and the last of them — starting a run over — is the only
//! one `games/atrium` did not already prove.

use std::path::Path;

use amadeo_app::App;
use amadeo_core::Tick;
use amadeo_input::{InputDriver, ScriptedSource};
use amadeo_transform::Transform;
use amadeo_ui::{Focus, UI_CONFIRM, UI_NEXT};
use warren::{Outcome, PAUSE, Screen, outcome, player, screen};

/// The generated level, freshly booted — which means sitting on the title screen.
fn booted() -> App {
    let mut app = warren::build_simulation().expect("the game builds");
    amadeo_input::install(
        &mut app.world,
        InputDriver::new(Box::new(ScriptedSource::new())),
    );
    app
}

/// Taps a named button for one tick, through the source, because they are all edge-triggered.
fn tap(app: &mut App, action: &str) {
    let now = app.tick();
    let release = Tick(now.0 + 1);
    let action = action.to_string();
    app.world
        .with_service_taken::<InputDriver, ()>(|_world, driver| {
            if let Some(scripted) = driver.source.as_any_mut().downcast_mut::<ScriptedSource>() {
                scripted.press(now, &action, true);
                scripted.press(release, &action, false);
            }
        });
    app.run_ticks(2).expect("ticks run");
}

/// Moves the highlight down `steps` times, then confirms.
fn choose_item(app: &mut App, steps: usize) {
    for _ in 0..steps {
        tap(app, UI_NEXT);
    }
    tap(app, UI_CONFIRM);
}

/// Whichever entity currently has the highlight.
fn focused(app: &App) -> Option<amadeo_ecs::Entity> {
    app.world.resource::<Focus>().and_then(|focus| focus.entity)
}

/// What a focused button would do, by its own component.
fn focused_choice(app: &App) -> Option<warren::MenuChoice> {
    focused(app)
        .and_then(|entity| app.world.get::<warren::MenuButton>(entity))
        .map(|button| button.choice)
}

// --- The title screen ---------------------------------------------------------------------------

#[test]
fn a_fresh_game_is_on_the_title_screen_and_frozen() {
    let mut app = booted();
    assert_eq!(screen(&app.world), Screen::Title);

    // **Frozen, and proved by the world not moving.** The player is placed a metre above the floor
    // and gravity is on, so half a second of a *running* game would drop them onto it. That the
    // position is unchanged is what says the gameplay stages are not running.
    //
    // **After one tick, not from zero**, and that is ADR 0065 rather than a fudge: `Paused` is read
    // once at the top of `step`, so a pause always takes effect on the tick *after* the one that
    // asked for it — and a fresh world asks for it in its very first `PreSimulation`. So tick one
    // runs in full, the player falls three centimetres onto the floor, and nothing moves again.
    app.run_ticks(1).expect("a tick runs");
    let before = app
        .world
        .get::<Transform>(player(&app.world).expect("a character"))
        .expect("placed")
        .translation;
    app.run_ticks(30).expect("half a second runs");
    let after = app
        .world
        .get::<Transform>(player(&app.world).expect("a character"))
        .expect("placed")
        .translation;
    assert_eq!(before, after, "the world moved behind the title screen");
}

#[test]
fn the_title_screen_highlights_its_first_button_by_itself() {
    // **The game does this, not the engine** (ADR 0063): `navigate_focus` deliberately will not seat
    // the highlight, because a menu that grabbed it the moment it appeared would override whatever
    // the game wanted focused. So `apply_screen` seats it, and this is what says so.
    let mut app = booted();
    app.run_ticks(1).expect("a tick runs");
    assert_eq!(focused_choice(&app), Some(warren::MenuChoice::Begin));
}

#[test]
fn the_highlight_never_lands_in_a_menu_that_is_not_up() {
    // **The failure this guards against has no symptom.** Three menus are authored in one scene and
    // two of them are hidden at any moment; a highlight left inside a hidden one is unreachable,
    // and pressing a direction would appear to do nothing at all.
    //
    // Walking further than there are buttons is deliberate: it makes the highlight wrap through the
    // whole reachable set, and if the set included hidden items it would land on one.
    let mut app = booted();
    app.run_ticks(1).expect("a tick runs");

    for _ in 0..8 {
        tap(&mut app, UI_NEXT);
        let entity = focused(&app).expect("something is always focused while a menu is up");
        let choice = app
            .world
            .get::<warren::MenuButton>(entity)
            .map(|button| button.choice)
            .expect("and it is always a button");
        assert!(
            matches!(
                choice,
                warren::MenuChoice::Begin | warren::MenuChoice::Load | warren::MenuChoice::Quit
            ),
            "the highlight reached {choice:?}, which is not on the title screen"
        );
    }
}

#[test]
fn begin_starts_the_run() {
    let mut app = booted();
    app.run_ticks(1).expect("a tick runs");
    choose_item(&mut app, 0);

    assert_eq!(screen(&app.world), Screen::Playing);
    // And the world is running again: the player falls the last few centimetres onto the floor.
    app.run_ticks(30).expect("half a second runs");
    assert!(
        warren::grounded(&app.world),
        "the run did not actually start"
    );
    // Nothing is highlighted while playing, or the first confirm after unpausing would press a
    // button nobody can see.
    assert_eq!(focused(&app), None);
}

// --- Pausing ------------------------------------------------------------------------------------

#[test]
fn escape_pauses_and_escape_again_does_not() {
    let mut app = booted();
    app.run_ticks(1).expect("a tick runs");
    choose_item(&mut app, 0);
    assert_eq!(screen(&app.world), Screen::Playing);

    tap(&mut app, PAUSE);
    assert_eq!(screen(&app.world), Screen::Paused);
    assert_eq!(focused_choice(&app), Some(warren::MenuChoice::Resume));

    tap(&mut app, PAUSE);
    assert_eq!(screen(&app.world), Screen::Playing);
}

#[test]
fn escape_does_nothing_on_the_title_screen() {
    // There is nothing to pause. Worth pinning because the toggle is a `match` and the tempting
    // spelling of it — flip between two states — would send the title screen into a paused game
    // that had never started.
    let mut app = booted();
    app.run_ticks(1).expect("a tick runs");
    tap(&mut app, PAUSE);
    assert_eq!(screen(&app.world), Screen::Title);
}

#[test]
fn a_paused_world_does_not_move() {
    let mut app = booted();
    app.run_ticks(1).expect("a tick runs");
    choose_item(&mut app, 0);
    app.run_ticks(30).expect("it settles on the floor");

    tap(&mut app, PAUSE);
    let before = app
        .world
        .get::<Transform>(player(&app.world).expect("a character"))
        .expect("placed")
        .translation;
    app.run_ticks(120).expect("two seconds run");
    let after = app
        .world
        .get::<Transform>(player(&app.world).expect("a character"))
        .expect("placed")
        .translation;
    assert_eq!(before, after);
}

// --- The ending ---------------------------------------------------------------------------------

/// Ends the run by walking into the warden, which is the shortest way to reach an ending.
fn get_caught(app: &mut App) {
    let warden = app
        .world
        .query::<(&warren::Warden,)>()
        .map(|(entity, _)| entity)
        .next()
        .expect("a warden");
    let at = app
        .world
        .get::<Transform>(warden)
        .expect("placed")
        .translation;
    if let Some(transform) = app
        .world
        .get_mut::<Transform>(player(&app.world).expect("p"))
    {
        transform.translation = [at[0], 1.0, at[2]];
    }
    app.run_ticks(4).expect("ticks run");
}

#[test]
fn the_run_ending_puts_the_ending_screen_up_by_itself() {
    // **Nothing chooses this.** `settle_the_run` decides that a run is over and `apply_screen`
    // decides what is on screen because of it, which is the same split `Screen` and `Paused` draw
    // one level down. A player who has been caught should not have to press anything to find out.
    let mut app = booted();
    app.run_ticks(1).expect("a tick runs");
    choose_item(&mut app, 0);
    get_caught(&mut app);

    assert_eq!(outcome(&app.world), Outcome::Caught);
    assert_eq!(screen(&app.world), Screen::Ended);
    assert_eq!(focused_choice(&app), Some(warren::MenuChoice::TryAgain));
}

#[test]
fn escape_does_not_get_you_out_of_an_ending() {
    // A run that is over has no state to go back to, so the pause toggle must not apply. The
    // tempting two-state toggle would put the player back in a world they had already lost.
    let mut app = booted();
    app.run_ticks(1).expect("a tick runs");
    choose_item(&mut app, 0);
    get_caught(&mut app);

    tap(&mut app, PAUSE);
    assert_eq!(screen(&app.world), Screen::Ended);
}

#[test]
fn trying_again_gives_you_a_run_that_has_not_ended() {
    // **The one transition `games/atrium` does not already prove**, and the reason `FreshStart`
    // exists as a *service*: it holds the world exactly as it loaded, and a snapshot restores
    // resources but never services (ADR 0009) — so a resource holding it would be replaced by the
    // very restore it exists to perform.
    let mut app = booted();
    app.run_ticks(1).expect("a tick runs");
    choose_item(&mut app, 0);
    get_caught(&mut app);
    assert_eq!(outcome(&app.world), Outcome::Caught);

    // "Try again" is the first item on the ending screen. It records a request; nothing inside a
    // tick replaces a world.
    choose_item(&mut app, 0);
    let said = warren::serve_requests(&mut app);
    assert!(
        said.iter().any(|line| line.contains("started again")),
        "{said:?}"
    );

    assert_eq!(outcome(&app.world), Outcome::Playing);
    assert_eq!(
        screen(&app.world),
        Screen::Playing,
        "starting again should drop you into the run, not back at the title"
    );

    // And it is a *world*, not just a cleared flag: the player is back where they woke up and the
    // level is still under them.
    app.run_ticks(30).expect("half a second runs");
    assert!(warren::grounded(&app.world));
}

// --- Saving and resuming ------------------------------------------------------------------------

#[test]
fn a_run_survives_being_saved_and_read_back() {
    // The exit gate's "save, quit, resume from save" without the quitting, which is a window
    // operation rather than a game one. Uses a scratch path so that two tests running at once do
    // not fight over one file.
    let mut app = booted();
    app.run_ticks(1).expect("a tick runs");
    choose_item(&mut app, 0);
    app.run_ticks(30).expect("it settles");

    // Somewhere distinctive, so a "restore" that quietly did nothing would not pass.
    let moved = [4.0, 1.0, -3.0];
    if let Some(transform) = app
        .world
        .get_mut::<Transform>(player(&app.world).expect("p"))
    {
        transform.translation = moved;
    }
    app.run_ticks(1).expect("a tick runs");
    let saved = app
        .world
        .get::<Transform>(player(&app.world).expect("p"))
        .expect("placed")
        .translation;

    // Save is the second item on the pause menu.
    tap(&mut app, PAUSE);
    choose_item(&mut app, 1);
    let said = warren::serve_requests(&mut app);
    assert!(
        said.iter().any(|line| line.starts_with("saved to")),
        "{said:?}"
    );
    assert!(Path::new(warren::SAVE_FILE).exists());

    // Walk somewhere else entirely, then load it back.
    tap(&mut app, PAUSE);
    if let Some(transform) = app
        .world
        .get_mut::<Transform>(player(&app.world).expect("p"))
    {
        transform.translation = [40.0, 1.0, 40.0];
    }
    app.run_ticks(5).expect("ticks run");

    tap(&mut app, PAUSE);
    choose_item(&mut app, 2);
    let said = warren::serve_requests(&mut app);
    assert!(
        said.iter().any(|line| line.starts_with("loaded")),
        "{said:?}"
    );

    let restored = app
        .world
        .get::<Transform>(player(&app.world).expect("p"))
        .expect("placed")
        .translation;
    assert!(
        (restored[0] - saved[0]).abs() < 0.01 && (restored[2] - saved[2]).abs() < 0.01,
        "saved at {saved:?} and came back at {restored:?}"
    );

    let _ = std::fs::remove_file(warren::SAVE_FILE);
}

#[test]
fn a_missing_save_is_survivable_and_says_so() {
    // ADR 0021's rule for assets, applied to a save: a game that refused to start because a file was
    // missing would be worse than one that carries on. "Continue" on the title screen of a fresh
    // install is exactly this, and it must not be a crash.
    let _ = std::fs::remove_file(warren::SAVE_FILE);
    let mut app = booted();
    app.run_ticks(1).expect("a tick runs");

    // "Continue" is the second item on the title screen.
    choose_item(&mut app, 1);
    let said = warren::serve_requests(&mut app);
    assert!(
        said.iter().any(|line| line.contains("could not read")),
        "{said:?}"
    );
    assert_eq!(
        screen(&app.world),
        Screen::Title,
        "a failed load should leave you where you were"
    );
}

// --- Quitting -----------------------------------------------------------------------------------

#[test]
fn quit_is_terminal() {
    // The window layer reads this and closes. Nothing gets out of it — a game shutting down and then
    // not shutting down because somebody was still holding a key would be a memorable bug.
    let mut app = booted();
    app.run_ticks(1).expect("a tick runs");
    choose_item(&mut app, 2);
    assert_eq!(screen(&app.world), Screen::Quitting);

    tap(&mut app, PAUSE);
    app.run_ticks(30).expect("half a second runs");
    assert_eq!(screen(&app.world), Screen::Quitting);
}

#[test]
fn the_view_does_not_turn_behind_a_menu() {
    // **The defect this exists for shipped, and a player found it by playing.**
    //
    // `apply_screen` projects the screen onto the engine's `Paused` with `resource_mut`, which hands
    // back `None` when the resource was never inserted — and this game never inserted it. So every
    // pause was a no-op: the world simulated behind the title screen and the mouse still turned the
    // view, which is the only symptom a frozen-looking world has.
    //
    // **`a_paused_world_does_not_move` above did not catch it, and could not have.** It compares the
    // player's translation, and a player with no movement input does not move whether the game is
    // paused or not. A test of "is it frozen" has to drive something that *would* change, and the
    // view is the one thing a player can always move.
    let mut app = booted();
    app.run_ticks(2).expect("ticks run");

    let player = player(&app.world).expect("a character");
    let eyes = warren::eyes(&app.world).expect("a camera");
    let before = (
        app.world.get::<Transform>(player).expect("placed").rotation,
        app.world.get::<Transform>(eyes).expect("placed").rotation,
    );

    for _ in 0..60 {
        if let Some(state) = app.world.resource_mut::<amadeo_input::InputState>() {
            state.set_button(amadeo_input::ActionId::new(amadeo_camera::LOOK), true);
            state.set_axis(amadeo_input::ActionId::new(amadeo_camera::LOOK_X), 20.0);
            state.set_axis(amadeo_input::ActionId::new(amadeo_camera::LOOK_Y), 20.0);
        }
        app.run_ticks(1).expect("a tick runs");
    }

    let after = (
        app.world.get::<Transform>(player).expect("placed").rotation,
        app.world.get::<Transform>(eyes).expect("placed").rotation,
    );
    assert_eq!(
        before, after,
        "a second of mouse turned the view behind a menu"
    );
}
