//! End-to-end tests for `#[derive(Reflect)]`.
//!
//! Deliberately an integration test rather than a unit test: it exercises the macro exactly as a
//! game crate would, through the public `amadeo_reflect` path. A unit test inside the crate would
//! resolve `::amadeo_reflect` differently and could pass while real users break.

use amadeo_reflect::{
    Interpolation, Reflect, ReflectError, SyncPolicy, TypeKind, TypeRegistry, Value,
};

// --- A representative component: named fields, docs, and the full attribute vocabulary ---

/// How much damage something can take.
///
/// Second line of the doc comment.
#[derive(Debug, Clone, PartialEq, Reflect)]
#[reflect(version = 3)]
struct Health {
    /// Current hit points.
    #[reflect(min = 0.0, max = 100.0, unit = "hp", sync = "on_change")]
    current: f32,
    /// Maximum hit points.
    #[reflect(min = 1.0, max = 100.0, unit = "hp")]
    max: f32,
    /// Whether damage is ignored entirely.
    invulnerable: bool,
    /// Recomputed every tick, never stored.
    #[reflect(skip)]
    cached_ratio: f32,
}

#[test]
fn schema_captures_docs_units_ranges_and_version() {
    let info = Health::type_info();

    assert_eq!(info.name, "Health");
    assert_eq!(info.version, 3);
    assert_eq!(
        info.docs,
        "How much damage something can take.\n\nSecond line of the doc comment."
    );

    let current = info.field("current").expect("current is reflected");
    assert_eq!(current.docs, "Current hit points.");
    assert_eq!(current.type_name, "f32");
    assert_eq!(current.unit.as_deref(), Some("hp"));
    let range = current.range.expect("a range was declared");
    assert_eq!((range.min, range.max), (0.0, 100.0));

    // An undecorated field still appears, just without metadata.
    let invulnerable = info.field("invulnerable").expect("reflected");
    assert_eq!(invulnerable.type_name, "bool");
    assert_eq!(invulnerable.range, None);
    assert_eq!(invulnerable.unit, None);
}

#[test]
fn skipped_fields_are_absent_from_the_schema_and_the_value() {
    let info = Health::type_info();
    assert!(
        info.field("cached_ratio").is_none(),
        "a skipped field must not appear in the schema"
    );
    assert_eq!(info.fields().len(), 3);

    let health = Health {
        current: 50.0,
        max: 100.0,
        invulnerable: false,
        cached_ratio: 0.5,
    };
    assert_eq!(health.to_value().field("cached_ratio"), None);
}

#[test]
fn a_skipped_field_comes_back_as_its_default() {
    let health = Health {
        current: 50.0,
        max: 100.0,
        invulnerable: false,
        cached_ratio: 0.5,
    };
    let restored = Health::from_value(&health.to_value()).expect("round trip");

    assert_eq!(restored.current, 50.0);
    assert_eq!(
        restored.cached_ratio, 0.0,
        "nothing restores a skipped field, so it defaults"
    );
}

#[test]
fn struct_values_are_sorted_by_field_name() {
    let health = Health {
        current: 50.0,
        max: 100.0,
        invulnerable: true,
        cached_ratio: 0.0,
    };
    // Declaration order is current, max, invulnerable. Canonical order is alphabetical.
    assert_eq!(
        health.to_value().to_string(),
        "{current: 50, invulnerable: true, max: 100}"
    );
}

// --- Replication annotations, reserved by ADR 0006 ---

#[test]
fn replication_annotations_survive_into_the_schema() {
    let info = Health::type_info();

    let current = info.field("current").expect("reflected");
    assert_eq!(current.replication.sync, SyncPolicy::OnChange);
    assert_eq!(current.replication.interpolate, Interpolation::None);
    assert!(current.replication.is_replicated());

    // Unannotated fields default to not replicated -- opting in is deliberate, opting out is not.
    let max = info.field("max").expect("reflected");
    assert_eq!(max.replication.sync, SyncPolicy::Never);
    assert!(!max.replication.is_replicated());

    let replicated: Vec<&str> = info
        .replicated_fields()
        .map(|field| field.name.as_str())
        .collect();
    assert_eq!(replicated, vec!["current"]);
}

/// A position that replicates and interpolates, the way a real networked transform would.
#[derive(Debug, PartialEq, Reflect)]
struct NetworkedPosition {
    /// Horizontal, in world units.
    #[reflect(unit = "m", sync = "on_change", interpolate = "linear")]
    x: f32,
    /// Facing, in radians.
    #[reflect(unit = "rad", sync = "always", interpolate = "angular")]
    heading: f32,
}

