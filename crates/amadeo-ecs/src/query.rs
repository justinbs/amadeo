//! Asking the world for entities — the read side.
//!
//! # What this replaces, and why
//!
//! Queries used to be hand-written methods, one per shape: `iter_pair`, `iter_triple`, and so on.
//! That worked, and it had two limits that turned out to matter.
//!
//! **It could not express an optional component.** A system wanting "every entity with a `Transform`
//! and a `Sprite`, plus its `SortOrder` if it has one" had no way to say so, so it asked for the
//! required pair and then did `world.get::<SortOrder>(entity)` per entity. That is exactly the
//! per-entity lookup archetype storage exists to avoid — at 20,000 sprites the renderer was doing
//! 40,000 of them, and after ADR 0024 removed the id cost it was what remained.
//!
//! **And every new shape needed an engine change.** Requiring a component means an entity silently
//! vanishing from a query when someone forgets to add it, so "optional" is the common case, not the
//! exotic one — and the combinations multiply. A game author who could not express a query without
//! editing the engine is a real limit on invariant I5.
//!
//! # How it works, in one paragraph
//!
//! A query is a **tuple of terms**. Each term is one thing you are asking for: `&T` means "must have
//! this", `Option<&T>` means "include it if present". The world walks its archetypes, skips any that
//! cannot satisfy the required terms, and for each one that can, **looks up each column exactly
//! once** and then walks its rows. So the per-entity cost is an array index, not a lookup.
//!
//! ```
//! use amadeo_ecs::{Component, World};
//! # use amadeo_reflect::Reflect;
//! # use amadeo_core::StableHash;
//! # #[derive(Debug, Clone, Copy, PartialEq, StableHash, Reflect)]
//! # struct Position { /// x
//! # x: f32 }
//! # impl Component for Position {}
//! # #[derive(Debug, Clone, Copy, PartialEq, StableHash, Reflect)]
//! # struct Label { /// n
//! # n: i32 }
//! # impl Component for Label {}
//! let mut world = World::new();
//! let a = world.spawn();
//! world.insert(a, Position { x: 1.0 });
//! world.insert(a, Label { n: 7 });
//!
//! let b = world.spawn();
//! world.insert(b, Position { x: 2.0 });   // no Label
//!
//! // Every entity with a Position, and its Label if it has one. `b` is included even though it has
//! // no Label -- that is what makes the term optional.
//! let mut found: Vec<_> = world
//!     .query::<(&Position, Option<&Label>)>()
//!     .map(|(_entity, (position, label))| (position.x, label.map(|l| l.n)))
//!     .collect();
//!
//! // Sorted here only so the example does not depend on iteration order, which is *archetype*
//! // order: `a` and `b` hold different component sets, so they live in different archetypes and
//! // `b` is visited first. Reproducible, but not the order they were spawned in.
//! found.sort_by(|left, right| left.0.total_cmp(&right.0));
//! assert_eq!(found, vec![(1.0, Some(7)), (2.0, None)]);
//! ```
//!
//! # Why this is the one piece of clever Rust in the ECS
//!
//! `CLAUDE.md` §6 asks for boring Rust, and this is a trait with an associated type plus a macro that
//! writes the tuple implementations. That is a deliberate exception, chosen by Justin in session 7
//! against the alternative of hand-writing every shape, and it is worth knowing why the machinery is
//! shaped the way it is:
//!
//! - **Rust has no variadic generics.** There is no way to write "a tuple of any length" once, so
//!   every arity needs its own `impl`. They are identical apart from the number of fields, which is
//!   what a macro is *for* — writing them by hand would be the same code eight times. Bevy does this
//!   too, up to 15 elements.
//! - **The lifetime `'w` is the world's.** It appears on the trait rather than on each method because
//!   the borrowed slices a term resolves to have to outlive the loop that reads them.
//! - **`Columns: Copy` is the trick that makes the loop cheap.** A term's per-archetype state is a
//!   slice (or an `Option` of one), both of which are `Copy`, so the row loop copies a fat pointer
//!   rather than re-borrowing anything.
//!
//! Read-only, deliberately. Mutable queries stay hand-written (`for_each_pair_mut` and friends)
//! because handing out two `&mut` into one archetype requires proving they are different columns —
//! which the existing methods do with `slice::get_disjoint_mut`, and which a generic version cannot
//! do without `unsafe`. This crate forbids `unsafe`, and the measured problem was entirely on the
//! read side.

