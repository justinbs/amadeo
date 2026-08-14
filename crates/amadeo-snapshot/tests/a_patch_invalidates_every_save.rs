//! What a patch does to a save file today, pinned rather than described.
//!
//! # Why this exists
//!
//! `games/atrium` saves by writing a `.snapshot`, and a snapshot is deliberately a **short-lived**
//! artefact — one moment of one run, no migration path, a mismatch refused rather than guessed at.
//! A save file is the opposite kind of thing: it belongs to a player and has to survive the game
//! being updated. **Q37** is that conflict, and this test is its evidence.
//!
//! The two builds are simulated in one process by giving two Rust types the same canonical name
//! with `#[reflect(name = "Thing")]` — which is what ADR 0017 makes identity mean, so as far as
//! every file, registry and state hash is concerned these *are* one component before and after a
//! patch.
//!
//! # The part that is not obvious, and is the reason this is a test rather than a paragraph
//!
//! Q37 records the expected fix as "restore leniently: take the fields the file has, default the
//! rest", and says that would make a save survive an added field **with no migration code at all**.
//! Measured, it does not. Leniency gets past the first error and into a second one: a defaulted
//! field is still a field, it is still hashed, and so the rebuilt world cannot hash to the number
//! the file recorded. `restore` checks that number and refuses.
//!
//! So the snapshot's integrity check and a save's survival of a patch are **structurally
//! exclusive**, not two strictnesses of one idea. Whatever Q37 decides has to say where that check
//! goes, and "be lenient about fields" on its own is not an answer. Both failures are asserted
//! below so that the day one of them changes, it changes here first.

use amadeo_core::StableHash;
use amadeo_ecs::{Component, ComponentRegistry, World};
use amadeo_reflect::{Reflect, Value};
use amadeo_snapshot::{RestoreError, capture, restore};

/// The component as the shipped build had it.
#[derive(Debug, Clone, PartialEq, StableHash, Reflect)]
#[reflect(name = "Thing")]
struct ThingV1 {
    /// A field that existed all along.
    a: f32,
}
impl Component for ThingV1 {}

/// The same component after a patch added one field — the smallest change a developer can make.
#[derive(Debug, Clone, PartialEq, StableHash, Reflect)]
#[reflect(name = "Thing")]
struct ThingV2 {
    /// A field that existed all along.
    a: f32,
    /// Added by the patch.
    b: f32,
}
impl Component for ThingV2 {}

/// Captures a one-entity world through the registry a build would have had.
fn save_from_the_shipped_build() -> amadeo_snapshot::Snapshot {
    let mut registry = ComponentRegistry::new();
    registry.register::<ThingV1>().expect("registers");

    let mut world = World::new();
    let entity = world.spawn();
    world.insert(entity, ThingV1 { a: 1.0 });

    capture(&world, &registry)
}

/// The registry the patched build would have.
fn the_patched_build() -> ComponentRegistry {
    let mut registry = ComponentRegistry::new();
    registry.register::<ThingV2>().expect("registers");
    registry
}

#[test]
fn adding_one_field_makes_an_existing_save_unreadable() {
    let save = save_from_the_shipped_build();
    let mut world = World::new();

    let error = restore(&mut world, &the_patched_build(), &save)
        .expect_err("a save written before the patch cannot be read after it");

    // `from_value` requires every field, so the file is rejected before anything is rebuilt. The
    // message names the component and the missing field, which is the one thing that is already
    // right about this failure.
    let RestoreError::BadComponent { reason, .. } = &error else {
        panic!("expected the value to be rejected, got {error:?}");
    };
    assert!(
        reason.contains("missing field `b`"),
        "the reason should name the field the file predates, got: {reason}"
    );
}

#[test]
fn defaulting_the_new_field_is_not_enough_because_the_hash_check_still_refuses() {
    // Exactly what a lenient restore would build: every field the file has, plus the new one at
    // its default. Patched here rather than implemented, so this test says what *would* happen
    // without committing the engine to a design Q37 has not chosen yet.
    let mut save = save_from_the_shipped_build();
    let Some(Value::Struct(fields)) = save.entities[0].components.get_mut("Thing") else {
        panic!("the captured component should be a struct");
    };
    fields.insert("b".to_string(), Value::F32(0.0));

    let mut world = World::new();
    let error = restore(&mut world, &the_patched_build(), &save)
        .expect_err("the rebuilt world cannot hash to a number computed without the new field");

    // The second wall, and the one Q37's sketch does not account for. Note what it means: the
    // world was rebuilt correctly and *then* refused, because the recorded hash belongs to a
    // component layout that no longer exists.
    assert!(
        matches!(error, RestoreError::HashMismatch { .. }),
        "expected the integrity check to refuse, got {error:?}"
    );
}
