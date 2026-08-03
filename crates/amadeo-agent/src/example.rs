//! `describe.example` — the gap between a schema and something that actually loads.
//!
//! # Why this exists
//!
//! M1 exit gate 4 found that `describe` is a schema and not a manual. Most of that gap is API
//! knowledge and stays in the repo's documentation (ADR 0030). One part of it is not: **the shape of
//! a valid value**. That is data the engine already holds, and having a reader reconstruct it from
//! type names is asking them to re-derive something the registry knows.
//!
//! Bevy's ecosystem arrived at the same conclusion from the other direction. Its remote protocol
//! serves a type schema, and a third-party crate added a `discover_format` method on top because the
//! schema *"doesn't show the actual JSON format needed"* — leaving people to reverse-engineer it out
//! of error messages. This is that, built in.
//!
//! # What "example" means here
//!
//! A **minimal valid instance**, not a realistic one. Numbers are zero (or a declared range's
//! minimum, since a zero outside the declared range would be bad advice), strings are empty, an enum
//! takes its first variant. The point is the *shape* — how many numbers a `translation` takes, that
//! a list of pairs needs `- ` lines, that a `phase` wants a bare word and not a quoted string.
//!
//! Both spellings come out of one [`Value`], so the scene form and the JSON form cannot disagree.

use crate::json::Json;
use amadeo_reflect::{EnumValue, ScalarKind, TypeInfo, TypeKind, TypeRegistry, Value};
use std::collections::BTreeMap;

/// How many elements to put in a list whose length the type does not fix.
///
/// One, not zero: an empty list is valid but shows nothing, and the whole point is to display the
/// element spelling. `points\n  - 0.0 0.0` teaches the `- ` syntax; `points` alone teaches nothing.
const OPEN_LIST_EXAMPLE_LEN: usize = 1;

/// Builds a worked example of one type, in both spellings an author might need.
///
/// ```text
/// {
///   "type": "Transform",
///   "scene": "  Transform\n    rotation 0.0 0.0 0.0\n    ...",
///   "json": { "rotation": [0.0, 0.0, 0.0], ... }
/// }
/// ```
///
/// # Errors
///
/// Returns a message naming the type it could not reach when a field's type is missing from
/// `types`. That should be impossible since ADR 0030 closed the schema, and it is reported rather
/// than papered over precisely because it would mean the closure had a hole.
pub fn describe_example(info: &TypeInfo, types: &TypeRegistry) -> Result<Json, String> {
    let mut stack = Vec::new();
    let value = example_value(info, types, &mut stack)?;

    let mut members = vec![
        ("type", Json::string(&info.name)),
        ("json", crate::inspect::value_to_json(&value)),
    ];

    // The scene form is omitted rather than faked when the format cannot express the value. A map is
    // the live case: the scene grammar has no syntax for one (ADR 0027 records the gap), so emitting
    // something would be emitting something that does not parse back.
    match scene_form(&info.name, &value) {
        Some(scene) => members.push(("scene", Json::string(scene))),
        None => members.push((
            "scene_note",
            Json::string(
                "this type contains a map, which the .scene format has no syntax for yet \
                 (ADR 0027) — the JSON form above is complete",
            ),
        )),
    }

    Ok(Json::object(members))
}

/// The scene spelling, or `None` when the format cannot express this value.
fn scene_form(name: &str, value: &Value) -> Option<String> {
    if contains_map(value) {
        return None;
    }
    Some(amadeo_scene::component_block(name, value))
}

/// Whether a map appears anywhere in this value.
fn contains_map(value: &Value) -> bool {
    match value {
        Value::Map(_) => true,
        Value::Struct(fields) => fields.values().any(contains_map),
        Value::List(items) => items.iter().any(contains_map),
        Value::Enum(inner) => contains_map(&inner.payload),
        Value::Unit
        | Value::Bool(_)
        | Value::I64(_)
        | Value::U64(_)
        | Value::F32(_)
        | Value::F64(_)
        | Value::String(_) => false,
    }
}

