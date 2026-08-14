//! Nested structs and enum payloads in a scene file â€” ADR 0032.
//!
//! The property that matters is a **byte-stable round trip**: text â†’ document â†’ text, unchanged
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
    // The whole rule, in one file. No schema is consulted â€” layer 1 has none.
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

// --- ADR 0067: a list whose items have named fields ---

#[test]
fn a_list_of_structs_round_trips() {
    // **The shape ADR 0032 left out.** A repeated compound entry — an animation track, a dialogue
    // line, a state machine's transitions — had nowhere to go, and `amadeo-anim` was the first
    // asset to want one.
    //
    // Round-tripping is the assertion that matters (I2): the field on the dash's own line is the
    // alphabetically first one, and if the writer and the parser disagreed about which, the bytes
    // would move every time somebody ran `amadeo fmt`.
    let source = "\
scene s
version 1

entity e1 \"Thing\"
  Clip
    tracks
      - component \"Transform\"
        field \"rotation\"
      - component \"PointLight\"
        field \"intensity\"
";
    let document = round_trips(source);

    let Value::Struct(clip) = field(&document, "Clip") else {
        panic!("a component is a struct");
    };
    let Value::List(items) = &clip["tracks"] else {
        panic!("tracks should be a list, got {:?}", clip["tracks"]);
    };
    assert_eq!(items.len(), 2);

    let Value::Struct(first) = &items[0] else {
        panic!("an item should be a struct, got {:?}", items[0]);
    };
    // The field on the dash's own line is one of the item's fields, not a header — YAML's rule, and
    // what makes the block beneath line up with it.
    assert_eq!(first["component"], Value::String("Transform".to_string()));
    assert_eq!(first["field"], Value::String("rotation".to_string()));
}

#[test]
fn a_list_item_can_hold_a_list_of_its_own() {
    // Two levels of the new shape, which is what a clip's tracks-of-keys actually is. If the
    // recursion were wrong this is where it would show rather than in the flat case above.
    let source = "\
scene s
version 1

entity e1 \"Thing\"
  Clip
    tracks
      - field \"rotation\"
        keys
          - time 0.0
            value 1.0 2.0 3.0
          - time 1.0
            value 4.0 5.0 6.0
";
    let document = round_trips(source);

    let Value::Struct(clip) = field(&document, "Clip") else {
        panic!("a component is a struct");
    };
    let Value::List(tracks) = &clip["tracks"] else {
        panic!("expected a list");
    };
    let Value::Struct(track) = &tracks[0] else {
        panic!("expected a struct");
    };
    let Value::List(keys) = &track["keys"] else {
        panic!("expected a list of keys, got {:?}", track["keys"]);
    };
    assert_eq!(keys.len(), 2);

    let Value::Struct(second) = &keys[1] else {
        panic!("expected a struct");
    };
    assert_eq!(second["time"], Value::F64(1.0));
    assert_eq!(
        second["value"],
        Value::List(vec![Value::F64(4.0), Value::F64(5.0), Value::F64(6.0)])
    );
}

#[test]
fn a_bare_dash_takes_every_field_from_the_block() {
    // The spelling for an item whose alphabetically first field is itself a block, so there is
    // nothing to put on the dash's line. It parses to the same value as the compact form, which is
    // what lets `amadeo fmt` always choose the compact one — so this one deliberately does *not*
    // round-trip, and reformatting it is the point.
    let document = parse(
        "\
scene s
version 1

entity e1 \"Thing\"
  Clip
    tracks
      -
        component \"Transform\"
        field \"rotation\"
",
    )
    .expect("parses");

    let Value::Struct(clip) = field(&document, "Clip") else {
        panic!("a component is a struct");
    };
    let Value::List(items) = &clip["tracks"] else {
        panic!("expected a list");
    };
    let Value::Struct(first) = &items[0] else {
        panic!("expected a struct");
    };
    assert_eq!(first["component"], Value::String("Transform".to_string()));
    assert_eq!(first.len(), 2);

    // And formatting it produces the compact spelling, which parses back to the same value.
    assert_eq!(parse(&to_text(&document)).expect("reparses"), document);
    assert!(to_text(&document).contains("- component \"Transform\""));
}

#[test]
fn a_flat_list_still_parses_the_way_it_always_did() {
    // The whole point of an additive change: nothing written before it moves.
    let source = "\
scene s
version 1

entity e1 \"Thing\"
  Path
    waypoints
      - 0.0 0.0
      - 4.0 2.0
";
    let document = round_trips(source);

    let Value::Struct(path) = field(&document, "Path") else {
        panic!("a component is a struct");
    };
    assert_eq!(
        path["waypoints"],
        Value::List(vec![
            Value::List(vec![Value::F64(0.0), Value::F64(0.0)]),
            Value::List(vec![Value::F64(4.0), Value::F64(2.0)]),
        ])
    );
}
