//! Prefab instancing, and the rules ADR 0029 chose to avoid Unity's failure modes.
//!
//! # What is actually being defended here
//!
//! Unity's prefab overrides are a documented source of lost work: they live in editor state, are
//! easy to create by accident, and with nesting they *evaporate* — on Apply, or when a nested
//! prefab loses the object an override targeted, or when stored file IDs desync. Practitioners
//! advise keeping nesting under two levels for exactly that reason.
//!
//! ADR 0029 avoids all of it with one rule: **an override reaches the instance root and nothing
//! else.** There is no syntax that can name something inside a prefab, so there is nothing for an
//! override to lose track of — which is what makes `nesting_is_safe_because_overrides_cannot_reach_
//! inside` true rather than hopeful.

use amadeo_core::StableHash;
use amadeo_ecs::{Component, ComponentRegistry, World};
use amadeo_reflect::Reflect;
use amadeo_scene::{InstantiateError, PrefabLibrary, instantiate_with, parse};
use amadeo_transform::Parent;

/// Where something is.
#[derive(Debug, Clone, Copy, PartialEq, StableHash, Reflect)]
struct Position {
    /// Across.
    x: f32,
    /// Up.
    y: f32,
}
impl Component for Position {}

/// How solid something is.
#[derive(Debug, Clone, Copy, PartialEq, StableHash, Reflect)]
struct Armour {
    /// Points of it.
    rating: u32,
}
impl Component for Armour {}

/// Marks a door.
#[derive(Debug, Clone, Copy, PartialEq, StableHash, Reflect)]
struct Door;
impl Component for Door {}

fn registry() -> ComponentRegistry {
    let mut registry = ComponentRegistry::new();
    registry.register::<Position>().expect("registers");
    registry.register::<Armour>().expect("registers");
    registry.register::<Door>().expect("registers");
    registry
}

/// A prefab: one root with a position and some armour.
const WALL_TILE: &str = "\
scene wall_tile
version 1

entity root \"Wall\"
  Armour
    rating 5
  Position
    x 0.0
    y 0.0
";

fn library() -> PrefabLibrary {
    let mut library = PrefabLibrary::new();
    library.insert("wall_tile", parse(WALL_TILE).expect("the prefab parses"));
    library
}

/// Instantiates a scene against a library, returning the world.
fn load(scene: &str, prefabs: &PrefabLibrary) -> Result<World, InstantiateError> {
    let document = parse(scene).expect("the scene parses");
    let mut world = World::new();
    instantiate_with(&document, &registry(), prefabs, &mut world)?;
    Ok(world)
}

#[test]
fn an_instance_gets_the_prefabs_components() {
    let world = load(
        "scene s\nversion 1\n\nentity w1 \"A wall\" from wall_tile\n",
        &library(),
    )
    .expect("instantiates");

    let entity = world.entities()[0];
    assert_eq!(world.get::<Armour>(entity), Some(&Armour { rating: 5 }));
    assert_eq!(
        world.get::<Position>(entity),
        Some(&Position { x: 0.0, y: 0.0 })
    );
}

#[test]
fn an_override_replaces_a_component_the_prefab_supplied() {
    // The whole point: one file says what a wall is, and each instance says where it is.
    let world = load(
        "scene s\nversion 1\n\nentity w1 \"A wall\" from wall_tile\n  override Position\n    x 3.0\n    y -1.0\n",
        &library(),
    )
    .expect("instantiates");

    let entity = world.entities()[0];
    assert_eq!(
        world.get::<Position>(entity),
        Some(&Position { x: 3.0, y: -1.0 })
    );
    // And everything not overridden still comes from the prefab.
    assert_eq!(world.get::<Armour>(entity), Some(&Armour { rating: 5 }));
}

#[test]
fn a_bare_component_block_adds_something_the_prefab_lacks() {
    // The other half of the distinction: `override` replaces, a plain block adds. Both are visible
    // in the file, which is invariant I1's requirement on override state.
    let world = load(
        "scene s\nversion 1\n\nentity w1 \"A door\" from wall_tile\n  Door\n",
        &library(),
    )
    .expect("instantiates");

    let entity = world.entities()[0];
    assert!(world.get::<Door>(entity).is_some());
    assert_eq!(world.get::<Armour>(entity), Some(&Armour { rating: 5 }));
}

