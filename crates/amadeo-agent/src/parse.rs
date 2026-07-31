//! Reading JSON — the half [`crate::Json`] was missing.
//!
//! ADR 0016 keeps this hand-written and next to the writer rather than taking a dependency, for the
//! reasons `json.rs` already lists plus one more: the RPC server reads whatever a client sends it, so
//! this is the engine's most exposed parsing surface. Something that small and legible is something
//! Justin can read end to end when it misbehaves.
//!
//! # It is strict on purpose
//!
//! This accepts JSON and nothing else. No trailing commas, no comments, no single quotes, no
//! unquoted keys, no `NaN`, no leading `+` or `01`. The scene parser made the same choice for the
//! same reason (ADR 0014): a parser that guesses is a parser whose errors arrive later, somewhere
//! else, as wrong behaviour rather than as a message.
//!
//! Two strictnesses beyond the letter of the spec, both deliberate:
//!
//! - **Duplicate object keys are an error**, not a last-one-wins overwrite. `Json::Object` is a
//!   `BTreeMap`, so a silent overwrite is exactly the shape of bug that looks like the message was
//!   received correctly.
//! - **Nesting is capped** at [`MAX_DEPTH`]. This is a recursive-descent parser reading from a pipe,
//!   and a few thousand `[` characters would otherwise overflow the stack — a crash rather than an
//!   error message.

use crate::json::Json;
use std::collections::BTreeMap;

/// How deeply arrays and objects may nest before parsing gives up.
///
/// Real protocol messages nest a handful of levels; `describe` output is about four. 128 is far
/// past anything legitimate and far below what would exhaust the stack.
pub const MAX_DEPTH: usize = 128;

/// A JSON document that could not be read.
#[derive(Debug, Clone, PartialEq, Eq, thiserror::Error)]
#[error("line {line}, column {column}: {kind}")]
pub struct JsonError {
    /// The 1-based line the problem is on.
    pub line: usize,
    /// The 1-based column, counted in characters.
    pub column: usize,
    /// What went wrong.
    pub kind: JsonErrorKind,
}

/// What specifically went wrong.
#[derive(Debug, Clone, PartialEq, Eq, thiserror::Error)]
pub enum JsonErrorKind {
    /// The document ended in the middle of something.
    #[error("the document ends here, but {expected} was expected")]
    UnexpectedEnd {
        /// What would have been valid.
        expected: String,
    },

    /// A character appeared where something else was required.
    #[error("found `{found}`, but {expected} was expected")]
    Unexpected {
        /// The character actually found.
        found: char,
        /// What would have been valid.
        expected: String,
    },

    /// The document was complete, then kept going.
    #[error(
        "the value ended here, but the document continues. \
         JSON has exactly one top-level value"
    )]
    TrailingContent,

    /// A number was shaped wrongly.
    #[error(
        "`{text}` is not a JSON number. JSON allows -1, 0, 1.5, and 2e10, \
         but not +1, 01, .5, or 5."
    )]
    InvalidNumber {
        /// The text that was rejected.
        text: String,
    },

    /// The same key appeared twice in one object.
    #[error(
        "the key `{key}` appears twice in this object; \
         the second value would silently replace the first, so it is rejected instead"
    )]
    DuplicateKey {
        /// The repeated key.
        key: String,
    },

    /// A backslash escape that JSON does not define.
    #[error(
        r#"`\{found}` is not a JSON escape; the escapes are \" \\ \/ \b \f \n \r \t and \uXXXX"#
    )]
    BadEscape {
        /// The character that followed the backslash.
        found: char,
    },

    /// A `\u` escape was not followed by four hex digits.
    #[error(r"a \u escape needs exactly four hex digits, as in é")]
    BadUnicodeEscape,

    /// Half of a surrogate pair, with no matching half.
    #[error(
        r"this \u escape is half of a surrogate pair with no matching half, \
          so it does not name a character"
    )]
    LoneSurrogate,

    /// A raw control character inside a string.
    #[error(
        "a raw control character (U+{code:04X}) cannot appear inside a JSON string; \
         write it as an escape such as \\n or \\u{code:04x}"
    )]
    ControlCharacterInString {
        /// The offending code point.
        code: u32,
    },

    /// Nesting went past [`MAX_DEPTH`].
    #[error(
        "nesting goes deeper than {MAX_DEPTH} levels. \
         Real messages nest a handful, so this is treated as malformed input rather than parsed"
    )]
    TooDeep,
}

