//! The replay file: a recorded action stream plus checkpoint hashes.
//!
//! # The format
//!
//! This is the project's first authored text format, so it is built to the rules every later format
//! must follow (invariants I1 and I2): hand-writable, line-oriented, canonically ordered, and
//! **byte-stable** — parsing a file and writing it back produces identical bytes.
//!
//! ```text
//! amadeo-replay 1
//! tick-rate 60
//! seed 1234
//! ticks 600
//!
//! 0 axis move_x 1.0
//! 12 button jump down
//! 15 button jump up
//!
//! checkpoint 100 c3e3fe2b8ec1d932
//! checkpoint 600 aabbccdd00112233
//! ```
//!
//! Only *changes* are recorded, not a value per action per tick. A 600-tick recording of a player
//! walking and jumping is a few dozen lines rather than several thousand, and the diff between two
//! recordings shows what actually differed.
//!
//! `checkpoint` lines are the assertions: at that tick, the world's state hash must equal that value.
//! They are what turns a recording into a regression test (ADR 0005).

use crate::action::{ActionId, ActionKind};
use amadeo_core::{TICK_RATE_HZ, Tick};
use std::collections::BTreeMap;
use std::fmt::Write as _;

/// The format version written into every file's first line.
const FORMAT_VERSION: u32 = 1;

/// What can go wrong reading a replay file.
///
/// Every variant carries the line number, because a parse error a human cannot locate is barely
/// better than no error at all.
#[derive(Debug, thiserror::Error, PartialEq, Eq)]
pub enum ReplayError {
    /// The file did not start with the expected magic line.
    #[error(
        "line 1: expected a header like 'amadeo-replay {FORMAT_VERSION}', found '{found}'; this does \
         not look like a replay file"
    )]
    BadHeader {
        /// What the first line actually contained.
        found: String,
    },

    /// The file was written by a different version of the format.
    #[error(
        "replay file uses format version {found}, but this build understands version \
         {FORMAT_VERSION}; re-record the replay"
    )]
    UnsupportedVersion {
        /// The version the file declares.
        found: u32,
    },

    /// The recording was made at a different simulation rate.
    #[error(
        "replay was recorded at {found} Hz but this build simulates at {expected} Hz; replaying it \
         would produce different behaviour, so it is refused. Re-record the replay."
    )]
    TickRateMismatch {
        /// The rate the file declares.
        found: u32,
        /// The rate this build uses.
        expected: u32,
    },

    /// A line could not be understood.
    #[error("line {line}: {reason}; got '{content}'")]
    Malformed {
        /// One-based line number.
        line: usize,
        /// What was expected.
        reason: String,
        /// The offending line.
        content: String,
    },

    /// A required header field was missing.
    #[error("missing required header field '{field}'")]
    MissingField {
        /// Which field was absent.
        field: &'static str,
    },
}

/// One change to one action at one tick.
#[derive(Debug, Clone, PartialEq)]
pub enum InputChange {
    /// A button went down or up.
    Button {
        /// The action's name, as written in the file.
        action: String,
        /// Whether it is now held.
        pressed: bool,
    },
    /// An axis moved to a new value.
    Axis {
        /// The action's name, as written in the file.
        action: String,
        /// The new value.
        value: f32,
    },
}

impl InputChange {
    /// The action's name.
    #[must_use]
    pub fn action_name(&self) -> &str {
        match self {
            InputChange::Button { action, .. } | InputChange::Axis { action, .. } => action,
        }
    }

    /// The action's id.
    #[must_use]
    pub fn action_id(&self) -> ActionId {
        ActionId::new(self.action_name())
    }

    /// Whether this is a button or an axis change.
    #[must_use]
    pub fn kind(&self) -> ActionKind {
        match self {
            InputChange::Button { .. } => ActionKind::Button,
            InputChange::Axis { .. } => ActionKind::Axis,
        }
    }
}

