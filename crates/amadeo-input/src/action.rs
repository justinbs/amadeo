//! Actions: the names gameplay code uses instead of physical keys.

use amadeo_core::StableHasher;
use std::fmt;

/// Identifies an input action, such as `"jump"` or `"move_x"`.
///
/// # Why gameplay never reads a key directly
///
/// A system asks "is `jump` pressed", never "is Space pressed". Three things fall out of that
/// indirection, and the third is the one this project cannot do without:
///
/// 1. **Remapping** works without touching gameplay code.
/// 2. **Controllers** map onto the same actions as a keyboard.
/// 3. **Replay determinism.** Actions are the recording boundary. A replay reproduces the *actions*
///    a player took, not the physical events that produced them, so it stays valid across different
///    devices, key bindings, and machines. Recording raw device events instead would tie a replay to
///    one specific keyboard layout.
///
/// Derived by hashing the action's name, so an id is stable across builds and can be written into a
/// replay file as readable text.
#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Hash)]
pub struct ActionId(u64);

impl ActionId {
    /// The id for an action name.
    ///
    /// ```
    /// use amadeo_input::ActionId;
    /// assert_eq!(ActionId::new("jump"), ActionId::new("jump"));
    /// assert_ne!(ActionId::new("jump"), ActionId::new("crouch"));
    /// ```
    #[must_use]
    pub fn new(name: &str) -> Self {
        let mut hasher = StableHasher::new();
        hasher.write_str(name);
        ActionId(hasher.finish())
    }

    /// The raw hash value, for diagnostics.
    #[must_use]
    pub fn raw(self) -> u64 {
        self.0
    }
}

impl amadeo_reflect::ReflectKey for ActionId {
    /// Plain decimal, which is injective because the id *is* a `u64`.
    ///
    /// # Why not the action's name, which is what a reader wants
    ///
    /// Because an `ActionId` does not have one. It is a hash, and the name that produced it is not
    /// kept — deliberately, since that is what makes an id fixed-size, `Copy`, and cheap enough to
    /// look up every tick.
    ///
    /// The result is that a reflected [`InputState`](crate::InputState) has unreadable keys. That is
    /// a real gap and it is recorded as one; the fix is for the protocol layer to join these against
    /// the input driver's name table when rendering, not for this type to start carrying a `String`.
    ///
    /// # Why not [`fmt::Display`], which already renders one
    ///
    /// `Display` produces `action#1a2b3c4d` for a diagnostic, and reusing it here would tie the
    /// on-disk key to how a log line happens to read. Changing a message should not rewrite saved
    /// files.
    fn key_type_name() -> String {
        "action-id".to_string()
    }

    fn to_key(&self) -> String {
        self.0.to_string()
    }

    fn from_key(text: &str) -> Result<Self, amadeo_reflect::ReflectError> {
        text.parse::<u64>()
            .map(ActionId)
            .map_err(|_| amadeo_reflect::ReflectError::TypeMismatch {
                type_name: "ActionId".to_string(),
                expected: "an action id written in decimal, as `amadeo describe` reports it"
                    .to_string(),
                found: format!("`{text}`"),
            })
    }
}

impl fmt::Display for ActionId {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(f, "action#{:016x}", self.0)
    }
}

/// Whether an action is a two-state button or a continuous axis.
#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Hash)]
pub enum ActionKind {
    /// Pressed or not. A key, a mouse button, a gamepad face button.
    Button,
    /// A continuous value, conventionally in `-1.0..=1.0`. A stick axis, a trigger, or a pair of
    /// keys combined into one signed value.
    Axis,
}

impl fmt::Display for ActionKind {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            ActionKind::Button => write!(f, "button"),
            ActionKind::Axis => write!(f, "axis"),
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn ids_are_stable_and_distinct() {
        assert_eq!(ActionId::new("fire"), ActionId::new("fire"));
        assert_ne!(ActionId::new("fire"), ActionId::new("reload"));
    }

    #[test]
    fn ids_are_case_sensitive() {
        // Worth pinning: silently folding case would make two distinct actions collide.
        assert_ne!(ActionId::new("Jump"), ActionId::new("jump"));
    }

    #[test]
    fn empty_name_is_allowed_and_distinct() {
        assert_ne!(ActionId::new(""), ActionId::new("a"));
    }

    #[test]
    fn kinds_render_as_the_replay_file_spells_them() {
        // These strings appear verbatim in replay files, so they are part of the format.
        assert_eq!(ActionKind::Button.to_string(), "button");
        assert_eq!(ActionKind::Axis.to_string(), "axis");
    }
}
