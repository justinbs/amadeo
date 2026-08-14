//! A state machine, headless — ADR 0068.
//!
//! # What these are really pinning
//!
//! Not that a state changes. That the machine is a **pure function of the file and the facts**:
//! transitions are tried in authored order, at most one fires per tick, and nothing about the result
//! depends on how the map was built or which entity ran first. Every one of those, got wrong, gives a
//! monster that behaves *nearly* right — which is the hardest kind of AI bug to see.

use amadeo_behaviour::{
    Behaviour, BehaviourChanged, BehaviourMachine, BehaviourState, Facts, Transition,
    run_behaviours,
};
use amadeo_core::FIXED_DT;
use amadeo_ecs::{Entity, World};
use amadeo_events::WorldEvents;

/// The stalker from `docs/05`'s exit gate: idle, pursue, search, and back.
fn stalker() -> BehaviourMachine {
    BehaviourMachine {
        initial: "idle".to_string(),
        states: vec![
            BehaviourState {
                name: "idle".to_string(),
                transitions: vec![Transition::when("pursue", "sees_player")],
            },
            BehaviourState {
                name: "pursue".to_string(),
                transitions: vec![Transition::unless("search", "sees_player")],
            },
            BehaviourState {
                name: "search".to_string(),
                transitions: vec![
                    // Order matters here and is the point: seeing the player again beats giving up,
                    // even on the tick both would fire.
                    Transition::when("pursue", "sees_player"),
                    Transition::after("idle", 0.5),
                ],
            },
        ],
    }
}

/// A world with one monster running `machine`.
fn monster(machine: BehaviourMachine) -> (World, Entity) {
    let mut world = World::new();
    world.register_event::<BehaviourChanged>();

    let entity = world.spawn();
    world.insert(entity, machine);
    world.insert(entity, Behaviour::default());
    world.insert(entity, Facts::default());
    (world, entity)
}

fn state(world: &World, entity: Entity) -> String {
    world
        .get::<Behaviour>(entity)
        .expect("still there")
        .state
        .clone()
}

fn know(world: &mut World, entity: Entity, fact: &str, value: bool) {
    if let Some(facts) = world.get_mut::<Facts>(entity) {
        facts.set(fact, value);
    }
}

fn changes(world: &mut World) -> Vec<(String, String)> {
    world.swap_events::<BehaviourChanged>();
    world
        .read_events::<BehaviourChanged>()
        .iter()
        .map(|record| (record.event.from.clone(), record.event.to.clone()))
        .collect()
}

#[test]
fn it_starts_in_the_initial_state() {
    let (mut world, monster) = monster(stalker());
    run_behaviours(&mut world);
    assert_eq!(state(&world, monster), "idle");
}

#[test]
fn seeing_the_player_starts_a_chase() {
    let (mut world, monster) = monster(stalker());
    run_behaviours(&mut world);

    know(&mut world, monster, "sees_player", true);
    run_behaviours(&mut world);
    assert_eq!(state(&world, monster), "pursue");
}

#[test]
fn losing_sight_starts_a_search_and_it_gives_up_after_a_while() {
    let (mut world, monster) = monster(stalker());
    run_behaviours(&mut world);
    know(&mut world, monster, "sees_player", true);
    run_behaviours(&mut world);

    know(&mut world, monster, "sees_player", false);
    run_behaviours(&mut world);
    assert_eq!(state(&world, monster), "search");

    // Half a second is thirty ticks. It must not give up before then, which is the half a naive
    // implementation gets wrong by comparing against zero.
    for _ in 0..20 {
        run_behaviours(&mut world);
    }
    assert_eq!(state(&world, monster), "search", "too early");

    for _ in 0..20 {
        run_behaviours(&mut world);
    }
    assert_eq!(state(&world, monster), "idle");
}

#[test]
fn the_first_matching_transition_wins() {
    // **The rule that makes the machine a pure function of the file.** In `search`, both "I see them
    // again" and "I have waited long enough" can hold on the same tick; the authored order says the
    // chase wins, and an implementation that picked by any other rule would give up on a monster
    // that is looking right at you.
    let (mut world, monster) = monster(stalker());
    run_behaviours(&mut world);
    know(&mut world, monster, "sees_player", true);
    run_behaviours(&mut world);
    know(&mut world, monster, "sees_player", false);
    run_behaviours(&mut world);

    // Wait past the give-up time *and* see the player, in the same tick.
    for _ in 0..40 {
        if state(&world, monster) != "search" {
            break;
        }
        know(&mut world, monster, "sees_player", true);
        run_behaviours(&mut world);
    }
    assert_eq!(state(&world, monster), "pursue");
}

#[test]
fn at_most_one_transition_fires_per_tick() {
    // A machine that chained transitions within a tick could cross its whole graph in one frame,
    // which makes "what state is it in" a property of the graph's shape rather than of time — and
    // turns a cycle into a hang rather than an oscillation somebody can watch.
    let (mut world, monster) = monster(BehaviourMachine {
        initial: "a".to_string(),
        states: vec![
            BehaviourState {
                name: "a".to_string(),
                transitions: vec![Transition::after("b", 0.0)],
            },
            BehaviourState {
                name: "b".to_string(),
                transitions: vec![Transition::after("c", 0.0)],
            },
            BehaviourState {
                name: "c".to_string(),
                transitions: Vec::new(),
            },
        ],
    });

    run_behaviours(&mut world);
    assert_eq!(state(&world, monster), "a", "the first tick only enters");
    run_behaviours(&mut world);
    assert_eq!(state(&world, monster), "b");
    run_behaviours(&mut world);
    assert_eq!(state(&world, monster), "c");
}