#[test]
fn two_instances_of_one_prefab_do_not_collide() {
    // Prefab-internal ids are not registered, so a prefab used twice is not a duplicate-id error.
    // That falls out of overrides being root-only: nothing can name a prefab's internals anyway.
    let world = load(
        "scene s\nversion 1\n\nentity a \"One\" from wall_tile\n\nentity b \"Two\" from wall_tile\n",
        &library(),
    )
    .expect("instantiates");

    assert_eq!(world.entities().len(), 2);
}

#[test]
fn a_prefabs_children_come_along() {
    let mut prefabs = PrefabLibrary::new();
    prefabs.insert(
        "post",
        parse("scene post\nversion 1\n\nentity root \"Post\"\n  Position\n    x 0.0\n    y 0.0\n\n  entity top \"Top\"\n    Armour\n      rating 1\n")
            .expect("parses"),
    );

    let world = load(
        "scene s\nversion 1\n\nentity p \"A post\" from post\n",
        &prefabs,
    )
    .expect("instantiates");

    assert_eq!(world.entities().len(), 2, "the root and its child");
    let child = world
        .entities()
        .into_iter()
        .find(|entity| world.get::<Parent>(*entity).is_some())
        .expect("the child carries a Parent");
    assert_eq!(world.get::<Armour>(child), Some(&Armour { rating: 1 }));
}

// --- The rules that exist to avoid Unity's failure modes ---

#[test]
fn a_stale_override_is_refused_rather_than_dropped() {
    // **The decision this whole design turns on.** Unity drops one of these silently and the value
    // reverts to the prefab's; nobody finds out until something behaves wrong much later. Here the
    // load fails, naming the entity, the component, and the prefab.
    let mut prefabs = PrefabLibrary::new();
    prefabs.insert(
        "bare",
        parse("scene bare\nversion 1\n\nentity root \"Bare\"\n  Position\n    x 0.0\n    y 0.0\n")
            .expect("parses"),
    );

    let error = load(
        "scene s\nversion 1\n\nentity w \"A wall\" from bare\n  override Armour\n    rating 9\n",
        &prefabs,
    )
    .expect_err("the prefab has no Armour to override");

    let message = error.to_string();
    assert!(message.contains("Armour"), "{message}");
    assert!(message.contains("bare"), "{message}");
    // And it says what to do instead.
    assert!(message.contains("plain `Armour` block"), "{message}");
}

#[test]
fn a_bare_block_that_would_shadow_the_prefab_is_refused() {
    // Silently letting it win either way would make an override invisible in the file, which is the
    // hidden-state problem invariant I1 exists to prevent.
    let error = load(
        "scene s\nversion 1\n\nentity w \"A wall\" from wall_tile\n  Armour\n    rating 9\n",
        &library(),
    )
    .expect_err("Armour already comes from the prefab");

    assert!(error.to_string().contains("override Armour"), "{error}");
}

#[test]
fn an_override_without_a_prefab_is_refused_at_parse_time() {
    // Caught by the *parser*, not by instantiation — which is the better place for it. It is a
    // syntax rule, so it is answerable from the text alone, and catching it there means a bad file
    // never touches a world at all. Found by writing the instantiate-level check first and
    // discovering it was unreachable.
    let error =
        parse("scene s\nversion 1\n\nentity w \"Nothing\"\n  override Position\n    x 1.0\n")
            .expect_err("nothing to override");

    assert!(
        error.to_string().contains("does not instance a prefab"),
        "{error}"
    );
}

#[test]
fn an_unknown_prefab_says_where_to_look() {
    let error = load(
        "scene s\nversion 1\n\nentity w \"A wall\" from no_such_thing\n",
        &library(),
    )
    .expect_err("no such prefab");

    let message = error.to_string();
    assert!(message.contains("no_such_thing"), "{message}");
    assert!(message.contains("amadeo assets"), "{message}");
}

#[test]
fn a_prefab_with_two_roots_is_refused() {
    // An instance *is* its prefab's root, so with two there is no way to say which the overrides
    // apply to. Refusing beats picking the first.
    let mut prefabs = PrefabLibrary::new();
    prefabs.insert(
        "pair",
        parse("scene pair\nversion 1\n\nentity a \"A\"\n  Door\n\nentity b \"B\"\n  Door\n")
            .expect("parses"),
    );

    let error =
        load("scene s\nversion 1\n\nentity w \"W\" from pair\n", &prefabs).expect_err("two roots");

    assert!(error.to_string().contains("2 root entities"), "{error}");
}

