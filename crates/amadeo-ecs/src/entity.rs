//! Entity handles and the allocator that hands them out.

use amadeo_core::StableHash;
use amadeo_reflect::Reflect;
use std::fmt;

/// A handle to an entity in a running [`World`](crate::World).
///
/// # This is a runtime handle, not an identity
///
/// An `Entity` is a slot index plus a generation counter. It is only meaningful inside the process
/// that produced it, and the slot is **reused** after the entity is despawned. It never appears in a
/// scene file and never travels over a network — see `amadeo_core::id` for the two types that do
/// ([`StableId`](amadeo_core::StableId) and [`NetId`](amadeo_core::NetId)).
///
/// # How the generation counter earns its keep
///
/// Reusing slots without a generation would let a stale handle silently address whatever entity
/// landed in the slot next — a use-after-free with no crash, producing wrong behaviour instead of an
/// error. Bumping the generation on despawn makes the stale handle detectably invalid instead.
///
/// # Why it is `Reflect` despite never appearing in a file
///
/// A component may *hold* one — `Parent` in `amadeo-transform` does — and `Component: Reflect`
/// (ADR 0013) means anything a component contains has to be reflectable too. It reflects as
/// `{ generation, index }`, which is useful for introspecting a live world and meaningless in a
/// saved one. Anything writing a world back out to a scene file must derive structure from these
/// handles rather than serialise them; see ADR 0015.
#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Hash, StableHash, Reflect)]
pub struct Entity {
    /// Index into the entity slot table.
    index: u32,
    /// How many times this slot has been reused. Distinguishes a live handle from a stale one.
    generation: u32,
}

impl Entity {
    /// Creates a handle. Internal: only the allocator should mint these.
    pub(crate) fn new(index: u32, generation: u32) -> Self {
        Self { index, generation }
    }

    /// Rebuilds a handle from its two halves. **For restoring a snapshot, and nothing else.**
    ///
    /// Normally the allocator is the only thing that mints handles, and that is what keeps a handle
    /// meaningful — one invented from thin air can address an entity that was never spawned, or a
    /// slot occupied by something else, which is the use-after-free the generation counter exists to
    /// prevent.
    ///
    /// A snapshot restore is the one caller with a legitimate need: it is *reinstating* handles that
    /// an allocator really did produce, and it must reproduce them exactly, because both halves go
    /// into `World::state_hash`. `World::restore_entities` rebuilds the allocator around them, so
    /// the handles this makes are live by the time anything can use them.
    #[must_use]
    pub fn from_parts(index: u32, generation: u32) -> Self {
        Self { index, generation }
    }

    /// The slot this entity occupies.
    #[must_use]
    pub fn index(self) -> u32 {
        self.index
    }

    /// How many times this entity's slot has been reused.
    #[must_use]
    pub fn generation(self) -> u32 {
        self.generation
    }
}

impl fmt::Display for Entity {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        // Rendered as index:generation so diagnostics can distinguish a reused slot from the
        // original occupant, which is exactly the confusion the generation exists to prevent.
        write!(f, "{}:{}", self.index, self.generation)
    }
}

/// Where an entity's component data lives.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(crate) struct EntityLocation {
    /// Which archetype holds it.
    pub archetype: usize,
    /// Which row within that archetype's columns.
    pub row: usize,
}

/// One slot in the entity table.
#[derive(Debug, Clone, Copy)]
struct EntitySlot {
    /// Bumped every time the slot is freed, invalidating outstanding handles.
    generation: u32,
    /// `None` when the slot is free.
    location: Option<EntityLocation>,
}

/// Allocates and tracks entity handles.
///
/// Free slots are kept on a stack so allocation reuses memory instead of growing forever. The stack
/// is drained last-in-first-out, which is deterministic — that matters, because entity indices feed
/// into archetype row order and therefore into state hashes (invariant I3).
#[derive(Debug, Default)]
pub(crate) struct EntityStore {
    slots: Vec<EntitySlot>,
    free: Vec<u32>,
}

impl EntityStore {
    /// Allocates a fresh handle, reusing a free slot when one is available.
    pub(crate) fn allocate(&mut self) -> Entity {
        if let Some(index) = self.free.pop() {
            let slot = &mut self.slots[index as usize];
            debug_assert!(
                slot.location.is_none(),
                "free list contained a slot that was still occupied"
            );
            Entity::new(index, slot.generation)
        } else {
            let index = u32::try_from(self.slots.len()).unwrap_or(u32::MAX);
            self.slots.push(EntitySlot {
                generation: 0,
                location: None,
            });
            Entity::new(index, 0)
        }
    }

    /// Frees a handle's slot, invalidating that handle and any copies of it.
    ///
    /// Returns the location the entity occupied, or `None` if the handle was already stale.
    pub(crate) fn free(&mut self, entity: Entity) -> Option<EntityLocation> {
        let slot = self.slots.get_mut(entity.index() as usize)?;
        if slot.generation != entity.generation() {
            return None;
        }
        let location = slot.location.take();
        // Wrapping keeps this sound after 4 billion reuses of one slot; a wrapped generation could
        // in principle collide with a very old handle, which is acceptable and is what every
        // generational-index scheme does.
        slot.generation = slot.generation.wrapping_add(1);
        self.free.push(entity.index());
        location
    }

    /// Whether this handle still refers to a live entity.
    pub(crate) fn contains(&self, entity: Entity) -> bool {
        self.slots
            .get(entity.index() as usize)
            .is_some_and(|slot| slot.generation == entity.generation() && slot.location.is_some())
    }

