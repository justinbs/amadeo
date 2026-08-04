//! Nested structs and enum payloads in a scene file — ADR 0032.
//!
//! The property that matters is a **byte-stable round trip**: text → document → text, unchanged
//! (invariant I2). Everything here is that, plus the errors the new shapes made possible.

use amadeo_reflect::Value;
use amadeo_scene::{parse, to_text};

/// Parses, reformats, and asserts the bytes came back identical.
fn round_trips(source: &str) -> amadeo_scene::SceneDocument {
    let document = parse(source).unwrap_or_else(|error| panic!("{error}\n{source}"));
    assert_eq!(to_text(&document), source, "round trip changed the bytes");
    document
}

fn field<'a>(document: &'a amadeo_scene::SceneDocument, component: &str) -> &'a Value {
    document.entities[0]
        .components
        .get(component)
        .expect("component is there")
}

#[test]
fn a_nested_struct_round_trips() {
    let source = "\
scene s
version 1

entity e1 \"Thing\"
  Material
    base_colour
      a 1.0
      b 0.25
      g 0.5
      r 1.0
    metallic 0.0
";
    let document = round_trips(source);
    let Value::Struct(fields) = field(&document, "Material") else {
        panic!("a component is a struct")
    };
    let Some(Value::Struct(colour)) = fields.get("base_colour") else {
        panic!("base_colour should be a nested struct, got {fields:?}")
    };
    assert_eq!(colour.get("r"), Some(&Value::F64(1.0)));
    assert_eq!(colour.len(), 4);
}

#[test]
fn nesting_goes_as_deep_as_the_indentation_does() {
    let source = "\
scene s
version 1

entity e1 \"Thing\"
  Deep
    outer
      inner
        leaf 3
";
    let document = round_trips(source);
    let Value::Struct(fields) = field(&document, "Deep") else {
        panic!("struct")
    };
    let Some(Value::Struct(outer)) = fields.get("outer") else {
        panic!("outer")
    };
    let Some(Value::Struct(inner)) = outer.get("inner") else {
        panic!("inner")
    };
    assert_eq!(inner.get("leaf"), Some(&Value::I64(3)));
}

#[test]
fn an_enum_variant_can_carry_fields() {
    // The shape ADR 0031's camera wanted and could not have.
    let source = "\
scene s
version 1

entity e1 \"Eye\"
  Camera
    projection Orthographic
      height 8.0
";
    let document = round_trips(source);
    let Value::Struct(fields) = field(&document, "Camera") else {
        panic!("struct")
    };
    let Some(Value::Enum(projection)) = fields.get("projection") else {
        panic!("projection should be an enum, got {fields:?}")
    };
    assert_eq!(projection.variant, "Orthographic");
    let Value::Struct(payload) = projection.payload.as_ref() else {
        panic!("payload")
    };
    assert_eq!(payload.get("height"), Some(&Value::F64(8.0)));
}

#[test]
fn a_fieldless_variant_still_writes_on_one_line() {
    // The thing that must not regress: ADR 0014 chose this format partly for how `state Patrol`
    // reads, and adding payloads must not have turned every variant into a block.
    let source = "\
scene s
version 1

entity e1 \"Thing\"
  Enemy
    state Patrol
";
    let document = round_trips(source);
    let Value::Struct(fields) = field(&document, "Enemy") else {
        panic!("struct")
    };
    let Some(Value::Enum(state)) = fields.get("state") else {
        panic!("state")
    };
    assert_eq!(state.variant, "Patrol");
    assert_eq!(state.payload.as_ref(), &Value::Unit);
}

#[test]
fn a_list_and_a_struct_are_told_apart_by_the_first_line() {
    // The whole rule, in one file. No schema is consulted — layer 1 has none.
    let source = "\
scene s
version 1

entity e1 \"Thing\"
  Mixed
    named
      x 1.0
    points
      - 0.0 0.0
      - 1.0 1.0
";
    let document = round_trips(source);
    let Value::Struct(fields) = field(&document, "Mixed") else {
        panic!("struct")
    };
    assert!(matches!(fields.get("named"), Some(Value::Struct(_))));
    assert!(matches!(fields.get("points"), Some(Value::List(_))));
}

#[test]
fn an_inline_value_with_a_list_beneath_it_is_an_error() {
    let error = parse(
        "\
scene s
version 1

entity e1 \"Thing\"
  Thing
    field 1.0
      - 2.0
",
    )
    .expect_err("a value and a list is meaningless");
    let message = error.to_string();
    assert!(message.contains("a `- ` list"), "{message}");
}

#[test]
fn a_number_with_fields_beneath_it_says_what_is_wrong() {
    // Only a *bare variant name* may carry a block. A number with fields under it is a mistake, and
    // the message has to name the one case where the shape is legitimate.
    let error = parse(
        "\
scene s
version 1

entity e1 \"Thing\"
  Thing
    field 1.0
      inner 2.0
",
    )
    .expect_err("a number cannot carry fields");
    let message = error.to_string();
    assert!(message.contains("enum variant"), "{message}");
}

#[test]
fn an_empty_block_is_still_an_error() {
    let error = parse(
        "\
scene s
version 1

entity e1 \"Thing\"
  Thing
    field
",
    )
    .expect_err("a field needs a value");
    assert!(error.to_string().contains("has no value"), "{error}");
}

#[test]
fn a_duplicate_nested_field_names_its_path() {
    let error = parse(
        "\
scene s
version 1

entity e1 \"Thing\"
  Material
    base_colour
      r 1.0
      r 0.5
",
    )
    .expect_err("r is set twice");
    let message = error.to_string();
    assert!(
        message.contains("Material.base_colour"),
        "the message should say where, got: {message}"
    );
}
