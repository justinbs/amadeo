//! Every resource can describe itself, and survive the round trip out to a value and back.
//!
//! # What this file is defending
//!
//! ADR 0027 makes `Reflect` a bound on `Resource`, closing the half of invariant I8 that ADR 0013
//! left open for components. The bound alone only proves the impls *compile*. These tests prove they
//! are **correct**, which is the property snapshots will rest on: `snapshot.restore` is exactly
//! `from_value(to_value(x))` with a file in the middle, so a resource whose round trip loses
//! information is a resource whose restore silently produces a different world.
//!
//! The sharpest test here is [`a_restored_generator_produces_the_same_numbers`]. Equality is not
//! enough for `SimRng` — two generators can compare equal and diverge on the next draw if only part
//! of the state survived — so it checks the thing that actually matters.

use amadeo_app::SimRng;
use amadeo_core::Rng;
use amadeo_ecs::World;
use amadeo_events::{Event, EventClock, EventRecord, Events, WorldEvents};
use amadeo_input::{ActionId, InputState};
use amadeo_reflect::{Reflect, TypeKind, Value};

/// Asserts a value survives the trip out to `Value` and back.
fn round_trips<T: Reflect + PartialEq + std::fmt::Debug>(value: T) {
    let encoded = value.to_value();
    let decoded = T::from_value(&encoded).expect("round trip");
    assert_eq!(decoded, value);
}

// --- SimRng ---

#[test]
fn a_generator_round_trips_through_reflection() {
    let mut rng = Rng::new(12345);
    // Advanced first, so this is not testing a freshly seeded generator whose state might happen to
    // survive a lossy conversion.
    for _ in 0..37 {
        rng.next_u64();
    }
    round_trips(SimRng(rng));
}

#[test]
fn a_restored_generator_produces_the_same_numbers() {
    // The property that matters, and the one equality alone does not prove. A snapshot restores a
    // generator so the run continues identically; if only part of the state survived, the two would
    // still compare equal on some fields and then diverge on the next draw.
    let mut original = Rng::new(999);
    for _ in 0..10 {
        original.next_u64();
    }

    let restored = SimRng::from_value(&SimRng(original.clone()).to_value()).expect("round trip");
    let mut restored = restored.0;

    let from_original: Vec<u64> = (0..20).map(|_| original.next_u64()).collect();
    let from_restored: Vec<u64> = (0..20).map(|_| restored.next_u64()).collect();

    assert_eq!(from_original, from_restored);
}

#[test]
fn reflecting_a_generator_does_not_consume_it() {
    // `to_value` reads the state rather than drawing from it. Drawing would be the obvious way to
    // observe a generator and is exactly wrong: it would change the thing being inspected, so
    // looking at a world would perturb it.
    // What an untouched generator seeded this way draws first.
    let expected_first = Rng::new(7).next_u64();

    let observed = SimRng(Rng::new(7));
    let _ = observed.to_value();

    // Still draws the same first number, so observing it advanced nothing.
    let mut after = observed.0;
    assert_eq!(after.next_u64(), expected_first);
}

#[test]
fn a_generators_schema_names_both_of_its_words() {
    let TypeKind::Struct { fields } = SimRng::type_info().kind else {
        panic!("SimRng is a struct");
    };
    let names: Vec<&str> = fields.iter().map(|f| f.name.as_str()).collect();
    assert_eq!(names, vec!["state", "increment"]);
    // Every field carries docs, because that is the agent's only description of what it means.
    assert!(fields.iter().all(|f| !f.docs.is_empty()));
}

#[test]
fn an_even_increment_is_repaired_rather_than_accepted() {
    // A hand-edited or corrupted value. An even increment halves the generator's period, which is a
    // silent statistical fault, so `Rng::from_state` forces it odd.
    let hand_written = Value::structure([("state", Value::U64(1)), ("increment", Value::U64(4))]);
    let repaired = SimRng::from_value(&hand_written).expect("accepted");
    assert_eq!(
        repaired.0.state()[1],
        5,
        "an even increment must be forced odd"
    );
}

#[test]
fn a_generator_missing_a_field_says_which_one() {
    let partial = Value::structure([("state", Value::U64(1))]);
    let error = SimRng::from_value(&partial).expect_err("incomplete");
    let message = error.to_string();

    assert!(message.contains("increment"), "{message}");
    assert!(message.contains("SimRng"), "{message}");
}