use crate::archetype::Archetype;
use crate::component::Component;
use crate::entity::Entity;

/// One thing a query asks for, per entity.
///
/// Implemented for `&T` (required), `Option<&T>` (optional), and tuples of those. **Effectively
/// sealed** — the methods take `&Archetype`, whose own methods are crate-private, so an
/// implementation outside this crate could not do anything.
pub trait QueryTerm<'w> {
    /// What the caller receives for one entity.
    type Item;

    /// This term's resolved state for a single archetype.
    ///
    /// `Copy` so that reading a row costs a pointer copy rather than a borrow — see the module docs.
    type Columns: Copy;

    /// Whether an archetype can satisfy this term at all.
    ///
    /// `Option<&T>` always returns `true`: an optional term never excludes an entity, which is the
    /// whole point of it.
    fn matches(archetype: &Archetype) -> bool;

    /// Resolves this term's columns for one archetype.
    ///
    /// Called **once per archetype**, not once per entity. That is where the speed comes from.
    fn columns(archetype: &'w Archetype) -> Option<Self::Columns>;

    /// Reads one row out of already-resolved columns.
    fn get(columns: Self::Columns, row: usize) -> Self::Item;
}

/// A required component: the entity must have it, or it is not in the results.
impl<'w, T: Component> QueryTerm<'w> for &'w T {
    type Item = &'w T;
    type Columns = &'w [T];

    fn matches(archetype: &Archetype) -> bool {
        archetype.has(crate::ComponentId::of::<T>())
    }

    fn columns(archetype: &'w Archetype) -> Option<&'w [T]> {
        Some(archetype.column::<T>()?.values())
    }

    fn get(columns: &'w [T], row: usize) -> &'w T {
        // The row always exists: it came from this archetype's own entity list, and every column in
        // an archetype has one value per entity -- `debug_assert_rectangular` enforces that.
        // `get` rather than indexing so a bug here is a `None` rather than a panic in a render loop.
        #[allow(clippy::missing_panics_doc)]
        columns.get(row).unwrap_or_else(|| {
            unreachable!("archetype column shorter than its entity list; this is an ECS bug")
        })
    }
}

/// An optional component: included when present, `None` when not, and never a reason to exclude.
impl<'w, T: Component> QueryTerm<'w> for Option<&'w T> {
    type Item = Option<&'w T>;
    type Columns = Option<&'w [T]>;

    fn matches(_archetype: &Archetype) -> bool {
        true
    }

    fn columns(archetype: &'w Archetype) -> Option<Option<&'w [T]>> {
        // The outer `Some` means "this term resolved"; the inner `Option` is whether the archetype
        // actually has the component. They are different questions and the nesting is unavoidable.
        Some(archetype.column::<T>().map(|column| column.values()))
    }

    fn get(columns: Option<&'w [T]>, row: usize) -> Option<&'w T> {
        columns?.get(row)
    }
}

/// Writes the `QueryTerm` implementation for a tuple of terms.
///
/// One implementation per arity, because Rust has no variadic generics. Every one is the same three
/// methods delegating to each element, so writing them out by hand would be this code eight times
/// with different numbers of letters in it.
macro_rules! impl_query_term_for_tuple {
    ($($name:ident),+) => {
        impl<'w, $($name: QueryTerm<'w>),+> QueryTerm<'w> for ($($name,)+) {
            type Item = ($($name::Item,)+);
            type Columns = ($($name::Columns,)+);

            // An archetype has to satisfy every term. An `Option<&T>` term always says yes, so a
            // query made only of optional terms matches every archetype -- which is correct, if
            // rarely what anyone wants.
            fn matches(archetype: &Archetype) -> bool {
                $($name::matches(archetype))&&+
            }

            fn columns(archetype: &'w Archetype) -> Option<Self::Columns> {
                Some(($($name::columns(archetype)?,)+))
            }

            #[allow(non_snake_case)]
            fn get(columns: Self::Columns, row: usize) -> Self::Item {
                // Destructured into names matching the type parameters, which is the tidiest way a
                // macro can get at tuple fields by position.
                let ($($name,)+) = columns;
                ($($name::get($name, row),)+)
            }
        }
    };
}