#[test]
fn every_sync_and_interpolation_mode_parses() {
    let info = NetworkedPosition::type_info();

    let x = info.field("x").expect("reflected");
    assert_eq!(x.replication.sync, SyncPolicy::OnChange);
    assert_eq!(x.replication.interpolate, Interpolation::Linear);

    let heading = info.field("heading").expect("reflected");
    assert_eq!(heading.replication.sync, SyncPolicy::Always);
    assert_eq!(heading.replication.interpolate, Interpolation::Angular);
    assert_eq!(heading.unit.as_deref(), Some("rad"));
}

// --- Newtype, unit, and renamed types ---

/// A score, wrapping a plain number.
#[derive(Debug, PartialEq, Reflect)]
struct Score(u32);

/// Marks the entity the player controls.
#[derive(Debug, PartialEq, Reflect)]
struct Player;

/// Registered under a different name than its Rust identifier.
#[derive(Debug, PartialEq, Reflect)]
#[reflect(name = "Transform")]
struct InternalTransformRepresentation {
    /// World position.
    position: [f32; 2],
}

#[test]
fn a_newtype_is_transparent() {
    // `Score(7)` writes `7`, not `{ "0": 7 }`. The wrapper is a Rust detail; a scene file should
    // not have to know about it.
    let score = Score(7);
    assert_eq!(score.to_value(), Value::U64(7));
    assert_eq!(
        Score::from_value(&Value::U64(7)).expect("round trip"),
        score
    );
}

#[test]
fn a_unit_struct_round_trips() {
    assert_eq!(Player.to_value(), Value::Unit);
    assert_eq!(
        Player::from_value(&Value::Unit).expect("round trip"),
        Player
    );
}

#[test]
fn a_type_can_be_renamed_for_the_registry() {
    assert_eq!(InternalTransformRepresentation::type_name(), "Transform");
    assert_eq!(
        InternalTransformRepresentation::type_info().name,
        "Transform"
    );

    let mut registry = TypeRegistry::new();
    registry
        .register::<InternalTransformRepresentation>()
        .expect("registers");
    assert!(registry.contains("Transform"));
}

#[test]
fn nested_types_report_their_element_type() {
    let info = InternalTransformRepresentation::type_info();
    let position = info.field("position").expect("reflected");
    assert_eq!(position.type_name, "array<f32, 2>");
}

// --- Enums ---

/// What an enemy is currently doing.
#[derive(Debug, Clone, PartialEq, Reflect)]
enum EnemyState {
    /// Walking a fixed circuit.
    Patrol,
    /// Moving toward a remembered position.
    Search {
        /// Where the player was last seen.
        last_seen: [f32; 2],
        /// Ticks remaining before giving up.
        patience: u32,
    },
    /// Chasing the player directly.
    Pursue,
}

#[test]
fn enum_variants_round_trip() {
    for state in [
        EnemyState::Patrol,
        EnemyState::Pursue,
        EnemyState::Search {
            last_seen: [1.5, -2.0],
            patience: 90,
        },
    ] {
        let encoded = state.to_value();
        assert_eq!(
            EnemyState::from_value(&encoded).expect("round trip"),
            state,
            "failed for {state:?}"
        );
    }
}

#[test]
fn a_fieldless_variant_writes_just_its_name() {
    assert_eq!(EnemyState::Patrol.to_value().to_string(), "Patrol");
    assert_eq!(
        EnemyState::Search {
            last_seen: [0.0, 1.0],
            patience: 5
        }
        .to_value()
        .to_string(),
        "Search({last_seen: [0, 1], patience: 5})"
    );
}

#[test]
fn enum_schema_lists_variants_and_their_docs() {
    let info = EnemyState::type_info();
    let TypeKind::Enum { variants } = &info.kind else {
        panic!("EnemyState should reflect as an enum, got {:?}", info.kind);
    };

    let names: Vec<&str> = variants.iter().map(|v| v.name.as_str()).collect();
    assert_eq!(names, vec!["Patrol", "Search", "Pursue"]);
    assert_eq!(variants[0].docs, "Walking a fixed circuit.");
    assert!(variants[0].fields.is_empty());

    // A named-field variant reports its fields, with their docs.
    assert_eq!(variants[1].fields.len(), 2);
    assert_eq!(variants[1].fields[0].name, "last_seen");
    assert_eq!(
        variants[1].fields[0].docs,
        "Where the player was last seen."
    );
}

// --- Error quality. These are an agent's only feedback channel (Pillar 5). ---

