//! Parsing a scene file into a [`SceneDocument`].
//!
//! Layer 1 of ADR 0014: syntax only. This never consults the reflection registry, so a scene naming
//! a component from an unloaded module still parses — which is what lets `amadeo fmt` work on any
//! file. Checking that `Transform` exists and has a `position` field is layer 2's job.
//!
//! # Errors are the product here
//!
//! `docs/03-ai-native-design.md` Pillar 5 makes error quality a functional requirement, and this is
//! the format we chose specifically so the messages would be ours (ADR 0014). Every error carries a
//! **line number** and says what was expected, not just what was wrong.

use crate::document::{SceneDocument, SceneEntity};
use amadeo_reflect::Value;
use std::collections::BTreeMap;

/// How many spaces one level of nesting costs.
pub const INDENT: usize = 2;

/// A scene file that could not be parsed.
#[derive(Debug, Clone, PartialEq, Eq, thiserror::Error)]
#[error("line {line}: {kind}")]
pub struct ParseError {
    /// The 1-based line the problem is on.
    pub line: usize,
    /// What went wrong.
    pub kind: ParseErrorKind,
}

/// What specifically went wrong.
#[derive(Debug, Clone, PartialEq, Eq, thiserror::Error)]
pub enum ParseErrorKind {
    /// A tab appeared in the leading whitespace.
    #[error(
        "indentation uses a tab; scene files indent with {INDENT} spaces per level. \
         Mixed indentation is invisible on screen, so it is rejected rather than guessed at"
    )]
    TabIndentation,

    /// Indentation was not a multiple of [`INDENT`].
    #[error(
        "indented by {found} spaces, which is not a multiple of {INDENT}; \
         run `amadeo fmt` to normalise it"
    )]
    OddIndentation {
        /// How many spaces were found.
        found: usize,
    },

    /// A line was indented further than its context allows.
    #[error(
        "indented to level {found}, but the block it sits in expects level {expected}. \
         Indentation is what nesting means here, so this is reported rather than guessed at"
    )]
    UnexpectedIndent {
        /// The level that would have been valid.
        expected: usize,
        /// The level found.
        found: usize,
    },

    /// The `scene` or `version` header was missing or out of order.
    #[error(
        "expected `{expected}` here. A scene file starts with:\n    scene <name>\n    version 1"
    )]
    MissingHeader {
        /// The keyword that was expected.
        expected: String,
    },

    /// The version was not a number.
    #[error("`{found}` is not a version number; expected a whole number such as `version 1`")]
    BadVersion {
        /// What was written instead.
        found: String,
    },

    /// An `entity` line was not shaped like one.
    #[error(
        "{detail}\nAn entity line looks like:\n    entity <id> \"<name>\" [from <prefab-path>]"
    )]
    MalformedEntity {
        /// What specifically was wrong.
        detail: String,
    },

    /// An `override` block appeared on an entity that instances no prefab.
    #[error(
        "entity `{id}` has an `override` block but does not instance a prefab; \
         add `from <prefab-path>` to its entity line, or make this a plain component"
    )]
    OverrideWithoutPrefab {
        /// The offending entity's id.
        id: String,
    },

    /// A quoted string had no closing quote.
    #[error("unterminated string; add a closing `\"`")]
    UnterminatedString,

    /// An entity's name was not quoted.
    #[error(
        "an entity's name must be quoted, as in `entity {id} \"My Entity\"`. \
         Bare words are identifiers and quoted words are text, so the two never blur"
    )]
    UnquotedEntityName {
        /// The entity's id, for the suggested fix.
        id: String,
    },

    /// A field had no value and no list beneath it.
    #[error(
        "field `{name}` has no value. Write it inline (`{name} 1.0`) or give it a list:\n    \
         {name}\n      - 1.0 2.0"
    )]
    EmptyField {
        /// The field's name.
        name: String,
    },

    /// A `- ` list item appeared where no list was open.
    #[error("list item `-` here belongs under a field, but no field is open above it")]
    ListItemOutsideField,

    /// Something appeared before any entity that is not a header.
    #[error("expected an `entity` line here, found `{found}`")]
    ExpectedEntity {
        /// The first word of the offending line.
        found: String,
    },

    /// The same component was declared twice on one entity.
    #[error(
        "entity `{entity}` declares `{component}` twice; an entity has at most one of each component"
    )]
    DuplicateComponent {
        /// The entity's id.
        entity: String,
        /// The repeated component name.
        component: String,
    },

    /// The same field was declared twice in one component.
    #[error("`{component}` sets `{field}` twice")]
    DuplicateField {
        /// The component's name.
        component: String,
        /// The repeated field name.
        field: String,
    },
}