/// A recorded run: the inputs, the seed they were played against, and the expected state hashes.
#[derive(Debug, Clone, PartialEq)]
pub struct Recording {
    /// The RNG seed the run started from. A replay must use the same one.
    pub seed: u64,
    /// How many ticks the recording covers.
    pub ticks: u64,
    /// Changes, kept sorted canonically by [`Recording::sort`].
    changes: Vec<(Tick, InputChange)>,
    /// Expected world state hash at particular ticks.
    checkpoints: BTreeMap<Tick, u64>,
}

impl Recording {
    /// Creates an empty recording for a given seed.
    #[must_use]
    pub fn new(seed: u64) -> Self {
        Self {
            seed,
            ticks: 0,
            changes: Vec::new(),
            checkpoints: BTreeMap::new(),
        }
    }

    /// Records a button change at a tick.
    pub fn push_button(&mut self, tick: Tick, action: &str, pressed: bool) {
        self.changes.push((
            tick,
            InputChange::Button {
                action: action.to_string(),
                pressed,
            },
        ));
        self.ticks = self.ticks.max(tick.0 + 1);
    }

    /// Records an axis change at a tick.
    pub fn push_axis(&mut self, tick: Tick, action: &str, value: f32) {
        self.changes.push((
            tick,
            InputChange::Axis {
                action: action.to_string(),
                value,
            },
        ));
        self.ticks = self.ticks.max(tick.0 + 1);
    }

    /// Records the expected state hash at a tick.
    pub fn push_checkpoint(&mut self, tick: Tick, state_hash: u64) {
        self.checkpoints.insert(tick, state_hash);
        self.ticks = self.ticks.max(tick.0);
    }

    /// The changes that apply at a given tick.
    #[must_use]
    pub fn changes_at(&self, tick: Tick) -> Vec<&InputChange> {
        self.changes
            .iter()
            .filter(|(at, _)| *at == tick)
            .map(|(_, change)| change)
            .collect()
    }

    /// The expected state hash at a tick, if one was recorded.
    #[must_use]
    pub fn checkpoint_at(&self, tick: Tick) -> Option<u64> {
        self.checkpoints.get(&tick).copied()
    }

    /// Every checkpoint, in tick order.
    pub fn checkpoints(&self) -> impl Iterator<Item = (Tick, u64)> {
        self.checkpoints.iter().map(|(tick, hash)| (*tick, *hash))
    }

    /// How many changes are recorded.
    #[must_use]
    pub fn change_count(&self) -> usize {
        self.changes.len()
    }

    /// Sorts changes into canonical order.
    ///
    /// Ordered by tick, then kind, then action name. Two recordings holding the same changes must
    /// serialise identically no matter what order they were pushed in — that is invariant I2, and it
    /// is what keeps a re-recorded replay's diff limited to what actually changed.
    fn sort(&mut self) {
        self.changes
            .sort_by(|(left_tick, left), (right_tick, right)| {
                left_tick
                    .cmp(right_tick)
                    .then_with(|| left.kind().cmp(&right.kind()))
                    .then_with(|| left.action_name().cmp(right.action_name()))
            });
    }

    /// Renders this recording in the canonical text format.
    ///
    /// Always emits LF line endings, never CRLF, so a file written on Windows and one written on
    /// Linux are byte-identical.
    #[must_use]
    pub fn to_text(&self) -> String {
        let mut sorted = self.clone();
        sorted.sort();

        let mut out = String::new();
        // `write!` to a String cannot fail, so the results are deliberately discarded.
        let _ = writeln!(out, "amadeo-replay {FORMAT_VERSION}");
        let _ = writeln!(out, "tick-rate {TICK_RATE_HZ}");
        let _ = writeln!(out, "seed {}", sorted.seed);
        let _ = writeln!(out, "ticks {}", sorted.ticks);

        if !sorted.changes.is_empty() {
            out.push('\n');
            for (tick, change) in &sorted.changes {
                match change {
                    InputChange::Button { action, pressed } => {
                        let word = if *pressed { "down" } else { "up" };
                        let _ = writeln!(out, "{} button {action} {word}", tick.0);
                    }
                    InputChange::Axis { action, value } => {
                        // `{:?}` on f32 gives the shortest representation that round-trips exactly,
                        // which is what canonical formatting needs.
                        let _ = writeln!(out, "{} axis {action} {value:?}", tick.0);
                    }
                }
            }
        }

        if !sorted.checkpoints.is_empty() {
            out.push('\n');
            for (tick, hash) in &sorted.checkpoints {
                let _ = writeln!(out, "checkpoint {} {hash:016x}", tick.0);
            }
        }

        out
    }

