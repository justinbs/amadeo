//! Where input comes from, and how it gets recorded.
//!
//! The simulation cannot tell a live player from a replay. That is the entire point: a recorded run
//! plays back through exactly the same code path that produced it, so a replay is a real execution
//! rather than a simulation of one.

use crate::action::ActionId;
use crate::recording::{InputChange, Recording};
use crate::state::InputState;
use amadeo_core::Tick;
use amadeo_ecs::Service;
use std::collections::BTreeMap;
use std::fmt;

/// Something that supplies input for a tick.
///
/// Implementations must be **pure with respect to the tick**: asked for tick 42 twice, they must
/// apply the same changes both times. A source that consults a real clock or a live device without
/// recording would break replay determinism.
pub trait InputSource: fmt::Debug + Send + Sync {
    /// Applies this tick's changes to the input state.
    fn apply(&mut self, tick: Tick, state: &mut InputState);

    /// Mutable upcast, so a windowing layer can reach its own concrete source.
    ///
    /// Needed because the driver stores the source as a trait object, but a live source has to be
    /// fed by whatever is receiving OS events — and that code knows the concrete type.
    fn as_any_mut(&mut self) -> &mut dyn std::any::Any;
}

/// An input source that produces nothing. Used for headless runs with no player.
#[derive(Debug, Default, Clone, Copy)]
pub struct NullSource;

impl InputSource for NullSource {
    fn apply(&mut self, _tick: Tick, _state: &mut InputState) {}

    fn as_any_mut(&mut self) -> &mut dyn std::any::Any {
        self
    }
}

/// Plays back a [`Recording`].
#[derive(Debug, Clone)]
pub struct ReplaySource {
    recording: Recording,
}

impl ReplaySource {
    /// Creates a source that plays a recording.
    #[must_use]
    pub fn new(recording: Recording) -> Self {
        Self { recording }
    }

    /// The recording being played.
    #[must_use]
    pub fn recording(&self) -> &Recording {
        &self.recording
    }

    /// The expected state hash at a tick, if the recording has one there.
    #[must_use]
    pub fn checkpoint_at(&self, tick: Tick) -> Option<u64> {
        self.recording.checkpoint_at(tick)
    }
}

impl InputSource for ReplaySource {
    fn apply(&mut self, tick: Tick, state: &mut InputState) {
        for change in self.recording.changes_at(tick) {
            apply_change(change, state);
        }
    }

    fn as_any_mut(&mut self) -> &mut dyn std::any::Any {
        self
    }
}

/// An input source driven by explicitly queued changes.
///
/// Used by tests and by the agent introspection layer's `input.inject`, which needs to synthesise a
/// player's actions at an exact tick.
#[derive(Debug, Default, Clone)]
pub struct ScriptedSource {
    queued: BTreeMap<Tick, Vec<InputChange>>,
}

impl ScriptedSource {
    /// Creates an empty scripted source.
    #[must_use]
    pub fn new() -> Self {
        Self::default()
    }

    /// Queues a button change for a tick.
    pub fn press(&mut self, tick: Tick, action: &str, pressed: bool) -> &mut Self {
        self.queued
            .entry(tick)
            .or_default()
            .push(InputChange::Button {
                action: action.to_string(),
                pressed,
            });
        self
    }

    /// Queues an axis change for a tick.
    pub fn axis(&mut self, tick: Tick, action: &str, value: f32) -> &mut Self {
        self.queued
            .entry(tick)
            .or_default()
            .push(InputChange::Axis {
                action: action.to_string(),
                value,
            });
        self
    }
}

impl InputSource for ScriptedSource {
    fn apply(&mut self, tick: Tick, state: &mut InputState) {
        if let Some(changes) = self.queued.get(&tick) {
            for change in changes {
                apply_change(change, state);
            }
        }
    }

    fn as_any_mut(&mut self) -> &mut dyn std::any::Any {
        self
    }
}

/// An input source fed by real devices.
///
/// # How this stays replay-safe
///
/// Whatever is receiving OS events writes the *current* value of each action here as it happens.
/// Once per tick the loop copies those values into [`InputState`], and a [`Recorder`] writes down
/// what changed.
///
/// The trait requires sources to be pure with respect to the tick, and this one satisfies that in
/// the way that matters: within a single tick it holds a fixed snapshot, so applying it twice for
/// the same tick produces the same result. What it cannot do is reproduce a *past* tick — that is
/// exactly what recording is for, and why a live session and its replay use different sources.
///
/// Deliberately knows nothing about keyboards, gamepads, or windowing. It stores action names and
/// values, so the platform layer owns the key-to-action mapping and this crate keeps no dependency
/// on any windowing library.
#[derive(Debug, Default, Clone)]
pub struct LiveSource {
    buttons: BTreeMap<ActionId, bool>,
    axes: BTreeMap<ActionId, f32>,
}

impl LiveSource {
    /// Creates a source with every action at rest.
    #[must_use]
    pub fn new() -> Self {
        Self::default()
    }