/// One meaningful line: blanks and comment-only lines never reach the parser.
#[derive(Debug, Clone)]
struct Line {
    /// 1-based, for diagnostics.
    number: usize,
    /// Nesting level, already divided by [`INDENT`].
    level: usize,
    /// The line's content, comment and indentation stripped.
    text: String,
}

/// A token, remembering whether it was written in quotes.
///
/// The distinction is load-bearing: a bare word is an identifier (an enum variant), a quoted word is
/// a string. `pattern Irregular` and `key_id "rusted_key"` mean different things.
#[derive(Debug, Clone, PartialEq, Eq)]
struct Token {
    text: String,
    quoted: bool,
}

/// Removes a trailing `# comment`, ignoring `#` inside quotes.
fn strip_comment(text: &str) -> &str {
    let mut in_quotes = false;
    let mut escaped = false;
    for (index, character) in text.char_indices() {
        if escaped {
            escaped = false;
            continue;
        }
        match character {
            '\\' if in_quotes => escaped = true,
            '"' => in_quotes = !in_quotes,
            '#' if !in_quotes => return &text[..index],
            _ => {}
        }
    }
    text
}

/// Splits a line into tokens, keeping quoted runs together.
fn tokenize(text: &str, line: usize) -> Result<Vec<Token>, ParseError> {
    let mut tokens = Vec::new();
    let mut characters = text.chars().peekable();

    while let Some(&character) = characters.peek() {
        if character.is_whitespace() {
            characters.next();
            continue;
        }

        if character == '"' {
            characters.next();
            let mut value = String::new();
            let mut closed = false;
            while let Some(inner) = characters.next() {
                match inner {
                    '\\' => match characters.next() {
                        // Only two escapes, deliberately: anything richer is a second syntax to
                        // learn, and a scene file is not a place for `\u{1F600}`.
                        Some('"') => value.push('"'),
                        Some('\\') => value.push('\\'),
                        Some(other) => {
                            value.push('\\');
                            value.push(other);
                        }
                        None => break,
                    },
                    '"' => {
                        closed = true;
                        break;
                    }
                    other => value.push(other),
                }
            }
            if !closed {
                return Err(ParseError {
                    line,
                    kind: ParseErrorKind::UnterminatedString,
                });
            }
            tokens.push(Token {
                text: value,
                quoted: true,
            });
            continue;
        }

        let mut value = String::new();
        while let Some(&inner) = characters.peek() {
            if inner.is_whitespace() {
                break;
            }
            value.push(inner);
            characters.next();
        }
        tokens.push(Token {
            text: value,
            quoted: false,
        });
    }

    Ok(tokens)
}

/// Turns one token into a value.
fn token_to_value(token: &Token) -> Value {
    if token.quoted {
        return Value::String(token.text.clone());
    }
    match token.text.as_str() {
        "true" => return Value::Bool(true),
        "false" => return Value::Bool(false),
        _ => {}
    }
    // An integer only if it is written as one: `1` is an integer, `1.0` is a float. The text says
    // which, so the reader and the parser agree without consulting a schema.
    let looks_like_float = token.text.contains(['.', 'e', 'E']);
    if !looks_like_float && let Ok(number) = token.text.parse::<i64>() {
        return Value::I64(number);
    }
    if let Ok(number) = token.text.parse::<f64>() {
        return Value::F64(number);
    }
    // Anything else is a bare identifier, which is how enum variants are written.
    Value::unit_variant(token.text.clone())
}

/// Builds a value from the tokens after a name: one is a scalar, several are a list.
fn values_to_value(tokens: &[Token]) -> Value {
    if tokens.len() == 1 {
        token_to_value(&tokens[0])
    } else {
        Value::List(tokens.iter().map(token_to_value).collect())
    }
}

