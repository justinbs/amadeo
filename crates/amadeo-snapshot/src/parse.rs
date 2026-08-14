//! Reading a `.snapshot` file back into a [`Snapshot`].
//!
//! # Errors carry a line number, always
//!
//! A snapshot is a file a human may edit and an agent may repair, and neither can ask a follow-up
//! question. So every failure names the line and says what was expected — the same standard the
//! `.replay` and `.scene` parsers hold to.
//!
//! # What it will not guess at
//!
//! Indentation is meaning here, so a line indented by an odd number of spaces, or indented deeper
//! than its context allows, is an error rather than a guess. A tab is refused outright: mixed
//! indentation is invisible on screen and would make two files that look identical parse
//! differently.

use crate::write::INDENT;
use crate::{FORMAT_VERSION, SchemaEntry, SchemaKind, Snapshot, SnapshotEntity};
use amadeo_core::Tick;
use amadeo_ecs::Entity;
use amadeo_reflect::Value;
use std::collections::BTreeMap;

/// Why a snapshot file could not be read.
#[derive(Debug, Clone, PartialEq, Eq, thiserror::Error)]
#[error("line {line}: {kind}")]
pub struct ParseError {
    /// Which line, counting from one.
    pub line: usize,
    /// What was wrong.
    pub kind: ParseErrorKind,
}

/// The specific problem a [`ParseError`] reports.
#[derive(Debug, Clone, PartialEq, Eq, thiserror::Error)]
pub enum ParseErrorKind {
    /// The first line is not the expected magic.
    #[error(
        "a snapshot must start with `amadeo-snapshot {FORMAT_VERSION}`, and this starts with \
         `{found}`"
    )]
    BadMagic {
        /// What the first line actually said.
        found: String,
    },

    /// The version is not the one this build writes.
    #[error(
        "this snapshot is format version {found}, and this build reads version \
         {FORMAT_VERSION}.\n\
         A snapshot captures one moment of one run, so there is no upgrade path -- take a fresh one"
    )]
    WrongVersion {
        /// The version the file claims.
        found: u32,
    },

    /// A required header line is missing.
    #[error(
        "the header is missing `{name}`; a snapshot needs `tick`, `state-hash` and `schema-hash`"
    )]
    MissingHeader {
        /// Which header line.
        name: &'static str,
    },

    /// A line in the `schema` block is not `component Name version`.
    #[error(
        "`{found}` is not a schema entry; they are written `component Name 1` or \
         `resource Name 1`"
    )]
    BadSchemaEntry {
        /// The line that was not one.
        found: String,
    },

    /// A number could not be parsed.
    #[error("`{field}` should be {expected}, but this says `{found}`")]
    BadNumber {
        /// Which field.
        field: String,
        /// What it should have looked like.
        expected: &'static str,
        /// What was there.
        found: String,
    },

    /// An entity handle is not `index:generation`.
    #[error(
        "`{found}` is not an entity handle; they are written `index:generation`, such as `3:0`"
    )]
    BadEntity {
        /// The text that was not a handle.
        found: String,
    },

    /// Indentation used a tab.
    #[error(
        "indentation uses a tab; snapshots indent with {INDENT} spaces per level. \
         Mixed indentation is invisible on screen, so it is rejected rather than guessed at"
    )]
    TabIndent,

    /// Indentation is not a whole number of levels.
    #[error("indented by {found} spaces, which is not a multiple of {INDENT}")]
    OddIndentation {
        /// How many spaces were found.
        found: usize,
    },

    /// A line sits at a depth its context does not allow.
    #[error(
        "indented to level {found}, but the block it sits in expects level {expected}. \
         Indentation is what nesting means here, so this is reported rather than guessed at"
    )]
    UnexpectedIndent {
        /// The level found.
        found: usize,
        /// The level expected.
        expected: usize,
    },

    /// A top-level word is not one of the known blocks.
    #[error(
        "`{found}` is not a snapshot block; the blocks are `schema`, `resources`, `entities`, \
         `free`"
    )]
    UnknownBlock {
        /// The word that was not recognised.
        found: String,
    },

    /// A field's value is not something this format can express.
    #[error(
        "`{field}` has a value this format cannot read back: `{found}`.\n\
         Nested structures inside a component are written out so nothing is lost, but they cannot \
         be parsed yet"
    )]
    UnreadableValue {
        /// Which field.
        field: String,
        /// What was there.
        found: String,
    },

    /// The same entity appears twice.
    #[error("entity {found} appears more than once")]
    DuplicateEntity {
        /// The repeated handle.
        found: String,
    },
}