impl Json {
    /// Reads a JSON document.
    ///
    /// ```
    /// use amadeo_agent::Json;
    ///
    /// let value = Json::parse(r#"{"method":"describe","id":1}"#).expect("valid JSON");
    /// assert_eq!(value.to_compact(), r#"{"id":1,"method":"describe"}"#);
    /// ```
    ///
    /// # Errors
    ///
    /// Returns a [`JsonError`] carrying a line and column, plus a message saying what was expected.
    pub fn parse(text: &str) -> Result<Json, JsonError> {
        // A leading byte-order mark is skipped. JSON does not allow one, but Windows tooling adds
        // it freely -- PowerShell's pipe does -- and the resulting error names an *invisible*
        // character, which is about the least actionable message this parser could produce. This is
        // not the parser guessing: U+FEFF at the very start has exactly one meaning.
        let input = text.strip_prefix('\u{feff}').unwrap_or(text);

        let mut parser = Parser { input, position: 0 };

        parser.skip_whitespace();
        let value = parser.parse_value(0)?;
        parser.skip_whitespace();

        if parser.position < parser.input.len() {
            return Err(parser.error(JsonErrorKind::TrailingContent));
        }
        Ok(value)
    }
}

/// Where we are in the input.
///
/// `position` is a **byte** offset, not a character index. That is safe to index with because every
/// character this parser makes a decision on — braces, commas, quotes, digits — is ASCII, so the
/// offset always lands on a character boundary.
struct Parser<'a> {
    input: &'a str,
    position: usize,
}

impl<'a> Parser<'a> {
    /// The next byte, without consuming it.
    fn peek(&self) -> Option<u8> {
        self.input.as_bytes().get(self.position).copied()
    }

    /// The next character, without consuming it.
    fn peek_char(&self) -> Option<char> {
        self.input[self.position..].chars().next()
    }

    /// Steps past `count` bytes.
    fn advance(&mut self, count: usize) {
        self.position += count;
    }

    /// Skips the four characters JSON counts as whitespace.
    fn skip_whitespace(&mut self) {
        while let Some(byte) = self.peek() {
            match byte {
                b' ' | b'\t' | b'\n' | b'\r' => self.advance(1),
                _ => break,
            }
        }
    }

    /// Builds an error at the current position, working out the line and column for it.
    ///
    /// The scan over everything before the cursor only ever runs once, on the failure path.
    fn error(&self, kind: JsonErrorKind) -> JsonError {
        let consumed = &self.input[..self.position.min(self.input.len())];
        let line = consumed.matches('\n').count() + 1;
        let column = match consumed.rfind('\n') {
            Some(index) => consumed[index + 1..].chars().count() + 1,
            None => consumed.chars().count() + 1,
        };
        JsonError { line, column, kind }
    }

    /// Errors with "found X, expected Y", or "the document ends, expected Y" at the end of input.
    fn unexpected<T>(&self, expected: &str) -> Result<T, JsonError> {
        let kind = match self.peek_char() {
            Some(found) => JsonErrorKind::Unexpected {
                found,
                expected: expected.to_string(),
            },
            None => JsonErrorKind::UnexpectedEnd {
                expected: expected.to_string(),
            },
        };
        Err(self.error(kind))
    }

    /// Consumes `word` if it is next, otherwise errors.
    fn expect_word(&mut self, word: &str) -> Result<(), JsonError> {
        if self.input[self.position..].starts_with(word) {
            self.advance(word.len());
            Ok(())
        } else {
            self.unexpected(&format!("`{word}`"))
        }
    }

    /// Parses any JSON value.
    ///
    /// `depth` counts how many arrays and objects enclose this one. It is passed down rather than
    /// stored on the parser so it cannot be left un-decremented by an early return.
    fn parse_value(&mut self, depth: usize) -> Result<Json, JsonError> {
        if depth > MAX_DEPTH {
            return Err(self.error(JsonErrorKind::TooDeep));
        }

        match self.peek() {
            Some(b'n') => {
                self.expect_word("null")?;
                Ok(Json::Null)
            }
            Some(b't') => {
                self.expect_word("true")?;
                Ok(Json::Bool(true))
            }
            Some(b'f') => {
                self.expect_word("false")?;
                Ok(Json::Bool(false))
            }
            Some(b'"') => Ok(Json::String(self.parse_string()?)),
            Some(b'[') => self.parse_array(depth),
            Some(b'{') => self.parse_object(depth),
            Some(b'-' | b'0'..=b'9') => self.parse_number(),
            _ => self.unexpected("a value"),
        }
    }

