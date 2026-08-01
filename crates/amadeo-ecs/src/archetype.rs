//! Archetypes: the tables that hold entities sharing an identical component set.

use crate::component::{Column, Component, ComponentId, TypedColumn};
use crate::entity::Entity;
use amadeo_core::{StableHasher, Tick};

/// One archetype's rows for a read-only three-component query: entities, then each column's values.
///
/// A named alias purely because the bare tuple trips clippy's `type_complexity`. It carries no
/// meaning the tuple does not.
type TripleRead<'a, A, B, C> = (&'a [Entity], &'a [A], &'a [B], &'a [C]);

/// One archetype's rows for a three-component query that writes `A` and `B` and reads `C`.
type TripleMut<'a, A, B, C> = (&'a [Entity], &'a mut [A], &'a mut [B], &'a [C]);

/// A table of entities that all have exactly the same set of component types.
///
/// Columns are stored in a `Vec` parallel to `component_ids`, and `component_ids` is kept sorted.
/// Two consequences, both deliberate:
///
/// - Lookup is a binary search rather than a hash, so **iteration order is identical on every build**
///   — a `HashMap` here would make state hashes build-dependent and break invariant I3.
/// - Columns live in a slice, which lets a query take disjoint mutable borrows of two different
///   columns via [`slice::get_disjoint_mut`]. That is what makes multi-component mutable queries
///   possible without `unsafe`.
#[derive(Debug)]
pub(crate) struct Archetype {
    /// The component types held here, sorted. Parallel to `columns`.
    component_ids: Vec<ComponentId>,
    /// One column per entry in `component_ids`.
    columns: Vec<Box<dyn Column>>,
    /// Which entity occupies each row. Parallel to every column.
    entities: Vec<Entity>,
}

impl Archetype {
    /// Creates an archetype from sorted ids and matching empty columns.
    pub(crate) fn new(component_ids: Vec<ComponentId>, columns: Vec<Box<dyn Column>>) -> Self {
        debug_assert!(
            component_ids.is_sorted(),
            "archetype component ids must be sorted, or binary search breaks"
        );
        debug_assert_eq!(
            component_ids.len(),
            columns.len(),
            "every component id needs exactly one column"
        );
        Self {
            component_ids,
            columns,
            entities: Vec::new(),
        }
    }

    /// The sorted component types held by this archetype.
    pub(crate) fn component_ids(&self) -> &[ComponentId] {
        &self.component_ids
    }

    /// How many entities are in this archetype.
    pub(crate) fn len(&self) -> usize {
        self.entities.len()
    }

    /// Asserts, in debug builds, that every column has exactly one value per entity.
    ///
    /// This table must stay rectangular. Every path that adds or removes a row touches the entity
    /// list and the columns separately, so a missed step leaves a column out of step with the rest —
    /// which would then surface much later as a wrong value or an out-of-bounds read rather than at
    /// the point of the mistake. Callers invoke this once a mutation is complete.
    pub(crate) fn debug_assert_rectangular(&self) {
        if cfg!(debug_assertions) {
            let rows = self.entities.len();
            for (id, column) in self.component_ids.iter().zip(self.columns.iter()) {
                debug_assert_eq!(
                    column.len(),
                    rows,
                    "column {id} holds {} values but the archetype has {rows} entities",
                    column.len()
                );
            }
        }
    }

    /// The entity in each row, in row order.
    pub(crate) fn entities(&self) -> &[Entity] {
        &self.entities
    }

    /// Finds the column slot for a component type, if this archetype has one.
    pub(crate) fn column_index(&self, id: ComponentId) -> Option<usize> {
        self.component_ids.binary_search(&id).ok()
    }

    /// Whether this archetype holds the given component type.
    pub(crate) fn has(&self, id: ComponentId) -> bool {
        self.column_index(id).is_some()
    }

    /// Adds a row for `entity` and returns its row index.
    ///
    /// The caller is responsible for pushing a value into every column so the table stays
    /// rectangular.
    pub(crate) fn push_entity(&mut self, entity: Entity) -> usize {
        let row = self.entities.len();
        self.entities.push(entity);
        row
    }