/// One meaningful line: its number, its indent level, and its content.
#[derive(Debug, Clone)]
struct Line {
    number: usize,
    level: usize,
    text: String,
}

/// Reads a snapshot file.
///
/// # Errors
///
/// A [`ParseError`] naming the line and what was expected.
pub fn parse(text: &str) -> Result<Snapshot, ParseError> {
    let lines = split_lines(text)?;
    let mut cursor = 0usize;

    // --- Header ---
    let magic = lines.first().ok_or(ParseError {
        line: 1,
        kind: ParseErrorKind::BadMagic {
            found: String::new(),
        },
    })?;
    let version = parse_magic(magic)?;
    if version != FORMAT_VERSION {
        return Err(ParseError {
            line: magic.number,
            kind: ParseErrorKind::WrongVersion { found: version },
        });
    }
    cursor += 1;

    let tick = Tick(take_header_number(&lines, &mut cursor, "tick", 10)?);
    let state_hash = take_header_number(&lines, &mut cursor, "state-hash", 16)?;
    let layout = take_header_number(&lines, &mut cursor, "schema-hash", 16)?;

    let mut snapshot = Snapshot {
        tick,
        state_hash,
        layout,
        schema: Vec::new(),
        resources: BTreeMap::new(),
        entities: Vec::new(),
        free_slots: Vec::new(),
    };

    // --- Blocks, in any order. Order is fixed on the way out, not on the way in: a hand-edited
    // file that moved a block is still unambiguous, and refusing it would be pedantry.
    while cursor < lines.len() {
        let line = lines[cursor].clone();
        if line.level != 0 {
            return Err(ParseError {
                line: line.number,
                kind: ParseErrorKind::UnexpectedIndent {
                    found: line.level,
                    expected: 0,
                },
            });
        }
        cursor += 1;

        match line.text.as_str() {
            "schema" => snapshot.schema = parse_schema(&lines, &mut cursor)?,
            "resources" => snapshot.resources = parse_named_values(&lines, &mut cursor, 1)?,
            "entities" => snapshot.entities = parse_entities(&lines, &mut cursor)?,
            "free" => snapshot.free_slots = parse_free(&lines, &mut cursor)?,
            other => {
                return Err(ParseError {
                    line: line.number,
                    kind: ParseErrorKind::UnknownBlock {
                        found: other.to_string(),
                    },
                });
            }
        }
    }

    Ok(snapshot)
}

/// Reads `amadeo-snapshot N` and returns N.
fn parse_magic(line: &Line) -> Result<u32, ParseError> {
    let rest = line
        .text
        .strip_prefix("amadeo-snapshot ")
        .ok_or_else(|| ParseError {
            line: line.number,
            kind: ParseErrorKind::BadMagic {
                found: line.text.clone(),
            },
        })?;

    rest.trim().parse::<u32>().map_err(|_| ParseError {
        line: line.number,
        kind: ParseErrorKind::BadNumber {
            field: "format version".to_string(),
            expected: "a whole number",
            found: rest.trim().to_string(),
        },
    })
}

