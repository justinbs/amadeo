//! Typed, double-buffered event queues.
//!
//! # The model
//!
//! Events are how systems communicate without depending on each other. A system *sends* an event; a
//! different system *reads* it next tick.
//!
//! ```
//! use amadeo_core::StableHash;
//! use amadeo_ecs::World;
//! use amadeo_events::{Event, WorldEvents};
//! use amadeo_reflect::Reflect;
//!
//! /// Something took damage.
//! #[derive(Debug, Clone, PartialEq, StableHash, Reflect)]
//! struct DamageDealt {
//!     /// How much.
//!     amount: u32,
//! }
//!
//! impl Event for DamageDealt {}
//!
//! let mut world = World::new();
//! world.register_event::<DamageDealt>();
//!
//! // Tick N: something sends an event.
//! world.send_event(DamageDealt { amount: 7 });
//! assert_eq!(world.read_events::<DamageDealt>().len(), 0, "not readable yet");
//!
//! // The app loop swaps buffers at the tick boundary.
//! world.swap_events::<DamageDealt>();
//!
//! // Tick N+1: readable.
//! let events = world.read_events::<DamageDealt>();
//! assert_eq!(events.len(), 1);
//! assert_eq!(events[0].event, DamageDealt { amount: 7 });
//! ```
//!
//! # Why no immediate dispatch
//!
//! There are no callbacks. A send never runs another system's code. Immediate dispatch makes
//! execution order implicit and allows reentrancy — a system mutating state that its own caller is
//! midway through reading. Both are hostile to determinism and to being able to reason about a tick
//! at all, so ordering here is always explicit and always deferred.
//!
//! # Events are not commands
//!
//! An event says *something happened*. A command says *change the world*. Structural changes —
//! spawning, despawning, adding components — are deferred commands, a separate mechanism with
//! different ordering rules. Conflating the two is a reliable source of ordering bugs, so keep them
//! apart.

use amadeo_core::{StableHash, StableHasher, Tick};
use amadeo_ecs::{Resource, World};
use amadeo_reflect::{FieldInfo, Reflect, ReflectError, Replication, TypeInfo, TypeKind, Value};
use std::fmt;

/// Something that happened, broadcast to any system that cares.
///
/// Named in the past tense by convention — `EntitySpawned`, `DamageDealt`, `CollisionStarted`.
///
/// [`StableHash`] is required because queued events are part of simulation state at a tick boundary:
/// two runs that agree on everything except pending events have not actually agreed.
///
/// [`Reflect`] is required for the same reason it is on `Resource` and `Component` (ADR 0027,
/// ADR 0013) plus one specific to events: **the event log is how an agent answers "what did I just
/// do?"** — Pillar 3 of `docs/03-ai-native-design.md`. A queue full of events nobody can read is a
/// queue that cannot serve its most valuable purpose.
pub trait Event: 'static + Send + Sync + fmt::Debug + StableHash + Reflect {}

/// An event plus when it happened and where it sits in the global order.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct EventRecord<T> {
    /// Position in the global send order across all event types. Strictly increasing.
    pub sequence: u64,
    /// The tick during which this event was sent.
    pub tick: Tick,
    /// The event itself.
    pub event: T,
}

impl<T: StableHash> StableHash for EventRecord<T> {
    fn stable_hash(&self, hasher: &mut StableHasher) {
        hasher.write_u64(self.sequence);
        hasher.write_u64(self.tick.0);
        self.event.stable_hash(hasher);
    }
}

/// Hand-written rather than derived because the derive does not handle a generic parameter, and
/// this type is generic over every event a game defines.
///
/// The body is what the derive would emit: fields sorted by name into a struct value.
impl<T: Reflect> Reflect for EventRecord<T> {
    fn type_name() -> String {
        format!("event-record<{}>", T::type_name())
    }

