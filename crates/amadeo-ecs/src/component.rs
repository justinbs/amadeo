//! Components and the columns that store them.
//!
//! See ADR 0008 for why storage is shaped this way: archetype columns are concrete `Vec<T>` behind a
//! safe trait object, downcast once per archetype per query rather than once per entity. That keeps
//! structure-of-arrays cache behaviour in the inner loop with no `unsafe` anywhere.

use crate::type_hash::hash_name;
use amadeo_core::{StableHash, StableHasher, Tick};
use amadeo_reflect::Reflect;
use std::any::Any;
use std::fmt;

/// Plain data attached to an entity.
///
/// # Why the supertraits
///
/// Each one closes a hole that would otherwise open silently, months later:
///
/// - `'static` — required to store the type behind `dyn Any`, which is how type-erased columns work.
/// - `Send + Sync` — not needed while simulation is single-threaded, but required the moment the
///   scheduler runs systems in parallel. Free to require now, invasive to add later.
/// - `fmt::Debug` — diagnostics must be able to print component values. Error messages are an API
///   here (see `docs/03-ai-native-design.md` Pillar 5), and one that cannot show a value is much
///   less useful.
/// - [`StableHash`] — a component that cannot be fingerprinted cannot participate in golden replay
///   assertions, which would create a silent hole in the project's only behavioural regression test
///   (invariant I3).
/// - [`Reflect`] — **invariant I8**. An unreflected component cannot be serialised, inspected, or
///   edited, so it exists at runtime and nowhere else. Trap 5 in `CLAUDE.md` section 7 is exactly
///   this: registration gets skipped, everything works, and three milestones later the editor and
///   the agent cannot see the type. A supertrait makes that impossible rather than discouraged —
///   the same move ADR 0009 made with `Resource: StableHash`.
///
/// Components hold **data only**. No methods with side effects, no `Rc`/`RefCell`, no interior
/// mutability. Behaviour lives in systems that query components (ADR 0004).
///
/// # Example
///
/// Both traits are derived. Writing either by hand is possible and almost never right — see
/// `StableHash`'s own docs for why a hand-written fingerprint is a hazard.
///
/// ```
/// use amadeo_core::StableHash;
/// use amadeo_ecs::Component;
/// use amadeo_reflect::Reflect;
///
/// /// How much damage something can take.
/// #[derive(Debug, Clone, Copy, PartialEq, StableHash, Reflect)]
/// struct Health {
///     /// Current hit points.
///     #[reflect(min = 0.0, max = 100.0, unit = "hp")]
///     current: f32,
/// }
///
/// impl Component for Health {}
/// ```
pub trait Component: 'static + Send + Sync + fmt::Debug + StableHash + Reflect {
    /// Whether this component is **recomputed from scratch every tick** from other components.
    ///
    /// Derived components are skipped by [`crate::World::state_hash`] — ADR 0019. Almost every
    /// component leaves this alone.
    ///
    /// # Set this only if it is true
    ///
    /// The rule is the name: the value must be rebuilt every tick from other state, so that hashing
    /// it would add nothing the inputs do not already say. `GlobalTransform` qualifies; it is
    /// recomputed from `Transform` and `Parent` on every tick.
    ///
    /// **Marking real simulation state as derived silently removes it from every replay
    /// assertion, and nothing fails.** That is the same failure `#[derive(StableHash)]` exists to
    /// prevent one level down — a hand-written hash that forgets a field still compiles and still
    /// produces a plausible number. If a system writes this component and expects the value to
    /// survive to the next tick, it is not derived.
    const DERIVED: bool = false;
}

/// Identifies a component type.
///
/// # Why not `std::any::TypeId`?
///
/// `TypeId` values are compiler-generated and **not stable across builds**. Using them as map keys
/// would make iteration order vary between compilations, so a state hash produced by one build would
/// disagree with the same logic compiled by another — which is precisely the failure invariant I3
/// exists to prevent.
///
/// # Why the canonical name and not the Rust path
///
/// ADR 0017. A `ComponentId` is the FNV-1a hash of [`Reflect::type_name`] — the same string a
/// `.scene` file writes and `amadeo describe` prints — **not** `std::any::type_name`, which is the
/// fully-qualified path.
///
/// Using the path coupled a component's identity to where its code lived: moving
/// `amadeo_render::components::Transform` to `amadeo_transform::Transform` silently changed its
/// id and would have invalidated every state hash containing it. A pure refactor is not supposed to
/// be a replay-invalidating change, and nothing warned you.
///
/// With the canonical name, the ECS's identity and the file's identity are literally the same
/// string, and `#[reflect(name = "...")]` lets the Rust type be renamed without changing identity.
///
/// **The cost:** two components with the same canonical name now collide.
/// [`crate::ComponentRegistry::register`] rejects that with a clear message, which covers every
/// component that satisfies I8. For anything unregistered, [`crate::World`] carries a debug-build
/// guard — see `World::insert`.
#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Hash)]
pub struct ComponentId(u64);

