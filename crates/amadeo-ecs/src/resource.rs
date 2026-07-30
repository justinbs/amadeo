//! Resources: single-instance global state inside a [`World`](crate::World).
//!
//! A component belongs to an entity; a resource is one value for the whole world. The simulation
//! clock's seed, the input state for the current tick, and event queues are all resources.

use crate::type_hash::hash_type_name;
use amadeo_core::{StableHash, StableHasher};
use std::any::Any;
use std::fmt;

/// Global simulation state, one instance per [`World`](crate::World).
///
/// # What belongs here, and what does not
///
/// **Simulation state only.** A resource participates in [`World::state_hash`](crate::World::state_hash),
/// so anything stored here is part of what golden replay tests assert on. That is why [`StableHash`]
/// is required.
///
/// Engine *services* — asset caches, the GPU device, the audio mixer, file handles — must **not** be
/// resources. They are not simulation state, they cannot be meaningfully hashed, and including them
/// would make replay assertions depend on machine configuration. Those live in the app layer instead.
///
/// This split is what keeps `World` hashable in full, which in turn is what makes snapshots and
/// replay verification possible (ADR 0005).
pub trait Resource: 'static + Send + Sync + fmt::Debug + StableHash {}

/// Identifies a resource type.
///
/// Derived by hashing the type's name rather than from `std::any::TypeId`, because `TypeId` values
/// are compiler-generated and carry no stability guarantee across builds — using them would make
/// state hashes disagree between compilations of identical logic (invariant I3).
#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Hash)]
pub struct ResourceId(u64);

impl ResourceId {
    /// The id for resource type `T`.
    #[must_use]
    pub fn of<T: Resource>() -> Self {
        ResourceId(hash_type_name::<T>())
    }

    /// The raw hash value, for diagnostics.
    #[must_use]
    pub fn raw(self) -> u64 {
        self.0
    }
}

impl fmt::Display for ResourceId {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(f, "resource#{:016x}", self.0)
    }
}

/// A type-erased resource value.
///
/// The blanket implementation below means any [`Resource`] can be stored as `Box<dyn ResourceSlot>`
/// without writing per-type glue.
pub(crate) trait ResourceSlot: fmt::Debug + Send + Sync {
    /// Upcast for downcasting back to the concrete type.
    fn as_any(&self) -> &dyn Any;

    /// Mutable upcast for downcasting.
    fn as_any_mut(&mut self) -> &mut dyn Any;

    /// Consuming upcast, so a resource can be moved back out of the world.
    fn into_any(self: Box<Self>) -> Box<dyn Any>;

    /// Feeds this resource into a state fingerprint.
    fn stable_hash_value(&self, hasher: &mut StableHasher);
}

impl<T: Resource> ResourceSlot for T {
    fn as_any(&self) -> &dyn Any {
        self
    }

    fn as_any_mut(&mut self) -> &mut dyn Any {
        self
    }

    fn into_any(self: Box<Self>) -> Box<dyn Any> {
        self
    }

    fn stable_hash_value(&self, hasher: &mut StableHasher) {
        self.stable_hash(hasher);
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[derive(Debug, PartialEq)]
    struct Score(u32);

    impl StableHash for Score {
        fn stable_hash(&self, hasher: &mut StableHasher) {
            self.0.stable_hash(hasher);
        }
    }
    impl Resource for Score {}

    #[derive(Debug)]
    struct Rounds(u32);

    impl StableHash for Rounds {
        fn stable_hash(&self, hasher: &mut StableHasher) {
            self.0.stable_hash(hasher);
        }
    }
    impl Resource for Rounds {}

    #[test]
    fn ids_differ_by_type() {
        assert_ne!(ResourceId::of::<Score>(), ResourceId::of::<Rounds>());
        assert_eq!(ResourceId::of::<Score>(), ResourceId::of::<Score>());
    }

    #[test]
    fn slot_round_trips_through_type_erasure() {
        let boxed: Box<dyn ResourceSlot> = Box::new(Score(7));
        assert_eq!(boxed.as_any().downcast_ref::<Score>(), Some(&Score(7)));

        let recovered = boxed.into_any().downcast::<Score>().expect("same type");
        assert_eq!(*recovered, Score(7));
    }

    #[test]
    fn slot_hashes_its_value() {
        let seven: Box<dyn ResourceSlot> = Box::new(Score(7));
        let eight: Box<dyn ResourceSlot> = Box::new(Score(8));

        let mut a = StableHasher::new();
        seven.stable_hash_value(&mut a);
        let mut b = StableHasher::new();
        eight.stable_hash_value(&mut b);
        assert_ne!(a.finish(), b.finish());
    }
}
