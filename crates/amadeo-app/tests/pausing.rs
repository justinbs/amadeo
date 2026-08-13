//! What a paused tick does — ADR 0065.
//!
//! # Why this is its own file
//!
//! Pausing is the first thing that makes two ticks *not* the same shape as each other, which is a
//! sharp edge in a project arranged around the idea that they are. Every assertion here is about
//! something that would be a plausible-looking bug rather than a crash: a game carrying on quietly
//! under a menu, a menu that cannot be closed, or a pause that plays back differently than it ran.

use amadeo_app::{App, Paused, Stage, system};
use amadeo_core::{FIXED_DT_NANOS, StableHash};
use amadeo_ecs::{Resource, World};
use amadeo_reflect::Reflect;

/// A counter each test's systems bump, so "did it run" is a number rather than a side effect.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default, StableHash, Reflect)]
struct Ran {
    pre: i64,
    sim: i64,
    post: i64,
    menu: i64,
}

impl Resource for Ran {}

fn bump(field: fn(&mut Ran)) -> impl FnMut(&mut World) {
    move |world: &mut World| {
        if let Some(counts) = world.resource_mut::<Ran>() {
            field(counts);
        }
    }
}

/// An app with one system in each simulation stage, plus one flagged `while_paused`.
fn app() -> App {
    let mut app = App::with_seed(7);
    app.insert_resource(Ran::default());

    app.add_system(
        Stage::PreSimulation,
        system("count_pre", bump(|counts| counts.pre += 1)),
    );
    app.add_system(
        Stage::Simulation,
        system("count_sim", bump(|counts| counts.sim += 1)),
    );
    app.add_system(
        Stage::PostSimulation,
        system("count_post", bump(|counts| counts.post += 1)),
    );
    app.add_system(
        Stage::Simulation,
        system("count_menu", bump(|counts| counts.menu += 1)).while_paused(),
    );
    app
}

fn counts(app: &App) -> Ran {
    *app.world.resource::<Ran>().expect("installed")
}

#[test]
fn a_game_with_no_pause_resource_is_never_paused() {
    // The default, and the one that must cost nothing: a game with no menu should not have to know
    // this feature exists.
    let mut app = app();
    app.run_ticks(3).expect("schedule resolves");

    let ran = counts(&app);
    assert_eq!((ran.pre, ran.sim, ran.post, ran.menu), (3, 3, 3, 3));
}

#[test]
fn pausing_stops_gameplay_and_keeps_the_menu() {
    let mut app = app();
    app.insert_resource(Paused { paused: true });
    app.run_ticks(4).expect("schedule resolves");

    let ran = counts(&app);
    assert_eq!(ran.sim, 0, "gameplay must stop");
    assert_eq!(ran.menu, 4, "the flagged system must not");
}

#[test]
fn post_simulation_is_skipped_too_and_that_is_not_an_afterthought() {
    // **The trap this exists to name.** `games/atrium` runs `play_footsteps` in `PostSimulation`,
    // and it reads the character's velocity — which does not change while paused. Skip only
    // `Simulation` and a paused game taps out footsteps forever, in a room nobody is walking
    // through. The symptom is audio, so nothing about it points at the scheduler.
    let mut app = app();
    app.insert_resource(Paused { paused: true });
    app.run_ticks(5).expect("schedule resolves");

    assert_eq!(counts(&app).post, 0);
}

#[test]
fn pre_simulation_always_runs_so_a_game_can_unpause_itself() {
    // Input is sampled in `PreSimulation`. A pause that stopped it would be a pause nothing could
    // ever end — which is not a subtle failure, but it is a total one.
    let mut app = app();
    app.insert_resource(Paused { paused: true });
    app.run_ticks(6).expect("schedule resolves");

    assert_eq!(counts(&app).pre, 6);
}

#[test]
fn the_tick_never_stops() {
    // Load-bearing, and the reason is not obvious: menu navigation is hashed state driven by hashed
    // input, and `amadeo-input` records input **per tick**. Freeze the counter and a keypress in a
    // menu has nowhere in a replay to live.
    let mut app = app();
    app.insert_resource(Paused { paused: true });

    let before = app.tick();
    app.run_ticks(10).expect("schedule resolves");
    assert_eq!(app.tick().0, before.0 + 10);
}

#[test]
fn a_pause_banks_no_backlog_to_burst_through_on_release() {
    // Q35 asked whether unpausing would run a flood of catch-up ticks. It cannot, and the reason is
    // the one above: `advance_real_time` keeps consuming its accumulator on cheap paused ticks, so
    // there is never anything banked. Nothing in the loop needed changing, which is the whole
    // argument for advancing the tick rather than freezing it.
    let mut app = app();
    app.insert_resource(Paused { paused: true });

    // Ten frames of real time go by while paused.
    for _ in 0..10 {
        app.advance_real_time(FIXED_DT_NANOS)
            .expect("schedule resolves");
    }

    if let Some(state) = app.world.resource_mut::<Paused>() {
        state.paused = false;
    }
    let after_release = app
        .advance_real_time(FIXED_DT_NANOS)
        .expect("schedule resolves");

    assert_eq!(
        after_release, 1,
        "one frame of real time should still be one tick, not eleven"
    );
}

#[test]
fn a_pause_reproduces_exactly() {
    // Invariant I3 across the new branch. Two apps paused and released on the same ticks must agree
    // bit for bit — including the tick counter, which is what a naive "freeze everything" pause
    // would quietly desynchronise between a live run and its replay.
    let run = || {
        let mut app = app();
        app.insert_resource(Paused::default());
        app.run_ticks(5).expect("resolves");
        if let Some(state) = app.world.resource_mut::<Paused>() {
            state.paused = true;
        }
        app.run_ticks(20).expect("resolves");
        if let Some(state) = app.world.resource_mut::<Paused>() {
            state.paused = false;
        }
        app.run_ticks(5).expect("resolves");
        (app.state_hash(), counts(&app), app.tick())
    };

    let (hash, ran, tick) = run();
    assert_eq!(run(), (hash, ran, tick));

    // And the pause really happened, so this is not two identical unpaused runs agreeing.
    assert_eq!(ran.sim, 10, "gameplay ran for the ten unpaused ticks only");
    assert_eq!(ran.pre, 30, "every tick sampled input");
    assert_eq!(tick.0, 30);
}

#[test]
fn the_flagged_systems_are_visible_to_an_agent() {
    // `schedule.list` reports these, so "why did my system not run" is answerable without reading
    // the game's source — invariant I5's standard applied to a scheduling rule.
    let mut app = app();
    assert_eq!(
        app.while_paused_order(Stage::Simulation).expect("resolves"),
        vec!["count_menu"]
    );
    assert!(
        app.while_paused_order(Stage::PostSimulation)
            .expect("resolves")
            .is_empty()
    );
}
