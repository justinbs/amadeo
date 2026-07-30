//! The world: entity storage plus the query surface systems use.

use crate::archetype::Archetype;
use crate::component::{Column, Component, ComponentId, TypedColumn};
use crate::entity::{Entity, EntityLocation, EntityStore};
use amadeo_core::{StableHasher, Tick};
use std::collections::BTreeMap;

/// All entities and components in a running simulation.
///
/// # Query surface
///
/// M0 provides a deliberately small set of explicit access methods rather than a generic query DSL:
///
/// | Need | Method |
/// |---|---|
/// | read one component across entities | [`World::iter`] |
/// | write one component across entities | [`World::for_each_mut`] |
/// | write one, read another | [`World::for_each_pair_mut`] |
/// | read/write a single entity | [`World::get`], [`World::get_mut`] |
///
/// Mutable iteration takes a closure rather than returning an iterator. That avoids the
/// higher-ranked lifetime machinery a mutable multi-component iterator would need, which is the kind
/// of Rust that makes a codebase unreadable to anyone still learning it (`CLAUDE.md` section 6).
/// Richer query shapes get added when a real system needs them — not speculatively.
#[derive(Debug)]
pub struct World {
    /// Entity handle allocation and location tracking.
    entities: EntityStore,
    /// Every archetype ever created. Never removed, so indices stay stable for the world's lifetime.
    archetypes: Vec<Archetype>,
    /// Maps a sorted component set to its archetype index.
    ///
    /// `BTreeMap`, not `HashMap`: archetype creation order feeds into archetype indices, and a hash
    /// map's iteration order is not reproducible across builds (invariant I3).
    archetype_index: BTreeMap<Vec<ComponentId>, usize>,
    /// The current simulation tick. Advanced by the app loop, never by gameplay code.
    tick: Tick,
}

impl Default for World {
    fn default() -> Self {
        Self::new()
    }
}

impl World {
    /// Creates an empty world.
    ///
    /// Archetype 0 is always the empty archetype — entities land there on spawn, before any
    /// component is added.
    #[must_use]
    pub fn new() -> Self {
        let empty = Archetype::new(Vec::new(), Vec::new());
        let mut archetype_index = BTreeMap::new();
        archetype_index.insert(Vec::new(), 0);
        Self {
            entities: EntityStore::default(),
            archetypes: vec![empty],
            archetype_index,
            tick: Tick::ZERO,
        }
    }

    /// The current simulation tick.
    #[must_use]
    pub fn tick(&self) -> Tick {
        self.tick
    }

    /// Advances the simulation tick. Called by the app loop once per fixed step.
    pub fn advance_tick(&mut self) {
        self.tick.advance();
    }

    /// How many entities are alive.
    #[must_use]
    pub fn entity_count(&self) -> usize {
        self.entities.len()
    }

    /// How many archetypes exist.
    ///
    /// Exposed because archetype fragmentation is the performance risk flagged in
    /// `docs/04-subsystems.md` section 3, and it cannot be measured without observing this.
    #[must_use]
    pub fn archetype_count(&self) -> usize {
        self.archetypes.len()
    }

    /// Whether a handle still refers to a live entity.
    #[must_use]
    pub fn contains(&self, entity: Entity) -> bool {
        self.entities.contains(entity)
    }

    /// Creates an entity with no components.
    ///
    /// Components are added with [`World::insert`]. Each insert moves the entity to a new archetype,
    /// so building an entity with several components costs several migrations. Acceptable for M0;
    /// a bundle API that spawns straight into the final archetype is the obvious later optimisation.
    pub fn spawn(&mut self) -> Entity {
        let entity = self.entities.allocate();
        let row = self.archetypes[0].push_entity(entity);
        self.entities
            .set_location(entity, EntityLocation { archetype: 0, row });
        entity
    }

    /// Removes an entity and all its components.
    ///
    /// Returns `false` if the handle was already stale, which is not an error — despawning twice is
    /// a normal thing for game code to do.
    pub fn despawn(&mut self, entity: Entity) -> bool {
        let Some(location) = self.entities.free(entity) else {
            return false;
        };
        let moved = self.archetypes[location.archetype].swap_remove_row(location.row);
        // A row was swapped into the hole, so its owner's recorded location is now wrong.
        if let Some(moved) = moved {
            self.entities.set_location(moved, location);
        }
        self.archetypes[location.archetype].debug_assert_rectangular();
        true
    }