    fn type_info() -> TypeInfo {
        TypeInfo {
            name: Self::type_name(),
            docs: "One event, with when it was sent and where it sits in the global order."
                .to_string(),
            version: 1,
            kind: TypeKind::Struct {
                fields: vec![
                    field(
                        "sequence",
                        "u64",
                        "Position in the global send order across all event types. Strictly increasing.",
                    ),
                    field("tick", "Tick", "The tick during which this event was sent."),
                    field("event", &T::type_name(), "The event itself."),
                ],
            },
        }
    }

    fn to_value(&self) -> Value {
        Value::structure([
            ("sequence", Value::U64(self.sequence)),
            ("tick", self.tick.to_value()),
            ("event", self.event.to_value()),
        ])
    }

    fn from_value(value: &Value) -> Result<Self, ReflectError> {
        let name = Self::type_name();
        let Value::Struct(fields) = value else {
            return Err(ReflectError::mismatch(name, "struct", value));
        };

        let take = |field_name: &str| -> Result<&Value, ReflectError> {
            fields
                .get(field_name)
                .ok_or_else(|| ReflectError::MissingField {
                    type_name: name.clone(),
                    field: field_name.to_string(),
                    required: "sequence, tick, event".to_string(),
                })
        };

        Ok(EventRecord {
            sequence: u64::from_value(take("sequence")?)?,
            tick: Tick::from_value(take("tick")?)?,
            event: T::from_value(take("event")?)?,
        })
    }
}

/// One entry of a hand-written schema.
///
/// A helper because `FieldInfo` has no constructor — the derive builds it literally, and the two
/// generic types here are the only ones in the engine that write a schema by hand.
fn field(name: &str, type_name: &str, docs: &str) -> FieldInfo {
    FieldInfo {
        name: name.to_string(),
        type_name: type_name.to_string(),
        docs: docs.to_string(),
        range: None,
        unit: None,
        replication: Replication::default(),
    }
}

/// Hands out the sequence numbers that give events a total order.
///
/// # Why a shared counter rather than per-type numbering
///
/// Per-type numbering would order events *within* a type but say nothing about two events of
/// different types. Reading "what happened this tick" then gives no causal order, which is exactly
/// what makes an event log useful for debugging — and for the agent introspection layer that reads it
/// (`docs/03-ai-native-design.md` Pillar 3).
///
/// A single monotonic counter is deterministic because sends happen in deterministic order, so this
/// costs nothing in reproducibility.
///
/// # Its one field is private but reflected
///
/// `next` has no public setter — handing out a way to rewind the event clock would let two events
/// share a sequence number, which is exactly what it exists to prevent. Reflection still sees it,
/// because invariant I8 is about what the agent and the editor can *observe*, and a resource with an
/// invisible field is a resource whose state a snapshot cannot restore.
#[derive(Debug, Default, Clone, PartialEq, Eq, StableHash, Reflect)]
pub struct EventClock {
    /// How many events have been sent since the world was created; the next one takes this number.
    next: u64,
}

impl Resource for EventClock {}

impl EventClock {
    /// Returns the next sequence number and advances the counter.
    pub fn take_sequence(&mut self) -> u64 {
        let sequence = self.next;
        self.next += 1;
        sequence
    }

    /// How many events have been sent since the world was created.
    #[must_use]
    pub fn sent_count(&self) -> u64 {
        self.next
    }
}

/// The queue for one event type.
///
/// Double-buffered: sends land in the write buffer, reads see the read buffer, and
/// [`WorldEvents::swap_events`] moves one to the other at the tick boundary. That one-tick delay is
/// what makes read order independent of system order — every reader sees the same complete set,
/// regardless of when in the tick it runs.
#[derive(Debug)]
pub struct Events<T: Event> {
    /// Events sent this tick. Not yet visible to readers.
    writing: Vec<EventRecord<T>>,
    /// Events sent last tick. What readers see.
    reading: Vec<EventRecord<T>>,
}

impl<T: Event> Default for Events<T> {
    fn default() -> Self {
        Self {
            writing: Vec::new(),
            reading: Vec::new(),
        }
    }
}