    fn parse_array(&mut self, depth: usize) -> Result<Json, JsonError> {
        self.advance(1); // the '['
        let mut items = Vec::new();

        self.skip_whitespace();
        if self.peek() == Some(b']') {
            self.advance(1);
            return Ok(Json::Array(items));
        }

        loop {
            self.skip_whitespace();
            items.push(self.parse_value(depth + 1)?);
            self.skip_whitespace();

            match self.peek() {
                Some(b',') => self.advance(1),
                Some(b']') => {
                    self.advance(1);
                    return Ok(Json::Array(items));
                }
                _ => return self.unexpected("`,` or `]`"),
            }
        }
    }

    fn parse_object(&mut self, depth: usize) -> Result<Json, JsonError> {
        self.advance(1); // the '{'
        let mut members: BTreeMap<String, Json> = BTreeMap::new();

        self.skip_whitespace();
        if self.peek() == Some(b'}') {
            self.advance(1);
            return Ok(Json::Object(members));
        }

        loop {
            self.skip_whitespace();

            // The key position is captured before reading it, so a duplicate is reported at the
            // key rather than after the value that follows it.
            let key_position = self.position;
            if self.peek() != Some(b'"') {
                return self.unexpected("a quoted key");
            }
            let key = self.parse_string()?;

            self.skip_whitespace();
            if self.peek() != Some(b':') {
                return self.unexpected("`:`");
            }
            self.advance(1);

            self.skip_whitespace();
            let value = self.parse_value(depth + 1)?;

            if members.insert(key.clone(), value).is_some() {
                let at_key = Parser {
                    input: self.input,
                    position: key_position,
                };
                return Err(at_key.error(JsonErrorKind::DuplicateKey { key }));
            }

            self.skip_whitespace();
            match self.peek() {
                Some(b',') => self.advance(1),
                Some(b'}') => {
                    self.advance(1);
                    return Ok(Json::Object(members));
                }
                _ => return self.unexpected("`,` or `}`"),
            }
        }
    }

    /// Reads a quoted string, resolving escapes.
    fn parse_string(&mut self) -> Result<String, JsonError> {
        self.advance(1); // the opening quote
        let mut output = String::new();

        loop {
            let Some(character) = self.peek_char() else {
                return Err(self.error(JsonErrorKind::UnexpectedEnd {
                    expected: "a closing `\"`".to_string(),
                }));
            };

            match character {
                '"' => {
                    self.advance(1);
                    return Ok(output);
                }
                '\\' => {
                    self.advance(1);
                    output.push(self.parse_escape()?);
                }
                // The spec forbids raw control characters in strings. Accepting them would mean a
                // literal newline inside a string parsed fine here and broke every other reader.
                control if (control as u32) < 0x20 => {
                    return Err(self.error(JsonErrorKind::ControlCharacterInString {
                        code: control as u32,
                    }));
                }
                other => {
                    self.advance(other.len_utf8());
                    output.push(other);
                }
            }
        }
    }

    /// Reads the character after a backslash.
    fn parse_escape(&mut self) -> Result<char, JsonError> {
        let Some(marker) = self.peek_char() else {
            return Err(self.error(JsonErrorKind::UnexpectedEnd {
                expected: "an escape character".to_string(),
            }));
        };

        let simple = match marker {
            '"' => Some('"'),
            '\\' => Some('\\'),
            '/' => Some('/'),
            'b' => Some('\u{8}'),
            'f' => Some('\u{c}'),
            'n' => Some('\n'),
            'r' => Some('\r'),
            't' => Some('\t'),
            _ => None,
        };

        if let Some(resolved) = simple {
            self.advance(marker.len_utf8());
            return Ok(resolved);
        }

        if marker != 'u' {
            let kind = JsonErrorKind::BadEscape { found: marker };
            return Err(self.error(kind));
        }
        self.advance(1); // the 'u'

        let first = self.parse_four_hex_digits()?;

        // Characters above U+FFFF are written as a surrogate pair, because JSON escapes are 16-bit.
        // A high surrogate on its own is not a character, so the matching low half is required.
        if (0xD800..0xDC00).contains(&first) {
            let before_pair = self.position;
            if !self.input[self.position..].starts_with("\\u") {
                let at_escape = Parser {
                    input: self.input,
                    position: before_pair,
                };
                return Err(at_escape.error(JsonErrorKind::LoneSurrogate));
            }
            self.advance(2); // the "\u"
            let second = self.parse_four_hex_digits()?;

            if !(0xDC00..0xE000).contains(&second) {
                let at_escape = Parser {
                    input: self.input,
                    position: before_pair,
                };
                return Err(at_escape.error(JsonErrorKind::LoneSurrogate));
            }

            let combined = 0x1_0000 + ((first - 0xD800) << 10) + (second - 0xDC00);
            return char::from_u32(combined)
                .ok_or_else(|| self.error(JsonErrorKind::LoneSurrogate));
        }

        // A low surrogate arriving first has nothing to pair with.
        char::from_u32(first).ok_or_else(|| self.error(JsonErrorKind::LoneSurrogate))
    }

