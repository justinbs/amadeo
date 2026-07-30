//! Archetypes: the tables that hold entities sharing an identical component set.

use crate::component::{Column, Component, ComponentId, TypedColumn};
use crate::entity::Entity;
use amadeo_core::{StableHasher, Tick};

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

    /// Feeds one row's components into a state fingerprint, in sorted component order.
    ///
    /// Sorted order is what makes the hash reproducible: it does not depend on the order components
    /// happened to be inserted, or on any hash map's layout.
    pub(crate) fn stable_hash_row(&self, row: usize, hasher: &mut StableHasher) {
        for (index, id) in self.component_ids.iter().enumerate() {
            hasher.write_u64(id.raw());
            self.columns[index].stable_hash_row(row, hasher);
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use amadeo_core::StableHash;

    #[derive(Debug, Clone, Copy, PartialEq)]
    struct Position(f32);

    impl StableHash for Position {
        fn stable_hash(&self, hasher: &mut StableHasher) {
            self.0.stable_hash(hasher);
        }
    }
    impl Component for Position {}

    #[derive(Debug, Clone, Copy, PartialEq)]
    struct Speed(f32);

    impl StableHash for Speed {
        fn stable_hash(&self, hasher: &mut StableHasher) {
            self.0.stable_hash(hasher);
        }
    }
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
    fn pair_access_refuses_a_missing_column() {
        #[derive(Debug)]
        struct Absent;
        impl StableHash for Absent {
            fn stable_hash(&self, _hasher: &mut StableHasher) {}
        }
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
