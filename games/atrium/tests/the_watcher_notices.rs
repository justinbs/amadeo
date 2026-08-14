//! Something in the room decides what it is doing — ADR 0068, in the real game.
//!
//! # What this checks that the module's own tests cannot
//!
//! `modules/amadeo-behaviour` proves the machine is a pure function of a file and a set of facts.
//! It cannot prove that the **boundary** works, because the boundary is the half the module
//! deliberately does not have: something must give `"sees_player"` a meaning, and something must
//! decide what `"pursue"` *does*.
//!
//! Both of those live in `games/atrium`, and this is the file that says they meet in the middle. It
//! is also the reason the module was built with a user rather than against a guess — the check
//! `modules/amadeo-camera` had and `modules/amadeo-interaction` did not.

use amadeo_app::App;
use amadeo_behaviour::{Behaviour, Facts};
use amadeo_character::CharacterController;
use amadeo_ecs::{Entity, World};
use amadeo_input::{InputDriver, ScriptedSource};
use amadeo_transform::Transform;

fn room() -> App {
    let mut app = atrium::build_simulation().expect("the room builds");
    amadeo_input::install(
        &mut app.world,
        InputDriver::new(Box::new(ScriptedSource::new())),
    );
    app
}

fn watcher(app: &App) -> Entity {
    app.world
        .query::<(&atrium::Watcher,)>()
        .map(|(entity, _)| entity)
        .next()
        .expect("the scene authors one watcher")
}

fn player(app: &App) -> Entity {
    app.world
        .query::<(&CharacterController,)>()
        .map(|(entity, _)| entity)
        .next()
        .expect("one character")
}

fn state(app: &App, entity: Entity) -> String {
    app.world
        .get::<Behaviour>(entity)
        .expect("still there")
        .state
        .clone()
}

fn position(world: &World, entity: Entity) -> [f32; 3] {
    world
        .get::<Transform>(entity)
        .expect("still there")
        .translation
}

/// Distance between two entities on the floor plane.
fn apart(world: &World, one: Entity, other: Entity) -> f32 {
    let a = position(world, one);
    let b = position(world, other);
    ((a[0] - b[0]).powi(2) + (a[2] - b[2]).powi(2)).sqrt()
}

/// Moves the **watcher**, which is how these tests change what it can see.
///
/// # Why not move the player, which is the obvious way to write this
///
/// Because it does not work, and it fails **silently** — `docs/07` and **Q30** both say so, and this
/// file cost a debug cycle finding out anyway. `step_physics` prefers `GlobalTransform`, which
/// `propagate_transforms` writes at the *end* of a tick, so a `Transform` assigned from outside is
/// read back stale on the next one and physics writes the old position straight over the new one.
/// The player is a kinematic body with a collider; there is no supported teleport.
///
/// The watcher is not a physics body at all — it has a mesh and no collider, and
/// `move_the_watcher` moves it by assignment for that reason. So moving *it* is both legal and
/// exactly what the game already does.
fn put_watcher_at(app: &mut App, place: [f32; 3]) {
    let watcher = watcher(app);
    if let Some(transform) = app.world.get_mut::<Transform>(watcher) {
        transform.translation = place;
    }
}

/// Comfortably inside the watcher's sight of the player's spawn at `[0, 1, 2]`.
const IN_SIGHT: [f32; 3] = [-4.0, 0.8, -4.0];

/// Comfortably outside it.
const OUT_OF_SIGHT: [f32; 3] = [-9.0, 0.8, -9.0];

#[test]
fn it_starts_idle_and_the_scene_says_so() {
    let mut app = room();
    let watcher = watcher(&app);
    app.run_ticks(1).expect("a tick runs");

    assert_eq!(state(&app, watcher), "idle");
}

#[test]
fn walking_into_view_starts_a_chase() {
    // **The boundary, end to end.** The game's perception writes a fact, the module's machine reads
    // it and changes state, and the game's movement reads the state. Three pieces, two of which know
    // nothing about each other.
    let mut app = room();
    let watcher = watcher(&app);

    // The scene parks it just past its own sight line, so the room opens with nothing happening.
    app.run_ticks(3).expect("ticks run");
    assert_eq!(state(&app, watcher), "idle", "the control case");

    put_watcher_at(&mut app, IN_SIGHT);
    app.run_ticks(3).expect("ticks run");
    assert_eq!(state(&app, watcher), "pursue");
}

