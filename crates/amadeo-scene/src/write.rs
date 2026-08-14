//! Writing a [`SceneDocument`] back out in canonical form.
//!
//! This is `amadeo fmt`'s output, and invariant I2 says it must be **byte-stable**: formatting an
//! already-formatted file changes nothing. Most of that falls out of the data structures — the
//! document stores components and fields in `BTreeMap`s, so they emerge sorted whether or not anyone
//! remembered to sort them. What is left is number formatting, which is the part that is easy to get
//! subtly wrong.

use crate::document::{SceneDocument, SceneEntity};
use crate::parse::INDENT;
use amadeo_reflect::Value;
use std::fmt::Write as _;

/// How long a plain decimal may get before exponent notation is preferable.
///
/// Generous enough that everything a scene realistically contains — positions, colours, ranges,
/// durations — stays in plain decimal, which is what a person wants to read.
pub const MAX_PLAIN_DIGITS: usize = 24;

/// Formats a float so that reading it back produces the same bits and the same text.
///
/// Three requirements, and the third is the one that surprises:
///
/// - **shortest round-trip** — Rust's `{}` guarantees that parsing the output returns the original
///   `f64`, so no precision is lost.
/// - **visibly a float** — `{}` prints `1.0` as `1`, which would parse back as an *integer*. The
///   value would survive and its type would not, and the file would stop meaning what it said. So a
///   bare-looking float gets `.0` appended.
/// - **not absurd** — Rust's `{}` never uses exponent notation, so `1e300` prints as three hundred
///   and one digits. That still round-trips, but nobody wants it in a file, so a plain form past
///   [`MAX_PLAIN_DIGITS`] falls back to `{:e}` (which is also round-trip shortest).
pub fn format_float(value: f64) -> String {
    if value.is_nan() {
        return "nan".to_string();
    }
    if value.is_infinite() {
        return if value.is_sign_negative() {
            "-inf"
        } else {
            "inf"
        }
        .to_string();
    }

    let plain = format!("{value}");
    let text = if plain.len() > MAX_PLAIN_DIGITS {
        let exponential = format!("{value:e}");
        if exponential.len() < plain.len() {
            exponential
        } else {
            plain
        }
    } else {
        plain
    };

    if text.contains(['.', 'e', 'E']) {
        text
    } else {
        format!("{text}.0")
    }
}

/// Escapes the two characters that would otherwise end or corrupt a quoted string.
pub fn escape(text: &str) -> String {
    text.replace('\\', "\\\\").replace('"', "\\\"")
}