// --- InputState, the resource that needed maps ---

#[test]
fn input_state_round_trips_with_real_actions() {
    let mut state = InputState::new();
    state.set_button(ActionId::new("jump"), true);
    state.set_axis(ActionId::new("move_x"), -0.75);
    state.begin_tick();
    state.set_button(ActionId::new("jump"), false);

    round_trips(state);
}

#[test]
fn input_state_reflects_as_two_maps() {
    // The shape ADR 0027's map variant exists for. Both maps are present even when empty, so a
    // reader never has to distinguish "no buttons" from "no buttons field".
    let state = InputState::new();
    let value = state.to_value();

    assert!(
        matches!(value.field("buttons"), Some(Value::Map(_))),
        "{value}"
    );
    assert!(
        matches!(value.field("axes"), Some(Value::Map(_))),
        "{value}"
    );
}

#[test]
fn an_input_map_is_keyed_by_the_action_id() {
    // Faithful, and deliberately not readable -- an `ActionId` is a hash whose name is not kept.
    // Recorded as a known gap rather than papered over; the fix belongs in the protocol layer,
    // which can join these against the input driver's name table.
    let mut state = InputState::new();
    let jump = ActionId::new("jump");
    state.set_button(jump, true);

    let value = state.to_value();
    let buttons = value.field("buttons").expect("buttons");
    assert!(
        buttons.entry(&jump.raw().to_string()).is_some(),
        "expected the raw id as the key, got {buttons}"
    );
}

#[test]
fn an_empty_input_state_round_trips() {
    round_trips(InputState::new());
}

// --- Events ---

/// A test event. Needs `Reflect` now, which is the whole point.
#[derive(Debug, Clone, PartialEq, Eq, amadeo_core::StableHash, Reflect)]
struct Landed {
    /// Which entity landed.
    entity_index: u32,
}

impl Event for Landed {}

#[test]
fn an_event_record_round_trips() {
    round_trips(EventRecord {
        sequence: 42,
        tick: amadeo_core::Tick(7),
        event: Landed { entity_index: 3 },
    });
}

#[test]
fn an_event_queue_round_trips_both_of_its_buffers() {
    // Both buffers are simulation state: an event sent this tick has not been read yet but will be,
    // so a snapshot that captured only the read buffer would restore a world that skipped a tick's
    // worth of events.
    let mut queue = Events::<Landed>::default();
    queue.send(Landed { entity_index: 1 }, 0, amadeo_core::Tick(1));
    queue.swap();
    queue.send(Landed { entity_index: 2 }, 1, amadeo_core::Tick(2));

    let restored = Events::<Landed>::from_value(&queue.to_value()).expect("round trip");

    assert_eq!(restored.read().len(), 1, "the read buffer survived");
    assert_eq!(restored.read()[0].event, Landed { entity_index: 1 });
    assert_eq!(
        restored.read_pending().len(),
        1,
        "the write buffer survived too"
    );
    assert_eq!(restored.read_pending()[0].event, Landed { entity_index: 2 });
}

#[test]
fn an_event_queues_schema_names_both_buffers() {
    let TypeKind::Struct { fields } = Events::<Landed>::type_info().kind else {
        panic!("Events is a struct");
    };
    let names: Vec<&str> = fields.iter().map(|f| f.name.as_str()).collect();
    assert_eq!(names, vec!["reading", "writing"]);
}

// --- EventClock ---

#[test]
fn the_event_clock_round_trips() {
    let mut clock = EventClock::default();
    for _ in 0..5 {
        clock.take_sequence();
    }
    assert_eq!(clock.sent_count(), 5);
    round_trips(clock);
}

// --- The invariant, end to end ---

#[test]
fn reflecting_every_resource_in_a_world_leaves_it_unchanged() {
    // Invariant I8 is about *observing*. Looking at a world must never perturb it, or an agent
    // asking a question would change the answer to the next one.
    let mut world = World::new();
    world.insert_resource(SimRng(Rng::new(4)));
    world.insert_resource(InputState::new());
    world.register_event::<Landed>();
    world.send_event(Landed { entity_index: 9 });

    let before = world.state_hash();

    let _ = world.resource::<SimRng>().expect("present").to_value();
    let _ = world.resource::<InputState>().expect("present").to_value();
    let _ = world.resource::<EventClock>().expect("present").to_value();

    assert_eq!(world.state_hash(), before);
}