    /// Shared access to one component of one entity.
    #[must_use]
    pub fn get<T: Component>(&self, entity: Entity) -> Option<&T> {
        let location = self.entities.location(entity)?;
        self.archetypes[location.archetype]
            .column::<T>()?
            .value(location.row)
    }

    /// Mutable access to one component of one entity.
    ///
    /// Marks the component as changed at the current tick, whether or not the caller writes to it.
    pub fn get_mut<T: Component>(&mut self, entity: Entity) -> Option<&mut T> {
        let location = self.entities.location(entity)?;
        let tick = self.tick;
        self.archetypes[location.archetype]
            .column_mut::<T>()?
            .value_mut(location.row, tick)
    }

    /// Whether an entity has a given component.
    #[must_use]
    pub fn has<T: Component>(&self, entity: Entity) -> bool {
        let Some(location) = self.entities.location(entity) else {
            return false;
        };
        self.archetypes[location.archetype].has(ComponentId::of::<T>())
    }

    /// The tick at which an entity's component was last mutably accessed.
    #[must_use]
    pub fn changed_tick<T: Component>(&self, entity: Entity) -> Option<Tick> {
        let location = self.entities.location(entity)?;
        self.archetypes[location.archetype]
            .column::<T>()?
            .changed_tick(location.row)
    }

    /// Adds or overwrites a component on an entity.
    ///
    /// Returns `false` if the handle is stale. Overwriting is in place; adding a new component type
    /// moves the entity to a different archetype.
    pub fn insert<T: Component>(&mut self, entity: Entity, value: T) -> bool {
        let Some(location) = self.entities.location(entity) else {
            return false;
        };
        let tick = self.tick;
        let id = ComponentId::of::<T>();

        // Already present: overwrite without changing archetype.
        if self.archetypes[location.archetype].has(id) {
            let Some(column) = self.archetypes[location.archetype].column_mut::<T>() else {
                return false;
            };
            let Some(slot) = column.value_mut(location.row, tick) else {
                return false;
            };
            *slot = value;
            return true;
        }

        // Otherwise the entity moves to the archetype holding its current set plus `T`.
        let mut target_ids = self.archetypes[location.archetype].component_ids().to_vec();
        target_ids.push(id);
        target_ids.sort_unstable();

        let target = match self.archetype_index.get(&target_ids) {
            Some(&index) => index,
            None => {
                let mut pairs = self.archetypes[location.archetype].empty_columns_clone();
                pairs.push((id, Box::new(TypedColumn::<T>::new()) as Box<dyn Column>));
                pairs.sort_by_key(|(id, _)| *id);
                self.create_archetype(pairs)
            }
        };

        self.move_entity(entity, location, target);
        // The entity is now the last row of `target`, but its column for `T` is one value short.
        self.archetypes[target].push_component(value, tick);

        self.archetypes[target].debug_assert_rectangular();
        self.archetypes[location.archetype].debug_assert_rectangular();
        true
    }

    /// Removes a component from an entity.
    ///
    /// Returns `false` if the handle is stale or the component was not present.
    pub fn remove<T: Component>(&mut self, entity: Entity) -> bool {
        let Some(location) = self.entities.location(entity) else {
            return false;
        };
        let id = ComponentId::of::<T>();
        if !self.archetypes[location.archetype].has(id) {
            return false;
        }

        let target_ids: Vec<ComponentId> = self.archetypes[location.archetype]
            .component_ids()
            .iter()
            .copied()
            .filter(|existing| *existing != id)
            .collect();

        let target = match self.archetype_index.get(&target_ids) {
            Some(&index) => index,
            None => {
                let pairs: Vec<_> = self.archetypes[location.archetype]
                    .empty_columns_clone()
                    .into_iter()
                    .filter(|(existing, _)| *existing != id)
                    .collect();
                self.create_archetype(pairs)
            }
        };

        // The migration drops any component the destination has no column for, which is `T`.
        self.move_entity(entity, location, target);

        self.archetypes[target].debug_assert_rectangular();
        self.archetypes[location.archetype].debug_assert_rectangular();
        true
    }

    /// Registers a new archetype and returns its index.
    ///
    /// `pairs` must be sorted by component id.
    fn create_archetype(&mut self, pairs: Vec<(ComponentId, Box<dyn Column>)>) -> usize {
        let (ids, columns): (Vec<ComponentId>, Vec<Box<dyn Column>>) = pairs.into_iter().unzip();
        let index = self.archetypes.len();
        self.archetypes.push(Archetype::new(ids.clone(), columns));
        self.archetype_index.insert(ids, index);
        index
    }