#[test]
fn entering_a_state_is_an_event_carrying_both_ends() {
    // The "on enter" hook without a callback: a game plays a roar on entering `pursue` by reading
    // these, and the module never learns what a roar is.
    let (mut world, monster) = monster(stalker());
    run_behaviours(&mut world);
    assert_eq!(
        changes(&mut world),
        vec![(String::new(), "idle".to_string())]
    );

    know(&mut world, monster, "sees_player", true);
    run_behaviours(&mut world);
    assert_eq!(
        changes(&mut world),
        vec![("idle".to_string(), "pursue".to_string())]
    );
}

#[test]
fn a_tick_that_changes_nothing_says_nothing() {
    // Otherwise a game reacting to `BehaviourChanged` roars sixty times a second for as long as the
    // monster is chasing, which is a bug that is *audible* rather than visible.
    let (mut world, _) = monster(stalker());
    run_behaviours(&mut world);
    let _ = changes(&mut world);

    run_behaviours(&mut world);
    assert!(changes(&mut world).is_empty());
}

#[test]
fn an_unknown_fact_is_false_rather_than_a_failure() {
    // A machine naming a fact the game has not written yet is inert, not broken. That matters
    // because the game's perception systems and the machine are authored by different hands.
    let (mut world, monster) = monster(stalker());
    run_behaviours(&mut world);
    run_behaviours(&mut world);
    assert_eq!(state(&world, monster), "idle");
}

#[test]
fn the_elapsed_clock_resets_on_entry() {
    // Otherwise "after six seconds" means "six seconds since the machine started", and every timed
    // transition after the first fires immediately.
    let (mut world, monster) = monster(stalker());
    run_behaviours(&mut world);
    for _ in 0..30 {
        run_behaviours(&mut world);
    }

    know(&mut world, monster, "sees_player", true);
    run_behaviours(&mut world);

    let elapsed = world.get::<Behaviour>(monster).expect("there").elapsed;
    assert_eq!(elapsed, 0.0, "entering a state restarts its clock");
}

#[test]
fn time_advances_by_the_fixed_step() {
    let (mut world, monster) = monster(stalker());
    run_behaviours(&mut world);
    run_behaviours(&mut world);

    let elapsed = world.get::<Behaviour>(monster).expect("there").elapsed;
    assert!((elapsed - FIXED_DT).abs() < 1e-6, "got {elapsed}");
}

#[test]
fn a_transition_to_a_state_that_does_not_exist_never_fires() {
    // And is reported rather than silent — see the next test. A monster that stops transitioning
    // looks like an AI bug and is a spelling one.
    let (mut world, monster) = monster(BehaviourMachine {
        initial: "idle".to_string(),
        states: vec![BehaviourState {
            name: "idle".to_string(),
            transitions: vec![Transition::after("prusue", 0.0)],
        }],
    });

    for _ in 0..5 {
        run_behaviours(&mut world);
    }
    assert_eq!(state(&world, monster), "idle");
}

#[test]
fn the_faults_that_run_anyway_are_all_reported() {
    let machine = BehaviourMachine {
        initial: "nowhere".to_string(),
        states: vec![
            BehaviourState {
                name: "idle".to_string(),
                transitions: vec![Transition::after("prusue", 1.0)],
            },
            BehaviourState {
                name: "idle".to_string(),
                transitions: Vec::new(),
            },
        ],
    };

    let problems = machine.problems();
    assert_eq!(problems.len(), 3, "got {problems:?}");
    assert!(
        problems.iter().any(|line| line.contains("prusue")),
        "the typo should be quoted: {problems:?}"
    );
    assert!(
        problems.iter().any(|line| line.contains("nowhere")),
        "an initial state that is not a state: {problems:?}"
    );
    assert!(
        problems.iter().any(|line| line.contains("both called")),
        "a duplicate state name: {problems:?}"
    );
}

#[test]
fn a_machine_with_nothing_wrong_reports_nothing() {
    assert!(
        stalker().problems().is_empty(),
        "{:?}",
        stalker().problems()
    );
}

#[test]
fn two_monsters_do_not_interfere() {
    // Each machine reads only its own facts. Sharing state between entities is the bug that turns
    // one monster spotting you into every monster spotting you.
    let (mut world, first) = monster(stalker());
    let second = world.spawn();
    world.insert(second, stalker());
    world.insert(second, Behaviour::default());
    world.insert(second, Facts::default());

    run_behaviours(&mut world);
    know(&mut world, first, "sees_player", true);
    run_behaviours(&mut world);

    assert_eq!(state(&world, first), "pursue");
    assert_eq!(state(&world, second), "idle");
}

#[test]
fn a_machine_reproduces() {
    // Invariant I3. The clock is `+` on a fixed step, the facts are a `BTreeMap` which iterates in
    // key order, and transitions are tried in authored order — nothing here can differ between two
    // runs or two machines.
    let run = || {
        let (mut world, monster) = monster(stalker());
        for tick in 0..90 {
            know(&mut world, monster, "sees_player", tick % 17 < 4);
            run_behaviours(&mut world);
        }
        (world.state_hash(), state(&world, monster))
    };
    assert_eq!(run(), run());
}