    /// Sets a button's current physical state.
    pub fn set_button(&mut self, action: &str, pressed: bool) {
        self.buttons.insert(ActionId::new(action), pressed);
    }

    /// Sets an axis's current value.
    pub fn set_axis(&mut self, action: &str, value: f32) {
        self.axes.insert(ActionId::new(action), value);
    }

    /// Sets an axis from a pair of opposing keys, the common keyboard case.
    ///
    /// Both held cancels to zero, which is the behaviour players expect from opposed movement keys.
    pub fn set_axis_from_keys(&mut self, action: &str, negative: bool, positive: bool) {
        let value = f32::from(positive) - f32::from(negative);
        self.set_axis(action, value);
    }
}

impl InputSource for LiveSource {
    fn apply(&mut self, _tick: Tick, state: &mut InputState) {
        for (action, pressed) in &self.buttons {
            state.set_button(*action, *pressed);
        }
        for (action, value) in &self.axes {
            state.set_axis(*action, *value);
        }
    }

    fn as_any_mut(&mut self) -> &mut dyn std::any::Any {
        self
    }
}

/// Applies one change to the input state.
fn apply_change(change: &InputChange, state: &mut InputState) {
    match change {
        InputChange::Button { pressed, .. } => state.set_button(change.action_id(), *pressed),
        InputChange::Axis { value, .. } => state.set_axis(change.action_id(), *value),
    }
}

/// Watches the input state each tick and writes down what changed.
///
/// Records **changes only**, by diffing against the previous tick. A player holding a key for three
/// seconds produces two lines, not a hundred and eighty.
#[derive(Debug, Clone)]
pub struct Recorder {
    recording: Recording,
    previous_buttons: BTreeMap<ActionId, bool>,
    previous_axes: BTreeMap<ActionId, f32>,
    /// Maps ids back to names, since the file format is written in readable action names but
    /// [`InputState`] only knows hashed ids.
    names: BTreeMap<ActionId, String>,
}

impl Recorder {
    /// Creates a recorder for a run started from `seed`.
    #[must_use]
    pub fn new(seed: u64) -> Self {
        Self {
            recording: Recording::new(seed),
            previous_buttons: BTreeMap::new(),
            previous_axes: BTreeMap::new(),
            names: BTreeMap::new(),
        }
    }

    /// Teaches the recorder an action's name.
    ///
    /// Required before that action can appear in the output, because ids are one-way hashes. An
    /// unregistered action is skipped rather than written under an unreadable id — a replay file
    /// full of hex would defeat the point of a text format.
    pub fn register_action(&mut self, name: &str) -> &mut Self {
        self.names.insert(ActionId::new(name), name.to_string());
        self
    }

    /// Records anything that changed since last tick.
    pub fn observe(&mut self, tick: Tick, state: &InputState) {
        for (action, pressed) in state.buttons() {
            let changed = self.previous_buttons.get(&action) != Some(&pressed);
            if changed {
                if let Some(name) = self.names.get(&action) {
                    self.recording.push_button(tick, name, pressed);
                }
                self.previous_buttons.insert(action, pressed);
            }
        }

        for (action, value) in state.axes() {
            // Exact comparison is correct here: an axis that was not written this tick holds the
            // identical bit pattern, and a tolerance would silently drop small deliberate movements.
            let changed = self.previous_axes.get(&action) != Some(&value);
            if changed {
                if let Some(name) = self.names.get(&action) {
                    self.recording.push_axis(tick, name, value);
                }
                self.previous_axes.insert(action, value);
            }
        }
    }

    /// Records the expected state hash at a tick.
    pub fn checkpoint(&mut self, tick: Tick, state_hash: u64) {
        self.recording.push_checkpoint(tick, state_hash);
    }

    /// The recording built so far.
    #[must_use]
    pub fn recording(&self) -> &Recording {
        &self.recording
    }

    /// Consumes the recorder and returns its recording.
    #[must_use]
    pub fn into_recording(self) -> Recording {
        self.recording
    }
}

/// Holds the active input source and optional recorder.
///
/// A [`Service`], not a resource: the *input* it produces is simulation state, but the machinery
/// producing it is not. Recording a run must not change the run's state hash, or a recorded playthrough
/// would not match the same playthrough unrecorded.
#[derive(Debug)]
pub struct InputDriver {
    /// Where input comes from this run.
    pub source: Box<dyn InputSource>,
    /// Set when this run is being recorded.
    pub recorder: Option<Recorder>,
}

impl Service for InputDriver {}

impl InputDriver {
    /// Creates a driver reading from `source`, not recording.
    #[must_use]
    pub fn new(source: Box<dyn InputSource>) -> Self {
        Self {
            source,
            recorder: None,
        }
    }

    /// Creates a driver that produces no input. The default for headless runs.
    #[must_use]
    pub fn null() -> Self {
        Self::new(Box::new(NullSource))
    }