/// Renders one scalar value as it appears after a field name.
///
/// Returns `None` for values that need their own lines — a list of lists, or a nested struct.
///
/// Two values are written as **explicit markers** rather than as nothing, because a field with no
/// value is not something this format has: `Unit` is `()` and an empty list is `[]`. Both are
/// checked by name when reading, before anything that would treat the brackets as the start of a
/// `Display`-form value.
pub fn inline_value(value: &Value) -> Option<String> {
    match value {
        Value::Bool(inner) => Some(inner.to_string()),
        Value::I64(inner) => Some(inner.to_string()),
        Value::U64(inner) => Some(inner.to_string()),
        Value::F32(inner) => Some(format_float(f64::from(*inner))),
        Value::F64(inner) => Some(format_float(*inner)),
        Value::String(inner) => Some(format!("\"{}\"", escape(inner))),
        // A bare identifier. A fieldless enum variant writes as just its name, which is what makes
        // `state Patrol` read the way it does.
        Value::Enum(inner) if matches!(inner.payload.as_ref(), Value::Unit) => {
            Some(inner.variant.clone())
        }
        // A *flat* list goes on one line: `position 0.0 0.0`.
        Value::List(items) => {
            // **An empty list is spelled out, for the reason `Unit` is written `()`.** Joining
            // nothing gives the empty string, so this used to write `name ` — a field with a
            // trailing space and no value, which is not something this format has. It parsed back as
            // `Unit`, so an empty `Vec` anywhere in a value made the file unreadable.
            //
            // That is not hypothetical: **the engine wrote snapshots it could not read back**. Every
            // registered event queue holds two empty lists at rest, so `amadeo snapshot` followed by
            // `amadeo status --from` failed on `games/atrium`, and had done since events were first
            // registered. Nothing noticed because nothing had restored one.
            if items.is_empty() {
                return Some("[]".to_string());
            }
            // A list of lists must not inline, even though every element would inline happily:
            // joining them would render `[[0,0],[4,0]]` as `0.0 0.0 4.0 0.0`, which parses back as
            // one flat list of four numbers. The grouping would be silently lost, and the round-trip
            // test is what makes that a caught bug rather than a corrupted scene.
            if items.iter().any(|item| matches!(item, Value::List(_))) {
                return None;
            }
            let parts: Option<Vec<String>> = items.iter().map(inline_value).collect();
            parts.map(|parts| parts.join(" "))
        }
        // These need their own lines, which `write_field` gives them. Since ADR 0032 that is a real
        // indented block rather than a failure: a struct or a map writes as `name value` lines, and
        // an enum variant carrying data writes as its name with those lines beneath it.
        //
        // `Value::Unit` is the exception and remains unwritable — it is `Option::None`, and every
        // spelling for it either collides with an enum variant or invents punctuation this format
        // does not have. Deliberately left out of ADR 0032; nothing uses one.
        Value::Unit | Value::Struct(_) | Value::Map(_) | Value::Enum(_) => None,
    }
}

/// Appends one `- ` list item whose value has named fields — ADR 0067.
///
/// The first field that will inline goes on the dash's own line and the rest sit beneath it, which
/// is YAML's shape and the one a person writes by hand. When nothing inlines — an item whose every
/// field is itself a block — the dash stands alone and the whole item is the block, which parses to
/// the same value.
fn write_struct_item(
    output: &mut String,
    level: usize,
    fields: &std::collections::BTreeMap<String, Value>,
) {
    let pad = " ".repeat(level * INDENT);

    // Alphabetically first, because a `BTreeMap` iterates in order and "first" therefore means the
    // same thing on every machine and every run. Anything else here would break byte-stability.
    let head = fields
        .iter()
        .next()
        .filter(|(_, value)| inline_value(value).is_some());

    match head {
        Some((name, value)) => {
            let inline = inline_value(value).expect("filtered on it");
            let _ = writeln!(output, "{pad}- {name} {inline}");
        }
        None => {
            let _ = writeln!(output, "{pad}-");
        }
    }

    // The rest, indented to line up with the field on the dash's line. `- ` is two characters, which
    // is exactly one level, so the continuation sits one level deeper than the dash.
    for (name, value) in fields {
        if head.is_some_and(|(head_name, _)| head_name == name) {
            continue;
        }
        write_field(output, level + 1, name, value);
    }
}

