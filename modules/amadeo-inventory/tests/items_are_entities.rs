//! ADR 0070's claims, checked rather than asserted.
//!
//! The one that matters most is **that removing a `Transform` is enough to take a thing out of the
//! world**. That is not a property of this module — it is a property of three passes in three other
//! crates, and if any of them ever stopped requiring a `Transform`, an item in a bag would start
//! being drawn at the world origin or colliding with the player carrying it. So it is pinned here,
//! against the real queries, in `a_stored_item_is_invisible_to_every_world_pass`.

use amadeo_ecs::{Component, Entity, World};
use amadeo_inventory::{
    Inventory, Item, StoreError, StoredIn, contents, count_of, drop_at, orphaned, store,
};
use amadeo_transform::{GlobalTransform, Parent, Transform, propagate_transforms};

/// A container with `slots` slots.
fn container(world: &mut World, slots: u32) -> Entity {
    let entity = world.spawn();
    world.insert(entity, Inventory { slots });
    entity
}

/// A thing lying in the world at the origin.
fn lying_about(world: &mut World, item: Item) -> Entity {
    let entity = world.spawn();
    world.insert(entity, item);
    world.insert(entity, Transform::default());
    entity
}

#[test]
fn storing_something_takes_it_out_of_the_world_and_keeps_everything_else() {
    let mut world = World::new();
    let bag = container(&mut world, 4);
    let key = lying_about(&mut world, Item::single("brass-key"));

    let slot = store(&mut world, key, bag).expect("there is room");

    assert_eq!(slot, 0);
    assert_eq!(contents(&world, bag), vec![(0, key)]);
    assert!(
        world.get::<Transform>(key).is_none(),
        "the one component that puts a thing in the world"
    );
    assert_eq!(
        world.get::<Item>(key).map(|item| item.kind.as_str()),
        Some("brass-key"),
        "and nothing else about it was touched"
    );
}

#[test]
fn a_stored_item_is_invisible_to_every_world_pass() {
    // The measurement ADR 0070 rests on, in the form the real passes ask it. Written against the
    // query shapes rather than against rendering and physics directly, because this module cannot
    // depend on those crates -- but the shapes are what those passes use, and a change to them is
    // exactly what this exists to catch.
    let mut world = World::new();
    let bag = container(&mut world, 4);
    let key = lying_about(&mut world, Item::single("brass-key"));
    world.insert(key, GlobalTransform::default());

    let in_world = world.query::<(&Transform,)>().count();
    store(&mut world, key, bag).expect("there is room");
    let after = world.query::<(&Transform,)>().count();

    assert_eq!(
        after,
        in_world - 1,
        "a stored item must not be found by anything that requires a `Transform`, which is what \
         collect_meshes, step_physics and propagate_transforms all do"
    );
    assert!(
        world.get::<GlobalTransform>(key).is_none(),
        "the derived one goes too, so a snapshot dump does not show a stale place"
    );
}

#[test]
fn propagation_leaves_a_stored_item_alone_even_when_it_has_a_parent() {
    // The awkward case: an item picked up while parented to something. Propagation must skip it
    // rather than composing a place for a thing that is not anywhere.
    let mut world = World::new();
    let bag = container(&mut world, 4);
    let holder = world.spawn();
    world.insert(holder, Transform::default());

    let key = lying_about(&mut world, Item::single("brass-key"));
    world.insert(key, Parent(holder));

    store(&mut world, key, bag).expect("there is room");
    propagate_transforms(&mut world);

    assert!(
        world.get::<GlobalTransform>(key).is_none(),
        "propagation requires a local `Transform` and this one has none"
    );
}

#[test]
fn dropping_is_the_exact_inverse() {
    let mut world = World::new();
    let bag = container(&mut world, 4);
    let key = lying_about(&mut world, Item::single("brass-key"));

    store(&mut world, key, bag).expect("there is room");
    let came_from = drop_at(&mut world, key, [1.0, 2.0, 3.0]).expect("it was stored");

    assert_eq!(came_from, bag);
    assert!(world.get::<StoredIn>(key).is_none());
    assert_eq!(
        world.get::<Transform>(key).map(|t| t.translation),
        Some([1.0, 2.0, 3.0])
    );
    assert!(contents(&world, bag).is_empty());
}