    fn parse_four_hex_digits(&mut self) -> Result<u32, JsonError> {
        let remaining = &self.input[self.position..];
        let digits: String = remaining.chars().take(4).collect();

        if digits.len() != 4 || !digits.chars().all(|c| c.is_ascii_hexdigit()) {
            return Err(self.error(JsonErrorKind::BadUnicodeEscape));
        }

        self.advance(4);
        u32::from_str_radix(&digits, 16).map_err(|_| self.error(JsonErrorKind::BadUnicodeEscape))
    }

    /// Reads a number, keeping integers and floats distinguishable.
    ///
    /// The distinction matters as much here as it does in the writer: a `layer` field that arrives
    /// as `0` is an integer and must stay one, or the value that comes back out has changed type.
    fn parse_number(&mut self) -> Result<Json, JsonError> {
        let start = self.position;

        if self.peek() == Some(b'-') {
            self.advance(1);
        }

        // JSON forbids a leading zero on a multi-digit integer, so `01` is rejected rather than
        // read as 1. That is the kind of input that means somebody built the message wrongly.
        match self.peek() {
            Some(b'0') => self.advance(1),
            Some(b'1'..=b'9') => {
                while matches!(self.peek(), Some(b'0'..=b'9')) {
                    self.advance(1);
                }
            }
            _ => return self.number_error(start),
        }

        let mut is_float = false;

        if self.peek() == Some(b'.') {
            is_float = true;
            self.advance(1);
            if !matches!(self.peek(), Some(b'0'..=b'9')) {
                return self.number_error(start);
            }
            while matches!(self.peek(), Some(b'0'..=b'9')) {
                self.advance(1);
            }
        }

        if matches!(self.peek(), Some(b'e' | b'E')) {
            is_float = true;
            self.advance(1);
            if matches!(self.peek(), Some(b'+' | b'-')) {
                self.advance(1);
            }
            if !matches!(self.peek(), Some(b'0'..=b'9')) {
                return self.number_error(start);
            }
            while matches!(self.peek(), Some(b'0'..=b'9')) {
                self.advance(1);
            }
        }

        let text = &self.input[start..self.position];

        if is_float {
            let value: f64 = text.parse().map_err(|_| {
                self.error(JsonErrorKind::InvalidNumber {
                    text: text.to_string(),
                })
            })?;
            return Ok(Json::Float(value));
        }

        // An integer too large for i64 becomes a float rather than an error. JSON numbers have no
        // width limit, so rejecting it would refuse a valid document; the precision loss is the
        // same trade every mainstream JSON reader makes.
        match text.parse::<i64>() {
            Ok(value) => Ok(Json::Int(value)),
            Err(_) => {
                let value: f64 = text.parse().map_err(|_| {
                    self.error(JsonErrorKind::InvalidNumber {
                        text: text.to_string(),
                    })
                })?;
                Ok(Json::Float(value))
            }
        }
    }

