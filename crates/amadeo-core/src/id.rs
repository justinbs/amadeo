//! Identity types.
//!
//! Amadeo deliberately has **three** separate notions of "which entity is this", because they have
//! genuinely different lifetimes and requirements. Conflating them is a known failure mode in other
//! engines (ADR 0003, ADR 0006), so they are distinct types that cannot be mixed up by accident.
//!
//! | Type | Scope | Stable across | Appears in |
//! |---|---|---|---|
//! | `Entity` (in `amadeo-ecs`) | one running process | nothing — it is a slot handle | memory only |
//! | [`StableId`] | one project, forever | saves, reloads, edits, reordering | scene text files |
//! | [`NetId`] | one multiplayer session | all peers in that session | network packets |
//!
//! An `Entity` is a runtime handle and may be reused after despawn. A [`StableId`] is authoring
//! identity: it is written into scene files and must survive reordering and reparenting so that
//! diffs stay minimal. A [`NetId`] is a shared identity across processes, which neither of the
//! others can serve because one is process-local and the other only exists for authored entities
//! (not for things spawned at runtime).

/// Authoring identity: persists in scene files and survives edits.
///
/// Assigned once when an entity is first authored and never changed afterwards — not on reorder,
/// not on reparent, not on save. This is what makes scene file diffs proportional to the actual
/// change rather than to the file size (invariant I2).
///
/// # Format
///
/// Currently an opaque 64-bit value rendered as hex. The final textual form is decided together
/// with the scene format in M1 (open question Q2), so treat the `Display` output as provisional.
/// The *type* and its guarantees are settled; only the spelling is open.
#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Hash)]
pub struct StableId(pub u64);

impl StableId {
    /// A reserved value meaning "no stable identity", used for entities spawned at runtime that
    /// were never authored in a scene file.
    pub const NONE: StableId = StableId(0);

    /// Whether this is a real authored identity rather than [`StableId::NONE`].
    #[must_use]
    pub fn is_some(self) -> bool {
        self.0 != 0
    }
}

impl std::fmt::Display for StableId {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        if self.0 == 0 {
            write!(f, "none")
        } else {
            write!(f, "{:016x}", self.0)
        }
    }
}

/// Network identity: shared by all peers within one multiplayer session.
///
/// # Why this exists before there is any networking
///
/// ADR 0006 reserves the multiplayer hooks during M0–M2 rather than building netcode. This type is
/// one of those hooks. Networking is the most painful retrofit in engine development precisely
/// because identity is threaded through everything; introducing the type now costs almost nothing,
/// while introducing it later means revisiting every system that refers to an entity.
///
/// Until M6 every entity's `NetId` is [`NetId::LOCAL`]. That is expected, not a placeholder bug.
#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Hash)]
pub struct NetId(pub u64);

impl NetId {
    /// The identity used in single-player, where there are no peers to agree with.
    pub const LOCAL: NetId = NetId(0);

    /// Whether this entity is replicated across the network.
    #[must_use]
    pub fn is_networked(self) -> bool {
        self.0 != 0
    }
}

impl std::fmt::Display for NetId {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        if self.0 == 0 {
            write!(f, "local")
        } else {
            write!(f, "net:{}", self.0)
        }
    }
}

/// Who is allowed to mutate an entity.
///
/// Also an ADR 0006 hook. In single-player everything is [`Authority::Local`]. Systems written
/// against this from the start stay correct when networking arrives; systems that assume universal
/// write access all need revisiting, which is the retrofit we are avoiding.
#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Hash, Default)]
pub enum Authority {
    /// This process owns the entity and may mutate it freely. The only variant used before M6.
    #[default]
    Local,
    /// The server owns it; this process may predict but the server's value wins on conflict.
    Remote,
}

impl Authority {
    /// Whether this process may write to the entity without reconciliation.
    #[must_use]
    pub fn can_write(self) -> bool {
        matches!(self, Authority::Local)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn stable_id_none_is_distinguishable() {
        assert!(!StableId::NONE.is_some());
        assert!(StableId(1).is_some());
        assert_eq!(StableId::NONE.to_string(), "none");
    }

    #[test]
    fn stable_id_formats_as_padded_hex() {
        assert_eq!(StableId(0x1234).to_string(), "0000000000001234");
    }

    #[test]
    fn net_id_defaults_to_local() {
        assert!(!NetId::LOCAL.is_networked());
        assert!(NetId(5).is_networked());
        assert_eq!(NetId::LOCAL.to_string(), "local");
    }

    #[test]
    fn authority_defaults_to_local_and_is_writable() {
        assert_eq!(Authority::default(), Authority::Local);
        assert!(Authority::Local.can_write());
        assert!(!Authority::Remote.can_write());
    }
}