#[test]
fn dropping_something_that_was_never_stored_answers_none() {
    let mut world = World::new();
    let key = lying_about(&mut world, Item::single("brass-key"));
    assert_eq!(drop_at(&mut world, key, [0.0; 3]), None);
}

// --- Stacking ------------------------------------------------------------------------------------

#[test]
fn two_stacks_of_one_kind_merge_into_one_slot() {
    let mut world = World::new();
    let bag = container(&mut world, 4);

    let first = lying_about(&mut world, Item::stack("arrow", 20, 64));
    let second = lying_about(&mut world, Item::stack("arrow", 30, 64));

    store(&mut world, first, bag).expect("room");
    let slot = store(&mut world, second, bag).expect("room");

    assert_eq!(slot, 0, "it joined the stack that was already there");
    assert_eq!(contents(&world, bag), vec![(0, first)]);
    assert_eq!(count_of(&world, bag, "arrow"), 50);
    assert!(
        !world.contains(second),
        "two entities describing one pile of arrows is the state that goes wrong later"
    );
}

#[test]
fn a_stack_that_overflows_leaves_the_remainder_in_its_own_slot() {
    let mut world = World::new();
    let bag = container(&mut world, 4);

    let first = lying_about(&mut world, Item::stack("arrow", 60, 64));
    let second = lying_about(&mut world, Item::stack("arrow", 10, 64));

    store(&mut world, first, bag).expect("room");
    let slot = store(&mut world, second, bag).expect("room");

    assert_eq!(slot, 1, "what would not fit needed a slot of its own");
    assert_eq!(count_of(&world, bag, "arrow"), 70, "and nothing was lost");
    assert_eq!(world.get::<Item>(first).map(|i| i.count), Some(64));
    assert_eq!(world.get::<Item>(second).map(|i| i.count), Some(6));
}

#[test]
fn unstackable_things_never_merge() {
    let mut world = World::new();
    let bag = container(&mut world, 4);

    let one = lying_about(&mut world, Item::single("sword"));
    let two = lying_about(&mut world, Item::single("sword"));

    store(&mut world, one, bag).expect("room");
    store(&mut world, two, bag).expect("room");

    assert_eq!(contents(&world, bag), vec![(0, one), (1, two)]);
    assert_eq!(count_of(&world, bag, "sword"), 2);
}

#[test]
fn different_kinds_never_merge_however_alike_they_look() {
    let mut world = World::new();
    let bag = container(&mut world, 4);

    let iron = lying_about(&mut world, Item::stack("arrow-iron", 5, 64));
    let steel = lying_about(&mut world, Item::stack("arrow-steel", 5, 64));

    store(&mut world, iron, bag).expect("room");
    store(&mut world, steel, bag).expect("room");

    assert_eq!(contents(&world, bag).len(), 2);
}

// --- Slots ---------------------------------------------------------------------------------------

#[test]
fn a_freed_slot_is_reused_before_a_later_one() {
    // The lowest free slot, deterministically. Anything else and two runs of the same game could
    // put the same item in different places.
    let mut world = World::new();
    let bag = container(&mut world, 4);

    let a = lying_about(&mut world, Item::single("a"));
    let b = lying_about(&mut world, Item::single("b"));
    let c = lying_about(&mut world, Item::single("c"));

    store(&mut world, a, bag).expect("room");
    store(&mut world, b, bag).expect("room");
    drop_at(&mut world, a, [0.0; 3]).expect("stored");

    assert_eq!(store(&mut world, c, bag), Ok(0), "slot 0 came free");
}

#[test]
fn contents_are_sorted_by_slot_rather_than_by_whatever_the_query_yields() {
    // The reason `StoredIn` carries a slot at all. Storage order is reproducible but not stable, so
    // an item's position in this list would otherwise move when an unrelated component was added.
    let mut world = World::new();
    let bag = container(&mut world, 4);

    let a = lying_about(&mut world, Item::single("a"));
    let b = lying_about(&mut world, Item::single("b"));
    store(&mut world, a, bag).expect("room");
    store(&mut world, b, bag).expect("room");

    // Move `a` into a different archetype without changing anything about where it is stored.
    #[derive(Debug, Clone, Copy, PartialEq, amadeo_core::StableHash, amadeo_reflect::Reflect)]
    struct Unrelated {
        /// Anything at all.
        value: u32,
    }
    impl Component for Unrelated {}
    world.insert(a, Unrelated { value: 1 });

    assert_eq!(contents(&world, bag), vec![(0, a), (1, b)]);
}