impl ComponentId {
    /// The id for component type `T`.
    ///
    /// # This is more expensive than it looks, and that is a known defect
    ///
    /// A type's canonical name is fixed at compile time, so its id is a constant — but this
    /// recomputes it on **every call**: [`Reflect::type_name`](amadeo_reflect::Reflect::type_name)
    /// allocates a fresh `String`, and `hash_name` then walks every byte of it. And this sits on the
    /// hot path of every component lookup, since `World::get`, `World::insert`, and every query call
    /// it to find a column.
    ///
    /// Found by measurement while benchmarking the sprite batcher: at 20,000 sprites the two
    /// optional-component lookups per sprite re-hash `"SortOrder"` and `"GlobalTransform"` forty
    /// thousand times per frame, and that dominates the batcher's cost. Numbers in
    /// `crates/amadeo-render/tests/sprite_throughput.rs`.
    ///
    /// **This is not a problem with ADR 0017's decision**, which is right — only with how often its
    /// answer is recomputed. The fix is to make the name an associated `&'static str` and the hash a
    /// `const fn`, so an id becomes a compile-time constant. That is a change to the `Reflect` trait,
    /// its derive, and every impl, so it is filed as **Q16** rather than done in passing.
    ///
    /// A caching attempt using a `static` inside this generic function was tried and **reverted**:
    /// such a static is shared across every instantiation, not one per `T`, so every component type
    /// collapsed onto a single id. The archetype tests caught it immediately.
    #[must_use]
    pub fn of<T: Component>() -> Self {
        ComponentId::of_name(&T::type_name())
    }

    /// The id for a component named at runtime.
    ///
    /// The counterpart to [`ComponentId::of`] for callers that have a name and no type — the scene
    /// loader and the agent layer both do. That these agree is the point of ADR 0017.
    #[must_use]
    pub fn of_name(name: &str) -> Self {
        ComponentId(hash_name(name))
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

    /// Whether this column holds a derived component, which the state hash skips.
    ///
    /// Lives on the trait rather than being looked up per call because the column is type-erased by
    /// the time the hash walks it -- `TypedColumn<T>` is the last place `T::DERIVED` is reachable.
    fn is_derived(&self) -> bool;
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

    fn is_derived(&self) -> bool {
        T::DERIVED
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

    #[derive(Debug, Clone, Copy, PartialEq, StableHash, Reflect)]
    struct Position {
        x: f32,
    }
    impl Component for Position {}

    #[derive(Debug, Clone, Copy, PartialEq, StableHash, Reflect)]
    struct Label(u32);
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
    fn component_id_derives_from_the_canonical_name() {
        // Pinned so an accidental switch to TypeId (not stable across builds) or back to the Rust
        // path (ADR 0017) is caught here rather than by a mysteriously failing replay.
        assert_eq!(
            ComponentId::of::<Position>().raw(),
            hash_name(&Position::type_name())
        );
    }

    #[test]
    fn a_name_and_a_type_agree_on_the_id() {
        // The whole point of ADR 0017: the id the ECS uses and the name a scene file writes are the
        // same string. A scene loader holding only a name gets the same id as a caller holding the
        // type.
        assert_eq!(
            ComponentId::of::<Position>(),
            ComponentId::of_name(&Position::type_name())
        );
        assert_eq!(
            ComponentId::of::<Position>(),
            ComponentId::of_name("Position")
        );
    }

    #[test]
    fn the_id_does_not_depend_on_where_the_type_lives() {
        // The bug ADR 0017 fixes. `type_name::<T>()` is the fully-qualified path, so it changes when
        // a type moves between crates or modules; the canonical name does not. If these were equal,
        // the id would still be path-derived.
        assert_ne!(
            ComponentId::of::<Position>().raw(),
            crate::type_hash::hash_type_name::<Position>(),
            "the path and the canonical name should differ, or this test proves nothing"
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
