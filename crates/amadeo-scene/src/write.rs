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
        //
        // A list of lists must not, even though every element would inline happily: joining them
        // would render `[[0,0],[4,0]]` as `0.0 0.0 4.0 0.0`, which parses back as one flat list of
        // four numbers. The grouping would be silently lost, and the round-trip test is what makes
        // that a caught bug rather than a corrupted scene.
        Value::List(items) => {
            if items.iter().any(|item| matches!(item, Value::List(_))) {
                return None;
            }
            let parts: Option<Vec<String>> = items.iter().map(inline_value).collect();
            parts.map(|parts| parts.join(" "))
        }
        // A map joins nested structs here, and for the same reason: the scene format has no syntax
        // for either yet. A field with no inline value parses as a *list* of `- ` items (see
        // `parse_field`), so writing a map as an indented block would produce a file the parser
        // reads back as something else. Falling through to `write_field`'s bare form instead means
        // a round trip fails loudly at parse time rather than quietly changing shape.
        //
        // Nothing authors a map in a scene today — resources are not scene-authorable at all — so
        // this is a gap ADR 0027 records rather than one it widens.
        Value::Unit | Value::Struct(_) | Value::Map(_) | Value::Enum(_) => None,
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
        // A list whose elements are themselves lists: one `- ` line each.
        Value::List(items) => {
            let _ = writeln!(output, "{pad}{name}");
            let item_pad = " ".repeat((level + 1) * INDENT);
            for item in items {
                match inline_value(item) {
                    Some(inline) => {
                        let _ = writeln!(output, "{item_pad}- {inline}");
                    }
                    // Deeper nesting than the format expresses today. Emitted as the debug form so
                    // nothing is silently dropped; layer 2 rejects it with a real message.
                    None => {
                        let _ = writeln!(output, "{item_pad}- {item}");
                    }
                }
            }
        }
        // A field with no value at all. Written bare so a round trip fails loudly at parse time
        // rather than quietly losing the field.
        Value::Unit => {
            let _ = writeln!(output, "{pad}{name}");
        }
        other => {
            let _ = writeln!(output, "{pad}{name} {other}");
        }
    }
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