#[test]
fn a_full_container_says_which_container_and_how_many_slots() {
    let mut world = World::new();
    let bag = container(&mut world, 1);

    let a = lying_about(&mut world, Item::single("a"));
    let b = lying_about(&mut world, Item::single("b"));
    store(&mut world, a, bag).expect("room");

    let error = store(&mut world, b, bag).expect_err("no room");
    let message = error.to_string();
    assert!(message.contains('b'), "{message}");
    assert!(message.contains("all 1 slots"), "{message}");
    assert!(
        message.contains("Take something out"),
        "an error a game can show has to say what would work: {message}"
    );
}

// --- The refusals --------------------------------------------------------------------------------

#[test]
fn something_that_is_not_an_item_is_refused_by_name() {
    let mut world = World::new();
    let bag = container(&mut world, 4);
    let rock = world.spawn();

    assert!(matches!(
        store(&mut world, rock, bag),
        Err(StoreError::NotAnItem { .. })
    ));
}

#[test]
fn something_that_is_not_a_container_is_refused_by_name() {
    let mut world = World::new();
    let key = lying_about(&mut world, Item::single("brass-key"));
    let wall = world.spawn();

    assert!(matches!(
        store(&mut world, key, wall),
        Err(StoreError::NotAContainer { .. })
    ));
}

#[test]
fn a_container_cannot_be_put_inside_itself() {
    let mut world = World::new();
    let bag = container(&mut world, 4);
    world.insert(bag, Item::single("bag"));

    assert!(matches!(
        store(&mut world, bag, bag),
        Err(StoreError::IntoItself { .. })
    ));
}

#[test]
fn a_bag_inside_a_bag_is_perfectly_fine() {
    // The feature the refusal above must not have broken.
    let mut world = World::new();
    let outer = container(&mut world, 4);
    let inner = container(&mut world, 4);
    world.insert(inner, Item::single("pouch"));
    world.insert(inner, Transform::default());

    let pebble = lying_about(&mut world, Item::single("pebble"));
    store(&mut world, pebble, inner).expect("the inner bag holds it");
    store(&mut world, inner, outer).expect("and goes into the outer one");

    assert_eq!(contents(&world, outer), vec![(0, inner)]);
    assert_eq!(
        contents(&world, inner),
        vec![(0, pebble)],
        "storing a container must not disturb what is in it"
    );
}

// --- Orphans -------------------------------------------------------------------------------------

#[test]
fn an_item_whose_container_died_is_reported_rather_than_destroyed() {
    // ADR 0015's call for a dangling `Parent`, one module along. Whether this is a leak or a
    // spilled bag is the game's decision, so nothing here acts on it.
    let mut world = World::new();
    let bag = container(&mut world, 4);
    let key = lying_about(&mut world, Item::single("brass-key"));
    store(&mut world, key, bag).expect("room");

    world.despawn(bag);

    assert_eq!(orphaned(&world), vec![key]);
    assert!(world.contains(key), "the item itself is untouched");

    // Written expecting this to come back empty, and it does not — which turned out to be the
    // better behaviour and is now documented on `contents`. A lookup by handle keeps answering, so
    // a game emptying a dead chest onto the floor can still see what was in it; filtering by
    // liveness would make an orphan invisible to every function here while still existing.
    assert_eq!(
        contents(&world, bag),
        vec![(0, key)],
        "a despawned container still answers, which is what makes an orphan recoverable"
    );

    // And it is not ambiguous, because a handle carries a generation: a new entity reusing the slot
    // is a different handle and inherits nothing.
    let reused = world.spawn();
    assert_ne!(reused, bag, "same index, later generation");
    assert!(contents(&world, reused).is_empty());
}