    /// Reports a malformed number, quoting enough of it to be recognisable.
    fn number_error<T>(&self, start: usize) -> Result<T, JsonError> {
        // Take the run of characters a number could plausibly have been made of, so the message
        // quotes `01` or `.5` rather than a single character.
        let tail: String = self.input[start..]
            .chars()
            .take_while(|c| c.is_ascii_digit() || matches!(c, '-' | '+' | '.' | 'e' | 'E'))
            .collect();
        let text = if tail.is_empty() {
            self.input[start..].chars().take(1).collect()
        } else {
            tail
        };

        let at_number = Parser {
            input: self.input,
            position: start,
        };
        Err(at_number.error(JsonErrorKind::InvalidNumber { text }))
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    /// Parses, or panics with the error — which is what makes a failing test readable.
    fn parse(text: &str) -> Json {
        Json::parse(text).unwrap_or_else(|error| panic!("{text} should parse, but: {error}"))
    }

    fn error_of(text: &str) -> JsonError {
        Json::parse(text).expect_err("should have been rejected")
    }

    #[test]
    fn reads_the_atoms() {
        assert_eq!(parse("null"), Json::Null);
        assert_eq!(parse("true"), Json::Bool(true));
        assert_eq!(parse("false"), Json::Bool(false));
        assert_eq!(parse("0"), Json::Int(0));
        assert_eq!(parse(r#""hi""#), Json::string("hi"));
    }

    #[test]
    fn integers_and_floats_stay_distinguishable() {
        // The property the writer works to preserve; a reader that collapses them undoes it.
        assert_eq!(parse("0"), Json::Int(0));
        assert_eq!(parse("0.0"), Json::Float(0.0));
        assert_eq!(parse("-7"), Json::Int(-7));
        assert_eq!(parse("1e3"), Json::Float(1000.0));
        assert_eq!(parse("1.5E-2"), Json::Float(0.015));
    }

    #[test]
    fn whitespace_between_anything_is_ignored() {
        let spaced = parse(" {\n  \"a\" : [ 1 , 2 ]\r\n} ");
        assert_eq!(
            spaced,
            Json::object([("a", Json::Array(vec![Json::Int(1), Json::Int(2)]))])
        );
    }

    #[test]
    fn empty_containers_parse() {
        assert_eq!(parse("[]"), Json::Array(Vec::new()));
        assert_eq!(parse("{}"), Json::Object(BTreeMap::new()));
        assert_eq!(parse("[[],{}]"), {
            Json::Array(vec![Json::Array(Vec::new()), Json::Object(BTreeMap::new())])
        });
    }

    #[test]
    fn escapes_resolve() {
        assert_eq!(parse(r#""a\"b""#), Json::string("a\"b"));
        assert_eq!(parse(r#""a\\b""#), Json::string("a\\b"));
        assert_eq!(parse(r#""a\/b""#), Json::string("a/b"));
        assert_eq!(parse(r#""a\nb""#), Json::string("a\nb"));
        assert_eq!(parse(r#""a\tb""#), Json::string("a\tb"));
        assert_eq!(parse(r#""A""#), Json::string("A"));
        assert_eq!(parse(r#""é""#), Json::string("é"));
    }

    #[test]
    fn surrogate_pairs_become_one_character() {
        // Anything above U+FFFF has to arrive as a pair, because JSON escapes are 16 bits.
        assert_eq!(parse(r#""😀""#), Json::string("😀"));
    }

    #[test]
    fn a_lone_surrogate_is_rejected_rather_than_replaced() {
        // Substituting U+FFFD would let corrupt input through looking like text.
        assert_eq!(error_of(r#""\ud83d""#).kind, JsonErrorKind::LoneSurrogate);
        assert_eq!(error_of(r#""\ud83dQ""#).kind, JsonErrorKind::LoneSurrogate);
        assert_eq!(error_of(r#""\ud83dA""#).kind, JsonErrorKind::LoneSurrogate);
        assert_eq!(error_of(r#""\ude00""#).kind, JsonErrorKind::LoneSurrogate);
    }

    #[test]
    fn duplicate_keys_are_an_error_not_an_overwrite() {
        // The whole point: `{"a":1,"a":2}` is a message somebody built wrongly, and last-one-wins
        // would hide that behind a value that looks perfectly reasonable.
        let error = error_of(r#"{"a":1,"a":2}"#);
        assert_eq!(
            error.kind,
            JsonErrorKind::DuplicateKey {
                key: "a".to_string()
            }
        );
        // Reported at the second key, not at the end of the object.
        assert_eq!(error.column, 8);
    }

    #[test]
    fn malformed_numbers_are_rejected() {
        for text in ["01", "+1", ".5", "5.", "1e", "1e+", "-", "1.2.3"] {
            let error = Json::parse(text);
            assert!(error.is_err(), "`{text}` should not parse, got {error:?}");
        }
    }

    #[test]
    fn an_integer_too_large_for_i64_becomes_a_float() {
        // Refusing it would reject a valid document; this is the trade every JSON reader makes.
        assert_eq!(parse("99999999999999999999"), Json::Float(1e20));
    }

    #[test]
    fn raw_control_characters_in_strings_are_rejected() {
        let error = error_of("\"a\nb\"");
        assert_eq!(
            error.kind,
            JsonErrorKind::ControlCharacterInString { code: 0x0A }
        );
    }

    #[test]
    fn trailing_content_is_reported_rather_than_ignored() {
        // `{"a":1} {"b":2}` silently returning only the first object is how half a message gets
        // acted on.
        assert_eq!(error_of("1 2").kind, JsonErrorKind::TrailingContent);
        assert_eq!(error_of("{} []").kind, JsonErrorKind::TrailingContent);
    }

    #[test]
    fn json_extensions_other_parsers_allow_are_refused() {
        for text in [
            "[1,]",          // trailing comma
            "{\"a\":1,}",    // trailing comma
            "{a:1}",         // unquoted key
            "'a'",           // single quotes
            "// comment\n1", // comment
            "NaN",           // not JSON
            "Infinity",      // not JSON
            "[1 2]",         // missing comma
            "{\"a\" 1}",     // missing colon
        ] {
            assert!(
                Json::parse(text).is_err(),
                "`{text}` is not JSON and should be refused"
            );
        }
    }

    #[test]
    fn deep_nesting_gives_an_error_rather_than_overflowing_the_stack() {
        // A pipe can deliver this, and a stack overflow is a crash rather than a message.
        let deep = "[".repeat(MAX_DEPTH + 10);
        assert_eq!(error_of(&deep).kind, JsonErrorKind::TooDeep);
    }

    #[test]
    fn unterminated_input_says_what_was_missing() {
        assert!(matches!(
            error_of(r#""abc"#).kind,
            JsonErrorKind::UnexpectedEnd { .. }
        ));
        assert!(matches!(
            error_of("[1,").kind,
            JsonErrorKind::UnexpectedEnd { .. }
        ));
        assert!(matches!(
            error_of("{").kind,
            JsonErrorKind::UnexpectedEnd { .. }
        ));
    }

    #[test]
    fn errors_carry_the_line_and_column() {
        // Pillar 5: the message is the product. A byte offset is not something a human can act on.
        let error = error_of("{\n  \"a\": 1,\n  \"b\": @\n}");
        assert_eq!(error.line, 3);
        assert_eq!(error.column, 8);
        assert!(
            error.to_string().starts_with("line 3, column 8:"),
            "got: {error}"
        );
    }

    #[test]
    fn a_leading_byte_order_mark_is_skipped() {
        // Windows tooling adds one without being asked; PowerShell's pipe does. Rejecting it
        // produced an error pointing at a character you cannot see, which is worse than useless.
        assert_eq!(parse("\u{feff}{\"a\":1}"), parse("{\"a\":1}"));
        assert_eq!(parse("\u{feff}null"), Json::Null);

        // Only at the very start, and only one. Anywhere else it is real content and stays an error.
        assert!(Json::parse("{\u{feff}\"a\":1}").is_err());
        assert!(Json::parse("\u{feff}\u{feff}null").is_err());
    }

    #[test]
    fn round_trips_through_the_writer() {
        // The property that matters for the RPC layer: what the writer emits, the reader reads back
        // as the same value. Anything else means a request and its echo disagree.
        let document = Json::object([
            ("null", Json::Null),
            ("yes", Json::Bool(true)),
            ("no", Json::Bool(false)),
            ("int", Json::Int(-42)),
            ("big", Json::Int(i64::MAX)),
            ("float", Json::Float(1.5)),
            ("negative_float", Json::Float(-2.0)),
            ("tiny", Json::Float(1e-300)),
            ("huge", Json::Float(1e300)),
            (
                "text",
                Json::string("quotes \" backslash \\ newline \n tab \t"),
            ),
            ("unicode", Json::string("héllo 😀")),
            ("control", Json::string("\u{1}")),
            ("empty_array", Json::Array(Vec::new())),
            ("empty_object", Json::Object(BTreeMap::new())),
            (
                "nested",
                Json::Array(vec![
                    Json::object([("deep", Json::Array(vec![Json::Int(1)]))]),
                    Json::Null,
                ]),
            ),
        ]);

        assert_eq!(Json::parse(&document.to_compact()), Ok(document.clone()));
        assert_eq!(Json::parse(&document.to_pretty()), Ok(document));
    }
}