    /// Parses the canonical text format.
    ///
    /// Blank lines and `#` comments are accepted but not preserved — canonical output has a fixed
    /// shape, so a formatted file is the authority on layout.
    pub fn parse(text: &str) -> Result<Self, ReplayError> {
        let mut lines = text.lines().enumerate();

        // Header.
        let (_, first) = lines.next().ok_or_else(|| ReplayError::BadHeader {
            found: String::new(),
        })?;
        let mut header = first.split_whitespace();
        if header.next() != Some("amadeo-replay") {
            return Err(ReplayError::BadHeader {
                found: first.to_string(),
            });
        }
        let version: u32 = header
            .next()
            .and_then(|value| value.parse().ok())
            .ok_or_else(|| ReplayError::BadHeader {
                found: first.to_string(),
            })?;
        if version != FORMAT_VERSION {
            return Err(ReplayError::UnsupportedVersion { found: version });
        }

        let mut tick_rate: Option<u32> = None;
        let mut seed: Option<u64> = None;
        let mut ticks: u64 = 0;
        let mut changes: Vec<(Tick, InputChange)> = Vec::new();
        let mut checkpoints: BTreeMap<Tick, u64> = BTreeMap::new();

        for (index, raw) in lines {
            let line = index + 1;
            let content = raw.split('#').next().unwrap_or("").trim();
            if content.is_empty() {
                continue;
            }
            let mut parts = content.split_whitespace();
            let first = parts.next().unwrap_or_default();

            match first {
                "tick-rate" => {
                    tick_rate = Some(parse_field(parts.next(), line, "a tick rate", content)?);
                }
                "seed" => {
                    seed = Some(parse_field(parts.next(), line, "a seed", content)?);
                }
                "ticks" => {
                    ticks = parse_field(parts.next(), line, "a tick count", content)?;
                }
                "checkpoint" => {
                    let tick: u64 = parse_field(parts.next(), line, "a tick number", content)?;
                    let hex = parts.next().ok_or_else(|| ReplayError::Malformed {
                        line,
                        reason: "expected a 16-digit hex state hash".to_string(),
                        content: content.to_string(),
                    })?;
                    let hash =
                        u64::from_str_radix(hex, 16).map_err(|_| ReplayError::Malformed {
                            line,
                            reason: "expected a 16-digit hex state hash".to_string(),
                            content: content.to_string(),
                        })?;
                    checkpoints.insert(Tick(tick), hash);
                    ticks = ticks.max(tick);
                }
                _ => {
                    // An input change: <tick> <kind> <action> <value>
                    let tick: u64 = first.parse().map_err(|_| ReplayError::Malformed {
                        line,
                        reason: "expected a tick number, a 'checkpoint' line, or a header field"
                            .to_string(),
                        content: content.to_string(),
                    })?;
                    let kind = parts.next().unwrap_or_default();
                    let action = parts.next().ok_or_else(|| ReplayError::Malformed {
                        line,
                        reason: "expected an action name".to_string(),
                        content: content.to_string(),
                    })?;
                    let value = parts.next().ok_or_else(|| ReplayError::Malformed {
                        line,
                        reason: "expected a value after the action name".to_string(),
                        content: content.to_string(),
                    })?;

                    let change = match kind {
                        "button" => {
                            let pressed = match value {
                                "down" => true,
                                "up" => false,
                                _ => {
                                    return Err(ReplayError::Malformed {
                                        line,
                                        reason: "a button value must be 'down' or 'up'".to_string(),
                                        content: content.to_string(),
                                    });
                                }
                            };
                            InputChange::Button {
                                action: action.to_string(),
                                pressed,
                            }
                        }
                        "axis" => {
                            let parsed: f32 =
                                value.parse().map_err(|_| ReplayError::Malformed {
                                    line,
                                    reason: "an axis value must be a number".to_string(),
                                    content: content.to_string(),
                                })?;
                            if !parsed.is_finite() {
                                return Err(ReplayError::Malformed {
                                    line,
                                    reason: "an axis value must be finite".to_string(),
                                    content: content.to_string(),
                                });
                            }
                            InputChange::Axis {
                                action: action.to_string(),
                                value: parsed,
                            }
                        }
                        other => {
                            return Err(ReplayError::Malformed {
                                line,
                                reason: format!("expected 'button' or 'axis', found '{other}'"),
                                content: content.to_string(),
                            });
                        }
                    };

                    ticks = ticks.max(tick + 1);
                    changes.push((Tick(tick), change));
                }
            }
        }

        let tick_rate = tick_rate.ok_or(ReplayError::MissingField { field: "tick-rate" })?;
        if tick_rate != TICK_RATE_HZ {
            // Refused rather than replayed wrong. ADR 0007 makes the tick rate part of a replay's
            // meaning, so a mismatch cannot be silently tolerated.
            return Err(ReplayError::TickRateMismatch {
                found: tick_rate,
                expected: TICK_RATE_HZ,
            });
        }

        let mut recording = Self {
            seed: seed.ok_or(ReplayError::MissingField { field: "seed" })?,
            ticks,
            changes,
            checkpoints,
        };
        recording.sort();
        Ok(recording)
    }
}