    /// Shared access to a typed column.
    pub(crate) fn column<T: Component>(&self) -> Option<&TypedColumn<T>> {
        let index = self.column_index(ComponentId::of::<T>())?;
        // The downcast happens once here, per archetype per query -- not per entity. That ratio is
        // the whole argument of ADR 0008.
        self.columns[index]
            .as_any()
            .downcast_ref::<TypedColumn<T>>()
    }

    /// Mutable access to a typed column.
    pub(crate) fn column_mut<T: Component>(&mut self) -> Option<&mut TypedColumn<T>> {
        let index = self.column_index(ComponentId::of::<T>())?;
        self.columns[index]
            .as_any_mut()
            .downcast_mut::<TypedColumn<T>>()
    }

    /// Appends a component value into the column for `T`.
    ///
    /// Returns `false` if this archetype has no column for `T`, which would leave the table ragged.
    pub(crate) fn push_component<T: Component>(&mut self, value: T, tick: Tick) -> bool {
        match self.column_mut::<T>() {
            Some(column) => {
                column.push(value, tick);
                true
            }
            None => false,
        }
    }

    /// Removes a row, swapping the last row into its place.
    ///
    /// Returns the entity that was moved into `row`, if any, so the caller can update its recorded
    /// location. Returns `None` when the removed row was the last one.
    pub(crate) fn swap_remove_row(&mut self, row: usize) -> Option<Entity> {
        if row >= self.entities.len() {
            return None;
        }
        for column in &mut self.columns {
            column.swap_remove(row);
        }
        self.entities.swap_remove(row);
        // If a row moved into the hole, it is now at `row` and its owner must be told.
        self.entities.get(row).copied()
    }

    /// Moves every component of `row` into `destination`, then removes the row here.
    ///
    /// Components present here but absent in `destination` are dropped, which is what happens when a
    /// component is removed from an entity. Components in `destination` that are absent here are
    /// left for the caller to fill — that is the component being added.
    ///
    /// Returns the entity swapped into `row`, as [`Archetype::swap_remove_row`] does.
    pub(crate) fn migrate_row_to(
        &mut self,
        row: usize,
        destination: &mut Archetype,
    ) -> Option<Entity> {
        if row >= self.entities.len() {
            return None;
        }

        for (index, id) in self.component_ids.iter().enumerate() {
            match destination.column_index(*id) {
                Some(destination_index) => {
                    // Split the borrow: source column from self, destination column from the other
                    // archetype. They are different objects, so this is straightforward.
                    let (source, target) = (
                        &mut self.columns[index],
                        &mut destination.columns[destination_index],
                    );
                    source.migrate_row_to(row, target.as_mut());
                }
                None => {
                    // Not wanted in the destination: drop it.
                    self.columns[index].swap_remove(row);
                }
            }
        }

        let entity = self.entities.swap_remove(row);
        destination.entities.push(entity);
        self.entities.get(row).copied()
    }

    /// Empty columns matching this archetype's types, for building a related archetype.
    ///
    /// Used when adding or removing a component: the new archetype needs columns of the same
    /// concrete types, and only the existing columns know what those types are.
    pub(crate) fn empty_columns_clone(&self) -> Vec<(ComponentId, Box<dyn Column>)> {
        self.component_ids
            .iter()
            .zip(self.columns.iter())
            .map(|(id, column)| (*id, column.empty_clone()))
            .collect()
    }

    /// Row entities alongside read-only access to two components' values.
    ///
    /// The read-only counterpart to [`Archetype::entities_with_pair_mut`]. Needed because a reader
    /// such as the renderer must not touch change ticks — marking every drawn entity as modified
    /// each frame would make change detection useless, and rendering is required to be read-only
    /// with respect to simulation (ADR 0005).
    ///
    /// Unlike the mutable version this needs no disjoint-borrow trick, since two shared borrows of
    /// the same slice are fine. It also permits `A` and `B` to be the same type.
    pub(crate) fn entities_with_pair<A: Component, B: Component>(
        &self,
    ) -> Option<(&[Entity], &[A], &[B])> {
        let column_a = self.column::<A>()?;
        let column_b = self.column::<B>()?;
        Some((&self.entities, column_a.values(), column_b.values()))
    }