/// Consumes a `name value` header line, parsing the value in the given radix.
fn take_header_number(
    lines: &[Line],
    cursor: &mut usize,
    name: &'static str,
    radix: u32,
) -> Result<u64, ParseError> {
    let line = lines.get(*cursor).ok_or(ParseError {
        line: lines.last().map_or(1, |last| last.number),
        kind: ParseErrorKind::MissingHeader { name },
    })?;

    let rest = line.text.strip_prefix(name).and_then(|rest| {
        // `strip_prefix` alone would match `tick-rate` when looking for `tick`.
        rest.strip_prefix(' ')
    });
    let Some(rest) = rest else {
        return Err(ParseError {
            line: line.number,
            kind: ParseErrorKind::MissingHeader { name },
        });
    };

    let value = u64::from_str_radix(rest.trim(), radix).map_err(|_| ParseError {
        line: line.number,
        kind: ParseErrorKind::BadNumber {
            field: name.to_string(),
            expected: if radix == 16 {
                "a hexadecimal number"
            } else {
                "a whole number"
            },
            found: rest.trim().to_string(),
        },
    })?;

    *cursor += 1;
    Ok(value)
}

/// Reads a run of `name` blocks at `level`, each with indented fields beneath it.
fn parse_named_values(
    lines: &[Line],
    cursor: &mut usize,
    level: usize,
) -> Result<BTreeMap<String, Value>, ParseError> {
    let mut found = BTreeMap::new();

    while let Some(line) = lines.get(*cursor) {
        if line.level < level {
            break;
        }
        if line.level > level {
            return Err(ParseError {
                line: line.number,
                kind: ParseErrorKind::UnexpectedIndent {
                    found: line.level,
                    expected: level,
                },
            });
        }

        // The same split a field line gets, so a resource that reflects as a scalar (`Countdown 9`)
        // and one that reflects as a struct (`Camera2d` plus indented fields) are read by one rule
        // rather than two. A named block *is* a field; only its position differs.
        let (name, rest) = split_once(&line.text);
        *cursor += 1;
        let value = finish_value(lines, cursor, level, rest, &name, line.number)?;
        found.insert(name, value);
    }

    Ok(found)
}

/// Reads the indented lines making up one struct, list, or nested value.
///
/// Mirrors `write_field` exactly, and reads the same four signals:
///
/// - a name with something after it is a scalar or a flat list, on one line;
/// - `name ()` is a unit value, spelled out so it cannot be confused with an empty struct;
/// - a name whose children are `- ` items is a list;
/// - anything else with children is a struct — which is also how a **map** comes back, since the two
///   are written identically and `Reflect for BTreeMap` accepts either (ADR 0027).
fn parse_fields(lines: &[Line], cursor: &mut usize, level: usize) -> Result<Value, ParseError> {
    // A list and a struct are told apart by their first child, so the children are collected once
    // and classified rather than guessed at from the parent's name.
    let is_list = lines
        .get(*cursor)
        .is_some_and(|line| line.level == level && line.text == "-" || line.text.starts_with("- "));

    if is_list {
        let mut items = Vec::new();
        while let Some(line) = lines.get(*cursor) {
            if line.level != level {
                break;
            }
            let (marker, rest) = split_once(&line.text);
            if marker != "-" {
                break;
            }
            *cursor += 1;
            items.push(finish_value(lines, cursor, level, rest, "-", line.number)?);
        }
        return Ok(Value::List(items));
    }

    let mut fields: BTreeMap<String, Value> = BTreeMap::new();

    while let Some(line) = lines.get(*cursor) {
        if line.level < level {
            break;
        }
        if line.level > level {
            return Err(ParseError {
                line: line.number,
                kind: ParseErrorKind::UnexpectedIndent {
                    found: line.level,
                    expected: level,
                },
            });
        }

        let (name, rest) = split_once(&line.text);
        *cursor += 1;
        let value = finish_value(lines, cursor, level, rest, &name, line.number)?;
        fields.insert(name, value);
    }

    Ok(Value::Struct(fields))
}

