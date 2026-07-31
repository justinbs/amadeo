//! A small, deterministic JSON writer.
//!
//! # Why hand-written rather than `serde_json`
//!
//! Two requirements `serde_json` does not give for free, and one it gives at a cost:
//!
//! - **Object keys must be sorted.** `amadeo describe` output is meant to be committed, diffed, and
//!   read by an agent across sessions. A document whose key order drifts is a document whose diff is
//!   noise. `BTreeMap` makes that structural rather than remembered — the same trick
//!   `amadeo_reflect::Value` uses.
//! - **Numbers must round-trip and stay visibly typed.** `1.0` printing as `1` would turn a float
//!   field into an integer one for anything reading the output, which is precisely the "plausible
//!   but wrong" failure Pillar 2 exists to kill.
//! - It is a large dependency for something this project needs about two hundred lines of, and
//!   ADR 0011 made the weight of the dependency graph load-bearing.
//!
//! This is deliberately *not* a JSON parser. Nothing here reads JSON yet; when the RPC server needs
//! to, that is a separate and larger piece of work.

use std::collections::BTreeMap;
use std::fmt::Write as _;

/// A JSON document.
#[derive(Debug, Clone, PartialEq)]
pub enum Json {
    /// `null`.
    Null,
    /// `true` or `false`.
    Bool(bool),
    /// A whole number, written without a decimal point.
    ///
    /// Kept distinct from [`Json::Float`] so a `layer: 0` stays an integer and a `rotation: 0.0`
    /// stays a float. Collapsing them would lose the type distinction the schema is trying to
    /// convey.
    Int(i64),
    /// A fractional number, always written with a decimal point or exponent.
    Float(f64),
    /// A string.
    String(String),
    /// An ordered list. Order is data, so it is preserved exactly.
    Array(Vec<Json>),
    /// Named members, **always emitted in sorted key order**.
    Object(BTreeMap<String, Json>),
}

impl Json {
    /// Builds an object from name/value pairs.
    #[must_use]
    pub fn object<I, K>(members: I) -> Self
    where
        I: IntoIterator<Item = (K, Json)>,
        K: Into<String>,
    {
        Json::Object(
            members
                .into_iter()
                .map(|(name, value)| (name.into(), value))
                .collect(),
        )
    }

    /// A string value, from anything string-like.
    #[must_use]
    pub fn string(text: impl Into<String>) -> Self {
        Json::String(text.into())
    }

    /// Renders indented, for a human or an agent to read.
    #[must_use]
    pub fn to_pretty(&self) -> String {
        let mut output = String::new();
        self.write(&mut output, 0, true);
        output.push('\n');
        output
    }

    /// Renders on one line, for a wire protocol.
    #[must_use]
    pub fn to_compact(&self) -> String {
        let mut output = String::new();
        self.write(&mut output, 0, false);
        output
    }

    fn write(&self, output: &mut String, depth: usize, pretty: bool) {
        match self {
            Json::Null => output.push_str("null"),
            Json::Bool(value) => {
                let _ = write!(output, "{value}");
            }
            Json::Int(value) => {
                let _ = write!(output, "{value}");
            }
            Json::Float(value) => output.push_str(&format_number(*value)),
            Json::String(value) => output.push_str(&quote(value)),

            Json::Array(items) => {
                if items.is_empty() {
                    output.push_str("[]");
                    return;
                }
                output.push('[');
                for (index, item) in items.iter().enumerate() {
                    if index > 0 {
                        output.push(',');
                    }
                    newline_and_indent(output, depth + 1, pretty);
                    item.write(output, depth + 1, pretty);
                }
                newline_and_indent(output, depth, pretty);
                output.push(']');
            }

            Json::Object(members) => {
                if members.is_empty() {
                    output.push_str("{}");
                    return;
                }
                output.push('{');
                // `BTreeMap`, so this is sorted with no sorting step.
                for (index, (name, value)) in members.iter().enumerate() {
                    if index > 0 {
                        output.push(',');
                    }
                    newline_and_indent(output, depth + 1, pretty);
                    output.push_str(&quote(name));
                    output.push(':');
                    if pretty {
                        output.push(' ');
                    }
                    value.write(output, depth + 1, pretty);
                }
                newline_and_indent(output, depth, pretty);
                output.push('}');
            }
        }
    }
}

fn newline_and_indent(output: &mut String, depth: usize, pretty: bool) {
    if pretty {
        output.push('\n');
        for _ in 0..depth {
            output.push_str("  ");
        }
    }
}