/// Parses one whitespace-separated field, turning any failure into a located error.
fn parse_field<T: std::str::FromStr>(
    value: Option<&str>,
    line: usize,
    expected: &str,
    content: &str,
) -> Result<T, ReplayError> {
    value
        .and_then(|text| text.parse().ok())
        .ok_or_else(|| ReplayError::Malformed {
            line,
            reason: format!("expected {expected}"),
            content: content.to_string(),
        })
}

#[cfg(test)]
mod tests {
    use super::*;

    fn sample() -> Recording {
        let mut recording = Recording::new(1234);
        recording.push_axis(Tick(0), "move_x", 1.0);
        recording.push_button(Tick(12), "jump", true);
        recording.push_button(Tick(15), "jump", false);
        recording.push_checkpoint(Tick(100), 0xc3e3_fe2b_8ec1_d932);
        recording
    }

    #[test]
    fn renders_the_documented_format() {
        let text = sample().to_text();
        let expected = "amadeo-replay 1\n\
                        tick-rate 60\n\
                        seed 1234\n\
                        ticks 100\n\
                        \n\
                        0 axis move_x 1.0\n\
                        12 button jump down\n\
                        15 button jump up\n\
                        \n\
                        checkpoint 100 c3e3fe2b8ec1d932\n";
        assert_eq!(text, expected);
    }

    #[test]
    fn round_trips_byte_identically() {
        // Invariant I2. If this fails, editor saves and hand edits would churn diffs.
        let original = sample();
        let text = original.to_text();
        let reparsed = Recording::parse(&text).expect("valid");
        assert_eq!(reparsed.to_text(), text);
        assert_eq!(reparsed, original);
    }

    #[test]
    fn output_is_independent_of_push_order() {
        // Canonical ordering: the same content pushed in a different order must serialise the same.
        let mut forward = Recording::new(7);
        forward.push_button(Tick(5), "b", true);
        forward.push_button(Tick(5), "a", true);
        forward.push_axis(Tick(5), "z", 0.5);

        let mut backward = Recording::new(7);
        backward.push_axis(Tick(5), "z", 0.5);
        backward.push_button(Tick(5), "a", true);
        backward.push_button(Tick(5), "b", true);

        assert_eq!(forward.to_text(), backward.to_text());
    }

    #[test]
    fn accepts_comments_and_blank_lines() {
        let text = "amadeo-replay 1\n\
                    # a comment\n\
                    tick-rate 60\n\
                    \n\
                    seed 9\n\
                    ticks 20\n\
                    3 button fire down   # trailing comment\n";
        let recording = Recording::parse(text).expect("valid");
        assert_eq!(recording.seed, 9);
        assert_eq!(recording.change_count(), 1);
        assert_eq!(recording.changes_at(Tick(3)).len(), 1);
    }