/// The minimal valid value for one type.
///
/// `stack` holds the type names currently being built, so a type that reaches itself terminates
/// instead of recursing forever — see [`example_for_named`].
fn example_value(
    info: &TypeInfo,
    types: &TypeRegistry,
    stack: &mut Vec<String>,
) -> Result<Value, String> {
    match &info.kind {
        TypeKind::Scalar(scalar) => Ok(scalar_example(*scalar)),

        TypeKind::Struct { fields } => {
            let mut members = BTreeMap::new();
            for field in fields {
                // A declared range is a promise about what is acceptable, so an example has to
                // respect it. `speed` with `min = 1.0` gets 1.0, not a zero the schema calls invalid.
                let value = match &field.range {
                    Some(range) => bounded_example(&field.type_name, range.min, types, stack)?,
                    None => example_for_named(&field.type_name, types, stack)?,
                };
                members.insert(field.name.clone(), value);
            }
            Ok(Value::Struct(members))
        }

        TypeKind::Enum { variants } => {
            // The first variant, because declaration order is the author's order and the first one
            // is conventionally the resting state — `Phase::Playing`, not `Phase::Lost`.
            let Some(variant) = variants.first() else {
                return Err(format!(
                    "`{}` is an enum with no variants, so no value of it exists",
                    info.name
                ));
            };

            let payload = if variant.fields.is_empty() {
                Value::Unit
            } else {
                let mut members = BTreeMap::new();
                for field in &variant.fields {
                    members.insert(
                        field.name.clone(),
                        example_for_named(&field.type_name, types, stack)?,
                    );
                }
                Value::Struct(members)
            };

            Ok(Value::Enum(EnumValue {
                variant: variant.name.clone(),
                payload: Box::new(payload),
            }))
        }

        TypeKind::List { element, length } => {
            let count = length.unwrap_or(OPEN_LIST_EXAMPLE_LEN);
            // A type reached through a list that is already being built gets an empty list, which is
            // both valid and finite. Only possible when the length is open — `[Node; 3]` genuinely
            // cannot be built, and says so below.
            if stack.iter().any(|name| name == element) {
                if length.is_some() {
                    return Err(format!(
                        "`{}` contains itself through a fixed-length array, so no finite value of \
                         it exists",
                        info.name
                    ));
                }
                return Ok(Value::List(Vec::new()));
            }

            let mut items = Vec::with_capacity(count);
            for _ in 0..count {
                items.push(example_for_named(element, types, stack)?);
            }
            Ok(Value::List(items))
        }

        // `Some`, not `None`. Both are valid, and `None` writes as a bare field name that the scene
        // parser reads back as an empty list — so the absent case is the one an example cannot
        // usefully show, and the present case is the one that teaches the spelling.
        TypeKind::Optional { inner } => example_for_named(inner, types, stack),

        // Empty, because a map's keys are the author's data and inventing one would be inventing
        // meaning. The schema's `key` and `value` names say what belongs in it.
        TypeKind::Map { .. } => Ok(Value::Map(BTreeMap::new())),
    }
}

/// Looks a type up by name and builds an example of it, guarding against recursion.
fn example_for_named(
    name: &str,
    types: &TypeRegistry,
    stack: &mut Vec<String>,
) -> Result<Value, String> {
    let Some(info) = types.get(name) else {
        return Err(format!(
            "the schema names a type `{name}` that is not registered, so no example can be built. \
             That is a hole in the registry rather than a bad request — every type a field names \
             should have been registered with it (ADR 0030)"
        ));
    };

    stack.push(name.to_string());
    let result = example_value(info, types, stack);
    stack.pop();
    result
}

/// An example that respects a declared minimum.
///
/// Only meaningful for numbers; anything else ignores the bound and falls through, because a range
/// on a string or a struct is a declaration mistake rather than something to honour.
fn bounded_example(
    name: &str,
    min: f64,
    types: &TypeRegistry,
    stack: &mut Vec<String>,
) -> Result<Value, String> {
    let Some(info) = types.get(name) else {
        return example_for_named(name, types, stack);
    };

    match &info.kind {
        TypeKind::Scalar(ScalarKind::Float32) => Ok(Value::F32(min as f32)),
        TypeKind::Scalar(ScalarKind::Float64) => Ok(Value::F64(min)),
        TypeKind::Scalar(ScalarKind::SignedInt) => Ok(Value::I64(min as i64)),
        // A negative minimum on an unsigned field is a contradiction in the annotation; zero is the
        // closest valid value and the alternative is an example that will not build.
        TypeKind::Scalar(ScalarKind::UnsignedInt) => Ok(Value::U64(min.max(0.0) as u64)),
        // A range on `[f32; 3]` bounds each element, which is how `#[reflect(min, max)]` reads on a
        // vector field. Applied elementwise rather than dropped.
        TypeKind::List { element, length } => {
            let count = length.unwrap_or(OPEN_LIST_EXAMPLE_LEN);
            let mut items = Vec::with_capacity(count);
            for _ in 0..count {
                items.push(bounded_example(element, min, types, stack)?);
            }
            Ok(Value::List(items))
        }
        _ => example_for_named(name, types, stack),
    }
}

/// The resting value of each primitive.
fn scalar_example(scalar: ScalarKind) -> Value {
    match scalar {
        ScalarKind::Bool => Value::Bool(false),
        ScalarKind::SignedInt => Value::I64(0),
        ScalarKind::UnsignedInt => Value::U64(0),
        ScalarKind::Float32 => Value::F32(0.0),
        ScalarKind::Float64 => Value::F64(0.0),
        ScalarKind::String => Value::String(String::new()),
    }
}