/// Turns the text after a name into a value, descending into indented children when there is none.
///
/// The cursor is already past the name's own line when this is called.
fn finish_value(
    lines: &[Line],
    cursor: &mut usize,
    level: usize,
    rest: &str,
    field: &str,
    line: usize,
) -> Result<Value, ParseError> {
    if !rest.is_empty() {
        let inline = parse_value(rest, field, line)?;
        // A value on the line *and* children beneath it means an enum variant carrying data. The
        // counterpart to the writer's `Value::Enum` arm; anything else in that shape is a corrupt
        // file, and says so rather than silently dropping the children.
        let has_children = lines.get(*cursor).is_some_and(|next| next.level > level);
        if !has_children {
            return Ok(inline);
        }
        let Value::Enum(variant) = inline else {
            return Err(ParseError {
                line,
                kind: ParseErrorKind::BadNumber {
                    field: field.to_string(),
                    expected: "a bare variant name, since fields are indented beneath it",
                    found: rest.to_string(),
                },
            });
        };
        let payload = parse_fields(lines, cursor, level + 1)?;
        return Ok(Value::Enum(amadeo_reflect::EnumValue {
            variant: variant.variant,
            payload: Box::new(payload),
        }));
    }

    // Nothing on the line, so the value is whatever is indented beneath it. No children at all means
    // an empty struct — a unit value is written `()` precisely so the two do not collide.
    let has_children = lines.get(*cursor).is_some_and(|next| next.level > level);
    if has_children {
        parse_fields(lines, cursor, level + 1)
    } else {
        Ok(Value::Struct(BTreeMap::new()))
    }
}

/// Reads the `entities` block.
fn parse_entities(lines: &[Line], cursor: &mut usize) -> Result<Vec<SnapshotEntity>, ParseError> {
    let mut entities = Vec::new();
    let mut seen: BTreeMap<String, ()> = BTreeMap::new();

    while let Some(line) = lines.get(*cursor) {
        if line.level < 1 {
            break;
        }
        if line.level > 1 {
            return Err(ParseError {
                line: line.number,
                kind: ParseErrorKind::UnexpectedIndent {
                    found: line.level,
                    expected: 1,
                },
            });
        }

        let entity = parse_entity(&line.text, line.number)?;
        if seen.insert(line.text.clone(), ()).is_some() {
            return Err(ParseError {
                line: line.number,
                kind: ParseErrorKind::DuplicateEntity {
                    found: line.text.clone(),
                },
            });
        }
        *cursor += 1;

        let components = parse_named_values(lines, cursor, 2)?;
        entities.push(SnapshotEntity { entity, components });
    }

    Ok(entities)
}

/// Reads the `schema` block: `component Name 1` or `resource Name 1` per line.
///
/// Three fields rather than two because a component and a resource may legitimately share a
/// canonical name — ADR 0017 hashes each into its own id space — and a block that conflated them
/// would describe a layout neither one has.
fn parse_schema(lines: &[Line], cursor: &mut usize) -> Result<Vec<SchemaEntry>, ParseError> {
    let mut entries = Vec::new();

    while let Some(line) = lines.get(*cursor) {
        if line.level < 1 {
            break;
        }
        if line.level > 1 {
            return Err(ParseError {
                line: line.number,
                kind: ParseErrorKind::UnexpectedIndent {
                    found: line.level,
                    expected: 1,
                },
            });
        }

        let parts: Vec<&str> = line.text.split_whitespace().collect();
        let [keyword, name, version] = parts.as_slice() else {
            return Err(ParseError {
                line: line.number,
                kind: ParseErrorKind::BadSchemaEntry {
                    found: line.text.clone(),
                },
            });
        };

        let kind = match *keyword {
            "component" => SchemaKind::Component,
            "resource" => SchemaKind::Resource,
            _ => {
                return Err(ParseError {
                    line: line.number,
                    kind: ParseErrorKind::BadSchemaEntry {
                        found: line.text.clone(),
                    },
                });
            }
        };

        let version = version.parse::<u32>().map_err(|_| ParseError {
            line: line.number,
            kind: ParseErrorKind::BadNumber {
                field: format!("{name}'s schema version"),
                expected: "a whole number",
                found: (*version).to_string(),
            },
        })?;

        entries.push(SchemaEntry {
            kind,
            name: (*name).to_string(),
            version,
        });
        *cursor += 1;
    }

    Ok(entries)
}