    /// Row entities alongside mutable access to one component's values.
    ///
    /// Returned together because `entities` and `columns` are separate fields, so the borrow can be
    /// split here — whereas a caller holding `&mut Archetype` could not borrow both at once.
    pub(crate) fn entities_with_column_mut<T: Component>(
        &mut self,
        tick: Tick,
    ) -> Option<(&[Entity], &mut [T])> {
        let index = self.column_index(ComponentId::of::<T>())?;
        let column = self.columns[index]
            .as_any_mut()
            .downcast_mut::<TypedColumn<T>>()?;
        Some((&self.entities, column.values_mut(tick)))
    }

    /// Row entities, mutable access to `A`'s values, and shared access to `B`'s.
    ///
    /// The primitive behind two-component mutable queries. `None` if either column is absent or if
    /// `A` and `B` are the same type.
    pub(crate) fn entities_with_pair_mut<A: Component, B: Component>(
        &mut self,
        tick: Tick,
    ) -> Option<(&[Entity], &mut [A], &[B])> {
        let index_a = self.column_index(ComponentId::of::<A>())?;
        let index_b = self.column_index(ComponentId::of::<B>())?;
        if index_a == index_b {
            debug_assert!(
                false,
                "entities_with_pair_mut called with the same component type twice"
            );
            return None;
        }

        let [slot_a, slot_b] = self.columns.get_disjoint_mut([index_a, index_b]).ok()?;
        let column_a = slot_a.as_any_mut().downcast_mut::<TypedColumn<A>>()?;
        let column_b = slot_b.as_any().downcast_ref::<TypedColumn<B>>()?;

        Some((&self.entities, column_a.values_mut(tick), column_b.values()))
    }

