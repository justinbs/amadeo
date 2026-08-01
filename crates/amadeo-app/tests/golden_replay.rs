//! Golden replay tests: the mechanism M0 exists to prove.
//!
//! A recording made once is committed to the repository and replayed by every later build. If the
//! simulation's behaviour changes for any reason — a real regression, or an intended change — the
//! checkpoint hashes stop matching and this fails.
//!
//! This is the project's only behavioural regression test for games, and everything in
//! `docs/03-ai-native-design.md` about verification depends on it working.
//!
//! # When this fails
//!
//! **Do not regenerate the golden file to make it pass.** First find out *why* the behaviour
//! changed. If the change was intended, regenerate deliberately:
//!
//! ```text
//! UPDATE_GOLDEN=1 cargo test -p amadeo-app --test golden_replay
//! ```
//!
//! and say so in the commit message, because every other recorded replay is invalidated at the same
//! time.

use amadeo_app::{App, Stage, system};
use amadeo_core::{FIXED_DT, StableHash, Tick};
use amadeo_ecs::{Component, World};
use amadeo_input::{
    ActionId, InputDriver, InputState, Recorder, Recording, SAMPLE_INPUT, ScriptedSource,
    sample_input,
};
use amadeo_reflect::Reflect;
use std::path::PathBuf;

// --- A tiny platformer, enough to exercise input -> state ---

#[derive(Debug, Clone, Copy, PartialEq, StableHash, Reflect)]
struct Position {
    x: f32,
    y: f32,
}
impl Component for Position {}

#[derive(Debug, Clone, Copy, PartialEq, StableHash, Reflect)]
struct Velocity {
    x: f32,
    y: f32,
}
impl Component for Velocity {}

const MOVE_X: &str = "move_x";
const JUMP: &str = "jump";

const GRAVITY: f32 = -30.0;
const JUMP_SPEED: f32 = 12.0;
const RUN_SPEED: f32 = 6.0;

/// Turns actions into velocity. The only system that reads input.
fn apply_input(world: &mut World) {
    let Some(input) = world.resource::<InputState>() else {
        return;
    };
    let horizontal = input.axis(ActionId::new(MOVE_X)) * RUN_SPEED;
    let jumped = input.just_pressed(ActionId::new(JUMP));

    world.for_each_mut::<Velocity>(|_entity, velocity| {
        velocity.x = horizontal;
        // Only jump when standing still vertically, so a held key cannot climb forever.
        if jumped && velocity.y.abs() < f32::EPSILON {
            velocity.y = JUMP_SPEED;
        }
    });
}

/// Applies gravity, then moves, then clamps to the ground.
fn integrate(world: &mut World) {
    world.for_each_mut::<Velocity>(|_entity, velocity| {
        velocity.y += GRAVITY * FIXED_DT;
    });

    world.for_each_pair_mut::<Position, Velocity>(|_entity, position, velocity| {
        position.x += velocity.x * FIXED_DT;
        position.y += velocity.y * FIXED_DT;
    });

    world.for_each_pair_mut::<Velocity, Position>(|_entity, velocity, position| {
        if position.y < 0.0 {
            velocity.y = 0.0;
        }
    });

    world.for_each_mut::<Position>(|_entity, position| {
        if position.y < 0.0 {
            position.y = 0.0;
        }
    });
}

/// Builds the app. `driver` decides whether input comes from a script or a replay.
fn build_app(seed: u64, driver: InputDriver) -> App {
    let mut app = App::with_seed(seed);
    amadeo_input::install(&mut app.world, driver);

    app.add_system(Stage::PreSimulation, system(SAMPLE_INPUT, sample_input));
    app.add_system(Stage::Simulation, system("apply_input", apply_input));
    app.add_system(
        Stage::Simulation,
        system("integrate", integrate).after("apply_input"),
    );

    for i in 0..3u32 {
        let entity = app.world.spawn();
        app.world.insert(
            entity,
            Position {
                x: i as f32 * 2.0,
                y: 0.0,
            },
        );
        app.world.insert(entity, Velocity { x: 0.0, y: 0.0 });
    }
    app
}