/// Splits the input into meaningful lines, validating indentation as it goes.
fn prepare(input: &str) -> Result<Vec<Line>, ParseError> {
    let mut lines = Vec::new();

    for (index, raw) in input.lines().enumerate() {
        let number = index + 1;
        let content = strip_comment(raw);
        if content.trim().is_empty() {
            continue;
        }

        let indent = content.len() - content.trim_start().len();
        if content[..indent].contains('\t') {
            return Err(ParseError {
                line: number,
                kind: ParseErrorKind::TabIndentation,
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
            text: content.trim().to_string(),
        });
    }

    Ok(lines)
}

/// Walks the prepared lines.
struct Parser {
    lines: Vec<Line>,
    cursor: usize,
}

impl Parser {
    fn peek(&self) -> Option<&Line> {
        self.lines.get(self.cursor)
    }

    /// The next line, but only if it sits at exactly `level`.
    fn peek_at(&self, level: usize) -> Option<&Line> {
        self.peek().filter(|line| line.level == level)
    }

    fn parse_document(&mut self) -> Result<SceneDocument, ParseError> {
        let name = self.parse_header("scene")?;
        let version_text = self.parse_header("version")?;
        let version = version_text.parse::<u32>().map_err(|_| ParseError {
            // The header line was already consumed, so point at the line before the cursor.
            line: self.lines[self.cursor.saturating_sub(1)].number,
            kind: ParseErrorKind::BadVersion {
                found: version_text.clone(),
            },
        })?;

        let mut entities = Vec::new();
        while let Some(line) = self.peek() {
            if line.level != 0 {
                return Err(ParseError {
                    line: line.number,
                    kind: ParseErrorKind::UnexpectedIndent {
                        expected: 0,
                        found: line.level,
                    },
                });
            }
            let first = line
                .text
                .split_whitespace()
                .next()
                .unwrap_or("")
                .to_string();
            if first != "entity" {
                return Err(ParseError {
                    line: line.number,
                    kind: ParseErrorKind::ExpectedEntity { found: first },
                });
            }
            entities.push(self.parse_entity(0)?);
        }

        Ok(SceneDocument {
            name,
            version,
            entities,
        })
    }

    /// Consumes a `<keyword> <value>` header line and returns the value.
    fn parse_header(&mut self, keyword: &str) -> Result<String, ParseError> {
        let Some(line) = self.peek().cloned() else {
            return Err(ParseError {
                line: self.lines.last().map_or(1, |last| last.number),
                kind: ParseErrorKind::MissingHeader {
                    expected: keyword.to_string(),
                },
            });
        };

        let tokens = tokenize(&line.text, line.number)?;
        if line.level != 0 || tokens.len() != 2 || tokens[0].text != keyword {
            return Err(ParseError {
                line: line.number,
                kind: ParseErrorKind::MissingHeader {
                    expected: keyword.to_string(),
                },
            });
        }

        self.cursor += 1;
        Ok(tokens[1].text.clone())
    }

    fn parse_entity(&mut self, level: usize) -> Result<SceneEntity, ParseError> {
        let line = self.lines[self.cursor].clone();
        let tokens = tokenize(&line.text, line.number)?;

        if tokens.len() < 3 {
            return Err(ParseError {
                line: line.number,
                kind: ParseErrorKind::MalformedEntity {
                    detail: "an entity needs an id and a quoted name".to_string(),
                },
            });
        }
        if !tokens[2].quoted {
            return Err(ParseError {
                line: line.number,
                kind: ParseErrorKind::UnquotedEntityName {
                    id: tokens[1].text.clone(),
                },
            });
        }

        let mut entity = SceneEntity::new(tokens[1].text.clone(), tokens[2].text.clone());

        match tokens.len() {
            3 => {}
            5 if tokens[3].text == "from" => {
                entity.prefab = Some(tokens[4].text.clone());
            }
            _ => {
                return Err(ParseError {
                    line: line.number,
                    kind: ParseErrorKind::MalformedEntity {
                        detail: format!(
                            "did not understand `{}` after the entity's name",
                            tokens[3..]
                                .iter()
                                .map(|token| token.text.as_str())
                                .collect::<Vec<_>>()
                                .join(" ")
                        ),
                    },
                });
            }
        }

        self.cursor += 1;
        let inner = level + 1;

        while let Some(next) = self.peek_at(inner).cloned() {
            let first = next.text.split_whitespace().next().unwrap_or("");
            match first {
                "entity" => entity.children.push(self.parse_entity(inner)?),
                "override" => {
                    if entity.prefab.is_none() {
                        return Err(ParseError {
                            line: next.number,
                            kind: ParseErrorKind::OverrideWithoutPrefab {
                                id: entity.id.clone(),
                            },
                        });
                    }
                    let (name, value) = self.parse_component(inner, true, &entity.id)?;
                    insert_component(&mut entity.overrides, name, value, &entity.id, next.number)?;
                }
                "-" => {
                    return Err(ParseError {
                        line: next.number,
                        kind: ParseErrorKind::ListItemOutsideField,
                    });
                }
                _ => {
                    let (name, value) = self.parse_component(inner, false, &entity.id)?;
                    insert_component(&mut entity.components, name, value, &entity.id, next.number)?;
                }
            }
        }

        // Anything indented deeper than one level below this entity has no parent to belong to.
        if let Some(stray) = self.peek()
            && stray.level > inner
        {
            return Err(ParseError {
                line: stray.number,
                kind: ParseErrorKind::UnexpectedIndent {
                    expected: inner,
                    found: stray.level,
                },
            });
        }

        Ok(entity)
    }

    /// Parses a component (or override) block and its fields.
    fn parse_component(
        &mut self,
        level: usize,
        is_override: bool,
        _entity: &str,
    ) -> Result<(String, Value), ParseError> {
        let line = self.lines[self.cursor].clone();
        let tokens = tokenize(&line.text, line.number)?;

        // `override Transform` carries the name in the second token; a plain component in the
        // first.
        let name = if is_override {
            tokens
                .get(1)
                .map(|token| token.text.clone())
                .ok_or_else(|| ParseError {
                    line: line.number,
                    kind: ParseErrorKind::MalformedEntity {
                        detail: "`override` needs a component name after it".to_string(),
                    },
                })?
        } else {
            tokens[0].text.clone()
        };

        self.cursor += 1;
        let inner = level + 1;
        let mut fields: BTreeMap<String, Value> = BTreeMap::new();

        while let Some(next) = self.peek_at(inner).cloned() {
            let field_tokens = tokenize(&next.text, next.number)?;
            if field_tokens.is_empty() {
                self.cursor += 1;
                continue;
            }
            if field_tokens[0].text == "-" {
                return Err(ParseError {
                    line: next.number,
                    kind: ParseErrorKind::ListItemOutsideField,
                });
            }

            let field_name = field_tokens[0].text.clone();
            self.cursor += 1;

            let value = if field_tokens.len() > 1 {
                values_to_value(&field_tokens[1..])
            } else {
                // No inline value, so the field's value is the list indented beneath it.
                let items = self.parse_list(inner + 1)?;
                if items.is_empty() {
                    return Err(ParseError {
                        line: next.number,
                        kind: ParseErrorKind::EmptyField { name: field_name },
                    });
                }
                Value::List(items)
            };

            if fields.insert(field_name.clone(), value).is_some() {
                return Err(ParseError {
                    line: next.number,
                    kind: ParseErrorKind::DuplicateField {
                        component: name,
                        field: field_name,
                    },
                });
            }
        }

        Ok((name, Value::Struct(fields)))
    }

    /// Collects `- ...` items at `level`.
    fn parse_list(&mut self, level: usize) -> Result<Vec<Value>, ParseError> {
        let mut items = Vec::new();
        while let Some(next) = self.peek_at(level).cloned() {
            let tokens = tokenize(&next.text, next.number)?;
            if tokens.first().map(|token| token.text.as_str()) != Some("-") {
                break;
            }
            if tokens.len() < 2 {
                return Err(ParseError {
                    line: next.number,
                    kind: ParseErrorKind::EmptyField {
                        name: "-".to_string(),
                    },
                });
            }
            items.push(values_to_value(&tokens[1..]));
            self.cursor += 1;
        }
        Ok(items)
    }
}

/// Inserts a component, refusing a duplicate rather than silently overwriting.
fn insert_component(
    into: &mut BTreeMap<String, Value>,
    name: String,
    value: Value,
    entity: &str,
    line: usize,
) -> Result<(), ParseError> {
    if into.insert(name.clone(), value).is_some() {
        return Err(ParseError {
            line,
            kind: ParseErrorKind::DuplicateComponent {
                entity: entity.to_string(),
                component: name,
            },
        });
    }
    Ok(())
}

/// Parses scene text into a document.
///
/// # Errors
///
/// Returns a [`ParseError`] carrying the 1-based line number and a message that says what was
/// expected there.
pub fn parse(input: &str) -> Result<SceneDocument, ParseError> {
    let lines = prepare(input)?;
    if lines.is_empty() {
        return Err(ParseError {
            line: 1,
            kind: ParseErrorKind::MissingHeader {
                expected: "scene".to_string(),
            },
        });
    }
    let mut parser = Parser { lines, cursor: 0 };
    parser.parse_document()
}
