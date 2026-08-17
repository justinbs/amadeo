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

    // The scene form is omitted rather than faked when the format cannot express the value. Since
    // ADR 0032 that is a narrow case — nested structs, maps and enum payloads all write now — and
    // what remains is anything *empty*, because an empty block is a parse error rather than an
    // empty value.
    match unwritable_reason(&value) {
        None => members.push((
            "scene",
            Json::string(amadeo_scene::component_block(&info.name, &value)),
        )),
        Some(reason) => members.push((
            "scene_note",
            Json::string(format!(
                "no scene form: {reason}. The JSON form above is complete"
            )),
        )),
    }

    Ok(Json::object(members))
}

/// Why this value has no scene spelling, or `None` when it has one.
///
/// The remaining gaps after ADR 0032, and both are the same shape: a field whose value is *nothing*
/// writes as a bare field name, and a bare field name is how the format opens a block — so the
/// parser reads it as an empty block and refuses it. There is no spelling for emptiness that does
/// not invent punctuation this format has deliberately avoided.
fn unwritable_reason(component: &Value) -> Option<String> {
    // The component itself may be empty — a marker writes as just its name, which is fine. Only its
    // *fields* have the problem, so the walk starts one level in.
    match component {
        Value::Struct(fields) => fields.values().find_map(field_unwritable),
        other => field_unwritable(other),
    }
}

/// The same question for something appearing as a field's value, where empty is fatal.
fn field_unwritable(value: &Value) -> Option<String> {
    match value {
        // `Option::None`. Left unsolved by ADR 0032 on purpose: `none` collides with an enum variant
        // of that name, and a sigil would be the format's first.
        Value::Unit => Some("it has an absent optional field, which has no spelling".to_string()),
        Value::Map(entries) if entries.is_empty() => {
            Some("it has an empty map field, and an empty block is a parse error".to_string())
        }
        Value::List(items) if items.is_empty() => {
            Some("it has an empty list field, and an empty block is a parse error".to_string())
        }
        Value::Struct(fields) if fields.is_empty() => {
            Some("it has an empty struct field, and an empty block is a parse error".to_string())
        }
        Value::Struct(fields) | Value::Map(fields) => fields.values().find_map(field_unwritable),
        Value::List(items) => items.iter().find_map(field_unwritable),
        Value::Enum(inner) => match inner.payload.as_ref() {
            // A fieldless variant is written inline, so its `Unit` payload is not a missing value.
            Value::Unit => None,
            payload => field_unwritable(payload),
        },
        Value::Bool(_)
        | Value::I64(_)
        | Value::U64(_)
        | Value::F32(_)
        | Value::F64(_)
        | Value::String(_) => None,
    }
}

/// The value one field gets in an example, best available information first.
///
/// Three sources, in order:
///
/// 1. **A declared default** (`#[reflect(default = ...)]`, ADR 0075), because it is the author's own
///    statement of what the field should be when nobody says otherwise. That makes it the only one of
///    the three guaranteed to be a *sensible* value rather than merely a legal one.
/// 2. **A range's minimum**, which is a promise about what is acceptable — `speed` with `min = 1.0`
///    gets 1.0, not a zero the schema calls invalid.
/// 3. **The type's zero**, when the schema says nothing.
///
/// The ordering matters more than it looks. Before defaults existed, `describe CylinderMesh --example`
/// answered with `radius 0.0`, `height 0.0`, `sides 3` — a legal instance of a cylinder that draws
/// nothing, offered as advice on how to author one. An example an agent cannot use is worse than no
/// example, because it looks like an answer.
fn field_example(
    field: &amadeo_reflect::FieldInfo,
    types: &TypeRegistry,
    stack: &mut Vec<String>,
) -> Result<Value, String> {
    if let Some(default) = &field.default {
        return Ok(default.clone());
    }
    match &field.range {
        Some(range) => bounded_example(&field.type_name, range.min, types, stack),
        None => example_for_named(&field.type_name, types, stack),
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
                let value = field_example(field, types, stack)?;
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
                    // Defaults and ranges are honoured here exactly as in a struct. Worth stating
                    // because ranges were silently *dropped* by the derive on variant fields until
                    // session 8, so this path looked correct while producing out-of-range advice.
                    let value = field_example(field, types, stack)?;
                    members.insert(field.name.clone(), value);
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