/// Appends `field <value>`, or the multi-line form when the value will not fit on one line.
fn write_field(output: &mut String, level: usize, name: &str, value: &Value) {
    let pad = " ".repeat(level * INDENT);

    if let Some(inline) = inline_value(value) {
        let _ = writeln!(output, "{pad}{name} {inline}");
        return;
    }

    match value {
        // A list whose elements are themselves lists or structs: one `- ` line each.
        Value::List(items) => {
            let _ = writeln!(output, "{pad}{name}");
            let item_pad = " ".repeat((level + 1) * INDENT);
            for item in items {
                match item {
                    _ if inline_value(item).is_some() => {
                        let inline = inline_value(item).expect("just checked");
                        let _ = writeln!(output, "{item_pad}- {inline}");
                    }
                    // An item with named fields — ADR 0067. The **alphabetically first** field goes
                    // on the dash's own line when it inlines, which is what makes the block beneath
                    // line up with it; a `BTreeMap` is what makes "first" mean the same thing every
                    // time, so this is byte-stable (I2).
                    Value::Struct(fields) | Value::Map(fields) => {
                        write_struct_item(output, level + 1, fields);
                    }
                    // Deeper nesting than the format expresses. Emitted as the debug form so nothing
                    // is silently dropped; layer 2 rejects it with a real message.
                    _ => {
                        let _ = writeln!(output, "{item_pad}- {item}");
                    }
                }
            }
        }
        // A nested struct, or a map. **The same lines either way**, which is not a shortcut: the two
        // are structurally identical (ADR 0027) and only the schema tells them apart, which layer 1
        // has not got. So both write as `name value` lines and both read back as a `Struct`; the
        // component's own `from_value` is what turns one into a map.
        Value::Struct(fields) | Value::Map(fields) => {
            let _ = writeln!(output, "{pad}{name}");
            for (field, inner) in fields {
                write_field(output, level + 1, field, inner);
            }
        }
        // An enum variant carrying data: the variant on the field's line, its fields beneath. The
        // *fieldless* case never reaches here — `inline_value` handles it, which is what keeps
        // `state Patrol` reading the way ADR 0014 designed it.
        Value::Enum(variant) => {
            let _ = writeln!(output, "{pad}{name} {}", variant.variant);
            if let Value::Struct(fields) = variant.payload.as_ref() {
                for (field, inner) in fields {
                    write_field(output, level + 1, field, inner);
                }
            }
        }
        // A field with no value at all — `Option::None`. Written bare so a round trip fails loudly
        // at parse time rather than quietly losing the field. ADR 0032 left this unsolved on
        // purpose: every spelling either collides with an enum variant or invents punctuation.
        Value::Unit => {
            let _ = writeln!(output, "{pad}{name}");
        }
        other => {
            let _ = writeln!(output, "{pad}{name} {other}");
        }
    }
}

/// Renders one component block exactly as it would appear inside an entity, indented two levels.
///
/// Two levels because that is where a component actually sits — under `entity`, under nothing else —
/// so what comes back can be pasted into a scene file unchanged.
///
/// Exists for `describe.example` (ADR 0030), which has a [`Value`] and a name and needs the scene
/// spelling of them. Sharing the writer rather than reimplementing it is the same reasoning that had
/// `amadeo-snapshot` borrow [`format_float`]: two copies of the canonical form would drift, and
/// invariant I2 depends on there being exactly one.
#[must_use]
pub fn component_block(name: &str, value: &Value) -> String {
    let mut output = String::new();
    write_component(&mut output, 1, name, value, "");
    output
}

/// Appends a component block and its fields.
fn write_component(output: &mut String, level: usize, name: &str, value: &Value, prefix: &str) {
    let pad = " ".repeat(level * INDENT);
    let _ = writeln!(output, "{pad}{prefix}{name}");

    if let Value::Struct(fields) = value {
        // `BTreeMap`, so this is sorted with no sorting step. See the note on `SceneEntity`.
        for (field, inner) in fields {
            write_field(output, level + 1, field, inner);
        }
    }
}

/// Appends one entity and everything beneath it.
fn write_entity(output: &mut String, level: usize, entity: &SceneEntity) {
    let pad = " ".repeat(level * INDENT);

    let _ = write!(
        output,
        "{pad}entity {} \"{}\"",
        entity.id,
        escape(&entity.name)
    );
    if let Some(prefab) = &entity.prefab {
        let _ = write!(output, " from {prefab}");
    }
    let _ = writeln!(output);

    for (name, value) in &entity.components {
        write_component(output, level + 1, name, value, "");
    }
    for (name, value) in &entity.overrides {
        write_component(output, level + 1, name, value, "override ");
    }

    // One blank line before each child, so siblings are visually separated without a trailing blank
    // inside the deepest block.
    for child in &entity.children {
        let _ = writeln!(output);
        write_entity(output, level + 1, child);
    }
}

