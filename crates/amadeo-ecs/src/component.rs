//! Components and the columns that store them.
//!
//! See ADR 0008 for why storage is shaped this way: archetype columns are concrete `Vec<T>` behind a
//! safe trait object, downcast once per archetype per query rather than once per entity. That keeps
//! structure-of-arrays cache behaviour in the inner loop with no `unsafe` anywhere.

use crate::type_hash::hash_type_name;
use amadeo_core::{StableHash, StableHasher, Tick};
use std::any::Any;
use std::fmt;

/// Plain data attached to an entity.
///
/// # Why the supertraits
///
/// - `'static` — required to store the type behind `dyn Any`, which is how type-erased columns work.
/// - `Send + Sync` — not needed while simulation is single-threaded, but required the moment the
///   scheduler runs systems in parallel. Free to require now, invasive to add later.
/// - `fmt::Debug` — diagnostics must be able to print component values. Error messages are an API
///   here (see `docs/03-ai-native-design.md` Pillar 5), and one that cannot show a value is much
///   less useful.
/// - [`StableHash`] — a component that cannot be fingerprinted cannot participate in golden replay
///   assertions, which would create a silent hole in the project's only behavioural regression test
///   (invariant I3). Requiring it means that hole cannot open by accident.
///
/// Components hold **data only**. No methods with side effects, no `Rc`/`RefCell`, no interior
/// mutability. Behaviour lives in systems that query components (ADR 0004).
///
/// # Example
///
/// ```
/// use amadeo_core::{StableHash, StableHasher};
/// use amadeo_ecs::Component;
///
/// #[derive(Debug, Clone, Copy, PartialEq)]
/// struct Health {
///     current: f32,
/// }
///
/// // Hand-written for now. A derive macro arrives with the reflection registry in M1.
/// impl StableHash for Health {
///     fn stable_hash(&self, hasher: &mut StableHasher) {
///         self.current.stable_hash(hasher);
///     }
/// }
///
/// impl Component for Health {}
/// ```
pub trait Component: 'static + Send + Sync + fmt::Debug + StableHash {}

/// Identifies a component type.
///
/// # Why not `std::any::TypeId`?
///
/// `TypeId` values are compiler-generated and **not stable across builds**. Using them as map keys
/// would make iteration order vary between compilations, so a state hash produced by one build would
/// disagree with the same logic compiled by another — which is precisely the failure invariant I3
/// exists to prevent.
///
/// So a `ComponentId` is the FNV-1a hash of the type's name instead. That is stable across builds,
/// traceable back to a readable name for diagnostics, and ordered consistently in a `BTreeMap`.
#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Hash)]
pub struct ComponentId(u64);

impl ComponentId {
    /// The id for component type `T`.
    #[must_use]
    pub fn of<T: Component>() -> Self {
        ComponentId(hash_type_name::<T>())
    }

    /// The raw hash value, for diagnostics and serialisation.
    #[must_use]
    pub fn raw(self) -> u64 {
        self.0
    }
}

impl fmt::Display for ComponentId {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(f, "component#{:016x}", self.0)
    }
}

/// A type-erased column of component values.
///
/// One column holds every value of a single component type within a single archetype, stored
/// contiguously. The trait exists only so that archetypes can hold columns of differing types; all
/// the real work happens on [`TypedColumn`] after a downcast.
pub(crate) trait Column: fmt::Debug + Send + Sync {
    /// Upcast for downcasting. Needed because trait objects cannot be downcast directly.
    fn as_any(&self) -> &dyn Any;

    /// Mutable upcast for downcasting.
    fn as_any_mut(&mut self) -> &mut dyn Any;

    /// How many values this column holds. Always equal to its archetype's row count.
    fn len(&self) -> usize;

    /// Removes the value at `row` by swapping the last value into its place.
    ///
    /// O(1), but it reorders rows — the caller must fix up the entity whose row moved.
    fn swap_remove(&mut self, row: usize);

    /// Moves the value at `row` out of this column and appends it to `dest`.
    ///
    /// Used when an entity changes archetype because a component was added or removed. `dest` must
    /// be a column of the same concrete type; a mismatch is an engine bug.
    fn migrate_row_to(&mut self, row: usize, dest: &mut dyn Column);

    /// Creates an empty column of the same concrete type as this one.
    fn empty_clone(&self) -> Box<dyn Column>;

