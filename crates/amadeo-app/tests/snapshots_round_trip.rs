//! A snapshot of a world with the things a real game has must read back.
//!
//! # Why this file exists rather than more tests inside `amadeo-snapshot`
//!
//! That crate's tests build worlds by hand, and hand-built worlds are exactly where this class of
//! bug hides: **nobody writes an empty collection into a fixture on purpose.**
//!
//! An empty list had no spelling in the format. Joining a list's elements with spaces gives the
//! empty string when there are no elements, so an empty `Vec` wrote as a field name with a trailing
//! space and no value — which is not something the format has, and which parses back as `Unit`.
//!
//! Every **registered event queue** holds two empty lists at rest, one reading and one writing. So
//! `amadeo snapshot` followed by `amadeo status --from` failed on `games/atrium`, and had done since
//! events were first registered. Nothing noticed, because until save and load nothing had ever
//! restored a real game's snapshot.
//!
//! So these are built through `App` — the thing games actually use — and every case is a shape that
//! turns up in a real world and would never turn up in a fixture.

use amadeo_app::App;
use amadeo_core::StableHash;
use amadeo_ecs::{Component, Resource};
use amadeo_events::Event;
use amadeo_reflect::Reflect;

/// An event, so the world carries the queues that broke the format.
#[derive(Debug, Clone, Copy, PartialEq, Eq, StableHash, Reflect)]
struct Nudged {
    /// How hard.
    strength: i64,
}

impl Event for Nudged {}

/// A component holding the collections a real one holds.
#[derive(Debug, Clone, PartialEq, Default, StableHash, Reflect)]
struct Bag {
    /// A list that is usually empty — the case that broke.
    contents: Vec<i64>,
    /// A list with one thing in it, which the format cannot spell inline either.
    just_one: Vec<f32>,
    /// A list with several, which always worked.
    several: Vec<f32>,
}

impl Component for Bag {}

/// A resource, since those travel by a different path in the format than components do.
#[derive(Debug, Clone, PartialEq, Default, StableHash, Reflect)]
struct Ledger {
    /// Empty at rest.
    entries: Vec<i64>,
}

impl Resource for Ledger {}

/// An app with a registered event, a component and a resource, all as a game would have them.
fn app() -> App {
    let mut app = App::with_seed(11);
    app.register_component::<Bag>().expect("registers");
    app.register_event::<Nudged>();
    app.insert_resource(Ledger::default());
    app
}

/// Captures, writes, reads and restores — the whole path `amadeo snapshot` and `--from` take.
fn round_trip(source: &App) -> App {
    let text = amadeo_snapshot::to_text(&source.capture_snapshot());
    let snapshot = amadeo_snapshot::parse(&text).unwrap_or_else(|error| {
        panic!("the engine wrote a snapshot it cannot read: {error}\n\n{text}")
    });

    let mut restored = app();
    restored
        .restore_snapshot(&snapshot)
        .unwrap_or_else(|error| panic!("the snapshot would not restore: {error}\n\n{text}"));
    restored
}

#[test]
fn a_world_with_a_registered_event_round_trips() {
    // **The regression.** A registered event queue holds two empty lists at rest and nothing else;
    // that alone made every snapshot of `games/atrium` unrestorable, and would have made every save
    // in every game unloadable.
    let mut app = app();
    app.run_ticks(3).expect("ticks run");

    let restored = round_trip(&app);
    assert_eq!(restored.state_hash(), app.state_hash());
}

#[test]
fn an_empty_list_survives_the_trip() {
    // The value that has no natural spelling. It is written `[]` for the reason `Unit` is written
    // `()`: a field with nothing after it is not something this format has.
    let mut app = app();
    let entity = app.world.spawn();
    app.world.insert(entity, Bag::default());

    let restored = round_trip(&app);
    assert_eq!(
        restored.world.get::<Bag>(entity),
        Some(&Bag::default()),
        "an empty list must come back empty rather than as a unit or a missing field"
    );
}

#[test]
fn lists_of_one_and_of_several_both_survive() {
    // A one-element list has no inline spelling either — `22.0` is a scalar, and layer 1 of the
    // format has no schema to tell it from a list of one. The *type* resolves that on the way back
    // in, and this is what says so.
    let mut app = app();
    let entity = app.world.spawn();
    let bag = Bag {
        contents: vec![4, 5, 6],
        just_one: vec![22.0],
        several: vec![1.5, 2.5],
    };
    app.world.insert(entity, bag.clone());

    let restored = round_trip(&app);
    assert_eq!(restored.world.get::<Bag>(entity), Some(&bag));
}

#[test]
fn an_empty_collection_in_a_resource_survives_too() {
    // Resources are written by a different path than components in this format, so the fix has to
    // reach both. It is one shared `inline_value`, and this is what keeps that true.
    let mut app = app();
    if let Some(ledger) = app.world.resource_mut::<Ledger>() {
        ledger.entries.clear();
    }

    let restored = round_trip(&app);
    assert_eq!(
        restored.world.resource::<Ledger>(),
        Some(&Ledger::default())
    );
}

#[test]
fn the_text_says_what_an_empty_list_is_rather_than_trailing_off() {
    // Pinned as *text*, because the failure was invisible in the value tree — the writer produced a
    // line ending in a space, which reads as a field with no value. If this ever regresses, the
    // round trip above fails somewhere confusing; this fails where the mistake is.
    let app = app();
    let text = amadeo_snapshot::to_text(&app.capture_snapshot());

    assert!(
        text.contains("[]"),
        "an empty list should be spelled:\n{text}"
    );
    for line in text.lines() {
        assert_eq!(
            line.trim_end(),
            line,
            "no line may end in whitespace — that is what a value-less field looked like"
        );
    }
}