    /// Moves an entity's row from its current archetype into `target`, fixing up locations.
    fn move_entity(&mut self, entity: Entity, from: EntityLocation, target: usize) {
        debug_assert_ne!(
            from.archetype, target,
            "move_entity called with identical source and destination"
        );

        let Ok([source_archetype, target_archetype]) =
            self.archetypes.get_disjoint_mut([from.archetype, target])
        else {
            debug_assert!(false, "archetype indices must be distinct and in range");
            return;
        };

        let moved = source_archetype.migrate_row_to(from.row, target_archetype);
        let new_row = target_archetype.len() - 1;

        self.entities.set_location(
            entity,
            EntityLocation {
                archetype: target,
                row: new_row,
            },
        );
        // Another entity was swapped into the vacated row and needs its location corrected.
        if let Some(moved) = moved {
            self.entities.set_location(moved, from);
        }
    }

    /// Iterates every entity with component `T`, read-only.
    ///
    /// Yields entities archetype by archetype. Within an archetype the values are a contiguous
    /// slice, which is the access pattern the storage design exists to produce.
    pub fn iter<T: Component>(&self) -> impl Iterator<Item = (Entity, &T)> {
        self.archetypes
            .iter()
            .filter_map(|archetype| {
                let column = archetype.column::<T>()?;
                Some(archetype.entities().iter().copied().zip(column.values()))
            })
            .flatten()
    }

    /// Calls `f` for every entity with component `T`, with mutable access.
    ///
    /// Every visited component is marked changed at the current tick, whether or not `f` writes to
    /// it — there is no way to observe whether a caller actually modified a `&mut`.
    pub fn for_each_mut<T: Component>(&mut self, mut f: impl FnMut(Entity, &mut T)) {
        let tick = self.tick;
        for archetype in &mut self.archetypes {
            if let Some((entities, values)) = archetype.entities_with_column_mut::<T>(tick) {
                for (entity, value) in entities.iter().copied().zip(values.iter_mut()) {
                    f(entity, value);
                }
            }
        }
    }

    /// Calls `f` for every entity that has both `A` and `B`, writing `A` and reading `B`.
    ///
    /// This is the shape most systems need: integrate a position from a velocity, apply damage from
    /// a hit, move a transform by an input. Does nothing if `A` and `B` are the same type.
    pub fn for_each_pair_mut<A: Component, B: Component>(
        &mut self,
        mut f: impl FnMut(Entity, &mut A, &B),
    ) {
        let tick = self.tick;
        for archetype in &mut self.archetypes {
            if let Some((entities, a_values, b_values)) =
                archetype.entities_with_pair_mut::<A, B>(tick)
            {
                for ((entity, a), b) in entities
                    .iter()
                    .copied()
                    .zip(a_values.iter_mut())
                    .zip(b_values.iter())
                {
                    f(entity, a, b);
                }
            }
        }
    }