/// Renders a document in canonical form.
///
/// Idempotent: formatting the result again produces the same bytes. That is invariant I2, and
/// `round_trip_is_byte_stable` in the crate's tests is what holds it.
#[must_use]
pub fn to_text(document: &SceneDocument) -> String {
    let mut output = String::new();

    let _ = writeln!(output, "scene {}", document.name);
    let _ = writeln!(output, "version {}", document.version);

    // Between the header and the entities, so the top of a file says what it needs before it says
    // what it contains. Omitted entirely when empty rather than written as a bare `assets` keyword,
    // which would be a block promising members it does not have.
    if !document.assets.is_empty() {
        let _ = writeln!(output);
        let _ = writeln!(output, "assets");
        let pad = " ".repeat(INDENT);
        // Already sorted -- it is a BTreeSet, so canonical order is the data structure's problem
        // rather than something to remember here (invariant I2).
        for id in &document.assets {
            let _ = writeln!(output, "{pad}{id}");
        }
    }

    for entity in &document.entities {
        let _ = writeln!(output);
        write_entity(&mut output, 0, entity);
    }

    output
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn floats_stay_visibly_floats() {
        // The trap: `{}` prints 1.0 as "1", which would parse back as an integer. The value would
        // survive the round trip and the type would not.
        assert_eq!(format_float(1.0), "1.0");
        assert_eq!(format_float(-2.5), "-2.5");
        assert_eq!(format_float(0.0), "0.0");
        assert_eq!(format_float(0.85), "0.85");
    }

    #[test]
    fn extreme_magnitudes_use_exponent_notation() {
        // Rust's `{}` never uses exponents, so 1e300 would otherwise print as 301 digits. It would
        // still round-trip -- it is unreadable, not wrong -- but a scene file has to be readable.
        assert_eq!(format_float(1e300), "1e300");
        assert_eq!(format_float(5e-324), "5e-324");
        // ...while ordinary values stay in plain decimal, which is what anyone wants to read.
        assert_eq!(format_float(1234.5), "1234.5");
        assert_eq!(format_float(0.000_001), "0.000001");
    }

    #[test]
    fn floats_round_trip_exactly() {
        for value in [0.1f64, 1.0 / 3.0, f64::MAX, f64::MIN_POSITIVE, -0.0] {
            let text = format_float(value);
            let parsed: f64 = text.parse().expect("formats as a parseable float");
            assert_eq!(
                parsed.to_bits(),
                value.to_bits(),
                "{value} formatted as {text} and came back different"
            );
        }
    }

    #[test]
    fn strings_escape_quotes_and_backslashes() {
        assert_eq!(escape(r#"say "hi""#), r#"say \"hi\""#);
        assert_eq!(escape(r"back\slash"), r"back\\slash");
    }

    #[test]
    fn a_bare_enum_variant_writes_as_its_name() {
        assert_eq!(
            inline_value(&Value::unit_variant("Patrol")).as_deref(),
            Some("Patrol")
        );
    }

    #[test]
    fn a_flat_list_goes_on_one_line() {
        let position = Value::List(vec![Value::F64(0.0), Value::F64(2.5)]);
        assert_eq!(inline_value(&position).as_deref(), Some("0.0 2.5"));
    }

    #[test]
    fn a_nested_list_does_not_go_on_one_line() {
        let waypoints = Value::List(vec![Value::List(vec![Value::F64(0.0), Value::F64(0.0)])]);
        // Inner list is inline-able, so the outer one is too -- but that would lose the structure,
        // so `write_field` handles it. Confirm the pieces behave as expected.
        let mut output = String::new();
        write_field(&mut output, 0, "waypoints", &waypoints);
        assert_eq!(output, "waypoints\n  - 0.0 0.0\n");
    }
}
