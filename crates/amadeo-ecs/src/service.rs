//! Services: engine state that lives in the world but is **not** simulation state.
//!
//! # Why this exists separately from [`Resource`](crate::Resource)
//!
//! A [`Resource`](crate::Resource) is simulation state: it is fingerprinted by
//! [`World::state_hash`](crate::World::state_hash) and therefore participates in every golden replay
//! assertion. That is correct for a score, an RNG, or an event queue.
//!
//! It is wrong for a GPU device, an asset cache, an audio mixer, a frame-timing counter, or a file
//! handle. Those are engine machinery. They cannot be meaningfully hashed, and including them would
//! make replay assertions depend on machine configuration and on whether rendering happened at all —
//! which would break invariant I7, since a headless run and a windowed run must agree exactly.
//!
//! So there are two stores with two traits, and the split is enforced by the type system rather than
//! by discipline: [`Resource`](crate::Resource) requires
//! [`StableHash`](amadeo_core::StableHash), and `Service` deliberately does not. A GPU device
//! *cannot* be filed as a resource, because it cannot implement the required trait.
//!
//! Both are reachable from a system through `&mut World`, so this costs nothing in ergonomics.

use crate::type_hash::hash_type_name;
use std::any::Any;
use std::fmt;

/// Engine state that is not part of the simulation and is excluded from state hashes.
///
/// If a value should influence replay assertions, it is a [`Resource`](crate::Resource), not a
/// service. If it is machinery — caches, devices, counters, handles — it belongs here.
pub trait Service: 'static + Send + Sync + fmt::Debug {}

/// Identifies a service type. Derived from the type name, like every other id in the engine.
#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Hash)]
pub struct ServiceId(u64);

impl ServiceId {
    /// The id for service type `T`.
    #[must_use]
    pub fn of<T: Service>() -> Self {
        ServiceId(hash_type_name::<T>())
    }

    /// The raw hash value, for diagnostics.
    #[must_use]
    pub fn raw(self) -> u64 {
        self.0
    }
}

impl fmt::Display for ServiceId {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(f, "service#{:016x}", self.0)
    }
}

/// A type-erased service value.
///
/// Note the absence of any hashing method — that omission is the entire point of this trait.
pub(crate) trait ServiceSlot: fmt::Debug + Send + Sync {
    /// Upcast for downcasting back to the concrete type.
    fn as_any(&self) -> &dyn Any;

    /// Mutable upcast for downcasting.
    fn as_any_mut(&mut self) -> &mut dyn Any;

    /// Consuming upcast, so a service can be moved back out.
    fn into_any(self: Box<Self>) -> Box<dyn Any>;
}

impl<T: Service> ServiceSlot for T {
    fn as_any(&self) -> &dyn Any {
        self
    }

    fn as_any_mut(&mut self) -> &mut dyn Any {
        self
    }

    fn into_any(self: Box<Self>) -> Box<dyn Any> {
        self
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[derive(Debug, PartialEq)]
    struct FrameCounter(u32);
    impl Service for FrameCounter {}

    #[derive(Debug)]
    struct FakeDevice;
    impl Service for FakeDevice {}

    #[test]
    fn ids_differ_by_type() {
        assert_ne!(
            ServiceId::of::<FrameCounter>(),
            ServiceId::of::<FakeDevice>()
        );
        assert_eq!(
            ServiceId::of::<FrameCounter>(),
            ServiceId::of::<FrameCounter>()
        );
    }

    #[test]
    fn slot_round_trips_through_type_erasure() {
        let boxed: Box<dyn ServiceSlot> = Box::new(FrameCounter(3));
        assert_eq!(
            boxed.as_any().downcast_ref::<FrameCounter>(),
            Some(&FrameCounter(3))
        );

        let recovered = boxed
            .into_any()
            .downcast::<FrameCounter>()
            .expect("same type");
        assert_eq!(*recovered, FrameCounter(3));
    }
}