impl_query_term_for_tuple!(A);
impl_query_term_for_tuple!(A, B);
impl_query_term_for_tuple!(A, B, C);
impl_query_term_for_tuple!(A, B, C, D);
impl_query_term_for_tuple!(A, B, C, D, E);
impl_query_term_for_tuple!(A, B, C, D, E, F);
impl_query_term_for_tuple!(A, B, C, D, E, F, G);
impl_query_term_for_tuple!(A, B, C, D, E, F, G, H);

/// Walks the archetypes matching a query and yields one item per entity.
///
/// Written as an explicit struct rather than a chain of iterator adaptors because the type of such a
/// chain is unspeakable and the borrow of `archetypes` has to outlive it. It is also easier to read:
/// two indices, an archetype cursor and a row cursor.
// `Debug` is derived rather than hand-written, which means it prints the archetype slice. That is
// verbose but honest; a manual impl hiding the contents would be worse to debug with.
#[derive(Debug)]
pub struct QueryIter<'w, Q: QueryTerm<'w>> {
    archetypes: &'w [Archetype],
    /// Which archetype to consider next.
    next_archetype: usize,
    /// The archetype currently being walked: its entities, and its resolved columns.
    current: Option<(&'w [Entity], Q::Columns)>,
    /// Which row of `current` comes next.
    row: usize,
}

impl<'w, Q: QueryTerm<'w>> QueryIter<'w, Q> {
    /// Starts a query over `archetypes`.
    pub(crate) fn new(archetypes: &'w [Archetype]) -> Self {
        QueryIter {
            archetypes,
            next_archetype: 0,
            current: None,
            row: 0,
        }
    }

    /// Finds the next archetype this query matches and resolves its columns.
    ///
    /// Returns `false` when there are none left.
    fn advance_archetype(&mut self) -> bool {
        while self.next_archetype < self.archetypes.len() {
            let archetype = &self.archetypes[self.next_archetype];
            self.next_archetype += 1;

            // Empty archetypes are skipped rather than resolved: they yield nothing, and an
            // archetype can legitimately be left empty after its last entity is despawned.
            if archetype.len() == 0 || !Q::matches(archetype) {
                continue;
            }

            // The lookup that happens once per archetype instead of once per entity.
            if let Some(columns) = Q::columns(archetype) {
                self.current = Some((archetype.entities(), columns));
                self.row = 0;
                return true;
            }
        }

        self.current = None;
        false
    }
}

impl<'w, Q: QueryTerm<'w>> Iterator for QueryIter<'w, Q> {
    type Item = (Entity, Q::Item);

    fn next(&mut self) -> Option<Self::Item> {
        loop {
            if let Some((entities, columns)) = self.current
                && self.row < entities.len()
            {
                let entity = entities[self.row];
                let item = Q::get(columns, self.row);
                self.row += 1;
                return Some((entity, item));
            }

            if !self.advance_archetype() {
                return None;
            }
        }
    }
}

#[cfg(test)]
mod tests {
    use crate::{Component, World};
    use amadeo_core::StableHash;
    use amadeo_reflect::Reflect;

    #[derive(Debug, Clone, Copy, PartialEq, StableHash, Reflect)]
    struct Position {
        /// Across.
        x: f32,
    }
    impl Component for Position {}

    #[derive(Debug, Clone, Copy, PartialEq, StableHash, Reflect)]
    struct Velocity {
        /// Across, per second.
        x: f32,
    }
    impl Component for Velocity {}

    #[derive(Debug, Clone, Copy, PartialEq, StableHash, Reflect)]
    struct Frozen {
        /// Whether it is stuck.
        stuck: bool,
    }
    impl Component for Frozen {}