/// The player's session: walk right, jump, turn around, jump again, stop.
fn scripted_session() -> ScriptedSource {
    let mut source = ScriptedSource::new();
    source.axis(Tick(0), MOVE_X, 1.0);
    source.press(Tick(20), JUMP, true);
    source.press(Tick(22), JUMP, false);
    source.axis(Tick(90), MOVE_X, -1.0);
    source.press(Tick(140), JUMP, true);
    source.press(Tick(142), JUMP, false);
    source.axis(Tick(240), MOVE_X, 0.0);
    source
}

const CHECKPOINTS: [u64; 4] = [1, 60, 180, 300];
const TOTAL_TICKS: u64 = 300;

/// Runs an app for `TOTAL_TICKS`, collecting the state hash at each checkpoint.
fn run_and_collect(app: &mut App) -> Vec<(Tick, u64)> {
    let mut collected = Vec::new();
    for tick in 1..=TOTAL_TICKS {
        app.step().expect("schedule resolves");
        if CHECKPOINTS.contains(&tick) {
            collected.push((Tick(tick), app.state_hash()));
        }
    }
    collected
}

fn golden_path() -> PathBuf {
    PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("tests/golden/walk_and_jump.replay")
}

/// Records the scripted session and returns the resulting recording, checkpoints included.
fn record_session() -> Recording {
    let mut recorder = Recorder::new(1234);
    recorder.register_action(MOVE_X);
    recorder.register_action(JUMP);

    let driver = InputDriver::new(Box::new(scripted_session())).recording_with(recorder);
    let mut app = build_app(1234, driver);

    let checkpoints = run_and_collect(&mut app);

    let mut driver = app
        .world
        .remove_service::<InputDriver>()
        .expect("driver installed");
    let mut recorder = driver.recorder.take().expect("recording");
    for (tick, hash) in checkpoints {
        recorder.checkpoint(tick, hash);
    }
    recorder.into_recording()
}

#[test]
fn recording_matches_the_committed_golden_file() {
    let recorded = record_session().to_text();
    let path = golden_path();

    if std::env::var("UPDATE_GOLDEN").is_ok() {
        std::fs::create_dir_all(path.parent().expect("has a parent")).expect("create dir");
        std::fs::write(&path, &recorded).expect("write golden file");
        eprintln!("updated golden file at {}", path.display());
        return;
    }

    let expected = std::fs::read_to_string(&path).unwrap_or_else(|error| {
        panic!(
            "could not read the golden replay at {}: {error}\n\
             If this is the first run, generate it with:\n    \
             UPDATE_GOLDEN=1 cargo test -p amadeo-app --test golden_replay",
            path.display()
        )
    });

    // Line endings first, because the message for a real mismatch actively misleads about this one.
    // A file checked out with `core.autocrlf=true` differs from the recording in every line and in
    // nothing else -- same inputs, same checkpoints, same hashes -- and being told that simulation
    // behaviour changed sends you looking in entirely the wrong place. It did exactly that once.
    assert!(
        !expected.contains('\r'),
        "\nThe committed golden replay has CRLF line endings, but the format specifies LF.\n\
         The simulation is fine -- the hashes are not even being compared yet.\n\
         Git rewrote the file on checkout, which `.gitattributes` exists to prevent.\n\
         Check that `.gitattributes` is present and that `core.autocrlf` is not overriding it.\n"
    );

    // Compare text, not just hashes: a diff of the file shows exactly which input or which
    // checkpoint moved, which a single mismatched integer would not.
    assert_eq!(
        recorded, expected,
        "\nThe recorded session no longer matches the committed golden replay.\n\
         Find out WHY before regenerating -- this usually means simulation behaviour changed,\n\
         which invalidates every other recorded replay too.\n\
         If the change was intended: UPDATE_GOLDEN=1 cargo test -p amadeo-app --test golden_replay\n"
    );
}