impl<T: Event> StableHash for Events<T> {
    fn stable_hash(&self, hasher: &mut StableHasher) {
        // Both buffers count: a pending event is state that will affect the next tick.
        self.writing.as_slice().stable_hash(hasher);
        self.reading.as_slice().stable_hash(hasher);
    }
}

/// Reflected as its two buffers, because **both are simulation state**.
///
/// An event sent this tick has not been read yet but will be, so a snapshot that captured only the
/// read buffer would restore a world that then silently skipped a tick's worth of events. The
/// `StableHash` impl above already takes both for exactly that reason; this keeps the two in step.
impl<T: Event> Reflect for Events<T> {
    fn type_name() -> String {
        format!("events<{}>", T::type_name())
    }

    fn type_info() -> TypeInfo {
        let record = EventRecord::<T>::type_name();
        TypeInfo {
            name: Self::type_name(),
            docs: "A double-buffered event queue. Sends land in `writing`; readers see `reading`; \
                   the two swap at the tick boundary."
                .to_string(),
            version: 1,
            kind: TypeKind::Struct {
                fields: vec![
                    field(
                        "reading",
                        &format!("list<{record}>"),
                        "Events sent last tick. What readers see now.",
                    ),
                    field(
                        "writing",
                        &format!("list<{record}>"),
                        "Events sent this tick, not yet visible to readers.",
                    ),
                ],
            },
        }
    }

    fn to_value(&self) -> Value {
        Value::structure([
            ("reading", self.reading.to_value()),
            ("writing", self.writing.to_value()),
        ])
    }

    fn from_value(value: &Value) -> Result<Self, ReflectError> {
        let name = Self::type_name();
        let Value::Struct(fields) = value else {
            return Err(ReflectError::mismatch(name, "struct", value));
        };

        let take = |field_name: &str| -> Result<Vec<EventRecord<T>>, ReflectError> {
            let inner = fields
                .get(field_name)
                .ok_or_else(|| ReflectError::MissingField {
                    type_name: name.clone(),
                    field: field_name.to_string(),
                    required: "reading, writing".to_string(),
                })?;
            Vec::<EventRecord<T>>::from_value(inner)
        };

        Ok(Events {
            reading: take("reading")?,
            writing: take("writing")?,
        })
    }
}

impl<T: Event> Resource for Events<T> {}

impl<T: Event> Events<T> {
    /// Queues an event into the write buffer.
    pub fn send(&mut self, event: T, sequence: u64, tick: Tick) {
        self.writing.push(EventRecord {
            sequence,
            tick,
            event,
        });
    }

    /// Events from the previous tick, in send order.
    #[must_use]
    pub fn read(&self) -> &[EventRecord<T>] {
        &self.reading
    }

    /// Events sent during the current tick, not yet swapped in.
    ///
    /// Use sparingly and only across a declared stage boundary. Reading these makes the result
    /// depend on whether the sender has run yet, which reintroduces the implicit ordering that
    /// double buffering exists to remove.
    #[must_use]
    pub fn read_pending(&self) -> &[EventRecord<T>] {
        &self.writing
    }

    /// Promotes this tick's events to readable and discards last tick's.
    ///
    /// Reuses the drained allocation rather than freeing it, so a steady event rate settles into
    /// zero allocation per tick.
    pub fn swap(&mut self) {
        self.reading.clear();
        std::mem::swap(&mut self.reading, &mut self.writing);
    }

    /// Discards everything in both buffers.
    pub fn clear(&mut self) {
        self.writing.clear();
        self.reading.clear();
    }

    /// Whether either buffer holds anything.
    #[must_use]
    pub fn is_empty(&self) -> bool {
        self.writing.is_empty() && self.reading.is_empty()
    }
}

/// Event operations on a [`World`].
///
/// An extension trait rather than inherent methods, because `amadeo-ecs` sits below `amadeo-events`
/// in the dependency order (invariant I6) and so cannot know about events.
pub trait WorldEvents {
    /// Prepares a world to carry events of type `T`.
    ///
    /// Must be called before sending or reading. Idempotent — calling it twice does not discard
    /// queued events.
    fn register_event<T: Event>(&mut self);