/// Formats a float so it round-trips and still looks like a float.
///
/// Same two traps as the scene writer: Rust's `{}` prints `1.0` as `1`, and never uses exponent
/// notation, so `1e300` becomes three hundred and one digits.
///
/// JSON has no way to write NaN or infinity. Rather than emit invalid JSON, those become `null` —
/// which is lossy and is the least-wrong option available. A NaN reaching this point is a simulation
/// bug anyway, and `null` in a description is at least visibly odd.
fn format_number(value: f64) -> String {
    if !value.is_finite() {
        return "null".to_string();
    }

    let plain = format!("{value}");
    let text = if plain.len() > 24 {
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

/// Quotes and escapes a string per the JSON spec.
fn quote(text: &str) -> String {
    let mut output = String::with_capacity(text.len() + 2);
    output.push('"');
    for character in text.chars() {
        match character {
            '"' => output.push_str("\\\""),
            '\\' => output.push_str("\\\\"),
            '\n' => output.push_str("\\n"),
            '\r' => output.push_str("\\r"),
            '\t' => output.push_str("\\t"),
            // Everything below 0x20 has to be escaped; JSON only names a few, so the rest go as
            // \u escapes.
            control if control < ' ' => {
                let _ = write!(output, "\\u{:04x}", control as u32);
            }
            other => output.push(other),
        }
    }
    output.push('"');
    output
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn object_keys_come_out_sorted_whatever_order_they_went_in() {
        // The property that makes a committed `describe` dump diffable.
        let forwards = Json::object([
            ("alpha", Json::Int(1)),
            ("beta", Json::Int(2)),
            ("gamma", Json::Int(3)),
        ]);
        let backwards = Json::object([
            ("gamma", Json::Int(3)),
            ("beta", Json::Int(2)),
            ("alpha", Json::Int(1)),
        ]);

        assert_eq!(forwards, backwards);
        assert_eq!(forwards.to_compact(), r#"{"alpha":1,"beta":2,"gamma":3}"#);
    }

    #[test]
    fn integers_and_floats_stay_distinguishable() {
        // `layer: 0` is an integer and `rotation: 0.0` is a float, and anything reading this output
        // has only the text to tell them apart.
        assert_eq!(Json::Int(0).to_compact(), "0");
        assert_eq!(Json::Float(0.0).to_compact(), "0.0");
        assert_eq!(Json::Float(1.5).to_compact(), "1.5");
        assert_eq!(Json::Float(-2.0).to_compact(), "-2.0");
    }

    #[test]
    fn extreme_floats_do_not_become_three_hundred_digits() {
        assert_eq!(Json::Float(1e300).to_compact(), "1e300");
        assert_eq!(Json::Float(1234.5).to_compact(), "1234.5");
    }

    #[test]
    fn non_finite_numbers_become_null_rather_than_invalid_json() {
        // JSON cannot express these. Emitting `NaN` would produce a document no parser accepts,
        // which is worse than losing the distinction.
        assert_eq!(Json::Float(f64::NAN).to_compact(), "null");
        assert_eq!(Json::Float(f64::INFINITY).to_compact(), "null");
    }

    #[test]
    fn strings_escape_what_the_spec_requires() {
        assert_eq!(Json::string(r#"say "hi""#).to_compact(), r#""say \"hi\"""#);
        assert_eq!(Json::string("a\\b").to_compact(), r#""a\\b""#);
        assert_eq!(Json::string("line\nbreak").to_compact(), r#""line\nbreak""#);
        assert_eq!(Json::string("tab\there").to_compact(), r#""tab\there""#);
        // A control character with no short escape becomes a \u sequence — a raw control character
        // inside a JSON string is invalid, not merely ugly.
        assert_eq!(Json::string("\u{1}").to_compact(), "\"\\u0001\"");
    }

    #[test]
    fn empty_containers_stay_on_one_line() {
        assert_eq!(Json::Array(Vec::new()).to_pretty(), "[]\n");
        assert_eq!(Json::Object(BTreeMap::new()).to_pretty(), "{}\n");
    }

    #[test]
    fn pretty_output_is_indented_and_readable() {
        let document = Json::object([
            ("name", Json::string("Transform2d")),
            (
                "fields",
                Json::Array(vec![Json::object([("name", Json::string("position"))])]),
            ),
        ]);

        assert_eq!(
            document.to_pretty(),
            "{\n  \"fields\": [\n    {\n      \"name\": \"position\"\n    }\n  ],\n  \"name\": \"Transform2d\"\n}\n"
        );
    }

    #[test]
    fn arrays_keep_their_order() {
        let list = Json::Array(vec![Json::Int(3), Json::Int(1), Json::Int(2)]);
        assert_eq!(list.to_compact(), "[3,1,2]");
    }
}
