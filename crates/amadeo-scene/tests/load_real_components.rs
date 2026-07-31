//! Loading a scene made of the engine's **real** components, end to end.
//!
//! Everything built in M1 so far meets here: the format (ADR 0014) parses, the reflection registry
//! (ADR 0012) resolves the names, the `Component: Reflect` bound (ADR 0013) guarantees every
//! component can be built from a value, and the result is entities in a `World`.
//!
//! A test using invented components would prove the plumbing. This proves the engine.

use amadeo_ecs::{ComponentRegistry, World};
use amadeo_render::{Quad, Transform2d};
use amadeo_scene::{instantiate, parse, to_text};

/// A fragment of the sort of level M3's horror slice needs.
const SCENE: &str = "\
scene corridor_a
version 1

entity floor \"Floor\"
  Quad
    color 0.243 0.286 0.333 1.0
    layer 0
    size 12.0 0.4
  Transform2d
    position 0.0 -3.0
    rotation 0.0
    scale 1.0 1.0

  entity marker \"Marker\"
    Quad
      color 0.898 0.588 0.243 1.0
      layer 10
      size 1.0 1.0
    Transform2d
      position 2.0 1.0
      rotation 0.25
      scale 2.0 2.0
";

fn registry() -> ComponentRegistry {
    let mut registry = ComponentRegistry::new();
    registry.register::<Transform2d>().expect("registers");
    registry.register::<Quad>().expect("registers");
    registry
}

#[test]
fn a_scene_file_loads_into_a_world_with_real_components() {
    let document = parse(SCENE).expect("parses");
    let mut world = World::new();
    let loaded = instantiate(&document, &registry(), &mut world).expect("instantiates");

    assert_eq!(world.entity_count(), 2);

    let marker = loaded.entities["marker"];
    assert_eq!(
        world.get::<Transform2d>(marker),
        Some(&Transform2d {
            position: [2.0, 1.0],
            rotation: 0.25,
            scale: [2.0, 2.0],
        })
    );
    assert_eq!(
        world.get::<Quad>(marker),
        Some(&Quad::new(1.0, 1.0, [0.898, 0.588, 0.243, 1.0]).on_layer(10))
    );

    // Nesting came through as a recorded parent link.
    assert_eq!(
        loaded.parents.get("marker").map(String::as_str),
        Some("floor")
    );
}

#[test]
fn the_scene_is_already_canonical() {
    // Hand-written above, and unchanged by the formatter -- which is the practical test of whether
    // the canonical form is one a person would naturally write. If `amadeo fmt` rearranged this,
    // the format would be fighting its author.
    let document = parse(SCENE).expect("parses");
    assert_eq!(to_text(&document), SCENE);
}

#[test]
fn a_loaded_scene_is_queryable_by_the_systems_that_will_draw_it() {
    // The renderer reads (Transform2d, Quad) pairs. If a loaded scene did not answer that query,
    // the format would be producing data the engine cannot use.
    let document = parse(SCENE).expect("parses");
    let mut world = World::new();
    instantiate(&document, &registry(), &mut world).expect("instantiates");

    let drawable: Vec<i32> = world
        .iter_pair::<Quad, Transform2d>()
        .map(|(_entity, quad, _transform)| quad.layer)
        .collect();
    assert_eq!(drawable.len(), 2);
    assert!(drawable.contains(&0) && drawable.contains(&10));
}

#[test]
fn loading_is_deterministic_across_worlds() {
    // I3 at the authoring boundary: the same file must produce the same state hash, which is what
    // makes a scene safe as a replay's starting point.
    let document = parse(SCENE).expect("parses");

    let mut first = World::new();
    instantiate(&document, &registry(), &mut first).expect("instantiates");
    let mut second = World::new();
    instantiate(&document, &registry(), &mut second).expect("instantiates");

    assert_eq!(first.state_hash(), second.state_hash());
}

#[test]
fn a_component_the_registry_does_not_know_names_the_real_ones() {
    // The registry only has Transform2d and Quad. Asking for Camera2d -- a real engine type that
    // simply was not registered -- must say so and list what is available, because "which module
    // did I forget to load" is the actual question being asked.
    let source = "scene s\nversion 1\n\nentity a \"A\"\n  Camera2d\n    height 10.0\n";
    let document = parse(source).expect("parses -- it is a schema problem, not a syntax one");
    let mut world = World::new();

    let error =
        instantiate(&document, &registry(), &mut world).expect_err("Camera2d is unregistered");
    let message = error.to_string();

    assert!(message.contains("entity `a`"), "{message}");
    assert!(message.contains("`Camera2d`"), "{message}");
    assert!(message.contains("Quad, Transform2d"), "{message}");
    assert_eq!(world.entity_count(), 0, "and nothing is left behind");
}

#[test]
fn a_field_that_does_not_exist_on_a_real_component_is_caught() {
    // The "plausible but wrong" failure Pillar 2 exists to kill: `rotation_degrees` is a completely
    // reasonable guess, and it is not a field.
    let source = "\
scene s
version 1

entity a \"A\"
  Transform2d
    position 0.0 0.0
    rotation_degrees 90.0
    scale 1.0 1.0
";
    let document = parse(source).expect("parses");
    let mut world = World::new();

    let error = instantiate(&document, &registry(), &mut world).expect_err("no such field");
    let message = error.to_string();
    assert!(
        message.contains("unknown field `rotation_degrees`"),
        "{message}"
    );
    assert!(
        message.contains("position, rotation, scale"),
        "the message should list the real fields: {message}"
    );
}
