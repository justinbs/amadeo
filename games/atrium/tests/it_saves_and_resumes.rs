//! Save, quit, resume — M3's exit gate item 1, in the real game.
//!
//! # What is actually being tested
//!
//! Not that a file round-trips; `amadeo-snapshot` has its own tests for that. This is the claim one
//! level up: that **a snapshot of a running physics game resumes into the same future it would have
//! had**. The room has rapier under it, a character standing on a floor, two animation clocks and a
//! stride counter mid-gait, and every one of those is a place where "the components came back" and
//! "the simulation continues correctly" can come apart.
//!
//! ADR 0028's lesson is the shape to watch for: the entity allocator's free list is excluded from
//! the state hash, so two worlds could hash identically and diverge on the next `spawn`. **Hash
//! equality after a restore is necessary and not sufficient**, which is why the tests here restore
//! and then *run on* rather than comparing hashes and stopping.

use amadeo_app::App;
use amadeo_character::{CharacterController, MOVE_FORWARD};
use amadeo_core::Tick;
use amadeo_ecs::Entity;
use amadeo_input::{ActionId, InputDriver, InputState, ScriptedSource};
use amadeo_physics::Physics;
use amadeo_transform::Transform;

fn room() -> App {
    let mut app = atrium::build_simulation().expect("the room builds");
    amadeo_input::install(
        &mut app.world,
        InputDriver::new(Box::new(ScriptedSource::new())),
    );
    app
}

/// Holds an axis for `ticks` ticks.
///
/// Set on the resource each tick rather than scripted, because `sample_input` rolls current values
/// into previous ones and then applies the source — so a value written here survives into the tick
/// that reads it. An axis has no edge to miss, unlike a button.
fn walk(app: &mut App, ticks: u64) {
    for _ in 0..ticks {
        if let Some(state) = app.world.resource_mut::<InputState>() {
            state.set_axis(ActionId::new(MOVE_FORWARD), 1.0);
        }
        app.run_ticks(1).expect("a tick runs");
    }
}

fn player(app: &App) -> Entity {
    app.world
        .query::<(&CharacterController,)>()
        .map(|(entity, _)| entity)
        .next()
        .expect("one character")
}

fn position(app: &App) -> [f32; 3] {
    app.world
        .get::<Transform>(player(app))
        .expect("still there")
        .translation
}

/// Captures the world as the text a save file would hold.
fn save(app: &App) -> String {
    amadeo_snapshot::to_text(&app.capture_snapshot())
}

/// Rebuilds the room and puts a save back into it, the way a fresh process would.
///
/// **`Physics::reset` is the line that is easy to leave out**, and what it is and is not worth is
/// the subject of `loading_into_a_running_game_needs_the_solver_reset` below.
fn load(text: &str) -> App {
    let mut app = room();
    let snapshot = amadeo_snapshot::parse(text).expect("the save parses");
    app.restore_snapshot(&snapshot).expect("the save restores");
    if let Some(physics) = app.world.service_mut::<Physics>() {
        physics.reset();
    }
    app
}

#[test]
fn a_save_taken_mid_walk_resumes_into_the_same_future() {
    // **The exit gate's actual claim.** Not "the file round-trips" — that a resumed game and one
    // that never stopped are the same game, checked by running both on and comparing where they
    // ended up rather than by comparing hashes at the moment of restore.
    let mut original = room();
    walk(&mut original, 40);

    let text = save(&original);
    let at_save = position(&original);

    walk(&mut original, 40);
    let kept_going = original.state_hash();

    let mut resumed = load(&text);
    assert_eq!(
        position(&resumed),
        at_save,
        "the restore should put the character exactly where the save did"
    );

    walk(&mut resumed, 40);
    assert_eq!(
        resumed.state_hash(),
        kept_going,
        "a resumed game and one that never stopped must be the same game"
    );
}

#[test]
fn loading_into_a_running_game_needs_the_solver_reset() {
    // **The case a pause-menu "Load" actually is**, and the one worth pinning: restoring *without
    // restarting the process*, into an app whose solver has been running and is warm.
    //
    // Written first as "restore into a fresh app and skip the reset", which passed — and passing was
    // the test being wrong rather than the reset being unnecessary. A fresh `room()` builds a new
    // `RapierPhysics` with no caches at all, so there was nothing stale to carry.
    //
    // **This does not prove the reset is needed, and that was checked rather than assumed**:
    // commenting the reset out leaves this passing. The Atrium's bodies are static plus one
    // kinematic character, and stale contact caches do not change that. Where it *does* matter is
    // sleeping dynamic bodies, which `crates/amadeo-physics/tests/reset_clears_the_solver.rs`
    // demonstrates at the level that can actually build one.
    //
    // The reset stays here because it is the contract for restoring a world (ADR 0036), and because
    // it is also what drops static geometry belonging to the level being left. A game that streams
    // terrain and skipped it would keep the previous world's ground.
    //
    // `PhysicsBackend::reset` has been documented since ADR 0036 as the thing that makes a physics
    // game snapshot-able, and until today **nothing outside `amadeo-physics` could call it** — the
    // backend is private on purpose, so the only callers were tests holding one directly.
    let mut original = room();
    walk(&mut original, 40);
    let text = save(&original);
    walk(&mut original, 40);
    let kept_going = original.state_hash();

    let snapshot = amadeo_snapshot::parse(&text).expect("parses");

    // A game that has been running for a while and then loads the save, the way pressing "Load" in
    // a pause menu does.
    let mut running = room();
    walk(&mut running, 120);
    running.restore_snapshot(&snapshot).expect("restores");
    if let Some(physics) = running.world.service_mut::<Physics>() {
        physics.reset();
    }

    // The restore alone agrees with the snapshot, which is the half that makes any divergence here
    // invisible: the hash check passes and the trouble starts on the next step.
    assert_eq!(running.state_hash(), snapshot.state_hash);

    walk(&mut running, 40);
    assert_eq!(
        running.state_hash(),
        kept_going,
        "loading into a running game must land in the same future as never having stopped"
    );
}