/// Reads the `free` block.
fn parse_free(lines: &[Line], cursor: &mut usize) -> Result<Vec<Entity>, ParseError> {
    let mut slots = Vec::new();

    while let Some(line) = lines.get(*cursor) {
        if line.level < 1 {
            break;
        }
        if line.level > 1 {
            return Err(ParseError {
                line: line.number,
                kind: ParseErrorKind::UnexpectedIndent {
                    found: line.level,
                    expected: 1,
                },
            });
        }
        slots.push(parse_entity(&line.text, line.number)?);
        *cursor += 1;
    }

    Ok(slots)
}

/// Reads `index:generation`.
fn parse_entity(text: &str, line: usize) -> Result<Entity, ParseError> {
    let bad = || ParseError {
        line,
        kind: ParseErrorKind::BadEntity {
            found: text.to_string(),
        },
    };

    let (index, generation) = text.split_once(':').ok_or_else(bad)?;
    let index: u32 = index.trim().parse().map_err(|_| bad())?;
    let generation: u32 = generation.trim().parse().map_err(|_| bad())?;

    Ok(Entity::from_parts(index, generation))
}

/// Splits a line into its first word and the rest.
fn split_once(text: &str) -> (String, &str) {
    match text.split_once(' ') {
        Some((name, rest)) => (name.to_string(), rest.trim()),
        None => (text.to_string(), ""),
    }
}

/// Reads a field's value from the text after its name.
///
/// Deliberately the same shape as the scene format's: whitespace-separated scalars, joined into a
/// list when there is more than one. Nothing here parses a nested structure, because the writer
/// cannot produce one that round-trips — see [`ParseErrorKind::UnreadableValue`].
fn parse_value(text: &str, field: &str, line: usize) -> Result<Value, ParseError> {
    if text.is_empty() {
        return Ok(Value::Unit);
    }
    // Spelled out by the writer so it cannot be confused with an empty struct, which is written as a
    // bare name with nothing beneath it.
    if text == "()" {
        return Ok(Value::Unit);
    }
    // The same treatment for an empty list, and **checked before the bracket guard below**, which
    // would otherwise refuse it as a `Display`-form value.
    //
    // This is the fix for a defect that made the format unusable on any real game: every registered
    // event queue holds two empty lists at rest, an empty list used to write as a field with no
    // value at all, and a field with no value reads back as `Unit`. So `amadeo snapshot` followed by
    // `amadeo status --from` failed on `games/atrium` — **the engine wrote a file it could not
    // read** — and had done since events were first registered, because nothing had restored one.
    if text == "[]" {
        return Ok(Value::List(Vec::new()));
    }
    // And an empty map, which had the identical hole: a component holding one — `Facts` in
    // `modules/amadeo-behaviour` is the first — could be captured and not read back.
    if text == "{}" {
        return Ok(Value::Map(std::collections::BTreeMap::new()));
    }

    // A `Display`-form struct or map, which the writer emits so nothing is silently dropped. It
    // cannot be read back, and saying so beats producing a wrong value.
    if text.starts_with('{') || text.starts_with('[') {
        return Err(ParseError {
            line,
            kind: ParseErrorKind::UnreadableValue {
                field: field.to_string(),
                found: text.to_string(),
            },
        });
    }

    let parts = split_scalars(text);
    let values: Vec<Value> = parts.iter().map(|part| scalar(part)).collect();

    Ok(if values.len() == 1 {
        values.into_iter().next().unwrap_or(Value::Unit)
    } else {
        Value::List(values)
    })
}