#[test]
fn a_missing_field_lists_what_was_required() {
    let incomplete = Value::structure([("current", Value::F32(1.0))]);
    let error = Health::from_value(&incomplete).expect_err("max and invulnerable are absent");

    // Only the first missing field in declaration order is named, but the message lists the whole
    // required set — so one round trip is enough to learn everything that is wrong, even though two
    // fields are absent.
    assert_eq!(
        error,
        ReflectError::MissingField {
            type_name: "Health".to_string(),
            field: "max".to_string(),
            required: "current, max, invulnerable".to_string(),
        }
    );
    assert_eq!(
        error.to_string(),
        "Health: missing field `max`; required fields are current, max, invulnerable"
    );
}

#[test]
fn a_misspelled_field_is_reported_rather_than_ignored() {
    // The whole point: silently dropping `curent` would produce a Health that mysteriously keeps
    // its default, and the typo would survive review.
    let typo = Value::structure([
        ("curent", Value::F32(1.0)),
        ("max", Value::F32(1.0)),
        ("invulnerable", Value::Bool(false)),
    ]);
    let error = Health::from_value(&typo).expect_err("`curent` is not a field");

    assert_eq!(
        error.to_string(),
        "Health: unknown field `curent`; Health has current, max, invulnerable"
    );
}

#[test]
fn a_wrong_shape_names_both_sides() {
    let error = Health::from_value(&Value::List(vec![])).expect_err("a list is not a struct");
    assert_eq!(error.to_string(), "Health: expected struct, found list");
}

#[test]
fn an_unknown_variant_lists_the_valid_ones() {
    let error = EnemyState::from_value(&Value::unit_variant("Fleeing"))
        .expect_err("Fleeing is not a variant");
    assert_eq!(
        error.to_string(),
        "EnemyState: `Fleeing` is not a variant; valid variants are Patrol, Search, Pursue"
    );
}

#[test]
fn a_bad_field_type_inside_a_struct_is_reported() {
    let wrong = Value::structure([
        ("current", Value::String("lots".into())),
        ("max", Value::F32(1.0)),
        ("invulnerable", Value::Bool(false)),
    ]);
    let error = Health::from_value(&wrong).expect_err("current is not a string");
    assert_eq!(error.to_string(), "f32: expected a number, found string");
}

// --- The registry, driven by derived types ---

#[test]
fn a_registry_of_derived_types_iterates_in_sorted_order() {
    let mut registry = TypeRegistry::new();
    registry.register::<Health>().expect("registers");
    registry.register::<Score>().expect("registers");
    registry.register::<Player>().expect("registers");
    registry.register::<EnemyState>().expect("registers");

    // Four types were registered and eight came back: registering one also registers every type it
    // *names*, transitively (ADR 0030). Without that, `Health`'s schema would say a field is an
    // `f32` and nothing could look `f32` up — a schema that names types it cannot describe.
    let names: Vec<&str> = registry.names().collect();
    assert_eq!(
        names,
        vec![
            "EnemyState",
            "Health",
            "Player",
            "Score",
            "array<f32, 2>",
            "bool",
            "f32",
            "u32",
        ]
    );

    // Still sorted, which is what makes anything generated from the registry diffable.
    let mut sorted = names.clone();
    sorted.sort_unstable();
    assert_eq!(names, sorted);

    // And the schema is reachable by the name a scene file would use.
    let health = registry.get("Health").expect("registered");
    assert_eq!(health.version, 3);
    assert_eq!(health.fields().len(), 3);
}

#[test]
fn a_type_that_reaches_itself_registers_without_recursing_forever() {
    // The guard in `TypeRegistry::register`: insert before recursing, so the second visit finds the
    // entry already there and stops. Worth a test because the failure mode is a stack overflow at
    // startup rather than a wrong answer.
    /// A node in a tree, which contains more of itself.
    #[derive(Debug, PartialEq, amadeo_reflect::Reflect)]
    struct Node {
        /// This node's children.
        children: Vec<Node>,
        /// How deep it sits.
        depth: u32,
    }

    let mut registry = TypeRegistry::new();
    registry.register::<Node>().expect("registers");

    assert_eq!(
        registry.names().collect::<Vec<_>>(),
        vec!["Node", "list<Node>", "u32"]
    );
}

#[test]
fn round_tripping_through_the_value_tree_is_stable() {
    // The property invariant I2 rests on: encoding, decoding, and re-encoding must not drift.
    let health = Health {
        current: 33.5,
        max: 99.0,
        invulnerable: true,
        cached_ratio: 0.0,
    };

    let once = health.to_value();
    let twice = Health::from_value(&once).expect("round trip").to_value();
    assert_eq!(once, twice);
}