#[test]
fn golden_file_replays_to_its_recorded_hashes() {
    // The actual regression test. Parse the committed file, replay it, and check every checkpoint.
    let path = golden_path();
    let Ok(text) = std::fs::read_to_string(&path) else {
        // Generated by the test above; skip cleanly on a fresh checkout rather than failing twice.
        eprintln!("golden file missing, skipping; run with UPDATE_GOLDEN=1 first");
        return;
    };

    let recording = Recording::parse(&text).expect("golden file parses");
    assert_eq!(recording.seed, 1234);

    let mut app = build_app(recording.seed, InputDriver::replaying(recording.clone()));

    let mut checked = 0;
    for tick in 1..=TOTAL_TICKS {
        app.step().expect("schedule resolves");
        if let Some(expected) = recording.checkpoint_at(Tick(tick)) {
            assert_eq!(
                app.state_hash(),
                expected,
                "state diverged from the recording at tick {tick}"
            );
            checked += 1;
        }
    }

    assert_eq!(
        checked,
        CHECKPOINTS.len(),
        "every checkpoint in the file should have been reached"
    );
}

#[test]
fn replaying_twice_gives_the_same_result() {
    // A replay must be re-runnable. Rollback and time-travel debugging depend on this later.
    let recording = record_session();

    let mut first = build_app(recording.seed, InputDriver::replaying(recording.clone()));
    let mut second = build_app(recording.seed, InputDriver::replaying(recording));

    first.run_ticks(TOTAL_TICKS).expect("schedule resolves");
    second.run_ticks(TOTAL_TICKS).expect("schedule resolves");

    assert_eq!(first.state_hash(), second.state_hash());
}

#[test]
fn a_replay_reproduces_the_run_that_recorded_it() {
    // The core claim: the simulation cannot tell a live session from a replay of it.
    let mut recorder = Recorder::new(1234);
    recorder.register_action(MOVE_X);
    recorder.register_action(JUMP);
    let live_driver = InputDriver::new(Box::new(scripted_session())).recording_with(recorder);

    let mut live = build_app(1234, live_driver);
    let live_checkpoints = run_and_collect(&mut live);

    let mut driver = live
        .world
        .remove_service::<InputDriver>()
        .expect("driver installed");
    let recording = driver.recorder.take().expect("recording").into_recording();

    let mut replayed = build_app(1234, InputDriver::replaying(recording));
    let replayed_checkpoints = run_and_collect(&mut replayed);

    assert_eq!(live_checkpoints, replayed_checkpoints);
    assert_eq!(live.state_hash(), replayed.state_hash());
}

#[test]
fn a_changed_input_stream_is_detected() {
    // If a corrupted replay still passed, the checkpoints would be worthless.
    let mut recording = record_session();
    let expected_at_end = recording
        .checkpoint_at(Tick(TOTAL_TICKS))
        .expect("recorded");

    // One extra input, late enough that the earlier checkpoints still pass.
    recording.push_button(Tick(250), JUMP, true);

    let mut app = build_app(recording.seed, InputDriver::replaying(recording));
    app.run_ticks(TOTAL_TICKS).expect("schedule resolves");

    assert_ne!(
        app.state_hash(),
        expected_at_end,
        "an altered input stream must produce a different state"
    );
}

#[test]
fn the_session_actually_exercises_the_simulation() {
    // Guards against the whole suite passing because nothing ever moves.
    let recording = record_session();
    assert!(
        recording.change_count() >= 6,
        "expected the scripted session to produce several input changes, got {}",
        recording.change_count()
    );

    let mut app = build_app(1234, InputDriver::replaying(recording));
    app.run_ticks(TOTAL_TICKS).expect("schedule resolves");

    let positions: Vec<Position> = app.world.iter::<Position>().map(|(_, p)| *p).collect();
    assert_eq!(positions.len(), 3);
    assert!(
        positions.iter().any(|p| p.x.abs() > 1.0),
        "entities should have moved horizontally: {positions:?}"
    );
}