    /// Spawns an entity with a position, and optionally a velocity and a frozen flag.
    fn spawn(world: &mut World, x: f32, velocity: Option<f32>, frozen: bool) -> crate::Entity {
        let entity = world.spawn();
        world.insert(entity, Position { x });
        if let Some(vx) = velocity {
            world.insert(entity, Velocity { x: vx });
        }
        if frozen {
            world.insert(entity, Frozen { stuck: true });
        }
        entity
    }

    /// The x values a query saw, sorted, so a test asserts on content rather than archetype order.
    fn sorted_xs(mut found: Vec<f32>) -> Vec<f32> {
        found.sort_by(f32::total_cmp);
        found
    }

    #[test]
    fn a_single_required_term_finds_every_entity_with_it() {
        let mut world = World::new();
        spawn(&mut world, 1.0, None, false);
        spawn(&mut world, 2.0, Some(9.0), false);

        let found: Vec<f32> = world
            .query::<(&Position,)>()
            .map(|(_, (position,))| position.x)
            .collect();

        assert_eq!(sorted_xs(found), vec![1.0, 2.0]);
    }

    #[test]
    fn a_required_term_excludes_entities_without_it() {
        let mut world = World::new();
        spawn(&mut world, 1.0, None, false);
        spawn(&mut world, 2.0, Some(9.0), false);

        let found: Vec<f32> = world
            .query::<(&Position, &Velocity)>()
            .map(|(_, (position, _))| position.x)
            .collect();

        assert_eq!(found, vec![2.0]);
    }

    #[test]
    fn an_optional_term_never_excludes_anything() {
        // The property the whole module exists for. Before this, a system wanting an optional
        // component had to ask for the required ones and then look the rest up per entity.
        let mut world = World::new();
        spawn(&mut world, 1.0, None, false);
        spawn(&mut world, 2.0, Some(9.0), false);
        spawn(&mut world, 3.0, None, true);

        let found: Vec<(f32, bool)> = world
            .query::<(&Position, Option<&Velocity>)>()
            .map(|(_, (position, velocity))| (position.x, velocity.is_some()))
            .collect();

        assert_eq!(found.len(), 3, "got: {found:?}");
        assert_eq!(
            found
                .iter()
                .filter(|(_, has_velocity)| *has_velocity)
                .count(),
            1
        );
    }

    #[test]
    fn optional_terms_report_the_right_values_not_just_presence() {
        // A weaker version of this test would pass even if the optional column were read at the
        // wrong row -- which is the mistake worth guarding, since rows differ between archetypes.
        let mut world = World::new();
        spawn(&mut world, 1.0, Some(10.0), false);
        spawn(&mut world, 2.0, Some(20.0), false);
        spawn(&mut world, 3.0, Some(30.0), false);

        let mut found: Vec<(f32, f32)> = world
            .query::<(&Position, Option<&Velocity>)>()
            .filter_map(|(_, (position, velocity))| velocity.map(|v| (position.x, v.x)))
            .collect();
        found.sort_by(|a, b| a.0.total_cmp(&b.0));

        assert_eq!(found, vec![(1.0, 10.0), (2.0, 20.0), (3.0, 30.0)]);
    }

    #[test]
    fn several_optional_terms_work_together() {
        let mut world = World::new();
        spawn(&mut world, 1.0, None, false);
        spawn(&mut world, 2.0, Some(9.0), true);

        let found: Vec<(f32, bool, bool)> = world
            .query::<(&Position, Option<&Velocity>, Option<&Frozen>)>()
            .map(|(_, (position, velocity, frozen))| {
                (position.x, velocity.is_some(), frozen.is_some())
            })
            .collect();

        assert_eq!(found.len(), 2);
        assert!(found.contains(&(1.0, false, false)), "got: {found:?}");
        assert!(found.contains(&(2.0, true, true)), "got: {found:?}");
    }