    /// Creates a driver that plays a recording back.
    #[must_use]
    pub fn replaying(recording: Recording) -> Self {
        Self::new(Box::new(ReplaySource::new(recording)))
    }

    /// Attaches a recorder, so this run is written down as it happens.
    #[must_use]
    pub fn recording_with(mut self, recorder: Recorder) -> Self {
        self.recorder = Some(recorder);
        self
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn jump() -> ActionId {
        ActionId::new("jump")
    }

    #[test]
    fn null_source_produces_nothing() {
        let mut source = NullSource;
        let mut state = InputState::new();
        source.apply(Tick(5), &mut state);
        assert!(!state.pressed(jump()));
    }

    #[test]
    fn scripted_source_applies_at_the_right_tick() {
        let mut source = ScriptedSource::new();
        source.press(Tick(3), "jump", true);
        source.axis(Tick(3), "move_x", -1.0);

        let mut state = InputState::new();
        source.apply(Tick(2), &mut state);
        assert!(!state.pressed(jump()), "must not fire early");

        source.apply(Tick(3), &mut state);
        assert!(state.pressed(jump()));
        assert_eq!(state.axis(ActionId::new("move_x")), -1.0);
    }

    #[test]
    fn replay_source_reproduces_a_recording() {
        let mut recording = Recording::new(0);
        recording.push_button(Tick(1), "jump", true);
        recording.push_button(Tick(4), "jump", false);

        let mut source = ReplaySource::new(recording);
        let mut state = InputState::new();

        state.begin_tick();
        source.apply(Tick(1), &mut state);
        assert!(state.pressed(jump()));

        state.begin_tick();
        source.apply(Tick(2), &mut state);
        assert!(state.pressed(jump()), "no change means still held");

        state.begin_tick();
        source.apply(Tick(4), &mut state);
        assert!(!state.pressed(jump()));
        assert!(state.just_released(jump()));
    }

    #[test]
    fn replay_source_is_pure_with_respect_to_tick() {
        // Asked for the same tick twice, a source must do the same thing -- otherwise a replay could
        // not be re-run, and rollback would be impossible later.
        let mut recording = Recording::new(0);
        recording.push_axis(Tick(2), "throttle", 0.5);
        let mut source = ReplaySource::new(recording);

        let mut first = InputState::new();
        source.apply(Tick(2), &mut first);
        let mut second = InputState::new();
        source.apply(Tick(2), &mut second);

        assert_eq!(first.axis(ActionId::new("throttle")), 0.5);
        assert_eq!(second.axis(ActionId::new("throttle")), 0.5);
    }

    #[test]
    fn recorder_writes_only_changes() {
        let mut recorder = Recorder::new(0);
        recorder.register_action("jump");

        let mut state = InputState::new();

        // Held for five ticks: two lines, not five.
        for tick in 0..5u64 {
            state.begin_tick();
            state.set_button(jump(), tick < 3);
            recorder.observe(Tick(tick), &state);
        }

        assert_eq!(
            recorder.recording().change_count(),
            2,
            "expected one press and one release"
        );
        assert_eq!(recorder.recording().changes_at(Tick(0)).len(), 1);
        assert_eq!(recorder.recording().changes_at(Tick(3)).len(), 1);
    }

    #[test]
    fn recorder_skips_unregistered_actions() {
        // Writing a hashed id into the file would produce something no human could read or edit.
        let mut recorder = Recorder::new(0);
        let mut state = InputState::new();
        state.set_button(jump(), true);
        recorder.observe(Tick(0), &state);
        assert_eq!(recorder.recording().change_count(), 0);
    }

    #[test]
    fn recorded_run_replays_to_the_same_input() {
        // The round trip that matters: record a session, play it back, and get identical input.
        let mut recorder = Recorder::new(0);
        recorder.register_action("jump");
        recorder.register_action("move_x");

        let mut live = InputState::new();
        let mut scripted = ScriptedSource::new();
        scripted.press(Tick(1), "jump", true);
        scripted.press(Tick(3), "jump", false);
        scripted.axis(Tick(0), "move_x", 1.0);
        scripted.axis(Tick(5), "move_x", -1.0);

        let mut live_snapshots = Vec::new();
        for tick in 0..8u64 {
            live.begin_tick();
            scripted.apply(Tick(tick), &mut live);
            recorder.observe(Tick(tick), &live);
            live_snapshots.push(live.clone());
        }

        let mut replay = ReplaySource::new(recorder.into_recording());
        let mut replayed = InputState::new();
        for (tick, expected) in live_snapshots.iter().enumerate() {
            replayed.begin_tick();
            replay.apply(Tick(tick as u64), &mut replayed);
            assert_eq!(&replayed, expected, "diverged at tick {tick}");
        }
    }

    #[test]
    fn driver_defaults_to_no_recording() {
        let driver = InputDriver::null();
        assert!(driver.recorder.is_none());

        let driver = InputDriver::null().recording_with(Recorder::new(3));
        assert!(driver.recorder.is_some());
    }
}