/// Splits a value into scalars, keeping a quoted string in one piece.
fn split_scalars(text: &str) -> Vec<String> {
    let mut parts = Vec::new();
    let mut current = String::new();
    let mut in_string = false;
    let mut escaped = false;

    for character in text.chars() {
        match character {
            _ if escaped => {
                current.push(character);
                escaped = false;
            }
            '\\' if in_string => {
                current.push(character);
                escaped = true;
            }
            '"' => {
                current.push(character);
                in_string = !in_string;
            }
            ' ' if !in_string => {
                if !current.is_empty() {
                    parts.push(std::mem::take(&mut current));
                }
            }
            _ => current.push(character),
        }
    }
    if !current.is_empty() {
        parts.push(current);
    }
    parts
}

/// Turns one token into a value.
///
/// Untyped, like the scene parser: the punctuation decides. `1` is an integer and `1.0` is a float,
/// and `Reflect` is lenient about which arrives where a number is wanted, because the schema — not
/// the spelling — decides the field's real width.
fn scalar(text: &str) -> Value {
    if let Some(inner) = text.strip_prefix('"').and_then(|s| s.strip_suffix('"')) {
        return Value::String(unescape(inner));
    }
    if text == "true" {
        return Value::Bool(true);
    }
    if text == "false" {
        return Value::Bool(false);
    }
    if let Ok(number) = text.parse::<i64>() {
        return Value::I64(number);
    }
    if let Ok(number) = text.parse::<u64>() {
        return Value::U64(number);
    }
    if let Ok(number) = text.parse::<f64>() {
        return Value::F64(number);
    }
    // A bare word: a fieldless enum variant, which is what makes `state Patrol` read the way it
    // does in a scene file.
    Value::Enum(amadeo_reflect::EnumValue {
        variant: text.to_string(),
        payload: Box::new(Value::Unit),
    })
}

/// Reverses the writer's escaping.
fn unescape(text: &str) -> String {
    let mut out = String::with_capacity(text.len());
    let mut chars = text.chars();
    while let Some(character) = chars.next() {
        if character == '\\' {
            match chars.next() {
                Some(next) => out.push(next),
                None => out.push('\\'),
            }
        } else {
            out.push(character);
        }
    }
    out
}

