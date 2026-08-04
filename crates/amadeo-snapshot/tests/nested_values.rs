//! A snapshot has to carry every shape a component can be — including the ones ADR 0032 added.
//!
//! # Why this exists
//!
//! Session 8 found the same defect twice, both times by snapshotting a real game and *reading the
//! file*. First `InputState`'s maps came out in `Display` form; then, after ADR 0032 made enum
//! payloads authorable, `Projection::Orthographic { height }` came out as
//! `Orthographic({height: 8})`. Both are the worst kind of broken — a snapshot that captures and
//! then refuses to restore looks like it worked until you need it.
//!
//! So this is not a unit test of the writer. It builds a world holding every awkward shape, captures
//! it, restores it into a fresh world, and asserts the **state hashes match** — which is the claim
//! the whole format exists to make.

use amadeo_core::StableHash;
use amadeo_ecs::{Component, ComponentRegistry, World};
use amadeo_reflect::Reflect;
use amadeo_snapshot::{capture, parse, restore, to_text};

/// Every shape a *hashed* component can hold, in one type.
///
/// No map: `BTreeMap` does not implement `StableHash`, so one cannot appear in a component at all.
/// Maps reach a snapshot only through resources such as `InputState`, and that path is already
/// covered — it is what the session-8 map fix was for.
#[derive(Debug, Clone, PartialEq, StableHash, Reflect)]
struct Awkward {
    /// A nested struct.
    nested: Inner,
    /// An enum variant carrying data — the shape ADR 0032 added.
    choice: Choice,
    /// A fieldless variant, which writes inline and must stay that way.
    plain: Choice,
    /// A list of lists.
    routes: Vec<[f32; 2]>,
}
impl Component for Awkward {}

/// A plain nested struct.
#[derive(Debug, Clone, PartialEq, StableHash, Reflect)]
struct Inner {
    /// How far.
    distance: f32,
    /// Deeper still, because one level of nesting proves less than two.
    deeper: Deepest,
}

/// Two levels down.
#[derive(Debug, Clone, PartialEq, StableHash, Reflect)]
struct Deepest {
    /// A leaf.
    leaf: bool,
}

/// An enum with a fieldless variant and one carrying data.
#[derive(Debug, Clone, PartialEq, StableHash, Reflect)]
enum Choice {
    /// Nothing to carry.
    Idle,
    /// Carries two fields, so the payload is a struct rather than a single value.
    Chasing {
        /// How fast.
        speed: f32,
        /// For how long.
        ticks: u32,
    },
}

fn awkward() -> Awkward {
    Awkward {
        nested: Inner {
            distance: 2.5,
            deeper: Deepest { leaf: true },
        },
        choice: Choice::Chasing {
            speed: 4.25,
            ticks: 12,
        },
        plain: Choice::Idle,
        routes: vec![[0.0, 0.0], [3.0, -1.5]],
    }
}

fn registry() -> ComponentRegistry {
    let mut registry = ComponentRegistry::new();
    registry.register::<Awkward>().expect("registers");
    registry
}

#[test]
fn every_value_shape_survives_a_snapshot() {
    let registry = registry();
    let mut world = World::new();
    let entity = world.spawn();
    world.insert(entity, awkward());

    let text = to_text(&capture(&world, &registry));
    let parsed = parse(&text).unwrap_or_else(|error| panic!("{error}\n{text}"));

    let mut restored = World::new();
    restore(&mut restored, &registry, &parsed).expect("restores");

    assert_eq!(
        restored.state_hash(),
        world.state_hash(),
        "the restored world must be the same world\n{text}"
    );

    // And the component itself came back byte-for-byte, not merely hash-equal.
    let entity = restored.entities()[0];
    assert_eq!(restored.get::<Awkward>(entity), Some(&awkward()));
}

#[test]
fn the_text_is_readable_rather_than_a_debug_dump() {
    // The specific regression: a payload enum used to come out as `Chasing({speed: 4.25, ...})`,
    // which is Rust's `Debug` and which nothing parses. It must be the same indented form a `.scene`
    // file uses.
    let registry = registry();
    let mut world = World::new();
    let entity = world.spawn();
    world.insert(entity, awkward());

    let text = to_text(&capture(&world, &registry));

    assert!(text.contains("choice Chasing"), "{text}");
    assert!(text.contains("speed 4.25"), "{text}");
    assert!(!text.contains("({"), "no Debug form should appear:\n{text}");
    // A fieldless variant stays on one line.
    assert!(text.contains("plain Idle"), "{text}");
}

#[test]
fn a_snapshot_is_byte_stable() {
    // Invariant I2 for this format too: writing what was read produces the same bytes.
    let registry = registry();
    let mut world = World::new();
    let entity = world.spawn();
    world.insert(entity, awkward());

    let once = to_text(&capture(&world, &registry));
    let twice = to_text(&parse(&once).expect("parses"));
    assert_eq!(once, twice);
}
