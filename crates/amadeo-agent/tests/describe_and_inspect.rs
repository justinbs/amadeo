//! What an agent actually gets back, over the engine's real components.
//!
//! M1 exit gate 4 is *"`amadeo describe` output is sufficient to write a new component and system
//! without reading engine source"*. That is a judgement a person makes, not something a test can
//! assert — but the individual facts it depends on can be pinned, and those are what this file
//! holds. If `describe` stops carrying units, or docs, or valid ranges, the gate quietly stops being
//! reachable and this fails first.

use amadeo_agent::{describe, entity, query, value_to_json};
use amadeo_ecs::{ComponentRegistry, World};
use amadeo_reflect::Value;
use amadeo_transform::{Parent, Transform};

fn registry() -> ComponentRegistry {
    let mut registry = ComponentRegistry::new();
    registry.register::<Transform>().expect("registers");
    registry.register::<Parent>().expect("registers");
    registry
}

// --- "What can I do?" ---

#[test]
fn describe_carries_everything_needed_to_use_a_component_unseen() {
    let schema = describe(&registry()).to_pretty();

    // The name you write in a scene file.
    assert!(schema.contains(r#""Transform""#), "{schema}");
    // What the type is for, in the author's own words.
    assert!(
        schema.contains("Where an entity is, how it is turned, and how big it is."),
        "{schema}"
    );
    // The exact field names -- the thing guessing gets wrong.
    assert!(schema.contains(r#""name": "rotation""#), "{schema}");
    // The field's type, so a list is not mistaken for a scalar.
    assert!(schema.contains(r#""type": "array<f32, 3>""#), "{schema}");
    // The unit, which is what stops radians being passed to a degrees field.
    assert!(schema.contains(r#""unit": "deg""#), "{schema}");
    // And the doc comment, which is the only explanation an agent gets.
    assert!(
        schema.contains("Rotation in degrees, applied Z then X then Y."),
        "{schema}"
    );
}

#[test]
fn describe_is_byte_stable_across_runs() {
    // The dump is meant to be committed and diffed. If it reordered between runs, every diff would
    // be noise and nobody would read them.
    assert_eq!(
        describe(&registry()).to_pretty(),
        describe(&registry()).to_pretty()
    );

    // ...including when the registry was built in a different order.
    let mut reversed = ComponentRegistry::new();
    reversed.register::<Parent>().expect("registers");
    reversed.register::<Transform>().expect("registers");
    assert_eq!(
        describe(&registry()).to_pretty(),
        describe(&reversed).to_pretty()
    );
}

/// How much damage something can take.
#[derive(Debug, Clone, Copy, PartialEq, amadeo_core::StableHash, amadeo_reflect::Reflect)]
struct Health {
    /// Current hit points.
    #[reflect(min = 0.0, max = 100.0, unit = "hp")]
    current: f32,
    /// Whether damage is ignored entirely.
    invulnerable: bool,
}
impl amadeo_ecs::Component for Health {}

#[test]
fn a_range_is_reported_so_an_editor_can_draw_a_slider() {
    let mut registry = ComponentRegistry::new();
    registry.register::<Health>().expect("registers");
    let schema = describe(&registry).to_pretty();

    assert!(schema.contains(r#""min": 0.0"#), "{schema}");
    assert!(schema.contains(r#""max": 100.0"#), "{schema}");
    assert!(schema.contains(r#""unit": "hp""#), "{schema}");
}

#[test]
fn absent_metadata_is_omitted_rather_than_emitted_as_null() {
    // `invulnerable` declares no unit, no range, and no replication. A reader checking whether a
    // field has bounds should get a straight yes or no, not a document full of nulls to sift.
    let mut registry = ComponentRegistry::new();
    registry.register::<Health>().expect("registers");
    let schema = describe(&registry).to_pretty();

    assert!(!schema.contains("null"), "{schema}");
    // Exactly one field carries a range and a unit -- the other one.
    assert_eq!(schema.matches(r#""range""#).count(), 1, "{schema}");
    assert_eq!(schema.matches(r#""unit""#).count(), 1, "{schema}");
    // And neither replicates, so the annotation is absent entirely.
    assert!(!schema.contains(r#""replication""#), "{schema}");
}

#[test]
fn replication_is_reported_only_where_it_says_something() {
    let schema = describe(&registry()).to_pretty();

    // Transform's fields all replicate, so the annotation is there...
    assert!(schema.contains(r#""sync": "on_change""#), "{schema}");
    assert!(schema.contains(r#""interpolate": "angular""#), "{schema}");

    // ...and the spellings match what `#[reflect(sync = "...")]` accepts, so what an agent reads is
    // what it can write back.
    assert!(!schema.contains(r#""OnChange""#), "{schema}");
}

#[test]
fn the_document_declares_its_own_format_version() {
    let schema = describe(&registry()).to_pretty();
    assert!(schema.contains(r#""format_version": 1"#), "{schema}");
}

// --- "What did I just do?" ---

#[test]
fn an_entity_reports_its_components_without_anyone_knowing_the_types() {
    let registry = registry();
    let mut world = World::new();

    let room = world.spawn();
    world.insert(room, Transform::at(0.0, 0.0));
    let lamp = world.spawn();
    world.insert(lamp, Transform::at(1.0, 2.5));
    world.insert(lamp, Parent(room));

    let dump = entity(&world, &registry, lamp).to_pretty();

    assert!(dump.contains(r#""Transform""#), "{dump}");
    assert!(dump.contains(r#""Parent""#), "{dump}");
    // Values come through, with floats still looking like floats.
    assert!(dump.contains("2.5"), "{dump}");
    assert!(dump.contains(r#""rotation": ["#), "{dump}");
}

#[test]
fn a_dead_handle_reports_nothing_rather_than_failing() {
    let registry = registry();
    let mut world = World::new();
    let entity_handle = world.spawn();
    world.despawn(entity_handle);

    assert_eq!(
        entity(&world, &registry, entity_handle).to_compact(),
        "null"
    );
}

#[test]
fn a_query_narrows_to_entities_having_every_named_component() {
    let registry = registry();
    let mut world = World::new();

    let root = world.spawn();
    world.insert(root, Transform::at(0.0, 0.0));

    let child = world.spawn();
    world.insert(child, Transform::at(1.0, 0.0));
    world.insert(child, Parent(root));

    // Everything with a transform.
    let all = query(&world, &registry, &["Transform"]).to_compact();
    assert!(all.starts_with(r#"{"count":2,"#), "{all}");

    // Only the ones that are children.
    let children = query(&world, &registry, &["Transform", "Parent"]).to_compact();
    assert!(children.starts_with(r#"{"count":1,"#), "{children}");

    // An empty filter is "show me everything".
    let everything = query(&world, &registry, &[]).to_compact();
    assert!(everything.starts_with(r#"{"count":2,"#), "{everything}");
}

#[test]
fn a_query_for_an_unregistered_name_narrows_to_nothing() {
    // Narrowing to nothing is a normal answer to a query, unlike writing a misspelled component to a
    // scene file, which is reported loudly.
    let registry = registry();
    let mut world = World::new();
    let handle = world.spawn();
    world.insert(handle, Transform::default());

    let found = query(&world, &registry, &["Nonsense"]).to_compact();
    assert!(found.starts_with(r#"{"count":0,"#), "{found}");
}

#[test]
fn query_output_is_stable_regardless_of_storage_churn() {
    // Two worlds holding the same entities must produce the same dump, or diffing a world between
    // two ticks shows changes that did not happen.
    let registry = registry();

    let mut direct = World::new();
    for index in 0..3 {
        let handle = direct.spawn();
        direct.insert(handle, Transform::at(index as f32, 0.0));
    }

    let mut churned = World::new();
    let scratch = churned.spawn();
    churned.despawn(scratch);
    for index in 0..3 {
        let handle = churned.spawn();
        churned.insert(handle, Transform::at(index as f32, 0.0));
    }

    // Not identical -- the churned world's handles have a bumped generation -- but the *shape* and
    // ordering must match, which is what `World::entities` sorting buys.
    let a = query(&direct, &registry, &["Transform"]).to_pretty();
    let b = query(&churned, &registry, &["Transform"]).to_pretty();
    assert_eq!(a.lines().count(), b.lines().count(), "\n{a}\n---\n{b}");
    assert_eq!(
        a.replace("\"generation\": 0", "\"generation\": X"),
        b.replace("\"generation\": 1", "\"generation\": X")
            .replace("\"generation\": 0", "\"generation\": X")
    );
}

// --- The value bridge ---

#[test]
fn reflected_values_map_onto_json_without_losing_their_type() {
    assert_eq!(value_to_json(&Value::Unit).to_compact(), "null");
    assert_eq!(value_to_json(&Value::Bool(true)).to_compact(), "true");
    assert_eq!(value_to_json(&Value::I64(-3)).to_compact(), "-3");
    // A float stays visibly a float, which an integer 0 would not.
    assert_eq!(value_to_json(&Value::F32(0.0)).to_compact(), "0.0");
    assert_eq!(
        value_to_json(&Value::String("hi".into())).to_compact(),
        r#""hi""#
    );
}

#[test]
fn a_fieldless_enum_variant_is_a_plain_string() {
    // `"state": "Patrol"` rather than `{"variant": "Patrol", "payload": null}`, because the common
    // case should read like the common case.
    assert_eq!(
        value_to_json(&Value::unit_variant("Patrol")).to_compact(),
        r#""Patrol""#
    );
}

#[test]
fn a_variant_carrying_data_keeps_both_halves() {
    let value = Value::Enum(amadeo_reflect::EnumValue {
        variant: "Search".to_string(),
        payload: Box::new(Value::structure([("patience", Value::I64(90))])),
    });
    assert_eq!(
        value_to_json(&value).to_compact(),
        r#"{"payload":{"patience":90},"variant":"Search"}"#
    );
}

#[test]
fn an_unrepresentable_integer_degrades_to_a_float_rather_than_a_wrong_integer() {
    // JSON has no u64. Emitting a wrapped-around i64 would be silently wrong; a float is lossy in
    // precision but right in magnitude, and visibly a float.
    let huge = value_to_json(&Value::U64(u64::MAX)).to_compact();
    assert!(huge.contains('e') || huge.contains('.'), "{huge}");
    // Ordinary unsigned values stay exact integers.
    assert_eq!(value_to_json(&Value::U64(42)).to_compact(), "42");
}