#[test]
fn a_chase_closes_the_distance() {
    // What `"pursue"` *means* in this game, which is the half the module does not have.
    let mut app = room();
    let watcher = watcher(&app);
    let player = player(&app);

    put_watcher_at(&mut app, IN_SIGHT);
    app.run_ticks(3).expect("ticks run");
    assert_eq!(state(&app, watcher), "pursue");

    let before = apart(&app.world, watcher, player);
    app.run_ticks(60).expect("ticks run");
    let after = apart(&app.world, watcher, player);

    assert!(
        after < before - 1.0,
        "a second of pursuit should close at least a metre: {before} -> {after}"
    );
}

#[test]
fn losing_you_makes_it_search_and_then_give_up() {
    // The whole point of `search` existing rather than going straight back to idle: a stalker that
    // forgot you the instant you broke line of sight would not be a stalker.
    let mut app = room();
    let watcher = watcher(&app);

    put_watcher_at(&mut app, IN_SIGHT);
    app.run_ticks(3).expect("ticks run");
    assert_eq!(state(&app, watcher), "pursue");

    put_watcher_at(&mut app, OUT_OF_SIGHT);
    app.run_ticks(3).expect("ticks run");
    assert_eq!(state(&app, watcher), "search");

    // The scene authors four seconds. It must still be searching well before that.
    app.run_ticks(120).expect("ticks run");
    assert_eq!(state(&app, watcher), "search", "gave up too early");

    app.run_ticks(180).expect("ticks run");
    assert_eq!(state(&app, watcher), "idle");
}

#[test]
fn being_seen_again_during_a_search_resumes_the_chase() {
    // The transition ordering in the scene file, cashed: in `search`, seeing the player is listed
    // before giving up, so it wins on a tick where both could fire.
    let mut app = room();
    let watcher = watcher(&app);

    put_watcher_at(&mut app, IN_SIGHT);
    app.run_ticks(3).expect("ticks run");
    put_watcher_at(&mut app, OUT_OF_SIGHT);
    app.run_ticks(3).expect("ticks run");
    assert_eq!(state(&app, watcher), "search");

    put_watcher_at(&mut app, IN_SIGHT);
    app.run_ticks(3).expect("ticks run");
    assert_eq!(state(&app, watcher), "pursue");
}

#[test]
fn the_fact_the_game_writes_is_visible_to_an_agent() {
    // `Facts` is data rather than a registry of condition functions (ADR 0068), and this is the
    // property that buys: "why is it not chasing me" is answerable by *looking*, through
    // `amadeo query`, without reading the game's source.
    let mut app = room();
    let watcher = watcher(&app);

    put_watcher_at(&mut app, IN_SIGHT);
    app.run_ticks(2).expect("ticks run");

    let facts = app.world.get::<Facts>(watcher).expect("written");
    assert!(facts.is("sees_player"));
    assert!(
        facts.known.contains_key("sees_player"),
        "the fact should be present by name: {:?}",
        facts.known
    );
}

#[test]
fn the_machine_the_scene_authored_has_nothing_wrong_with_it() {
    // A transition naming a state that does not exist never fires, and a monster that stops
    // transitioning looks like an AI bug rather than a spelling one. This is the game's own machine
    // put through the validator it ships with — session 9's lesson.
    let app = room();
    let machine = app
        .world
        .get::<amadeo_behaviour::BehaviourMachine>(watcher(&app))
        .expect("authored");

    assert!(machine.problems().is_empty(), "{:?}", machine.problems());
}

#[test]
fn a_pursuit_reproduces() {
    // Invariant I3 over the whole boundary: perception, machine and movement together, alongside
    // physics and the character. A fact computed from a distance is float arithmetic, and this is
    // what says it lands the same way twice.
    let run = || {
        let mut app = room();
        put_watcher_at(&mut app, IN_SIGHT);
        for tick in 0..120 {
            if tick == 40 {
                put_watcher_at(&mut app, OUT_OF_SIGHT);
            }
            app.run_ticks(1).expect("a tick runs");
        }
        (app.state_hash(), state(&app, watcher(&app)))
    };
    assert_eq!(run(), run());
}
