//! The per-tick snapshot of what the player is doing.

use crate::action::ActionId;
use amadeo_core::{StableHash, StableHasher};
use amadeo_ecs::Resource;
use std::collections::BTreeMap;

/// A button's state this tick and last tick.
///
/// Keeping the previous value is what makes "just pressed" derivable without any timing or edge
/// bookkeeping: it is simply `pressed && !was_pressed`.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
struct ButtonState {
    pressed: bool,
    was_pressed: bool,
}

/// An axis's value this tick and last tick.
#[derive(Debug, Clone, Copy, PartialEq, Default)]
struct AxisState {
    value: f32,
    previous: f32,
}

/// What the player is doing, as of the current tick.
///
/// A [`Resource`], so it participates in [`state_hash`](amadeo_ecs::World::state_hash) — two runs
/// receiving different input have diverged, and that must be detectable rather than silent.
///
/// `BTreeMap` throughout: input feeds directly into simulation, so iteration order has to be
/// reproducible (invariant I3).
#[derive(Debug, Clone, Default, PartialEq)]
pub struct InputState {
    buttons: BTreeMap<ActionId, ButtonState>,
    axes: BTreeMap<ActionId, AxisState>,
}

impl StableHash for InputState {
    fn stable_hash(&self, hasher: &mut StableHasher) {
        hasher.write_u64(self.buttons.len() as u64);
        for (action, state) in &self.buttons {
            hasher.write_u64(action.raw());
            hasher.write_bool(state.pressed);
            hasher.write_bool(state.was_pressed);
        }
        hasher.write_u64(self.axes.len() as u64);
        for (action, state) in &self.axes {
            hasher.write_u64(action.raw());
            hasher.write_f32(state.value);
            hasher.write_f32(state.previous);
        }
    }
}

impl Resource for InputState {}

impl InputState {
    /// Creates an empty input state, with every action considered released.
    #[must_use]
    pub fn new() -> Self {
        Self::default()
    }

    /// Whether a button action is currently held.
    #[must_use]
    pub fn pressed(&self, action: ActionId) -> bool {
        self.buttons.get(&action).is_some_and(|state| state.pressed)
    }

    /// Whether a button action went down this tick.
    ///
    /// True for exactly one tick per press, which is what menu selections, jumps, and interactions
    /// want. Using [`InputState::pressed`] for those would fire every tick the key is held.
    #[must_use]
    pub fn just_pressed(&self, action: ActionId) -> bool {
        self.buttons
            .get(&action)
            .is_some_and(|state| state.pressed && !state.was_pressed)
    }

    /// Whether a button action came up this tick.
    #[must_use]
    pub fn just_released(&self, action: ActionId) -> bool {
        self.buttons
            .get(&action)
            .is_some_and(|state| !state.pressed && state.was_pressed)
    }

    /// An axis action's current value, or `0.0` if it has never been set.
    #[must_use]
    pub fn axis(&self, action: ActionId) -> f32 {
        self.axes.get(&action).map_or(0.0, |state| state.value)
    }

    /// How much an axis moved since last tick.
    #[must_use]
    pub fn axis_delta(&self, action: ActionId) -> f32 {
        self.axes
            .get(&action)
            .map_or(0.0, |state| state.value - state.previous)
    }

    /// Sets a button's current value. Called by the input source, not by gameplay.
    pub fn set_button(&mut self, action: ActionId, pressed: bool) {
        self.buttons.entry(action).or_default().pressed = pressed;
    }

    /// Sets an axis's current value. Called by the input source, not by gameplay.
    ///
    /// Non-finite values are ignored rather than stored: a NaN reaching simulation would poison
    /// every value it touches and make the state hash meaningless.
    pub fn set_axis(&mut self, action: ActionId, value: f32) {
        if value.is_finite() {
            self.axes.entry(action).or_default().value = value;
        }
    }

    /// Rolls current values into previous ones, ready for this tick's changes to be applied.
    ///
    /// Must run exactly once per tick, before any changes are applied. Running it twice would lose a
    /// press that lasted a single tick; skipping it would make `just_pressed` stay true.
    pub fn begin_tick(&mut self) {
        for state in self.buttons.values_mut() {
            state.was_pressed = state.pressed;
        }
        for state in self.axes.values_mut() {
            state.previous = state.value;
        }
    }