    /// Feeds the value at `row` into a state fingerprint.
    fn stable_hash_row(&self, row: usize, hasher: &mut StableHasher);
}

/// A concrete column: every value of one component type in one archetype.
///
/// This is the whole point of ADR 0008. `values` is an ordinary `Vec<T>`, so once a query has
/// downcast to this type it iterates a plain contiguous slice — the exact access pattern
/// data-oriented design is chosen for.
#[derive(Debug)]
pub(crate) struct TypedColumn<T: Component> {
    /// The component values, one per row.
    values: Vec<T>,
    /// The tick each value was last mutably accessed, parallel to `values`.
    ///
    /// Marked on mutable access rather than on actual modification, since there is no way to detect
    /// whether a caller holding `&mut T` really changed anything. Conservative in the safe
    /// direction: it may report an unchanged value as changed, never the reverse.
    changed: Vec<Tick>,
}

impl<T: Component> TypedColumn<T> {
    /// Creates an empty column.
    pub(crate) fn new() -> Self {
        Self {
            values: Vec::new(),
            changed: Vec::new(),
        }
    }

    /// Appends a value, recording `tick` as its change tick.
    pub(crate) fn push(&mut self, value: T, tick: Tick) {
        self.values.push(value);
        self.changed.push(tick);
    }

    /// Shared access to the values as a contiguous slice.
    pub(crate) fn values(&self) -> &[T] {
        &self.values
    }

    /// Mutable access to the values, marking every row as changed at `tick`.
    ///
    /// Marks the whole column because the caller receives the whole slice. Row-granular marking is
    /// available via [`TypedColumn::value_mut`].
    pub(crate) fn values_mut(&mut self, tick: Tick) -> &mut [T] {
        self.changed.fill(tick);
        &mut self.values
    }

    /// Shared access to one value.
    pub(crate) fn value(&self, row: usize) -> Option<&T> {
        self.values.get(row)
    }

    /// Mutable access to one value, marking just that row as changed.
    pub(crate) fn value_mut(&mut self, row: usize, tick: Tick) -> Option<&mut T> {
        if let Some(slot) = self.changed.get_mut(row) {
            *slot = tick;
        }
        self.values.get_mut(row)
    }

    /// The tick at which `row` was last mutably accessed.
    pub(crate) fn changed_tick(&self, row: usize) -> Option<Tick> {
        self.changed.get(row).copied()
    }
}

impl<T: Component> Column for TypedColumn<T> {
    fn as_any(&self) -> &dyn Any {
        self
    }

    fn as_any_mut(&mut self) -> &mut dyn Any {
        self
    }

    fn len(&self) -> usize {
        self.values.len()
    }

    fn swap_remove(&mut self, row: usize) {
        if row < self.values.len() {
            self.values.swap_remove(row);
            self.changed.swap_remove(row);
        }
    }

    fn migrate_row_to(&mut self, row: usize, dest: &mut dyn Column) {
        if row >= self.values.len() {
            return;
        }
        let value = self.values.swap_remove(row);
        let changed = self.changed.swap_remove(row);

        match dest.as_any_mut().downcast_mut::<TypedColumn<T>>() {
            Some(dest) => {
                dest.values.push(value);
                dest.changed.push(changed);
            }
            None => {
                // Reaching here means an archetype was built with mismatched column types, which is
                // an engine bug rather than anything a game can cause. Assert loudly in dev and
                // test builds; in release the value is dropped, which loses data but does not
                // corrupt memory.
                debug_assert!(
                    false,
                    "migrate_row_to: destination column type does not match source"
                );
            }
        }
    }

    fn empty_clone(&self) -> Box<dyn Column> {
        Box::new(TypedColumn::<T>::new())
    }

