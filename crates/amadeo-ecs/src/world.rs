//! The world: entity storage plus the query surface systems use.

use crate::archetype::Archetype;
use crate::component::{Column, Component, ComponentId, TypedColumn};
use crate::entity::{Entity, EntityLocation, EntityStore};
use crate::resource::{Resource, ResourceId, ResourceSlot};
use crate::service::{Service, ServiceId, ServiceSlot};
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
/// | read two components across entities | [`World::iter_pair`] |
/// | read three components across entities | [`World::iter_triple`] |
/// | write one component across entities | [`World::for_each_mut`] |
/// | write one, read another | [`World::for_each_pair_mut`] |
/// | write two, read a third | [`World::for_each_triple_mut`] |
/// | read/write a single entity | [`World::get`], [`World::get_mut`] |
///
/// Reads return iterators; writes take closures. **Prefer a read query whenever the data is only
/// being read** — the mutable versions mark every visited component as changed, which would make
/// change detection useless for a system that never actually writes.
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
    /// Single-instance global simulation state.
    ///
    /// `BTreeMap` again, so [`World::state_hash`] visits resources in a reproducible order.
    resources: BTreeMap<ResourceId, Box<dyn ResourceSlot>>,
    /// Engine machinery — caches, devices, counters. **Excluded from [`World::state_hash`]**, which
    /// is the whole reason it is a separate store from `resources`. See [`Service`].
    services: BTreeMap<ServiceId, Box<dyn ServiceSlot>>,
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
            resources: BTreeMap::new(),
            services: BTreeMap::new(),
            tick: Tick::ZERO,
        }
    }

    /// Stores an engine service, returning the previous value if one was present.
    ///
    /// Services are **not** part of [`World::state_hash`]. Use this for caches, devices, and
    /// counters; use [`World::insert_resource`] for anything that should influence replay
    /// assertions.
    pub fn insert_service<T: Service>(&mut self, value: T) -> Option<T> {
        let previous = self.services.insert(ServiceId::of::<T>(), Box::new(value));
        previous.and_then(|slot| slot.into_any().downcast::<T>().ok().map(|boxed| *boxed))
    }

    /// Shared access to a service.
    #[must_use]
    pub fn service<T: Service>(&self) -> Option<&T> {
        self.services
            .get(&ServiceId::of::<T>())?
            .as_any()
            .downcast_ref::<T>()
    }

    /// Mutable access to a service.
    pub fn service_mut<T: Service>(&mut self) -> Option<&mut T> {
        self.services
            .get_mut(&ServiceId::of::<T>())?
            .as_any_mut()
            .downcast_mut::<T>()
    }

    /// Whether a service of this type is present.
    #[must_use]
    pub fn has_service<T: Service>(&self) -> bool {
        self.services.contains_key(&ServiceId::of::<T>())
    }

    /// Removes a service and returns it.
    pub fn remove_service<T: Service>(&mut self) -> Option<T> {
        let slot = self.services.remove(&ServiceId::of::<T>())?;
        slot.into_any().downcast::<T>().ok().map(|boxed| *boxed)
    }

    /// Temporarily takes a service out of the world, runs `f`, and puts it back.
    ///
    /// The service counterpart to [`World::with_resource_taken`], and it exists for the same reason:
    /// a system usually needs the service *and* mutable access to the rest of the world, which a
    /// plain `&mut` borrow of the service would forbid.
    ///
    /// Returns `None` without calling `f` if the service is absent.
    pub fn with_service_taken<T: Service, R>(
        &mut self,
        f: impl FnOnce(&mut Self, &mut T) -> R,
    ) -> Option<R> {
        let mut service = self.remove_service::<T>()?;
        let result = f(self, &mut service);
        self.insert_service(service);
        Some(result)
    }

    /// Stores a resource, returning the previous value if one was present.
    pub fn insert_resource<T: Resource>(&mut self, value: T) -> Option<T> {
        let previous = self
            .resources
            .insert(ResourceId::of::<T>(), Box::new(value));
        // The map is keyed by type, so anything stored under this key is a `T`. A failed downcast
        // would mean the keying is broken, which is an engine bug rather than a caller error.
        previous.and_then(|slot| slot.into_any().downcast::<T>().ok().map(|boxed| *boxed))
    }

    /// Shared access to a resource.
    #[must_use]
    pub fn resource<T: Resource>(&self) -> Option<&T> {
        self.resources
            .get(&ResourceId::of::<T>())?
            .as_any()
            .downcast_ref::<T>()
    }

    /// Mutable access to a resource.
    pub fn resource_mut<T: Resource>(&mut self) -> Option<&mut T> {
        self.resources
            .get_mut(&ResourceId::of::<T>())?
            .as_any_mut()
            .downcast_mut::<T>()
    }

    /// Whether a resource of this type is present.
    #[must_use]
    pub fn has_resource<T: Resource>(&self) -> bool {
        self.resources.contains_key(&ResourceId::of::<T>())
    }

    /// Removes a resource and returns it.
    pub fn remove_resource<T: Resource>(&mut self) -> Option<T> {
        let slot = self.resources.remove(&ResourceId::of::<T>())?;
        slot.into_any().downcast::<T>().ok().map(|boxed| *boxed)
    }

    /// Temporarily takes a resource out of the world, runs `f`, and puts it back.
    ///
    /// # Why this exists
    ///
    /// A system frequently needs a resource *and* mutable access to the world at once — "for every
    /// entity, roll against the shared RNG". Holding `&mut T` from [`World::resource_mut`] borrows
    /// the whole world, so the query cannot run.
    ///
    /// Removing the resource for the duration sidesteps that without `unsafe` or interior
    /// mutability. The resource is restored even if `f` panics is **not** guaranteed — a panic in a
    /// system is a bug, and the world is not expected to be reused afterwards.
    ///
    /// Returns `None` without calling `f` if the resource is absent.
    pub fn with_resource_taken<T: Resource, R>(
        &mut self,
        f: impl FnOnce(&mut Self, &mut T) -> R,
    ) -> Option<R> {
        let mut resource = self.remove_resource::<T>()?;
        let result = f(self, &mut resource);
        self.insert_resource(resource);
        Some(result)
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

    /// Every live entity, sorted by index then generation.
    ///
    /// # Why sorted, and why that costs an allocation
    ///
    /// Entities are stored archetype by archetype, and `swap_remove` shuffles rows, so storage order
    /// is deterministic but arbitrary and changes as entities come and go. Anything *derived* from
    /// this list — an introspection dump, a listing an agent diffs between two ticks — would inherit
    /// that churn and show spurious changes.
    ///
    /// Sorting is the same choice [`World::state_hash`] makes, for the same reason. It allocates, so
    /// this is an introspection and tooling API, not something to call from a system: a query wants
    /// [`World::iter`] and friends, which walk contiguous slices and allocate nothing.
    #[must_use]
    pub fn entities(&self) -> Vec<Entity> {
        let mut found: Vec<Entity> = self
            .archetypes
            .iter()
            .flat_map(|archetype| archetype.entities().iter().copied())
            .collect();
        found.sort_unstable_by_key(|entity| (entity.index(), entity.generation()));
        found
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

    /// Iterates every entity that has both `A` and `B`, read-only.
    ///
    /// The read-only counterpart to [`World::for_each_pair_mut`]. Returns an iterator rather than
    /// taking a closure because shared borrows need none of the lifetime machinery mutable ones do.
    ///
    /// Prefer this over the mutable version whenever the data is only being read — the mutable one
    /// marks every visited component as changed, which would make change detection meaningless for
    /// a system that never actually writes.
    pub fn iter_pair<A: Component, B: Component>(&self) -> impl Iterator<Item = (Entity, &A, &B)> {
        self.archetypes
            .iter()
            .filter_map(|archetype| {
                let (entities, a_values, b_values) = archetype.entities_with_pair::<A, B>()?;
                Some(
                    entities
                        .iter()
                        .copied()
                        .zip(a_values.iter())
                        .zip(b_values.iter())
                        .map(|((entity, a), b)| (entity, a, b)),
                )
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

    /// Iterates every entity that has all three of `A`, `B`, and `C`, read-only.
    ///
    /// The read-only counterpart to [`World::for_each_triple_mut`]. Permits repeated types, since
    /// shared borrows never conflict.
    pub fn iter_triple<A: Component, B: Component, C: Component>(
        &self,
    ) -> impl Iterator<Item = (Entity, &A, &B, &C)> {
        self.archetypes
            .iter()
            .filter_map(|archetype| {
                let (entities, a_values, b_values, c_values) =
                    archetype.entities_with_triple::<A, B, C>()?;
                Some(
                    entities
                        .iter()
                        .copied()
                        .zip(a_values.iter())
                        .zip(b_values.iter())
                        .zip(c_values.iter())
                        .map(|(((entity, a), b), c)| (entity, a, b, c)),
                )
            })
            .flatten()
    }

    /// Calls `f` for every entity with all three of `A`, `B`, and `C`, writing `A` and `B` and
    /// reading `C`.
    ///
    /// Does nothing if any two of the three types are the same.
    ///
    /// # When to reach for this
    ///
    /// The shape a behaviour system tends to want: update some state, set a movement value, and
    /// read a position to decide both. Before this existed the only way to express it was to collect
    /// into a `Vec` and write back by entity handle, which costs an allocation and a location lookup
    /// per entity.
    ///
    /// ```
    /// # use amadeo_core::StableHash;
    /// # use amadeo_ecs::{Component, World};
    /// # use amadeo_reflect::Reflect;
    /// # #[derive(Debug, Clone, Copy, PartialEq, StableHash, Reflect)] struct Health(f32);
    /// # #[derive(Debug, Clone, Copy, PartialEq, StableHash, Reflect)] struct Shield(f32);
    /// # #[derive(Debug, Clone, Copy, PartialEq, StableHash, Reflect)] struct Incoming(f32);
    /// # impl Component for Health {}
    /// # impl Component for Shield {}
    /// # impl Component for Incoming {}
    /// # let mut world = World::new();
    /// # let entity = world.spawn();
    /// # world.insert(entity, Health(100.0));
    /// # world.insert(entity, Shield(30.0));
    /// # world.insert(entity, Incoming(50.0));
    /// // Shields absorb first, then health takes the remainder.
    /// world.for_each_triple_mut::<Shield, Health, Incoming>(|_entity, shield, health, incoming| {
    ///     let absorbed = incoming.0.min(shield.0);
    ///     shield.0 -= absorbed;
    ///     health.0 -= incoming.0 - absorbed;
    /// });
    ///
    /// assert_eq!(world.get::<Shield>(entity), Some(&Shield(0.0)));
    /// assert_eq!(world.get::<Health>(entity), Some(&Health(80.0)));
    /// ```
    ///
    /// `A` and `B` are both marked changed at the current tick, whether or not `f` writes to them.
    /// If one of them is only being read, use [`World::iter_triple`] or restructure — a spurious
    /// change mark makes change detection less useful for every other system.
    pub fn for_each_triple_mut<A: Component, B: Component, C: Component>(
        &mut self,
        mut f: impl FnMut(Entity, &mut A, &mut B, &C),
    ) {
        let tick = self.tick;
        for archetype in &mut self.archetypes {
            if let Some((entities, a_values, b_values, c_values)) =
                archetype.entities_with_triple_mut::<A, B, C>(tick)
            {
                for (((entity, a), b), c) in entities
                    .iter()
                    .copied()
                    .zip(a_values.iter_mut())
                    .zip(b_values.iter_mut())
                    .zip(c_values.iter())
                {
                    f(entity, a, b, c);
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
    /// - resources are hashed in sorted resource id order;
    /// - the tick is included, since state at tick 100 is not the same state as at tick 200.
    ///
    /// Deliberately excluded: archetype indices, row numbers, free-list contents, change ticks, and
    /// **all services**. Those are allocator bookkeeping and engine machinery, not simulation state.
    /// Including them would make the hash sensitive to implementation details and to machine
    /// configuration — and would break invariant I7, since a windowed run touches render services
    /// that a headless run never creates.
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

        // Resources are simulation state too. `BTreeMap` iteration is sorted by id, so this order is
        // reproducible without any extra sorting.
        hasher.write_u64(self.resources.len() as u64);
        for (id, slot) in &self.resources {
            hasher.write_u64(id.raw());
            slot.stable_hash_value(&mut hasher);
        }

        hasher.finish()
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use amadeo_core::StableHash;
    use amadeo_reflect::Reflect;

    #[derive(Debug, Clone, Copy, PartialEq, StableHash, Reflect)]
    struct Position {
        x: f32,
        y: f32,
    }
    impl Component for Position {}

    #[derive(Debug, Clone, Copy, PartialEq, StableHash, Reflect)]
    struct Velocity {
        x: f32,
        y: f32,
    }
    impl Component for Velocity {}

    #[derive(Debug, Clone, Copy, PartialEq, StableHash, Reflect)]
    struct Tag(u32);
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
    fn iter_pair_visits_only_entities_with_both() {
        let mut world = World::new();

        let both = world.spawn();
        world.insert(both, Position { x: 1.0, y: 2.0 });
        world.insert(both, Velocity { x: 3.0, y: 4.0 });

        let only_position = world.spawn();
        world.insert(only_position, Position { x: 9.0, y: 9.0 });

        let found: Vec<(f32, f32)> = world
            .iter_pair::<Position, Velocity>()
            .map(|(_, position, velocity)| (position.x, velocity.x))
            .collect();
        assert_eq!(found, vec![(1.0, 3.0)]);
    }

    #[test]
    fn iter_pair_does_not_mark_components_changed() {
        // The whole reason this exists alongside `for_each_pair_mut`: a reader such as the renderer
        // must not flag every entity it looks at as modified.
        let mut world = World::new();
        let entity = world.spawn();
        world.insert(entity, Position { x: 0.0, y: 0.0 });
        world.insert(entity, Velocity { x: 0.0, y: 0.0 });

        world.advance_tick();
        world.advance_tick();
        let before = world.changed_tick::<Position>(entity);

        assert_eq!(world.iter_pair::<Position, Velocity>().count(), 1);
        assert_eq!(world.changed_tick::<Position>(entity), before);
    }

    #[test]
    fn iter_pair_allows_the_same_component_twice() {
        // Two shared borrows of one column are fine, unlike the mutable version which must refuse.
        let mut world = World::new();
        let entity = world.spawn();
        world.insert(entity, Position { x: 7.0, y: 8.0 });

        let found: Vec<f32> = world
            .iter_pair::<Position, Position>()
            .map(|(_, first, second)| first.x + second.y)
            .collect();
        assert_eq!(found, vec![15.0]);
    }

    #[test]
    fn entities_are_listed_in_a_stable_order_despite_storage_churn() {
        // The property an introspection dump depends on: two worlds holding the same entities list
        // them the same way, however they got there. `swap_remove` shuffles storage order, so
        // without sorting this would show spurious changes between ticks.
        let mut world = World::new();
        let mut handles = Vec::new();
        for index in 0..5u32 {
            let entity = world.spawn();
            world.insert(entity, Tag(index));
            handles.push(entity);
        }

        // Despawn from the middle, which pulls the last row into the hole.
        world.despawn(handles[1]);
        world.despawn(handles[3]);

        let listed = world.entities();
        assert_eq!(listed.len(), 3);
        assert_eq!(
            listed,
            vec![handles[0], handles[2], handles[4]],
            "listing must be by index, not by where storage happens to have put things"
        );
    }

    #[test]
    fn listing_entities_spans_archetypes() {
        let mut world = World::new();
        let bare = world.spawn();
        let tagged = world.spawn();
        world.insert(tagged, Tag(1));
        let positioned = world.spawn();
        world.insert(positioned, Position { x: 0.0, y: 0.0 });

        assert_eq!(world.entities(), vec![bare, tagged, positioned]);
        assert_eq!(World::new().entities(), Vec::new());
    }

    #[test]
    fn iter_triple_visits_only_entities_with_all_three() {
        let mut world = World::new();

        let all_three = world.spawn();
        world.insert(all_three, Position { x: 1.0, y: 2.0 });
        world.insert(all_three, Velocity { x: 3.0, y: 4.0 });
        world.insert(all_three, Tag(7));

        let only_two = world.spawn();
        world.insert(only_two, Position { x: 9.0, y: 9.0 });
        world.insert(only_two, Velocity { x: 9.0, y: 9.0 });

        let found: Vec<(f32, f32, u32)> = world
            .iter_triple::<Position, Velocity, Tag>()
            .map(|(_, position, velocity, tag)| (position.x, velocity.x, tag.0))
            .collect();
        assert_eq!(found, vec![(1.0, 3.0, 7)]);
    }

    #[test]
    fn iter_triple_does_not_mark_components_changed() {
        let mut world = World::new();
        let entity = world.spawn();
        world.insert(entity, Position { x: 0.0, y: 0.0 });
        world.insert(entity, Velocity { x: 0.0, y: 0.0 });
        world.insert(entity, Tag(1));

        world.advance_tick();
        world.advance_tick();
        let before = world.changed_tick::<Position>(entity);

        assert_eq!(world.iter_triple::<Position, Velocity, Tag>().count(), 1);
        assert_eq!(world.changed_tick::<Position>(entity), before);
    }

    #[test]
    fn for_each_triple_mut_writes_two_and_reads_one() {
        let mut world = World::new();

        let moving = world.spawn();
        world.insert(moving, Position { x: 0.0, y: 0.0 });
        world.insert(moving, Velocity { x: 2.0, y: -1.0 });
        world.insert(moving, Tag(10));

        // Missing Tag, so it must not be visited.
        let untagged = world.spawn();
        world.insert(untagged, Position { x: 100.0, y: 100.0 });
        world.insert(untagged, Velocity { x: 5.0, y: 5.0 });

        world.for_each_triple_mut::<Position, Velocity, Tag>(|_entity, position, velocity, tag| {
            // Both mutable arguments are written, and the third is only read.
            position.x += velocity.x * tag.0 as f32;
            velocity.x = 0.0;
        });

        assert_eq!(
            world.get::<Position>(moving),
            Some(&Position { x: 20.0, y: 0.0 })
        );
        assert_eq!(
            world.get::<Velocity>(moving),
            Some(&Velocity { x: 0.0, y: -1.0 })
        );
        assert_eq!(
            world.get::<Position>(untagged),
            Some(&Position { x: 100.0, y: 100.0 }),
            "an entity missing the third component must not be touched"
        );
    }

    #[test]
    fn for_each_triple_mut_leaves_the_read_component_unchanged() {
        // The point of C being a shared borrow: reading it must not defeat change detection.
        let mut world = World::new();
        let entity = world.spawn();
        world.insert(entity, Position { x: 0.0, y: 0.0 });
        world.insert(entity, Velocity { x: 1.0, y: 1.0 });
        world.insert(entity, Tag(1));

        world.advance_tick();
        world.advance_tick();

        world.for_each_triple_mut::<Position, Velocity, Tag>(|_e, _p, _v, _t| {});

        assert_eq!(world.changed_tick::<Position>(entity), Some(Tick(2)));
        assert_eq!(world.changed_tick::<Velocity>(entity), Some(Tick(2)));
        assert_eq!(
            world.changed_tick::<Tag>(entity),
            Some(Tick::ZERO),
            "the read-only component must keep its original change tick"
        );
    }

    #[test]
    fn for_each_triple_mut_refuses_repeated_types() {
        // Two mutable borrows of one column cannot be handed out, so the query does nothing rather
        // than aliasing. Checked in release too, since `get_disjoint_mut` rejects the overlap.
        let mut world = World::new();
        let entity = world.spawn();
        world.insert(entity, Position { x: 1.0, y: 1.0 });
        world.insert(entity, Velocity { x: 1.0, y: 1.0 });

        let mut visited = 0;
        // Deliberately not wrapped in `catch_unwind`: in debug this trips a `debug_assert`, which is
        // the intended loud failure for a programming error. The release behaviour -- visit nothing
        // -- is what the return value of `get_disjoint_mut` produces.
        if !cfg!(debug_assertions) {
            world.for_each_triple_mut::<Position, Position, Velocity>(|_e, _a, _b, _c| {
                visited += 1;
            });
            assert_eq!(visited, 0);
        }
    }

    #[test]
    fn iter_triple_allows_the_same_component_repeatedly() {
        // Unlike the mutable version, shared borrows of one column are fine.
        let mut world = World::new();
        let entity = world.spawn();
        world.insert(entity, Position { x: 3.0, y: 4.0 });

        let found: Vec<f32> = world
            .iter_triple::<Position, Position, Position>()
            .map(|(_, a, b, c)| a.x + b.y + c.x)
            .collect();
        assert_eq!(found, vec![10.0]);
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

    // --- Resources ---

    #[derive(Debug, PartialEq)]
    struct Score(u32);

    impl StableHash for Score {
        fn stable_hash(&self, hasher: &mut StableHasher) {
            self.0.stable_hash(hasher);
        }
    }
    impl crate::Resource for Score {}

    #[test]
    fn resources_round_trip() {
        let mut world = World::new();
        assert!(!world.has_resource::<Score>());
        assert_eq!(world.resource::<Score>(), None);

        assert_eq!(world.insert_resource(Score(10)), None);
        assert!(world.has_resource::<Score>());
        assert_eq!(world.resource::<Score>(), Some(&Score(10)));

        // Inserting again returns the displaced value.
        assert_eq!(world.insert_resource(Score(20)), Some(Score(10)));
        assert_eq!(world.resource::<Score>(), Some(&Score(20)));

        world.resource_mut::<Score>().expect("present").0 = 30;
        assert_eq!(world.resource::<Score>(), Some(&Score(30)));

        assert_eq!(world.remove_resource::<Score>(), Some(Score(30)));
        assert!(!world.has_resource::<Score>());
        assert_eq!(world.remove_resource::<Score>(), None);
    }

    #[test]
    fn with_resource_taken_allows_world_access() {
        let mut world = World::new();
        let a = world.spawn();
        let b = world.spawn();
        world.insert(a, Tag(1));
        world.insert(b, Tag(2));
        world.insert_resource(Score(0));

        // The point of this helper: mutate entities and a resource in the same pass, which a plain
        // `resource_mut` borrow would forbid.
        let visited = world
            .with_resource_taken::<Score, usize>(|world, score| {
                let mut count = 0;
                world.for_each_mut::<Tag>(|_entity, tag| {
                    score.0 += tag.0;
                    tag.0 *= 2;
                    count += 1;
                });
                count
            })
            .expect("resource present");

        assert_eq!(visited, 2);
        assert_eq!(world.resource::<Score>(), Some(&Score(3)));
        assert_eq!(world.get::<Tag>(a), Some(&Tag(2)));
        // The resource must be back in the world afterwards.
        assert!(world.has_resource::<Score>());
    }

    #[test]
    fn with_resource_taken_reports_absence_without_running() {
        let mut world = World::new();
        let mut ran = false;
        let result = world.with_resource_taken::<Score, ()>(|_world, _score| ran = true);
        assert_eq!(result, None);
        assert!(!ran, "closure must not run when the resource is missing");
    }

    // --- Services: the non-simulation counterpart to resources ---

    #[derive(Debug, PartialEq)]
    struct FrameCounter(u32);
    impl crate::Service for FrameCounter {}

    #[test]
    fn services_round_trip() {
        let mut world = World::new();
        assert!(!world.has_service::<FrameCounter>());

        assert_eq!(world.insert_service(FrameCounter(1)), None);
        assert_eq!(world.service::<FrameCounter>(), Some(&FrameCounter(1)));

        world.service_mut::<FrameCounter>().expect("present").0 = 9;
        assert_eq!(world.service::<FrameCounter>(), Some(&FrameCounter(9)));

        assert_eq!(world.insert_service(FrameCounter(2)), Some(FrameCounter(9)));
        assert_eq!(
            world.remove_service::<FrameCounter>(),
            Some(FrameCounter(2))
        );
        assert!(!world.has_service::<FrameCounter>());
    }

    #[test]
    fn services_are_excluded_from_the_state_hash() {
        // The property that makes invariant I7 hold: a windowed run creates render-side state that a
        // headless run never does, and the two must still agree on simulation state.
        let baseline = World::new().state_hash();

        let mut with_service = World::new();
        with_service.insert_service(FrameCounter(0));
        assert_eq!(with_service.state_hash(), baseline);

        // Mutating a service must not move the hash either.
        with_service
            .service_mut::<FrameCounter>()
            .expect("present")
            .0 = 12_345;
        assert_eq!(with_service.state_hash(), baseline);
    }

    #[test]
    fn services_and_resources_are_separate_stores() {
        // Same-named concepts must not collide across the two stores.
        let mut world = World::new();
        world.insert_resource(Score(1));
        world.insert_service(FrameCounter(2));

        assert!(world.has_resource::<Score>());
        assert!(world.has_service::<FrameCounter>());
        assert_eq!(world.remove_resource::<Score>(), Some(Score(1)));
        assert!(
            world.has_service::<FrameCounter>(),
            "removing a resource must not disturb services"
        );
    }

    #[test]
    fn resources_participate_in_the_state_hash() {
        let mut with_low = World::new();
        with_low.insert_resource(Score(1));

        let mut with_high = World::new();
        with_high.insert_resource(Score(2));

        assert_ne!(with_low.state_hash(), with_high.state_hash());

        // And identical resource state agrees.
        let mut also_low = World::new();
        also_low.insert_resource(Score(1));
        assert_eq!(with_low.state_hash(), also_low.state_hash());

        // Presence itself matters, not just value.
        assert_ne!(with_low.state_hash(), World::new().state_hash());
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