    /// Every button action currently known, with its pressed state.
    ///
    /// Ordered by action id. Used by the recorder and by the agent introspection layer.
    pub fn buttons(&self) -> impl Iterator<Item = (ActionId, bool)> {
        self.buttons
            .iter()
            .map(|(action, state)| (*action, state.pressed))
    }

    /// Every axis action currently known, with its value. Ordered by action id.
    pub fn axes(&self) -> impl Iterator<Item = (ActionId, f32)> {
        self.axes
            .iter()
            .map(|(action, state)| (*action, state.value))
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn jump() -> ActionId {
        ActionId::new("jump")
    }

    fn move_x() -> ActionId {
        ActionId::new("move_x")
    }

    #[test]
    fn unknown_actions_read_as_neutral() {
        let state = InputState::new();
        assert!(!state.pressed(jump()));
        assert!(!state.just_pressed(jump()));
        assert!(!state.just_released(jump()));
        assert_eq!(state.axis(move_x()), 0.0);
        assert_eq!(state.axis_delta(move_x()), 0.0);
    }

    #[test]
    fn just_pressed_is_true_for_exactly_one_tick() {
        let mut state = InputState::new();

        state.begin_tick();
        state.set_button(jump(), true);
        assert!(state.pressed(jump()));
        assert!(state.just_pressed(jump()));

        // Held into the next tick: still pressed, no longer *just* pressed.
        state.begin_tick();
        assert!(state.pressed(jump()));
        assert!(!state.just_pressed(jump()));
    }

    #[test]
    fn just_released_is_true_for_exactly_one_tick() {
        let mut state = InputState::new();
        state.begin_tick();
        state.set_button(jump(), true);

        state.begin_tick();
        state.set_button(jump(), false);
        assert!(!state.pressed(jump()));
        assert!(state.just_released(jump()));

        state.begin_tick();
        assert!(!state.just_released(jump()));
    }

    #[test]
    fn a_single_tick_press_is_not_lost() {
        // Press and release within one tick still registers as a press that tick.
        let mut state = InputState::new();
        state.begin_tick();
        state.set_button(jump(), true);
        assert!(state.just_pressed(jump()));
    }

    #[test]
    fn axis_tracks_value_and_delta() {
        let mut state = InputState::new();

        state.begin_tick();
        state.set_axis(move_x(), 0.5);
        assert_eq!(state.axis(move_x()), 0.5);
        assert_eq!(state.axis_delta(move_x()), 0.5);

        state.begin_tick();
        state.set_axis(move_x(), 0.75);
        assert_eq!(state.axis(move_x()), 0.75);
        assert!((state.axis_delta(move_x()) - 0.25).abs() < 1e-6);

        // Unchanged axis has zero delta after the roll.
        state.begin_tick();
        assert_eq!(state.axis_delta(move_x()), 0.0);
    }

    #[test]
    fn non_finite_axis_values_are_rejected() {
        // A NaN reaching simulation would spread through every value it touches and make the state
        // hash meaningless, so it is refused at the boundary.
        let mut state = InputState::new();
        state.set_axis(move_x(), 1.0);
        state.set_axis(move_x(), f32::NAN);
        assert_eq!(state.axis(move_x()), 1.0);

        state.set_axis(move_x(), f32::INFINITY);
        assert_eq!(state.axis(move_x()), 1.0);
    }

    #[test]
    fn iteration_is_ordered_by_action_id() {
        let mut state = InputState::new();
        // Inserted in arbitrary order.
        for name in ["zeta", "alpha", "mu"] {
            state.set_button(ActionId::new(name), true);
        }

        let ids: Vec<u64> = state.buttons().map(|(action, _)| action.raw()).collect();
        let mut sorted = ids.clone();
        sorted.sort_unstable();
        assert_eq!(ids, sorted, "iteration must be ordered for determinism");
    }

    #[test]
    fn identical_input_hashes_identically() {
        use amadeo_core::hash::stable_hash_of;

        let mut first = InputState::new();
        let mut second = InputState::new();
        for state in [&mut first, &mut second] {
            state.begin_tick();
            state.set_button(jump(), true);
            state.set_axis(move_x(), -0.25);
        }
        assert_eq!(stable_hash_of(&first), stable_hash_of(&second));

        second.set_axis(move_x(), 0.25);
        assert_ne!(stable_hash_of(&first), stable_hash_of(&second));
    }
}