#[test]
fn the_menu_records_the_request_rather_than_touching_a_disk() {
    // **The split that keeps a filesystem out of the simulation.** A system that wrote a file would
    // put "what happened to be on disk" inside a deterministic tick, so a replay of a game that
    // saved would depend on the machine it ran on. The menu records the *decision* — hashed,
    // replayable, restorable — and the platform layer carries it out between ticks.
    let mut app = room();
    walk(&mut app, 5);

    // What choosing "Save" does, without the menu in the way.
    if let Some(request) = app.world.resource_mut::<atrium::SaveRequest>() {
        request.save = true;
    }

    // Ticking does not act on it. If a system ever started doing the writing, this is what notices.
    let before = app.state_hash();
    app.run_ticks(3).expect("ticks run");
    assert_eq!(
        app.world.resource::<atrium::SaveRequest>().copied(),
        Some(atrium::SaveRequest {
            save: true,
            load: false
        }),
        "a request must survive until something outside the tick serves it"
    );
    assert_ne!(before, app.state_hash(), "the control case: time passed");
}

#[test]
fn a_served_request_is_cleared_even_when_it_fails() {
    // Otherwise a save that cannot be written is retried every frame for the rest of the game, which
    // turns one failed write into a stall nobody can attribute.
    let mut app = room();
    if let Some(request) = app.world.resource_mut::<atrium::SaveRequest>() {
        request.load = true;
    }

    // Nothing has been saved, so this cannot succeed — and it must say so rather than panic.
    let said = atrium::serve_save_requests(&mut app);
    assert_eq!(said.len(), 1, "it should have reported something: {said:?}");
    assert!(said[0].contains("could not read"), "{}", said[0]);

    assert_eq!(
        app.world.resource::<atrium::SaveRequest>().copied(),
        Some(atrium::SaveRequest::default()),
        "a failed request must not be retried forever"
    );
}

#[test]
fn a_save_is_a_text_file_a_person_can_read() {
    // Invariant I1 reaches saves too. A binary save would be one more thing an agent cannot inspect
    // and a person cannot diff, and the format already avoids that.
    let mut app = room();
    walk(&mut app, 10);
    let text = save(&app);

    assert!(text.starts_with("amadeo-snapshot 2\n"), "{}", &text[..40]);
    // What ADR 0069 added, and the reason a save can outlive the build that wrote it: the shape of
    // everything in the file, so a reader can tell "this predates a patch" from "this is corrupt".
    assert!(text.contains("schema-hash "), "the layout is fingerprinted");
    assert!(
        text.contains("  component CharacterController 1"),
        "and each type's version is recorded, even though nothing reads it yet"
    );
    // The things a save has to contain to be a save at all.
    assert!(text.contains("CharacterController"), "the player is in it");
    assert!(
        text.contains("Screen"),
        "which screen you were on is gameplay"
    );
    assert!(text.contains("Stride"), "and where you were in your gait");
}

#[test]
fn the_tick_and_the_gait_come_back_too() {
    // The two pieces of state most likely to be forgotten, because neither is a position. `Stride`
    // is hashed on purpose (ADR 0061) so a save restores you mid-step rather than resetting your
    // footfalls, and `Tick` is part of the hash so a replay taken after a load lines up.
    let mut app = room();
    walk(&mut app, 37);

    let stride = app.world.resource::<atrium::Stride>().cloned();
    let tick = app.tick();
    let resumed = load(&save(&app));

    assert_eq!(resumed.tick(), tick);
    assert_ne!(tick, Tick(0), "the control case: time actually passed");
    assert_eq!(resumed.world.resource::<atrium::Stride>().cloned(), stride);
}

#[test]
fn loading_restores_which_screen_you_were_on() {
    // `Screen` is this game's hashed resource (ADR 0065), so a save taken from the pause menu comes
    // back paused — with the menu up and the world frozen, rather than dropping the player into a
    // running game they were not looking at.
    let mut app = room();
    walk(&mut app, 20);
    if let Some(screen) = app.world.resource_mut::<atrium::Screen>() {
        *screen = atrium::Screen::Paused;
    }
    app.run_ticks(2).expect("ticks run");

    let resumed = load(&save(&app));
    assert_eq!(
        resumed.world.resource::<atrium::Screen>().copied(),
        Some(atrium::Screen::Paused)
    );
}
