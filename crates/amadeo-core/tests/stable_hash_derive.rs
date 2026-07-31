//! End-to-end tests for `#[derive(StableHash)]`.
//!
//! These pin properties that golden replay assertions depend on. If one of them breaks, replays
//! stop catching real divergence — the failure this derive exists to prevent.

use amadeo_core::{StableHash, StableHasher, stable_hash_of};

#[derive(StableHash)]
struct Velocity {
    x: f32,
    y: f32,
}

#[test]
fn every_field_contributes() {
    // The property a forgotten field would break: changing any field must move the hash.
    let baseline = stable_hash_of(&Velocity { x: 1.0, y: 2.0 });
    assert_ne!(baseline, stable_hash_of(&Velocity { x: 9.0, y: 2.0 }));
    assert_ne!(baseline, stable_hash_of(&Velocity { x: 1.0, y: 9.0 }));
    assert_eq!(baseline, stable_hash_of(&Velocity { x: 1.0, y: 2.0 }));
}

/// The same logical data, with the fields declared in the opposite order.
#[derive(StableHash)]
struct ReorderedVelocity {
    y: f32,
    x: f32,
}

#[test]
fn declaration_order_does_not_affect_the_hash() {
    // Reordering fields is a pure refactor. If it changed the fingerprint, every committed replay
    // in the project would need regenerating for a change with no behavioural meaning.
    assert_eq!(
        stable_hash_of(&Velocity { x: 1.0, y: 2.0 }),
        stable_hash_of(&ReorderedVelocity { y: 2.0, x: 1.0 })
    );
}

#[test]
fn the_derive_matches_an_equivalent_hand_written_impl() {
    // Hashing x then y by hand -- alphabetical, which is what the derive does.
    let mut by_hand = StableHasher::new();
    1.5f32.stable_hash(&mut by_hand);
    (-2.5f32).stable_hash(&mut by_hand);

    assert_eq!(
        stable_hash_of(&Velocity { x: 1.5, y: -2.5 }),
        by_hand.finish()
    );
}

#[derive(StableHash)]
struct WithCache {
    authoritative: u32,
    /// Derived every tick and never serialised, so it must not enter the fingerprint either.
    ///
    /// Never read, which is exactly what being skipped means — the `allow` documents that rather
    /// than working around it.
    #[reflect(skip)]
    #[allow(dead_code)]
    cached: f32,
}

#[test]
fn skipped_fields_are_excluded() {
    // A skipped field does not round-trip through serialisation, so including it here would make a
    // reloaded value hash differently from the original and break save/load comparison.
    let with_cache = WithCache {
        authoritative: 1,
        cached: 100.0,
    };
    let without_cache = WithCache {
        authoritative: 1,
        cached: -7.0,
    };
    assert_eq!(
        stable_hash_of(&with_cache),
        stable_hash_of(&without_cache),
        "a skipped field must not influence the hash"
    );

    // ...but the field that is not skipped still does.
    let different = WithCache {
        authoritative: 2,
        cached: 100.0,
    };
    assert_ne!(stable_hash_of(&with_cache), stable_hash_of(&different));
}

#[derive(StableHash)]
struct Player;

#[test]
fn a_unit_struct_contributes_nothing() {
    // Its presence is already recorded by the component id the archetype writes before hashing the
    // value, so the value itself has nothing to add.
    assert_eq!(stable_hash_of(&Player), StableHasher::new().finish());
}

#[derive(StableHash)]
struct Score(u32);

#[derive(StableHash)]
struct Bounds(f32, f32);

#[test]
fn tuple_structs_hash_their_fields_positionally() {
    assert_ne!(stable_hash_of(&Score(1)), stable_hash_of(&Score(2)));
    assert_eq!(stable_hash_of(&Score(7)), stable_hash_of(&Score(7)));

    // Positional fields have no names to sort by, so order is the identity and swapping matters.
    assert_ne!(
        stable_hash_of(&Bounds(1.0, 2.0)),
        stable_hash_of(&Bounds(2.0, 1.0))
    );
}

#[derive(StableHash)]
enum EnemyState {
    Patrol,
    Search { last_seen: [f32; 2], patience: u32 },
    Pursue,
}

/// The same enum with a variant inserted in the middle.
///
/// Used to prove the hash keys on variant *names*, not on positional indices. `Patrol` and
/// `Fleeing` are never constructed on purpose: their job is to sit *before* `Search` and `Pursue`
/// and shift their indices, which is the thing being tested.
#[derive(StableHash)]
#[allow(dead_code)]
enum EnemyStateWithExtra {
    Patrol,
    Fleeing,
    Search { last_seen: [f32; 2], patience: u32 },
    Pursue,
}

#[test]
fn enum_variants_are_distinguished() {
    assert_ne!(
        stable_hash_of(&EnemyState::Patrol),
        stable_hash_of(&EnemyState::Pursue)
    );
}

#[test]
fn variant_payloads_contribute() {
    let baseline = EnemyState::Search {
        last_seen: [1.0, 2.0],
        patience: 90,
    };
    let moved = EnemyState::Search {
        last_seen: [1.0, 3.0],
        patience: 90,
    };
    let impatient = EnemyState::Search {
        last_seen: [1.0, 2.0],
        patience: 10,
    };

    assert_ne!(stable_hash_of(&baseline), stable_hash_of(&moved));
    assert_ne!(stable_hash_of(&baseline), stable_hash_of(&impatient));
}

#[test]
fn inserting_a_variant_does_not_disturb_the_others() {
    // Keying on the variant name rather than its index means adding `Fleeing` in the middle leaves
    // every other variant's fingerprint alone. With an index, everything after it would shift and
    // invalidate replays that never touched the new variant.
    assert_eq!(
        stable_hash_of(&EnemyState::Pursue),
        stable_hash_of(&EnemyStateWithExtra::Pursue)
    );
    assert_eq!(
        stable_hash_of(&EnemyState::Search {
            last_seen: [4.0, 5.0],
            patience: 1
        }),
        stable_hash_of(&EnemyStateWithExtra::Search {
            last_seen: [4.0, 5.0],
            patience: 1
        })
    );
}

#[derive(StableHash)]
struct Nested {
    inner: Velocity,
    label: String,
    flags: Vec<bool>,
}

#[test]
fn nested_and_container_fields_work() {
    let baseline = Nested {
        inner: Velocity { x: 1.0, y: 2.0 },
        label: "a".to_string(),
        flags: vec![true, false],
    };

    assert_eq!(
        stable_hash_of(&baseline),
        stable_hash_of(&Nested {
            inner: Velocity { x: 1.0, y: 2.0 },
            label: "a".to_string(),
            flags: vec![true, false],
        })
    );
    assert_ne!(
        stable_hash_of(&baseline),
        stable_hash_of(&Nested {
            inner: Velocity { x: 1.0, y: 2.0 },
            label: "a".to_string(),
            flags: vec![false, true],
        })
    );
}