    /// Looks up where an entity's data lives, or `None` if the handle is stale.
    pub(crate) fn location(&self, entity: Entity) -> Option<EntityLocation> {
        let slot = self.slots.get(entity.index() as usize)?;
        if slot.generation != entity.generation() {
            return None;
        }
        slot.location
    }

    /// Records where an entity's data lives.
    pub(crate) fn set_location(&mut self, entity: Entity, location: EntityLocation) {
        if let Some(slot) = self.slots.get_mut(entity.index() as usize)
            && slot.generation == entity.generation()
        {
            slot.location = Some(location);
        }
    }

    /// How many entities are currently alive.
    pub(crate) fn len(&self) -> usize {
        self.slots.len() - self.free.len()
    }

    /// The free stack, as `index:generation` handles, **bottom first**.
    ///
    /// # Why a snapshot has to record this
    ///
    /// [`World::state_hash`](crate::World::state_hash) deliberately excludes the free list — it is
    /// allocator bookkeeping, not simulation state. Which means two worlds can hash **identically**
    /// and then hand out different entity handles on the very next `spawn`, because one of them has
    /// a slot to reuse and the other does not.
    ///
    /// So a snapshot that captured only the live entities would restore a world that looked right by
    /// every available measure and diverged a few ticks later, once something spawned. Capturing the
    /// free list is what closes that, and it is the reason snapshot correctness is tested by *running
    /// on* after a restore rather than by comparing hashes.
    ///
    /// Order is preserved because the stack is drained last-in-first-out, so the last entry here is
    /// the next slot to be reused.
    pub(crate) fn free_slots(&self) -> Vec<Entity> {
        self.free
            .iter()
            .map(|index| Entity::new(*index, self.slots[*index as usize].generation))
            .collect()
    }

    /// Rebuilds an allocator holding exactly these live and free slots.
    ///
    /// For restoring a snapshot, and nothing else. `live` supplies the occupied slots and `free` the
    /// stack in the order [`EntityStore::free_slots`] produced it.
    ///
    /// Locations are left empty: the caller re-inserts each entity's components, which is what
    /// establishes where its data actually lives.
    ///
    /// **Every slot index below the highest must appear in one list or the other**, since in a real
    /// allocator a slot is either occupied or free. A gap would produce a slot that is neither, and
    /// therefore never allocated again — a leak that nothing would report. That cannot happen from a
    /// captured world; it can happen from a hand-edited snapshot, so the snapshot reader validates
    /// it and says so, and this carries a `debug_assert` as the backstop.
    pub(crate) fn restore(live: &[Entity], free: &[Entity]) -> EntityStore {
        let highest = live
            .iter()
            .chain(free.iter())
            .map(|entity| entity.index())
            .max();

        let mut slots = vec![
            EntitySlot {
                generation: 0,
                location: None,
            };
            highest.map_or(0, |index| index as usize + 1)
        ];

        debug_assert_eq!(
            live.len() + free.len(),
            slots.len(),
            "every slot must be either live or free; a gap would leak one permanently"
        );

        for entity in live.iter().chain(free.iter()) {
            slots[entity.index() as usize].generation = entity.generation();
        }

        EntityStore {
            slots,
            free: free.iter().map(|entity| entity.index()).collect(),
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn allocates_distinct_entities() {
        let mut store = EntityStore::default();
        let a = store.allocate();
        let b = store.allocate();
        assert_ne!(a, b);
        assert_eq!(a.index(), 0);
        assert_eq!(b.index(), 1);
    }

    #[test]
    fn reuses_freed_slots_and_bumps_generation() {
        let mut store = EntityStore::default();
        let first = store.allocate();
        store.set_location(
            first,
            EntityLocation {
                archetype: 0,
                row: 0,
            },
        );
        store.free(first);

        let second = store.allocate();
        assert_eq!(second.index(), first.index(), "slot should be reused");
        assert_ne!(
            second.generation(),
            first.generation(),
            "generation must change"
        );
    }

    #[test]
    fn stale_handle_is_rejected() {
        let mut store = EntityStore::default();
        let entity = store.allocate();
        store.set_location(
            entity,
            EntityLocation {
                archetype: 0,
                row: 0,
            },
        );
        assert!(store.contains(entity));

        store.free(entity);
        assert!(!store.contains(entity), "freed handle must not be live");
        assert_eq!(store.location(entity), None);

        // The reused slot must not be addressable through the old handle. This is the
        // use-after-free the generation counter exists to catch.
        let reused = store.allocate();
        store.set_location(
            reused,
            EntityLocation {
                archetype: 1,
                row: 7,
            },
        );
        assert!(!store.contains(entity));
        assert_eq!(store.location(entity), None);
        assert!(store.contains(reused));
    }

    #[test]
    fn double_free_is_harmless() {
        let mut store = EntityStore::default();
        let entity = store.allocate();
        store.set_location(
            entity,
            EntityLocation {
                archetype: 0,
                row: 0,
            },
        );
        assert!(store.free(entity).is_some());
        // Second free returns None rather than corrupting the free list.
        assert_eq!(store.free(entity), None);
    }

    #[test]
    fn len_tracks_live_entities() {
        let mut store = EntityStore::default();
        assert_eq!(store.len(), 0);
        let a = store.allocate();
        let _b = store.allocate();
        assert_eq!(store.len(), 2);
        store.free(a);
        assert_eq!(store.len(), 1);
    }

    #[test]
    fn display_shows_index_and_generation() {
        assert_eq!(Entity::new(3, 5).to_string(), "3:5");
    }
}