    #[test]
    fn hand_written_files_parse() {
        // Invariant I1: a human or an agent must be able to author one of these directly.
        let text = "amadeo-replay 1\ntick-rate 60\nseed 0\nticks 2\n1 axis throttle -0.5\n";
        let recording = Recording::parse(text).expect("valid");
        match recording.changes_at(Tick(1))[0] {
            InputChange::Axis { action, value } => {
                assert_eq!(action, "throttle");
                assert!((value + 0.5).abs() < 1e-6);
            }
            other => panic!("expected an axis change, got {other:?}"),
        }
    }

    #[test]
    fn rejects_a_wrong_tick_rate() {
        // ADR 0007: the tick rate is part of what a replay means.
        let text = "amadeo-replay 1\ntick-rate 30\nseed 0\nticks 1\n";
        assert_eq!(
            Recording::parse(text),
            Err(ReplayError::TickRateMismatch {
                found: 30,
                expected: 60,
            })
        );
    }

    #[test]
    fn rejects_a_bad_header() {
        assert!(matches!(
            Recording::parse("something else\n"),
            Err(ReplayError::BadHeader { .. })
        ));
        assert!(matches!(
            Recording::parse(""),
            Err(ReplayError::BadHeader { .. })
        ));
    }

    #[test]
    fn rejects_a_future_version() {
        let text = "amadeo-replay 99\ntick-rate 60\nseed 0\nticks 0\n";
        assert_eq!(
            Recording::parse(text),
            Err(ReplayError::UnsupportedVersion { found: 99 })
        );
    }

    #[test]
    fn reports_missing_required_fields() {
        let text = "amadeo-replay 1\nseed 0\nticks 0\n";
        assert_eq!(
            Recording::parse(text),
            Err(ReplayError::MissingField { field: "tick-rate" })
        );
    }

    #[test]
    fn malformed_lines_report_their_line_number() {
        let text = "amadeo-replay 1\ntick-rate 60\nseed 0\nticks 1\n0 button jump sideways\n";
        let error = Recording::parse(text).expect_err("should fail");
        match &error {
            ReplayError::Malformed { line, .. } => assert_eq!(*line, 5),
            other => panic!("expected a malformed-line error, got {other:?}"),
        }
        // The message has to be actionable on its own -- an agent cannot ask a follow-up question.
        let text = error.to_string();
        assert!(text.contains("line 5"), "{text}");
        assert!(text.contains("'down' or 'up'"), "{text}");
    }

    #[test]
    fn rejects_non_finite_axis_values() {
        let text = "amadeo-replay 1\ntick-rate 60\nseed 0\nticks 1\n0 axis x NaN\n";
        assert!(matches!(
            Recording::parse(text),
            Err(ReplayError::Malformed { .. })
        ));
    }

    #[test]
    fn rejects_an_unknown_change_kind() {
        let text = "amadeo-replay 1\ntick-rate 60\nseed 0\nticks 1\n0 wiggle x 1.0\n";
        let error = Recording::parse(text).expect_err("should fail");
        assert!(error.to_string().contains("wiggle"), "{error}");
    }

    #[test]
    fn checkpoints_are_readable_by_tick() {
        let recording = sample();
        assert_eq!(
            recording.checkpoint_at(Tick(100)),
            Some(0xc3e3_fe2b_8ec1_d932)
        );
        assert_eq!(recording.checkpoint_at(Tick(50)), None);
        assert_eq!(recording.checkpoints().count(), 1);
    }

    #[test]
    fn tick_count_tracks_the_furthest_entry() {
        let mut recording = Recording::new(0);
        assert_eq!(recording.ticks, 0);
        recording.push_button(Tick(9), "a", true);
        assert_eq!(recording.ticks, 10, "a change at tick 9 means 10 ticks ran");
        recording.push_checkpoint(Tick(50), 1);
        assert_eq!(recording.ticks, 50);
    }

    #[test]
    fn empty_recording_still_round_trips() {
        let recording = Recording::new(42);
        let text = recording.to_text();
        assert_eq!(Recording::parse(&text).expect("valid").to_text(), text);
    }
}