#[test]
fn nesting_is_safe_because_overrides_cannot_reach_inside() {
    // A prefab instancing another prefab. This is where Unity's overrides evaporate, and it is safe
    // here for a structural reason: an override can only ever name the instance root, so there is
    // no cross-level resolution to get wrong.
    let mut prefabs = library();
    prefabs.insert(
        "reinforced",
        parse("scene reinforced\nversion 1\n\nentity root \"Reinforced\" from wall_tile\n  override Armour\n    rating 20\n")
            .expect("parses"),
    );

    let world = load(
        "scene s\nversion 1\n\nentity w \"A wall\" from reinforced\n  override Position\n    x 7.0\n    y 0.0\n",
        &prefabs,
    )
    .expect("instantiates");

    let entity = world.entities()[0];
    // The inner prefab supplied Position, the middle prefab raised Armour, the instance moved it.
    assert_eq!(world.get::<Armour>(entity), Some(&Armour { rating: 20 }));
    assert_eq!(
        world.get::<Position>(entity),
        Some(&Position { x: 7.0, y: 0.0 })
    );
}

#[test]
fn a_prefab_cycle_is_refused_rather_than_expanded_forever() {
    let mut prefabs = PrefabLibrary::new();
    prefabs.insert(
        "loop_a",
        parse("scene loop_a\nversion 1\n\nentity root \"A\" from loop_b\n").expect("parses"),
    );
    prefabs.insert(
        "loop_b",
        parse("scene loop_b\nversion 1\n\nentity root \"B\" from loop_a\n").expect("parses"),
    );

    let error = load(
        "scene s\nversion 1\n\nentity w \"W\" from loop_a\n",
        &prefabs,
    )
    .expect_err("a cycle");

    let message = error.to_string();
    assert!(message.contains("loop_a -> loop_b -> loop_a"), "{message}");
    assert!(message.contains("never finishes"), "{message}");
}

#[test]
fn a_failed_instance_leaves_no_entities_behind() {
    // Atomicity, which prefabs make more important rather than less: one bad override could
    // otherwise leave a whole prefab's worth of entities in a world that reported failure.
    let mut world = World::new();
    let document = parse(
        "scene s\nversion 1\n\nentity ok \"Fine\" from wall_tile\n\nentity bad \"Bad\" from wall_tile\n  override Door\n",
    )
    .expect("parses");

    let before = world.entities().len();
    instantiate_with(&document, &registry(), &library(), &mut world)
        .expect_err("the second entity's override is stale");

    assert_eq!(world.entities().len(), before, "everything was rolled back");
}

#[test]
fn a_prefab_reference_counts_as_a_required_asset() {
    // ADR 0029: a prefab is an asset, so ADR 0021's barrier covers it without anything special --
    // and writing `from wall_tile` is itself the declaration, with no need to repeat it in the
    // `assets` block.
    let document = parse("scene s\nversion 1\n\nentity w \"W\" from wall_tile\n").expect("parses");
    assert!(document.required_assets().contains("wall_tile"));
}

#[test]
fn an_override_patches_named_fields_and_leaves_the_rest() {
    // What makes prefabs pleasant rather than merely possible: moving an instance should not mean
    // restating every field of the component just to leave them alone.
    let world = load(
        "scene s\nversion 1\n\nentity w \"A wall\" from wall_tile\n  override Position\n    x 9.0\n",
        &library(),
    )
    .expect("instantiates");

    let entity = world.entities()[0];
    assert_eq!(
        world.get::<Position>(entity),
        Some(&Position { x: 9.0, y: 0.0 }),
        "x came from the override, y from the prefab"
    );
}

#[test]
fn a_full_override_still_works() {
    // A patch that happens to cover every field. Merging is strictly more permissive than
    // replacement, so nothing written the old way stops working.
    let world = load(
        "scene s\nversion 1\n\nentity w \"A wall\" from wall_tile\n  override Position\n    x 1.0\n    y 2.0\n",
        &library(),
    )
    .expect("instantiates");

    let entity = world.entities()[0];
    assert_eq!(
        world.get::<Position>(entity),
        Some(&Position { x: 1.0, y: 2.0 })
    );
}