    /// Queues an event, assigning it the next global sequence number.
    ///
    /// Returns `false` if `T` was never registered, which is a setup bug rather than a runtime
    /// condition. Reported instead of panicking so a misconfigured game stays inspectable.
    fn send_event<T: Event>(&mut self, event: T) -> bool;

    /// Events of type `T` from the previous tick. Empty if `T` was never registered.
    fn read_events<T: Event>(&self) -> &[EventRecord<T>];

    /// Promotes `T`'s pending events to readable. Called once per tick by the app loop.
    fn swap_events<T: Event>(&mut self);
}

impl WorldEvents for World {
    fn register_event<T: Event>(&mut self) {
        if !self.has_resource::<EventClock>() {
            self.insert_resource(EventClock::default());
        }
        if !self.has_resource::<Events<T>>() {
            self.insert_resource(Events::<T>::default());
        }
    }

    fn send_event<T: Event>(&mut self, event: T) -> bool {
        let tick = self.tick();

        // Two sequential borrows, not two simultaneous ones: take the sequence number, let that
        // borrow end, then borrow the queue. Holding both at once would need two mutable references
        // into the same resource map.
        let Some(clock) = self.resource_mut::<EventClock>() else {
            return false;
        };
        let sequence = clock.take_sequence();

        let Some(events) = self.resource_mut::<Events<T>>() else {
            return false;
        };
        events.send(event, sequence, tick);
        true
    }

    fn read_events<T: Event>(&self) -> &[EventRecord<T>] {
        // An unregistered type reads as empty rather than as an error. A system that reacts to
        // events nobody sends should do nothing, not fail.
        self.resource::<Events<T>>().map_or(&[], Events::read)
    }

