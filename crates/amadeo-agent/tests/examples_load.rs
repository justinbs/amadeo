//! The property that makes `describe.example` worth having: **what it emits actually loads**.
//!
//! An example that merely looks plausible is worse than no example, because it fails at the moment
//! someone trusts it. So the test does not inspect the text — it pastes the emitted block into a
//! scene file and instantiates it with the engine's real registry, for every component the engine
//! has. If a future component reflects into something the scene format cannot express, this fails
//! rather than shipping advice that does not work.
//!
//! ADR 0030.

use amadeo_agent::describe_example;
use amadeo_ecs::{ComponentRegistry, World};
use amadeo_render::{Camera, Quad, SortOrder, Sprite};
use amadeo_transform::{GlobalTransform, Parent, Transform};

/// Every component in the engine that a scene can carry, in one registry.
fn engine_registry() -> ComponentRegistry {
    let mut registry = ComponentRegistry::new();
    registry.register::<Transform>().expect("registers");
    registry.register::<GlobalTransform>().expect("registers");
    registry.register::<Parent>().expect("registers");
    registry.register::<Quad>().expect("registers");
    registry.register::<Sprite>().expect("registers");
    registry.register::<SortOrder>().expect("registers");
    registry.register::<Camera>().expect("registers");
    registry
}

/// Components and resources together, the way `describe` assembles them.
fn full_schema() -> amadeo_reflect::TypeRegistry {
    let registry = engine_registry();
    let mut types = registry.types().clone();

    // A world with a resource in it, so the resource half of the schema is exercised. `Camera` is a
    // *component* since ADR 0031, so it comes in through the registry below instead.
    let world = World::new();
    world
        .register_resource_schemas(&mut types)
        .expect("no name collisions");

    types
}

/// Wraps one component block in the smallest scene that can hold it.
fn scene_around(block: &str) -> String {
    format!("scene example\nversion 1\n\nentity e1 \"Example\"\n{block}")
}

#[test]
fn every_component_example_parses_and_instantiates() {
    let registry = engine_registry();
    let types = registry.types();

    for name in registry.names() {
        let info = types
            .get(name)
            .expect("a registered component has a schema");
        let example = describe_example(info, types).expect("an example exists");

        let amadeo_agent::Json::Object(members) = &example else {
            panic!("`{name}`: an example is an object");
        };
        let Some(amadeo_agent::Json::String(block)) = members.get("scene") else {
            panic!(
                "`{name}`: every engine component is scene-expressible, so `scene` must be there"
            );
        };

        let source = scene_around(block);
        let document = amadeo_scene::parse(&source).unwrap_or_else(|error| {
            panic!("`{name}`: the emitted example does not parse: {error}\n{source}")
        });

        // Layer 2: the values have to fit the real component types, not merely be well-formed text.
        let diagnostics = amadeo_scene::validate(&document, &registry, None);
        assert!(
            diagnostics.is_empty(),
            "`{name}`: the emitted example does not validate: {diagnostics:?}\n{source}"
        );

        let mut world = World::new();
        amadeo_scene::instantiate(&document, &registry, &mut world).unwrap_or_else(|error| {
            panic!("`{name}`: the emitted example does not load: {error}\n{source}")
        });
    }
}

#[test]
fn an_example_respects_a_declared_range() {
    // `Camera::height` is annotated `min = 0.1`, and a zero-height camera is exactly the
    // plausible-but-wrong value an unbounded example would have suggested.
    let types = full_schema();
    let info = types.get("Camera").expect("registered");

    let example = describe_example(info, &types).expect("an example exists");
    let amadeo_agent::Json::Object(members) = &example else {
        panic!("an example is an object");
    };
    let Some(amadeo_agent::Json::Object(json)) = members.get("json") else {
        panic!("an example carries a json form");
    };

    let field = info
        .field("height")
        .expect("Camera has a height")
        .range
        .expect("and it is range-annotated, which is what this test is about");

    match json.get("height") {
        Some(amadeo_agent::Json::Float(height)) => assert!(
            *height >= field.min,
            "the example's height {height} is below the declared minimum {}",
            field.min
        ),
        other => panic!("height should be a number, found {other:?}"),
    }
}

#[test]
fn the_json_form_and_the_scene_form_agree() {
    // Both come out of one `Value`, so this is really a guard against someone later generating them
    // separately — at which point they would drift and one of the two would be a lie.
    let registry = engine_registry();
    let types = registry.types();
    let info = types.get("Transform").expect("registered");

    let example = describe_example(info, types).expect("an example exists");
    let amadeo_agent::Json::Object(members) = &example else {
        panic!("an example is an object");
    };
    let Some(amadeo_agent::Json::String(block)) = members.get("scene") else {
        panic!("Transform is scene-expressible");
    };

    let document = amadeo_scene::parse(&scene_around(block)).expect("parses");
    let component = document.entities[0]
        .components
        .get("Transform")
        .expect("the block is a Transform");

    let round_tripped = amadeo_agent::value_to_json(component);
    assert_eq!(
        round_tripped.to_compact(),
        members.get("json").expect("a json form").to_compact(),
        "the two spellings of one example must describe the same value"
    );
}