/// Splits input into meaningful lines, validating indentation as it goes.
///
/// Blank lines are dropped — they are the format's paragraph breaks between blocks and carry no
/// meaning. There are no comments: a snapshot is a machine artefact that a human may edit, not a
/// document, so there is nothing to annotate that would survive the next capture.
fn split_lines(text: &str) -> Result<Vec<Line>, ParseError> {
    let mut lines = Vec::new();

    for (offset, raw) in text.lines().enumerate() {
        let number = offset + 1;
        if raw.trim().is_empty() {
            continue;
        }

        let indent = raw.len() - raw.trim_start().len();
        if raw[..indent].contains('\t') {
            return Err(ParseError {
                line: number,
                kind: ParseErrorKind::TabIndent,
            });
        }
        if !indent.is_multiple_of(INDENT) {
            return Err(ParseError {
                line: number,
                kind: ParseErrorKind::OddIndentation { found: indent },
            });
        }

        lines.push(Line {
            number,
            level: indent / INDENT,
            text: raw.trim().to_string(),
        });
    }

    Ok(lines)
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::to_text;

    const MINIMAL: &str =
        "amadeo-snapshot 2\ntick 0\nstate-hash 0000000000000000\nschema-hash 0000000000000000\n";

    #[test]
    fn a_minimal_snapshot_parses() {
        let snapshot = parse(MINIMAL).expect("valid");
        assert_eq!(snapshot.tick, Tick(0));
        assert_eq!(snapshot.state_hash, 0);
        assert!(snapshot.entities.is_empty());
    }

    #[test]
    fn the_header_values_come_back() {
        let text = "amadeo-snapshot 2\ntick 240\nstate-hash 54d624e36fa50dd4\n\
                    schema-hash 1f0a77c3b2d94e58\n";
        let snapshot = parse(text).expect("valid");
        assert_eq!(snapshot.tick, Tick(240));
        assert_eq!(snapshot.state_hash, 0x54d6_24e3_6fa5_0dd4);
        assert_eq!(snapshot.layout, 0x1f0a_77c3_b2d9_4e58);
    }

    #[test]
    fn the_schema_block_round_trips_with_both_kinds() {
        let text = format!("{MINIMAL}\nschema\n  component Transform 1\n  resource SimRng 3\n");
        let snapshot = parse(&text).expect("valid");

        assert_eq!(
            snapshot.schema,
            vec![
                SchemaEntry {
                    kind: SchemaKind::Component,
                    name: "Transform".to_string(),
                    version: 1,
                },
                SchemaEntry {
                    kind: SchemaKind::Resource,
                    name: "SimRng".to_string(),
                    version: 3,
                },
            ]
        );
        assert_eq!(to_text(&snapshot), text);
    }

    #[test]
    fn a_schema_line_that_is_not_one_says_what_they_look_like() {
        let text = format!("{MINIMAL}schema\n  Transform\n");
        let error = parse(&text).expect_err("two fields missing");
        assert!(error.to_string().contains("component Name 1"), "{error}");
    }

    #[test]
    fn a_wrong_version_is_refused_rather_than_guessed_at() {
        let error = parse("amadeo-snapshot 99\ntick 0\nstate-hash 0\n").expect_err("version");
        let message = error.to_string();
        assert!(message.contains("version 99"), "{message}");
        assert!(message.contains("take a fresh one"), "{message}");
    }

    #[test]
    fn something_that_is_not_a_snapshot_says_so() {
        let error = parse("amadeo-replay 1\ntick-rate 60\n").expect_err("wrong format");
        assert!(error.to_string().contains("amadeo-snapshot"), "{error}");
    }

    #[test]
    fn a_missing_header_line_names_it() {
        let error = parse("amadeo-snapshot 2\nstate-hash 0\n").expect_err("no tick");
        assert!(error.to_string().contains("tick"), "{error}");
    }

    #[test]
    fn a_missing_schema_hash_names_it() {
        let error = parse("amadeo-snapshot 2\ntick 0\nstate-hash 0\n").expect_err("no schema-hash");
        assert!(error.to_string().contains("schema-hash"), "{error}");
    }

    #[test]
    fn errors_carry_a_line_number() {
        let error = parse("amadeo-snapshot 2\ntick 0\nstate-hash 0\nschema-hash 0\nnonsense\n")
            .expect_err("unknown block");
        assert_eq!(error.line, 5);
        assert!(error.to_string().starts_with("line 5:"), "{error}");
    }

    #[test]
    fn a_tab_is_refused() {
        let text = format!("{MINIMAL}entities\n\t0:0\n");
        let error = parse(&text).expect_err("tab");
        assert!(error.to_string().contains("tab"), "{error}");
    }

    #[test]
    fn odd_indentation_is_refused() {
        let text = format!("{MINIMAL}entities\n   0:0\n");
        let error = parse(&text).expect_err("three spaces");
        assert!(error.to_string().contains("not a multiple"), "{error}");
    }

    #[test]
    fn a_bad_entity_handle_says_what_one_looks_like() {
        let text = format!("{MINIMAL}entities\n  seven\n");
        let error = parse(&text).expect_err("not a handle");
        assert!(error.to_string().contains("index:generation"), "{error}");
    }

    #[test]
    fn a_repeated_entity_is_refused() {
        let text = format!("{MINIMAL}entities\n  0:0\n  0:0\n");
        let error = parse(&text).expect_err("duplicate");
        assert!(error.to_string().contains("more than once"), "{error}");
    }

    #[test]
    fn the_free_list_keeps_the_order_it_was_written_in() {
        let text = format!("{MINIMAL}free\n  4:2\n  3:1\n");
        let snapshot = parse(&text).expect("valid");
        let indices: Vec<u32> = snapshot.free_slots.iter().map(|e| e.index()).collect();
        assert_eq!(indices, vec![4, 3]);
    }

    #[test]
    fn blocks_may_appear_in_any_order() {
        // Fixed on the way out, permissive on the way in: a hand-edited file that moved a block is
        // still unambiguous, and refusing it would be pedantry.
        let text = format!("{MINIMAL}free\n  1:0\n\nentities\n  0:0\n");
        let snapshot = parse(&text).expect("valid");
        assert_eq!(snapshot.entities.len(), 1);
        assert_eq!(snapshot.free_slots.len(), 1);
    }

    #[test]
    fn a_value_the_writer_could_not_express_is_refused_rather_than_misread() {
        let text = format!("{MINIMAL}resources\n  Weird\n    nested {{a: 1}}\n");
        let error = parse(&text).expect_err("unreadable");
        assert!(error.to_string().contains("cannot read back"), "{error}");
    }

    // --- Round trips: the property invariant I2 actually needs. ---

    #[test]
    fn text_survives_a_round_trip_unchanged() {
        let text = concat!(
            "amadeo-snapshot 2\n",
            "tick 240\n",
            "state-hash 54d624e36fa50dd4\n",
            "schema-hash 1f0a77c3b2d94e58\n",
            "\n",
            "schema\n",
            "  component Velocity 1\n",
            "  resource Camera2d 1\n",
            "\n",
            "resources\n",
            "  Camera2d\n",
            "    center 0.0 0.0\n",
            "    height 10.0\n",
            "\n",
            "entities\n",
            "  0:0\n",
            "    Velocity\n",
            "      x 1.5\n",
            "      y -2.0\n",
            "  1:0\n",
            "\n",
            "free\n",
            "  4:2\n",
            "  3:1\n",
        );

        let parsed = parse(text).expect("valid");
        assert_eq!(to_text(&parsed), text);
    }

    #[test]
    fn a_scalar_resource_survives_a_round_trip() {
        let text = concat!(
            "amadeo-snapshot 2\n",
            "tick 0\n",
            "state-hash 0000000000000000\n",
            "schema-hash 0000000000000000\n",
            "\n",
            "resources\n",
            "  Countdown 9\n",
        );
        let parsed = parse(text).expect("valid");
        // `I64` rather than `U64`: the parser is untyped, like the scene parser, so the punctuation
        // decides and a bare `9` is a signed integer. That is not a lossy guess -- `Reflect` accepts
        // either integer variant wherever a number is wanted, because the *schema* decides the
        // field's real width, not how it happened to be written.
        assert_eq!(parsed.resources.get("Countdown"), Some(&Value::I64(9)));
        assert_eq!(to_text(&parsed), text);
    }

    #[test]
    fn a_quoted_string_with_a_space_stays_one_value() {
        let text = format!("{MINIMAL}\nresources\n  Label\n    text \"hello world\"\n");
        let parsed = parse(&text).expect("valid");
        let label = parsed.resources.get("Label").expect("present");
        assert_eq!(
            label.field("text"),
            Some(&Value::String("hello world".to_string()))
        );
    }

    #[test]
    fn an_escaped_quote_survives() {
        let text = format!("{MINIMAL}\nresources\n  Label\n    text \"say \\\"hi\\\"\"\n");
        let parsed = parse(&text).expect("valid");
        let label = parsed.resources.get("Label").expect("present");
        assert_eq!(
            label.field("text"),
            Some(&Value::String("say \"hi\"".to_string()))
        );
    }

    #[test]
    fn an_entity_with_no_components_is_not_the_same_as_no_entity() {
        let text = format!("{MINIMAL}entities\n  0:0\n");
        let parsed = parse(&text).expect("valid");
        assert_eq!(parsed.entities.len(), 1);
        assert!(parsed.entities[0].components.is_empty());
    }
}
