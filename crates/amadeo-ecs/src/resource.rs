//! Resources: single-instance global state inside a [`World`](crate::World).
//!
//! A component belongs to an entity; a resource is one value for the whole world. The simulation
//! clock's seed, the input state for the current tick, and event queues are all resources.

use crate::type_hash::hash_name;
use amadeo_core::{StableHash, StableHasher};
use amadeo_reflect::Reflect;
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
///
/// # Why [`Reflect`] is required — ADR 0027, closing invariant I8
///
/// I8 says a type that cannot be reflected cannot be serialised, inspected, or edited. ADR 0013 made
/// that a compiler-enforced bound for components; this is the other half, and it was deliberately
/// deferred then because two resources could not yet reflect at all — `SimRng`'s state was private
/// to `amadeo-core`, and `InputState` needed a map in the value tree that did not exist.
///
/// Requiring it here rather than trusting a convention is the same argument ADR 0013 made: a
/// resource that is invisible to the agent still works perfectly at runtime, so the omission is
/// found three milestones later by someone wondering why `world.resources` is missing something.
/// The bound makes that unrepresentable.
///
/// The practical consequence: **a resource cannot be a type whose state is private to a lower
/// crate.** That is not a burden, it is the invariant doing its job — see [`Rng::state`] for what it
/// forced, and why exposing it was right for three independent reasons.
///
/// [`Reflect`]: amadeo_reflect::Reflect
/// [`Rng::state`]: amadeo_core::Rng::state
pub trait Resource: 'static + Send + Sync + fmt::Debug + StableHash + Reflect {}

/// Identifies a resource type.
///
/// Derived by hashing the type's name rather than from `std::any::TypeId`, because `TypeId` values
/// are compiler-generated and carry no stability guarantee across builds — using them would make
/// state hashes disagree between compilations of identical logic (invariant I3).
#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Hash)]
pub struct ResourceId(u64);

impl ResourceId {
    /// The id for resource type `T`, from its **canonical** name.
    ///
    /// # Why the canonical name and not the Rust path — ADR 0017, second instalment
    ///
    /// ADR 0017 made a component's identity its canonical name so that moving one between crates is
    /// free and *renaming* one is the deliberate breaking change. It could not do the same for
    /// resources at the time, because a resource was not reflected and so had no canonical name —
    /// and it said so: "resources get this treatment when `Resource: Reflect` lands".
    ///
    /// ADR 0027 landed that bound. This is the deferred half coming due.
    ///
    /// Until it did, a resource's identity was `std::any::type_name`, so the two followed **opposite
    /// rules** — moving a resource between crates silently invalidated every golden replay, with
    /// nothing in the type's own definition having changed. Filed as Q22, found by writing a
    /// stand-in type with the same canonical name and the same fields and watching it hash
    /// differently.
    ///
    /// Free at runtime for any derived resource, the same way [`ComponentId::of`] is: the derive
    /// fills in `STATIC_NAME` and the hash is computed at compile time. The fallback covers a type
    /// whose name is built from parameters and has no constant.
    ///
    /// [`ComponentId::of`]: crate::ComponentId::of
    #[must_use]
    pub fn of<T: Resource>() -> Self {
        if T::STATIC_NAME.is_empty() {
            ResourceId(hash_name(&T::type_name()))
        } else {
            ResourceId(T::STATIC_NAME_HASH)
        }
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

    /// The resource's canonical name, as `describe` reports it.
    ///
    /// Reachable only because of the [`Reflect`] bound on [`Resource`] — a resource stored behind a
    /// trait object has otherwise thrown away everything about its type except an id, which is a
    /// hash and cannot be turned back into a name.
    fn type_name_value(&self) -> String;

    /// The resource's state as a reflected value.
    ///
    /// This is what makes `world.resources` possible at all, and it is the concrete payoff of
    /// ADR 0027: before the bound, the world could hash a resource but could not *show* one.
    fn to_reflected_value(&self) -> amadeo_reflect::Value;

    /// Puts this resource's *schema* into a registry, along with everything it names.
    ///
    /// The counterpart to [`ResourceSlot::to_reflected_value`]: that one reports what a resource
    /// currently *is*, this one reports what it *could be*. `describe` needs the second — M1 exit
    /// gate 4 found that a resource holding a game's win condition was invisible to it (ADR 0030).
    ///
    /// # Errors
    ///
    /// [`amadeo_reflect::RegistryError`] if a different type already holds this name — a component
    /// and a resource called the same thing with different shapes.
    fn register_schema(
        &self,
        registry: &mut amadeo_reflect::TypeRegistry,
    ) -> Result<(), amadeo_reflect::RegistryError>;
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

    fn type_name_value(&self) -> String {
        T::type_name()
    }

    fn to_reflected_value(&self) -> amadeo_reflect::Value {
        Reflect::to_value(self)
    }

    fn register_schema(
        &self,
        registry: &mut amadeo_reflect::TypeRegistry,
    ) -> Result<(), amadeo_reflect::RegistryError> {
        // `T` is concrete inside this impl, so the ordinary generic call works and brings the
        // resource's nested types in with it.
        registry.register::<T>()
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[derive(Debug, PartialEq, amadeo_reflect::Reflect)]
    struct Score(u32);

    impl StableHash for Score {
        fn stable_hash(&self, hasher: &mut StableHasher) {
            self.0.stable_hash(hasher);
        }
    }
    impl Resource for Score {}

    #[derive(Debug, amadeo_reflect::Reflect)]
    struct Rounds(u32);

    impl StableHash for Rounds {
        fn stable_hash(&self, hasher: &mut StableHasher) {
            self.0.stable_hash(hasher);
        }
    }
    impl Resource for Rounds {}

    /// Two *different* Rust types claiming one canonical name, in different modules.
    ///
    /// This is what ADR 0017 bought for components and ADR 0027 made possible for resources: the
    /// identity is the name, so where the type lives is irrelevant.
    mod elsewhere {
        use super::*;

        /// The same resource, imagined as having moved to another crate.
        #[derive(Debug, PartialEq, amadeo_reflect::Reflect)]
        #[reflect(name = "Score")]
        pub(super) struct Score(u32);

        impl StableHash for Score {
            fn stable_hash(&self, hasher: &mut StableHasher) {
                self.0.stable_hash(hasher);
            }
        }
        impl Resource for Score {}
    }

    #[test]
    fn moving_a_resource_between_crates_does_not_change_its_id() {
        // Q22, and the second half of ADR 0017. A resource used to be identified by
        // `std::any::type_name`, so moving one between crates silently invalidated every golden
        // replay -- with nothing in the type's own definition having changed.
        //
        // The two types below are genuinely different Rust types in different modules. They share a
        // canonical name, so they share an id.
        assert_eq!(
            ResourceId::of::<Score>(),
            ResourceId::of::<elsewhere::Score>()
        );
    }

    #[test]
    fn a_resource_id_matches_its_canonical_name() {
        // Pins the derivation. Reverting to the Rust path would fail here rather than silently
        // moving every state hash.
        let mut expected = StableHasher::new();
        expected.write_str("Score");
        assert_eq!(ResourceId::of::<Score>().raw(), expected.finish());
    }

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