    fn swap_events<T: Event>(&mut self) {
        if let Some(events) = self.resource_mut::<Events<T>>() {
            events.swap();
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[derive(Debug, Clone, PartialEq, Eq, Reflect)]
    struct Hit {
        amount: u32,
    }

    impl StableHash for Hit {
        fn stable_hash(&self, hasher: &mut StableHasher) {
            self.amount.stable_hash(hasher);
        }
    }
    impl Event for Hit {}

    #[derive(Debug, Clone, PartialEq, Eq, Reflect)]
    struct Spawned;

    impl StableHash for Spawned {
        fn stable_hash(&self, _hasher: &mut StableHasher) {}
    }
    impl Event for Spawned {}

    fn world_with_events() -> World {
        let mut world = World::new();
        world.register_event::<Hit>();
        world.register_event::<Spawned>();
        world
    }

    #[test]
    fn events_are_not_readable_until_swapped() {
        let mut world = world_with_events();
        assert!(world.send_event(Hit { amount: 5 }));

        assert!(
            world.read_events::<Hit>().is_empty(),
            "an event must not be readable in the tick it was sent"
        );

        world.swap_events::<Hit>();
        let events = world.read_events::<Hit>();
        assert_eq!(events.len(), 1);
        assert_eq!(events[0].event, Hit { amount: 5 });
    }

    #[test]
    fn swapping_twice_discards_old_events() {
        let mut world = world_with_events();
        world.send_event(Hit { amount: 1 });
        world.swap_events::<Hit>();
        assert_eq!(world.read_events::<Hit>().len(), 1);

        // Nothing sent this tick, so the next swap empties the read buffer.
        world.swap_events::<Hit>();
        assert!(world.read_events::<Hit>().is_empty());
    }

    #[test]
    fn send_order_is_preserved() {
        let mut world = world_with_events();
        for amount in 0..5 {
            world.send_event(Hit { amount });
        }
        world.swap_events::<Hit>();

        let amounts: Vec<u32> = world
            .read_events::<Hit>()
            .iter()
            .map(|record| record.event.amount)
            .collect();
        assert_eq!(amounts, vec![0, 1, 2, 3, 4]);
    }

    #[test]
    fn sequence_numbers_order_across_types() {
        // The reason a shared clock exists: interleaved events of different types must still be
        // orderable against each other.
        let mut world = world_with_events();
        world.send_event(Hit { amount: 1 });
        world.send_event(Spawned);
        world.send_event(Hit { amount: 2 });

        world.swap_events::<Hit>();
        world.swap_events::<Spawned>();

        let hits = world.read_events::<Hit>();
        let spawns = world.read_events::<Spawned>();

        assert_eq!(hits[0].sequence, 0);
        assert_eq!(spawns[0].sequence, 1);
        assert_eq!(hits[1].sequence, 2);
        // Strictly increasing, so a merged log has an unambiguous order.
        assert!(hits[0].sequence < spawns[0].sequence);
        assert!(spawns[0].sequence < hits[1].sequence);
    }

    #[test]
    fn events_record_the_tick_they_were_sent() {
        let mut world = world_with_events();
        world.advance_tick();
        world.advance_tick();
        world.send_event(Hit { amount: 1 });
        world.swap_events::<Hit>();

        assert_eq!(world.read_events::<Hit>()[0].tick, Tick(2));
    }

    #[test]
    fn unregistered_types_are_handled_without_panicking() {
        let mut world = World::new();
        // Nothing registered: sending reports failure, reading is empty, swapping is a no-op.
        assert!(!world.send_event(Hit { amount: 1 }));
        assert!(world.read_events::<Hit>().is_empty());
        world.swap_events::<Hit>();
    }

    #[test]
    fn registering_twice_preserves_queued_events() {
        let mut world = world_with_events();
        world.send_event(Hit { amount: 42 });

        world.register_event::<Hit>();

        world.swap_events::<Hit>();
        assert_eq!(world.read_events::<Hit>().len(), 1);
    }

    #[test]
    fn pending_and_readable_buffers_are_distinct() {
        let mut world = world_with_events();
        world.send_event(Hit { amount: 1 });
        world.swap_events::<Hit>();
        world.send_event(Hit { amount: 2 });

        let events = world.resource::<Events<Hit>>().expect("registered");
        assert_eq!(events.read().len(), 1);
        assert_eq!(events.read_pending().len(), 1);
        assert_eq!(events.read()[0].event.amount, 1);
        assert_eq!(events.read_pending()[0].event.amount, 2);
        assert!(!events.is_empty());
    }

    #[test]
    fn clear_empties_both_buffers() {
        let mut world = world_with_events();
        world.send_event(Hit { amount: 1 });
        world.swap_events::<Hit>();
        world.send_event(Hit { amount: 2 });

        world
            .resource_mut::<Events<Hit>>()
            .expect("registered")
            .clear();
        assert!(
            world
                .resource::<Events<Hit>>()
                .expect("registered")
                .is_empty()
        );
    }

    #[test]
    fn queued_events_affect_the_state_hash() {
        // Pending events are state at a tick boundary. Two worlds agreeing on entities but not on
        // queued events have not agreed, and a replay that lost an event must be caught.
        let mut with_event = world_with_events();
        with_event.send_event(Hit { amount: 1 });

        let without_event = world_with_events();
        assert_ne!(with_event.state_hash(), without_event.state_hash());
    }

    #[test]
    fn identical_event_traffic_hashes_identically() {
        let mut first = world_with_events();
        let mut second = world_with_events();
        for amount in 0..4 {
            first.send_event(Hit { amount });
            second.send_event(Hit { amount });
        }
        assert_eq!(first.state_hash(), second.state_hash());

        first.swap_events::<Hit>();
        second.swap_events::<Hit>();
        assert_eq!(first.state_hash(), second.state_hash());
    }

    #[test]
    fn differing_send_order_is_detected() {
        // Same events, different order: sequence numbers differ, so the hashes must differ. This is
        // what catches a system-ordering change that would otherwise pass unnoticed.
        let mut ascending = world_with_events();
        ascending.send_event(Hit { amount: 1 });
        ascending.send_event(Spawned);

        let mut descending = world_with_events();
        descending.send_event(Spawned);
        descending.send_event(Hit { amount: 1 });

        assert_ne!(ascending.state_hash(), descending.state_hash());
    }
}