    /// Row entities alongside read-only access to three components' values.
    ///
    /// Like [`Archetype::entities_with_pair`], needs no disjoint-borrow trick and permits repeated
    /// types, because shared borrows of the same slice do not conflict.
    pub(crate) fn entities_with_triple<A: Component, B: Component, C: Component>(
        &self,
    ) -> Option<TripleRead<'_, A, B, C>> {
        let column_a = self.column::<A>()?;
        let column_b = self.column::<B>()?;
        let column_c = self.column::<C>()?;
        Some((
            &self.entities,
            column_a.values(),
            column_b.values(),
            column_c.values(),
        ))
    }

    /// Row entities, mutable access to `A` and `B`'s values, and shared access to `C`'s.
    ///
    /// The primitive behind three-component mutable queries. `None` if any column is absent or if
    /// any two of the three types are the same.
    ///
    /// # Why two writes and one read
    ///
    /// This is the shape a real system needed — the Q1 spike's enemy AI writes its behaviour state
    /// and its velocity while reading its transform (ADR 0011). The write-one-read-two shape is not
    /// provided because nothing has needed it yet; adding query shapes on demand rather than
    /// speculatively is the policy this module already follows.
    pub(crate) fn entities_with_triple_mut<A: Component, B: Component, C: Component>(
        &mut self,
        tick: Tick,
    ) -> Option<TripleMut<'_, A, B, C>> {
        let index_a = self.column_index(ComponentId::of::<A>())?;
        let index_b = self.column_index(ComponentId::of::<B>())?;
        let index_c = self.column_index(ComponentId::of::<C>())?;
        if index_a == index_b || index_a == index_c || index_b == index_c {
            debug_assert!(
                false,
                "entities_with_triple_mut called with the same component type more than once"
            );
            return None;
        }

        // Three disjoint indices, checked at runtime. Same trick as the pair version -- this is what
        // makes a multi-component mutable query possible with no `unsafe`.
        let [slot_a, slot_b, slot_c] = self
            .columns
            .get_disjoint_mut([index_a, index_b, index_c])
            .ok()?;
        let column_a = slot_a.as_any_mut().downcast_mut::<TypedColumn<A>>()?;
        let column_b = slot_b.as_any_mut().downcast_mut::<TypedColumn<B>>()?;
        // `C` is read-only, so a shared downcast -- which is what keeps its change ticks untouched.
        let column_c = slot_c.as_any().downcast_ref::<TypedColumn<C>>()?;

        Some((
            &self.entities,
            column_a.values_mut(tick),
            column_b.values_mut(tick),
            column_c.values(),
        ))
    }

    /// Feeds one row's components into a state fingerprint, in sorted component order.
    ///
    /// Sorted order is what makes the hash reproducible: it does not depend on the order components
    /// happened to be inserted, or on any hash map's layout.
    pub(crate) fn stable_hash_row(&self, row: usize, hasher: &mut StableHasher) {
        for (index, id) in self.component_ids.iter().enumerate() {
            // Derived components contribute nothing -- not their value and not even their id
            // (ADR 0019). Skipping the id too matters: were it written, adding `GlobalTransform` to
            // an entity would still move the hash, which is exactly the coupling being avoided.
            if self.columns[index].is_derived() {
                continue;
            }
            hasher.write_u64(id.raw());
            self.columns[index].stable_hash_row(row, hasher);
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use amadeo_core::StableHash;
    use amadeo_reflect::Reflect;

    #[derive(Debug, Clone, Copy, PartialEq, StableHash, Reflect)]
    struct Position(f32);
    impl Component for Position {}

    #[derive(Debug, Clone, Copy, PartialEq, StableHash, Reflect)]
    struct Speed(f32);
    impl Component for Speed {}

    /// Builds an archetype holding Position and Speed, with ids sorted as the invariant requires.
    fn position_and_speed() -> Archetype {
        let mut pairs: Vec<(ComponentId, Box<dyn Column>)> = vec![
            (
                ComponentId::of::<Position>(),
                Box::new(TypedColumn::<Position>::new()),
            ),
            (
                ComponentId::of::<Speed>(),
                Box::new(TypedColumn::<Speed>::new()),
            ),
        ];
        pairs.sort_by_key(|(id, _)| *id);
        let (ids, columns): (Vec<_>, Vec<_>) = pairs.into_iter().unzip();
        Archetype::new(ids, columns)
    }

    fn add(archetype: &mut Archetype, entity: Entity, position: f32, speed: f32) {
        archetype.push_entity(entity);
        assert!(archetype.push_component(Position(position), Tick::ZERO));
        assert!(archetype.push_component(Speed(speed), Tick::ZERO));
    }

    #[test]
    fn reports_its_component_types() {
        let archetype = position_and_speed();
        assert!(archetype.has(ComponentId::of::<Position>()));
        assert!(archetype.has(ComponentId::of::<Speed>()));
        assert!(archetype.component_ids().is_sorted());
        assert_eq!(archetype.len(), 0);
        archetype.debug_assert_rectangular();
    }

    #[test]
    fn stores_and_reads_rows() {
        let mut archetype = position_and_speed();
        add(&mut archetype, Entity::new(0, 0), 1.0, 10.0);
        add(&mut archetype, Entity::new(1, 0), 2.0, 20.0);

        assert_eq!(archetype.len(), 2);
        let positions = archetype.column::<Position>().expect("has positions");
        assert_eq!(positions.values(), &[Position(1.0), Position(2.0)]);
    }

    #[test]
    fn pair_access_is_disjoint_and_writable() {
        let mut archetype = position_and_speed();
        add(&mut archetype, Entity::new(0, 0), 1.0, 10.0);
        add(&mut archetype, Entity::new(1, 0), 2.0, 20.0);

        let (entities, positions, speeds) = archetype
            .entities_with_pair_mut::<Position, Speed>(Tick(1))
            .expect("both columns present");
        assert_eq!(entities.len(), 2);

        // Write through one column while reading another -- the case a plain map lookup cannot
        // express, and the reason `get_disjoint_mut` is used.
        for (position, speed) in positions.iter_mut().zip(speeds.iter()) {
            position.0 += speed.0;
        }

        let positions = archetype.column::<Position>().expect("has positions");
        assert_eq!(positions.values(), &[Position(11.0), Position(22.0)]);
        archetype.debug_assert_rectangular();
    }

    #[test]
    fn triple_access_writes_two_columns_and_reads_a_third() {
        #[derive(Debug, Clone, Copy, PartialEq, StableHash, Reflect)]
        struct Mass(f32);
        impl Component for Mass {}

        let mut pairs: Vec<(ComponentId, Box<dyn Column>)> = vec![
            (
                ComponentId::of::<Position>(),
                Box::new(TypedColumn::<Position>::new()),
            ),
            (
                ComponentId::of::<Speed>(),
                Box::new(TypedColumn::<Speed>::new()),
            ),
            (
                ComponentId::of::<Mass>(),
                Box::new(TypedColumn::<Mass>::new()),
            ),
        ];
        pairs.sort_by_key(|(id, _)| *id);
        let (ids, columns): (Vec<_>, Vec<_>) = pairs.into_iter().unzip();
        let mut archetype = Archetype::new(ids, columns);

        archetype.push_entity(Entity::new(0, 0));
        assert!(archetype.push_component(Position(1.0), Tick::ZERO));
        assert!(archetype.push_component(Speed(10.0), Tick::ZERO));
        assert!(archetype.push_component(Mass(2.0), Tick::ZERO));

        let (entities, positions, speeds, masses) = archetype
            .entities_with_triple_mut::<Position, Speed, Mass>(Tick(1))
            .expect("all three columns present");
        assert_eq!(entities.len(), 1);

        // Two disjoint mutable borrows plus a shared one, from the same column slice.
        for ((position, speed), mass) in positions
            .iter_mut()
            .zip(speeds.iter_mut())
            .zip(masses.iter())
        {
            position.0 += speed.0 / mass.0;
            speed.0 = 0.0;
        }

        assert_eq!(
            archetype.column::<Position>().expect("present").values(),
            &[Position(6.0)]
        );
        assert_eq!(
            archetype.column::<Speed>().expect("present").values(),
            &[Speed(0.0)]
        );
        archetype.debug_assert_rectangular();
    }

    #[test]
    fn pair_access_refuses_a_missing_column() {
        #[derive(Debug, StableHash, Reflect)]
        struct Absent;
        impl Component for Absent {}

        let mut archetype = position_and_speed();
        assert!(
            archetype
                .entities_with_pair_mut::<Position, Absent>(Tick::ZERO)
                .is_none()
        );
    }

    #[test]
    fn swap_remove_reports_the_moved_entity() {
        let mut archetype = position_and_speed();
        let a = Entity::new(0, 0);
        let b = Entity::new(1, 0);
        let c = Entity::new(2, 0);
        add(&mut archetype, a, 1.0, 10.0);
        add(&mut archetype, b, 2.0, 20.0);
        add(&mut archetype, c, 3.0, 30.0);

        // Removing row 0 pulls the last row (c) into it.
        assert_eq!(archetype.swap_remove_row(0), Some(c));
        assert_eq!(archetype.len(), 2);
        assert_eq!(archetype.entities(), &[c, b]);
        let positions = archetype.column::<Position>().expect("has positions");
        assert_eq!(positions.values(), &[Position(3.0), Position(2.0)]);

        // Removing the final row moves nothing.
        assert_eq!(archetype.swap_remove_row(1), None);
    }

    #[test]
    fn swap_remove_out_of_range_is_ignored() {
        let mut archetype = position_and_speed();
        assert_eq!(archetype.swap_remove_row(0), None);
    }

    #[test]
    fn migrate_carries_shared_components_and_drops_the_rest() {
        // Destination has only Position, so Speed must be dropped in transit.
        let mut destination = Archetype::new(
            vec![ComponentId::of::<Position>()],
            vec![Box::new(TypedColumn::<Position>::new())],
        );

        let mut source = position_and_speed();
        let a = Entity::new(0, 0);
        let b = Entity::new(1, 0);
        add(&mut source, a, 1.0, 10.0);
        add(&mut source, b, 2.0, 20.0);

        assert_eq!(source.migrate_row_to(0, &mut destination), Some(b));

        assert_eq!(source.len(), 1);
        assert_eq!(destination.len(), 1);
        assert_eq!(destination.entities(), &[a]);
        assert_eq!(
            destination
                .column::<Position>()
                .expect("has positions")
                .values(),
            &[Position(1.0)]
        );
    }

    #[test]
    fn row_hash_depends_on_values_not_insertion_order() {
        let mut first = position_and_speed();
        add(&mut first, Entity::new(0, 0), 1.5, 2.5);

        let mut second = position_and_speed();
        add(&mut second, Entity::new(9, 3), 1.5, 2.5);

        let mut a = StableHasher::new();
        first.stable_hash_row(0, &mut a);
        let mut b = StableHasher::new();
        second.stable_hash_row(0, &mut b);

        // Same component values hash the same even though the entity handles differ -- the hash
        // fingerprints simulation state, not allocator bookkeeping.
        assert_eq!(a.finish(), b.finish());

        let mut different = position_and_speed();
        add(&mut different, Entity::new(0, 0), 1.5, 9.9);
        let mut c = StableHasher::new();
        different.stable_hash_row(0, &mut c);
        assert_ne!(a.finish(), c.finish());
    }
}
