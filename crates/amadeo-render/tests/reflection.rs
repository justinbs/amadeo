//! Reflection over the engine's real components, not test doubles.
//!
//! This is the first end-to-end evidence for invariant I8 and Pillar 2 of
//! `docs/03-ai-native-design.md`: an agent can ask what a component is, what its fields mean, and
//! what a sane value looks like — without reading engine source. `amadeo describe` will render
//! exactly this data once `amadeo-cli` exists.

use amadeo_reflect::{Reflect, SyncPolicy, TypeRegistry, Value};
use amadeo_render::{Camera2d, Quad};
use amadeo_transform::Transform;

#[test]
fn the_engines_components_register_and_are_discoverable_by_name() {
    let mut registry = TypeRegistry::new();
    registry.register::<Transform>().expect("registers");
    registry.register::<Quad>().expect("registers");
    registry.register::<Camera2d>().expect("registers");

    // Sorted, so anything generated from this listing is diffable. The vector and scalar types come
    // along because registering a type registers everything it names (ADR 0030) — which is what lets
    // a reader resolve `"type": "array<f32, 3>"` instead of having to parse the string.
    assert_eq!(
        registry.names().collect::<Vec<_>>(),
        vec![
            "Camera2d",
            "Quad",
            "Transform",
            "array<f32, 2>",
            "array<f32, 3>",
            "array<f32, 4>",
            "f32",
        ]
    );
}

#[test]
fn a_schema_answers_what_a_field_means_without_reading_source() {
    let info = Transform::type_info();

    assert_eq!(info.name, "Transform");
    assert_eq!(
        info.docs.lines().next(),
        Some("Where an entity is, how it is turned, and how big it is.")
    );

    let rotation = info.field("rotation").expect("reflected");
    assert_eq!(rotation.type_name, "array<f32, 3>");
    assert_eq!(
        rotation.docs,
        "Rotation in degrees, applied Z then X then Y."
    );
    // The unit is the thing that stops an agent passing radians to a degrees field.
    assert_eq!(rotation.unit.as_deref(), Some("deg"));

    let translation = info.field("translation").expect("reflected");
    assert_eq!(translation.type_name, "array<f32, 3>");

    // A colour channel advertises its valid range, which is what an editor slider needs.
    let colour = Quad::type_info().field("color").expect("reflected").range;
    let colour = colour.expect("a range was declared");
    assert_eq!((colour.min, colour.max), (0.0, 1.0));
}

#[test]
fn real_components_round_trip_through_the_value_tree() {
    let transform = Transform {
        translation: [1.5, -2.25, 0.5],
        rotation: [0.0, 0.0, 45.0],
        scale: [2.0, 3.0, 1.0],
    };
    assert_eq!(
        Transform::from_value(&transform.to_value()).expect("round trip"),
        transform
    );

    let quad = Quad::new(1.0, 2.0, [0.1, 0.2, 0.3, 1.0]);
    assert_eq!(
        Quad::from_value(&quad.to_value()).expect("round trip"),
        quad
    );

    let camera = Camera2d {
        center: [4.0, 5.0],
        height: 12.0,
    };
    assert_eq!(
        Camera2d::from_value(&camera.to_value()).expect("round trip"),
        camera
    );
}

#[test]
fn a_components_value_is_canonically_ordered() {
    // `Quad` declares size then color. The value tree sorts them, which is what makes a saved
    // scene byte-stable (I2) without every writer having to remember to sort.
    let quad = Quad::new(1.0, 2.0, [0.0, 0.0, 0.0, 1.0]);
    assert_eq!(
        quad.to_value().to_string(),
        "{color: [0, 0, 0, 1], size: [1, 2]}"
    );
}

#[test]
fn transform_fields_carry_the_replication_annotations_m6_will_need() {
    // Reserved by ADR 0006, unused until M6. Tested now so a wrong annotation is found while the
    // component is fresh rather than during netcode work two years later.
    let info = Transform::type_info();

    let replicated: Vec<&str> = info
        .replicated_fields()
        .map(|field| field.name.as_str())
        .collect();
    assert_eq!(replicated, vec!["translation", "rotation", "scale"]);

    assert_eq!(
        info.field("rotation").expect("reflected").replication.sync,
        SyncPolicy::OnChange
    );

    // A `Quad` is presentation, not simulation state another machine needs, so nothing on it
    // replicates.
    assert_eq!(Quad::type_info().replicated_fields().count(), 0);
}

#[test]
fn a_typo_in_a_component_field_is_reported_with_the_valid_names() {
    // The failure mode this exists to prevent: a scene file with a misspelled field loading
    // "successfully" and silently keeping the default.
    let typo = Value::structure([
        (
            "translaton",
            Value::List(vec![Value::F32(0.0), Value::F32(0.0), Value::F32(0.0)]),
        ),
        (
            "rotation",
            Value::List(vec![Value::F32(0.0), Value::F32(0.0), Value::F32(0.0)]),
        ),
        (
            "scale",
            Value::List(vec![Value::F32(1.0), Value::F32(1.0), Value::F32(1.0)]),
        ),
    ]);

    let error = Transform::from_value(&typo).expect_err("`translaton` is not a field");
    assert_eq!(
        error.to_string(),
        "Transform: unknown field `translaton`; Transform has translation, rotation, scale"
    );
}