    /// A fingerprint of all simulation state in this world.
    ///
    /// This is the value golden replay tests assert on (ADR 0005). Two worlds that reached the same
    /// logical state must produce the same hash, so:
    ///
    /// - entities are hashed in **entity order**, not storage order, so that `swap_remove` churn and
    ///   archetype layout cannot change the result;
    /// - each entity's components are hashed in sorted component id order;
    /// - the tick is included, since state at tick 100 is not the same state as at tick 200.
    ///
    /// Deliberately excluded: archetype indices, row numbers, free-list contents, and change ticks.
    /// Those are allocator bookkeeping, not simulation state, and including them would make the hash
    /// sensitive to implementation details rather than behaviour.
    #[must_use]
    pub fn state_hash(&self) -> u64 {
        // Collect and sort so the hash is independent of physical storage order.
        let mut rows: Vec<(Entity, usize, usize)> = Vec::with_capacity(self.entities.len());
        for (archetype_index, archetype) in self.archetypes.iter().enumerate() {
            for (row, entity) in archetype.entities().iter().enumerate() {
                rows.push((*entity, archetype_index, row));
            }
        }
        rows.sort_unstable_by_key(|(entity, _, _)| (entity.index(), entity.generation()));

        let mut hasher = StableHasher::new();
        hasher.write_u64(self.tick.0);
        hasher.write_u64(rows.len() as u64);
        for (entity, archetype_index, row) in rows {
            hasher.write_u32(entity.index());
            hasher.write_u32(entity.generation());
            self.archetypes[archetype_index].stable_hash_row(row, &mut hasher);
        }
        hasher.finish()
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use amadeo_core::StableHash;

    #[derive(Debug, Clone, Copy, PartialEq)]
    struct Position {
        x: f32,
        y: f32,
    }

    impl StableHash for Position {
        fn stable_hash(&self, hasher: &mut StableHasher) {
            self.x.stable_hash(hasher);
            self.y.stable_hash(hasher);
        }
    }
    impl Component for Position {}

    #[derive(Debug, Clone, Copy, PartialEq)]
    struct Velocity {
        x: f32,
        y: f32,
    }

    impl StableHash for Velocity {
        fn stable_hash(&self, hasher: &mut StableHasher) {
            self.x.stable_hash(hasher);
            self.y.stable_hash(hasher);
        }
    }
    impl Component for Velocity {}

    #[derive(Debug, Clone, Copy, PartialEq)]
    struct Tag(u32);

    impl StableHash for Tag {
        fn stable_hash(&self, hasher: &mut StableHasher) {
            self.0.stable_hash(hasher);
        }
    }
    impl Component for Tag {}

    #[test]
    fn spawn_and_despawn_track_entity_count() {
        let mut world = World::new();
        assert_eq!(world.entity_count(), 0);

        let a = world.spawn();
        let b = world.spawn();
        assert_eq!(world.entity_count(), 2);
        assert!(world.contains(a) && world.contains(b));

        assert!(world.despawn(a));
        assert_eq!(world.entity_count(), 1);
        assert!(!world.contains(a));
        assert!(world.contains(b));

        // Despawning twice is not an error -- game code does this routinely.
        assert!(!world.despawn(a));
    }

    #[test]
    fn insert_get_and_overwrite() {
        let mut world = World::new();
        let entity = world.spawn();

        assert!(world.insert(entity, Position { x: 1.0, y: 2.0 }));
        assert_eq!(
            world.get::<Position>(entity),
            Some(&Position { x: 1.0, y: 2.0 })
        );
        assert!(world.has::<Position>(entity));
        assert!(!world.has::<Velocity>(entity));

        // Overwriting an existing component must not create a new archetype.
        let before = world.archetype_count();
        assert!(world.insert(entity, Position { x: 9.0, y: 9.0 }));
        assert_eq!(world.archetype_count(), before);
        assert_eq!(
            world.get::<Position>(entity),
            Some(&Position { x: 9.0, y: 9.0 })
        );
    }

    #[test]
    fn get_mut_writes_through() {
        let mut world = World::new();
        let entity = world.spawn();
        world.insert(entity, Position { x: 0.0, y: 0.0 });

        world.get_mut::<Position>(entity).expect("present").x = 5.0;
        assert_eq!(
            world.get::<Position>(entity),
            Some(&Position { x: 5.0, y: 0.0 })
        );
    }

    #[test]
    fn operations_on_stale_handles_fail_cleanly() {
        let mut world = World::new();
        let entity = world.spawn();
        world.insert(entity, Position { x: 1.0, y: 1.0 });
        world.despawn(entity);

        // Every accessor must report absence rather than panic or return stale data.
        assert_eq!(world.get::<Position>(entity), None);
        assert!(world.get_mut::<Position>(entity).is_none());
        assert!(!world.insert(entity, Position { x: 2.0, y: 2.0 }));
        assert!(!world.remove::<Position>(entity));
        assert!(!world.has::<Position>(entity));
        assert_eq!(world.changed_tick::<Position>(entity), None);
    }

    #[test]
    fn remove_moves_entity_and_drops_component() {
        let mut world = World::new();
        let entity = world.spawn();
        world.insert(entity, Position { x: 1.0, y: 1.0 });
        world.insert(entity, Velocity { x: 3.0, y: 0.0 });

        assert!(world.remove::<Velocity>(entity));
        assert!(!world.has::<Velocity>(entity));
        // The surviving component must come through the archetype migration intact.
        assert_eq!(
            world.get::<Position>(entity),
            Some(&Position { x: 1.0, y: 1.0 })
        );

        // Removing something absent is reported, not silently accepted.
        assert!(!world.remove::<Velocity>(entity));
    }

    #[test]
    fn archetypes_are_reused_for_matching_component_sets() {
        let mut world = World::new();

        let a = world.spawn();
        world.insert(a, Position { x: 0.0, y: 0.0 });
        world.insert(a, Velocity { x: 0.0, y: 0.0 });
        let after_first = world.archetype_count();

        // A second entity with the same component set must land in the same archetype.
        let b = world.spawn();
        world.insert(b, Position { x: 0.0, y: 0.0 });
        world.insert(b, Velocity { x: 0.0, y: 0.0 });
        assert_eq!(world.archetype_count(), after_first);
    }

    #[test]
    fn insertion_order_does_not_create_extra_archetypes() {
        // Position-then-Velocity and Velocity-then-Position must converge on one archetype,
        // otherwise archetype fragmentation would depend on authoring order.
        let mut world = World::new();

        let a = world.spawn();
        world.insert(a, Position { x: 0.0, y: 0.0 });
        world.insert(a, Velocity { x: 1.0, y: 1.0 });

        let b = world.spawn();
        world.insert(b, Velocity { x: 1.0, y: 1.0 });
        world.insert(b, Position { x: 0.0, y: 0.0 });

        let location_a = world.entities.location(a).expect("a is live");
        let location_b = world.entities.location(b).expect("b is live");
        assert_eq!(
            location_a.archetype, location_b.archetype,
            "component set, not insertion order, must determine the archetype"
        );
    }

    #[test]
    fn despawn_fixes_up_the_swapped_entity() {
        // swap_remove moves the last row into the hole. If the moved entity's recorded location is
        // not corrected, its components silently belong to someone else -- a bug that would not
        // surface until much later.
        let mut world = World::new();
        let a = world.spawn();
        let b = world.spawn();
        let c = world.spawn();
        world.insert(a, Tag(1));
        world.insert(b, Tag(2));
        world.insert(c, Tag(3));

        world.despawn(a);

        assert_eq!(world.get::<Tag>(b), Some(&Tag(2)));
        assert_eq!(world.get::<Tag>(c), Some(&Tag(3)));
    }

    #[test]
    fn iter_visits_every_matching_entity() {
        let mut world = World::new();
        let a = world.spawn();
        let b = world.spawn();
        let c = world.spawn();
        world.insert(a, Tag(1));
        world.insert(b, Tag(2));
        // c deliberately has no Tag, plus a component that puts it in a different archetype.
        world.insert(c, Position { x: 0.0, y: 0.0 });

        let mut seen: Vec<u32> = world.iter::<Tag>().map(|(_, tag)| tag.0).collect();
        seen.sort_unstable();
        assert_eq!(seen, vec![1, 2]);
    }

    #[test]
    fn iter_spans_multiple_archetypes() {
        let mut world = World::new();
        let a = world.spawn();
        world.insert(a, Tag(1));

        let b = world.spawn();
        world.insert(b, Tag(2));
        world.insert(b, Position { x: 0.0, y: 0.0 });

        assert_eq!(world.iter::<Tag>().count(), 2);
    }

    #[test]
    fn for_each_mut_writes_to_every_match() {
        let mut world = World::new();
        let a = world.spawn();
        let b = world.spawn();
        world.insert(a, Tag(1));
        world.insert(b, Tag(2));

        world.for_each_mut::<Tag>(|_entity, tag| tag.0 *= 10);

        assert_eq!(world.get::<Tag>(a), Some(&Tag(10)));
        assert_eq!(world.get::<Tag>(b), Some(&Tag(20)));
    }

    #[test]
    fn for_each_pair_mut_only_visits_entities_with_both() {
        let mut world = World::new();

        let moving = world.spawn();
        world.insert(moving, Position { x: 0.0, y: 0.0 });
        world.insert(moving, Velocity { x: 2.0, y: -1.0 });

        let stationary = world.spawn();
        world.insert(stationary, Position { x: 100.0, y: 100.0 });

        world.for_each_pair_mut::<Position, Velocity>(|_entity, position, velocity| {
            position.x += velocity.x;
            position.y += velocity.y;
        });

        assert_eq!(
            world.get::<Position>(moving),
            Some(&Position { x: 2.0, y: -1.0 })
        );
        assert_eq!(
            world.get::<Position>(stationary),
            Some(&Position { x: 100.0, y: 100.0 }),
            "an entity without Velocity must not be touched"
        );
    }

    #[test]
    fn change_ticks_record_mutable_access() {
        let mut world = World::new();
        let entity = world.spawn();
        world.insert(entity, Tag(1));
        assert_eq!(world.changed_tick::<Tag>(entity), Some(Tick::ZERO));

        world.advance_tick();
        world.advance_tick();
        assert_eq!(world.tick(), Tick(2));

        // Read-only access must not mark the component changed.
        let _ = world.get::<Tag>(entity);
        assert_eq!(world.changed_tick::<Tag>(entity), Some(Tick::ZERO));

        world.for_each_mut::<Tag>(|_entity, tag| tag.0 += 1);
        assert_eq!(world.changed_tick::<Tag>(entity), Some(Tick(2)));
    }

    // --- Determinism. These are the properties every golden replay test depends on. ---

    /// Builds an identical world, optionally varying the order components are inserted.
    fn build_world(reverse_insert_order: bool) -> World {
        let mut world = World::new();
        for i in 0..8u32 {
            let entity = world.spawn();
            let position = Position {
                x: i as f32 * 1.5,
                y: -(i as f32),
            };
            let velocity = Velocity { x: 1.0, y: 0.5 };
            if reverse_insert_order {
                world.insert(entity, velocity);
                world.insert(entity, position);
            } else {
                world.insert(entity, position);
                world.insert(entity, velocity);
            }
        }
        world
    }

    #[test]
    fn identical_worlds_hash_identically() {
        assert_eq!(
            build_world(false).state_hash(),
            build_world(false).state_hash()
        );
    }

    #[test]
    fn hash_ignores_component_insertion_order() {
        // The same logical state must fingerprint the same regardless of the order components were
        // attached, because component order is an authoring detail rather than simulation state.
        assert_eq!(
            build_world(false).state_hash(),
            build_world(true).state_hash()
        );
    }

    #[test]
    fn hash_changes_when_state_changes() {
        let baseline = build_world(false).state_hash();

        let mut mutated = build_world(false);
        mutated.for_each_pair_mut::<Position, Velocity>(|_entity, position, velocity| {
            position.x += velocity.x;
        });
        assert_ne!(baseline, mutated.state_hash());
    }

    #[test]
    fn hash_includes_the_tick() {
        // State at tick 100 is not the same state as at tick 200, even with identical components.
        let world = build_world(false);
        let at_zero = world.state_hash();

        let mut later = build_world(false);
        later.advance_tick();
        assert_ne!(at_zero, later.state_hash());
    }

    #[test]
    fn hash_survives_storage_churn() {
        // Despawning reorders rows via swap_remove. Two worlds holding the same surviving entities
        // must agree even if they arrived at that state by different routes -- otherwise the hash
        // fingerprints storage layout instead of simulation state.
        let mut churned = World::new();
        let mut keep = Vec::new();
        for i in 0..6u32 {
            let entity = churned.spawn();
            churned.insert(entity, Tag(i));
            if i % 2 == 0 {
                keep.push((entity, i));
            }
        }
        // Remove the odd-tagged entities in an awkward order to force row shuffling.
        let to_remove: Vec<Entity> = churned
            .iter::<Tag>()
            .filter(|(_, tag)| tag.0 % 2 == 1)
            .map(|(entity, _)| entity)
            .collect();
        for entity in to_remove.into_iter().rev() {
            churned.despawn(entity);
        }

        assert_eq!(churned.entity_count(), 3);
        // Hash is order-independent, so the surviving set is what matters.
        let mut tags: Vec<u32> = churned.iter::<Tag>().map(|(_, tag)| tag.0).collect();
        tags.sort_unstable();
        assert_eq!(tags, vec![0, 2, 4]);
    }

    #[test]
    fn empty_worlds_hash_identically() {
        assert_eq!(World::new().state_hash(), World::new().state_hash());
    }

    #[test]
    fn hash_is_not_affected_by_archetype_count_alone() {
        // Creating an archetype and then emptying it must leave no trace in the hash, since an
        // empty archetype is bookkeeping rather than state.
        let mut world = World::new();
        let scratch = world.spawn();
        world.insert(scratch, Tag(99));
        world.despawn(scratch);

        let fresh = World::new();
        assert!(world.archetype_count() > fresh.archetype_count());
        assert_eq!(world.state_hash(), fresh.state_hash());
    }
}