    #[test]
    fn a_query_of_only_optional_terms_matches_everything() {
        // Correct, if rarely useful. Worth pinning because it falls out of `matches` being an AND
        // over terms that all say yes, and a future change could break it without meaning to.
        let mut world = World::new();
        world.spawn();
        spawn(&mut world, 1.0, None, false);

        let count = world.query::<(Option<&Position>,)>().count();
        assert_eq!(count, 2);
    }

    #[test]
    fn iteration_is_reproducible() {
        // Invariant I3. Draw order and state hashes both depend on this, so two identical walks of
        // the same world must agree exactly.
        let mut world = World::new();
        for i in 0..30 {
            spawn(
                &mut world,
                i as f32,
                (i % 3 == 0).then_some(1.0),
                i % 5 == 0,
            );
        }

        let first: Vec<_> = world
            .query::<(&Position, Option<&Velocity>)>()
            .map(|(entity, (position, velocity))| (entity, position.x, velocity.copied()))
            .collect();
        let second: Vec<_> = world
            .query::<(&Position, Option<&Velocity>)>()
            .map(|(entity, (position, velocity))| (entity, position.x, velocity.copied()))
            .collect();

        assert_eq!(first, second);
    }

    #[test]
    fn the_entity_handle_matches_its_components() {
        // The pairing that everything downstream relies on: the entity yielded must be the one the
        // components belong to. An off-by-one between the entity list and a column would show up
        // here and nowhere else.
        let mut world = World::new();
        let a = spawn(&mut world, 1.0, None, false);
        let b = spawn(&mut world, 2.0, None, false);

        for (entity, (position,)) in world.query::<(&Position,)>() {
            if entity == a {
                assert_eq!(position.x, 1.0);
            } else if entity == b {
                assert_eq!(position.x, 2.0);
            } else {
                panic!("unexpected entity {entity:?}");
            }
        }
    }

    #[test]
    fn an_empty_world_yields_nothing() {
        assert_eq!(World::new().query::<(&Position,)>().count(), 0);
    }

    #[test]
    fn despawned_entities_are_not_visited() {
        // An archetype can be left empty after its last entity goes, and an empty archetype must
        // contribute nothing rather than an out-of-range row.
        let mut world = World::new();
        let only = spawn(&mut world, 1.0, None, false);
        world.despawn(only);

        assert_eq!(world.query::<(&Position,)>().count(), 0);
    }

    #[test]
    fn results_agree_with_the_hand_written_pair_query() {
        // The new API and the old one must see the same world. If these ever disagreed, one of them
        // would be wrong and the renderer sits on top of both.
        let mut world = World::new();
        for i in 0..10 {
            spawn(&mut world, i as f32, (i % 2 == 0).then_some(1.0), false);
        }

        let mut old: Vec<_> = world
            .iter_pair::<Position, Velocity>()
            .map(|(entity, position, _)| (entity, position.x))
            .collect();
        let mut new: Vec<_> = world
            .query::<(&Position, &Velocity)>()
            .map(|(entity, (position, _))| (entity, position.x))
            .collect();

        old.sort_by(|a, b| a.1.total_cmp(&b.1));
        new.sort_by(|a, b| a.1.total_cmp(&b.1));
        assert_eq!(old, new);
    }

    #[test]
    fn queries_do_not_mark_anything_changed() {
        // Read-only, and it has to stay that way: a query that touched change ticks would make
        // change detection useless for every system that runs after a render pass.
        let mut world = World::new();
        let entity = spawn(&mut world, 1.0, Some(2.0), false);
        world.advance_tick();
        world.advance_tick();

        let before = world.changed_tick::<Position>(entity);
        let _ = world.query::<(&Position, Option<&Velocity>)>().count();

        assert_eq!(world.changed_tick::<Position>(entity), before);
    }

    #[test]
    fn a_query_does_not_move_the_state_hash() {
        let mut world = World::new();
        spawn(&mut world, 1.0, Some(2.0), true);

        let before = world.state_hash();
        for _ in 0..5 {
            let _ = world.query::<(&Position, Option<&Frozen>)>().count();
        }
        assert_eq!(world.state_hash(), before);
    }
}