    fn stable_hash_row(&self, row: usize, hasher: &mut StableHasher) {
        if let Some(value) = self.values.get(row) {
            value.stable_hash(hasher);
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[derive(Debug, Clone, Copy, PartialEq)]
    struct Position {
        x: f32,
    }

    impl StableHash for Position {
        fn stable_hash(&self, hasher: &mut StableHasher) {
            self.x.stable_hash(hasher);
        }
    }

    impl Component for Position {}

    #[derive(Debug, Clone, Copy, PartialEq)]
    struct Label(u32);

    impl StableHash for Label {
        fn stable_hash(&self, hasher: &mut StableHasher) {
            self.0.stable_hash(hasher);
        }
    }

    impl Component for Label {}

    #[test]
    fn component_ids_differ_by_type() {
        assert_ne!(ComponentId::of::<Position>(), ComponentId::of::<Label>());
    }

    #[test]
    fn component_id_is_stable_for_same_type() {
        assert_eq!(ComponentId::of::<Position>(), ComponentId::of::<Position>());
    }

    #[test]
    fn component_id_derives_from_type_name() {
        // Pinned so an accidental switch to TypeId (which is not stable across builds) is caught.
        assert_eq!(
            ComponentId::of::<Position>().raw(),
            hash_type_name::<Position>()
        );
    }

    #[test]
    fn push_and_read_values() {
        let mut column = TypedColumn::<Position>::new();
        column.push(Position { x: 1.0 }, Tick(3));
        column.push(Position { x: 2.0 }, Tick(3));

        assert_eq!(column.len(), 2);
        assert_eq!(column.values(), &[Position { x: 1.0 }, Position { x: 2.0 }]);
        assert_eq!(column.changed_tick(0), Some(Tick(3)));
    }

    #[test]
    fn mutable_access_updates_change_tick() {
        let mut column = TypedColumn::<Position>::new();
        column.push(Position { x: 1.0 }, Tick(1));
        column.push(Position { x: 2.0 }, Tick(1));

        // Row-granular: only row 0 is marked.
        column.value_mut(0, Tick(9)).expect("row 0 exists").x = 5.0;
        assert_eq!(column.changed_tick(0), Some(Tick(9)));
        assert_eq!(column.changed_tick(1), Some(Tick(1)));
        assert_eq!(column.value(0), Some(&Position { x: 5.0 }));

        // Whole-slice access marks everything.
        column.values_mut(Tick(20));
        assert_eq!(column.changed_tick(0), Some(Tick(20)));
        assert_eq!(column.changed_tick(1), Some(Tick(20)));
    }

    #[test]
    fn swap_remove_moves_last_row_into_the_hole() {
        let mut column = TypedColumn::<Label>::new();
        column.push(Label(10), Tick::ZERO);
        column.push(Label(11), Tick::ZERO);
        column.push(Label(12), Tick::ZERO);

        column.swap_remove(0);
        assert_eq!(column.len(), 2);
        // The last element took index 0. This reordering is why callers must fix up locations.
        assert_eq!(column.values(), &[Label(12), Label(11)]);
    }

    #[test]
    fn swap_remove_out_of_bounds_is_ignored() {
        let mut column = TypedColumn::<Label>::new();
        column.push(Label(1), Tick::ZERO);
        column.swap_remove(99);
        assert_eq!(column.len(), 1);
    }

    #[test]
    fn migrate_row_moves_value_between_columns() {
        let mut source = TypedColumn::<Label>::new();
        source.push(Label(1), Tick(4));
        source.push(Label(2), Tick(5));

        let mut destination = TypedColumn::<Label>::new();
        source.migrate_row_to(0, &mut destination);

        assert_eq!(source.len(), 1);
        assert_eq!(destination.len(), 1);
        assert_eq!(destination.values(), &[Label(1)]);
        // The change tick travels with the value rather than being reset.
        assert_eq!(destination.changed_tick(0), Some(Tick(4)));
    }

    #[test]
    fn empty_clone_produces_matching_type() {
        let source = TypedColumn::<Position>::new();
        let mut cloned = source.empty_clone();
        assert_eq!(cloned.len(), 0);
        assert!(
            cloned
                .as_any_mut()
                .downcast_mut::<TypedColumn<Position>>()
                .is_some(),
            "empty_clone must preserve the concrete type"
        );
    }

    #[test]
    fn stable_hash_row_reflects_value() {
        let mut column = TypedColumn::<Label>::new();
        column.push(Label(7), Tick::ZERO);
        column.push(Label(8), Tick::ZERO);

        let mut a = StableHasher::new();
        column.stable_hash_row(0, &mut a);
        let mut b = StableHasher::new();
        column.stable_hash_row(1, &mut b);
        assert_ne!(a.finish(), b.finish());

        // Same row hashes identically on repeat.
        let mut c = StableHasher::new();
        column.stable_hash_row(0, &mut c);
        assert_eq!(a.finish(), c.finish());
    }
}
