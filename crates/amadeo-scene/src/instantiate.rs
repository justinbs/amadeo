//! Turning a parsed [`SceneDocument`] into live entities — layer 2 of ADR 0014.
//!
//! Where [`crate::parse`] is pure syntax, this is where the reflection registry finally gets
//! consulted: every component name has to resolve, and every value has to fit the component's real
//! shape.

use crate::document::{SceneDocument, SceneEntity};
use amadeo_ecs::{ComponentRegistry, Entity, RegistryError, World};
use amadeo_transform::Parent;
use std::collections::BTreeMap;

/// What can go wrong turning a document into entities.
///
/// Every variant names the **authoring id** of the entity involved, because that is the thing the
/// reader can search the file for.
#[derive(Debug, Clone, PartialEq, Eq, thiserror::Error)]
pub enum InstantiateError {
    /// Two entities claim the same authoring id.
    #[error(
        "entity id `{id}` is used more than once; ids have to be unique because they are how \
         everything else refers to an entity"
    )]
    DuplicateId {
        /// The contested id.
        id: String,
    },

    /// A component would not resolve or would not build.
    #[error("entity `{entity}`: {source}")]
    Component {
        /// The authoring id of the entity being built.
        entity: String,
        /// What the registry objected to.
        ///
        /// Boxed because it is much the largest thing here — a `RegistryError` can carry a
        /// `ReflectError` carrying three strings — and every `Result` in this module would
        /// otherwise be sized for the unhappy path.
        #[source]
        source: Box<RegistryError>,
    },

    /// The entity instances a prefab, and prefabs cannot be resolved yet.
    #[error(
        "entity `{entity}` instances the prefab `{prefab}`, which cannot be loaded yet: \
         resolving a prefab path needs the asset layer (`amadeo-assets`, M1). \
         The scene parses correctly; it just cannot be instantiated until then"
    )]
    PrefabNotSupported {
        /// The authoring id of the instance.
        entity: String,
        /// The prefab path it asked for.
        prefab: String,
    },
}

/// The result of loading a scene: the bridge between authoring ids and runtime handles.
///
/// ADR 0004 and `docs/04-subsystems.md` §3 call these two ID spaces out explicitly — a stable,
/// human-meaningful id that lives in the file, and a generational handle that lives for one run.
/// This is the mapping between them, and anything that wants to resolve a cross-file reference needs
/// it.
#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub struct Instantiated {
    /// Authoring id to runtime handle, for every entity created.
    pub entities: BTreeMap<String, Entity>,
    /// Child authoring id to parent authoring id, for every non-root entity.
    ///
    /// The hierarchy is also **materialised** as [`Parent`] components on the entities themselves
    /// (ADR 0015) — this map is the authoring-id view of the same fact, for anything resolving a
    /// reference by id without walking the world.
    pub parents: BTreeMap<String, String>,
}

/// Creates every entity in `document` and attaches its components.
///
/// # Atomicity
///
/// If anything fails, **every entity this call created is despawned** before the error is returned.
/// A half-loaded scene is worse than no scene: it looks like it worked, and the missing half turns
/// up much later as a mysteriously absent object.
///
/// # Errors
///
/// See [`InstantiateError`]. Component problems carry the registry's own message, which lists every
/// valid component name when a name does not resolve.
pub fn instantiate(
    document: &SceneDocument,
    registry: &ComponentRegistry,
    world: &mut World,
) -> Result<Instantiated, InstantiateError> {
    let mut result = Instantiated::default();
    let mut created: Vec<Entity> = Vec::new();

    let outcome = build(document, registry, world, &mut result, &mut created);

    if outcome.is_err() {
        // Unwind in reverse creation order. Not strictly necessary -- despawn handles any order --
        // but it keeps entity slot reuse predictable, which keeps state hashes predictable.
        for entity in created.into_iter().rev() {
            world.despawn(entity);
        }
    }

    outcome.map(|()| result)
}

/// The fallible half of [`instantiate`], split out so the caller can roll back on failure.
fn build(
    document: &SceneDocument,
    registry: &ComponentRegistry,
    world: &mut World,
    result: &mut Instantiated,
    created: &mut Vec<Entity>,
) -> Result<(), InstantiateError> {
    // Roots pass `None` as their parent, so nothing at the top level gets a `Parent` component.
    for entity in &document.entities {
        spawn_entity(entity, None, registry, world, result, created)?;
    }
    Ok(())
}

/// Creates one entity and everything beneath it.
fn spawn_entity(
    source: &SceneEntity,
    parent: Option<(&str, Entity)>,
    registry: &ComponentRegistry,
    world: &mut World,
    result: &mut Instantiated,
    created: &mut Vec<Entity>,
) -> Result<(), InstantiateError> {
    if result.entities.contains_key(&source.id) {
        return Err(InstantiateError::DuplicateId {
            id: source.id.clone(),
        });
    }

    if let Some(prefab) = &source.prefab {
        return Err(InstantiateError::PrefabNotSupported {
            entity: source.id.clone(),
            prefab: prefab.clone(),
        });
    }

    let entity = world.spawn();
    created.push(entity);
    result.entities.insert(source.id.clone(), entity);

    if let Some((parent_id, parent_entity)) = parent {
        result
            .parents
            .insert(source.id.clone(), parent_id.to_string());
        // The file expresses hierarchy by nesting; this is where that becomes data the engine can
        // query (ADR 0004, ADR 0015). Inserted before the entity's own components so that a
        // component failing later still rolls the whole thing back.
        world.insert(entity, Parent(parent_entity));
    }

    for (name, value) in &source.components {
        registry
            .insert(world, entity, name, value)
            .map_err(|error| InstantiateError::Component {
                entity: source.id.clone(),
                source: Box::new(error),
            })?;
    }

    for child in &source.children {
        spawn_entity(
            child,
            Some((&source.id, entity)),
            registry,
            world,
            result,
            created,
        )?;
    }

    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::parse;
    use amadeo_core::StableHash;
    use amadeo_ecs::Component;
    use amadeo_reflect::Reflect;

    /// Where something is.
    #[derive(Debug, Clone, Copy, PartialEq, StableHash, Reflect)]
    struct Position {
        /// Horizontal.
        x: f32,
        /// Vertical.
        y: f32,
    }
    impl Component for Position {}

    /// Marks the player.
    #[derive(Debug, Clone, Copy, PartialEq, StableHash, Reflect)]
    struct Player;
    impl Component for Player {}

    fn registry() -> ComponentRegistry {
        let mut registry = ComponentRegistry::new();
        registry.register::<Position>().expect("registers");
        registry.register::<Player>().expect("registers");
        registry
    }

    #[test]
    fn a_scene_file_becomes_entities_with_components() {
        let source = "\
scene level
version 1

entity hero \"Hero\"
  Player
  Position
    x 1.5
    y -2.0
";
        let document = parse(source).expect("parses");
        let mut world = World::new();
        let loaded = instantiate(&document, &registry(), &mut world).expect("instantiates");

        let hero = loaded.entities["hero"];
        assert_eq!(world.entity_count(), 1);
        assert!(world.has::<Player>(hero));
        assert_eq!(
            world.get::<Position>(hero),
            Some(&Position { x: 1.5, y: -2.0 })
        );
    }

    #[test]
    fn integers_written_without_a_decimal_still_fill_float_fields() {
        // The parser has no schema, so `x 2` arrives as an integer. A designer writing a whole
        // number into a float field is doing something completely ordinary.
        let document =
            parse("scene s\nversion 1\n\nentity a \"A\"\n  Position\n    x 2\n    y 3\n")
                .expect("parses");
        let mut world = World::new();
        let loaded = instantiate(&document, &registry(), &mut world).expect("instantiates");

        assert_eq!(
            world.get::<Position>(loaded.entities["a"]),
            Some(&Position { x: 2.0, y: 3.0 })
        );
    }

    #[test]
    fn nesting_becomes_real_parent_components() {
        let source = "\
scene level
version 1

entity room \"Room\"

  entity lamp \"Lamp\"

    entity bulb \"Bulb\"
";
        let document = parse(source).expect("parses");
        let mut world = World::new();
        let loaded = instantiate(&document, &registry(), &mut world).expect("instantiates");

        assert_eq!(world.entity_count(), 3);

        // The map view, by authoring id.
        assert_eq!(loaded.parents.get("lamp").map(String::as_str), Some("room"));
        assert_eq!(loaded.parents.get("bulb").map(String::as_str), Some("lamp"));
        assert_eq!(loaded.parents.get("room"), None, "a root has no parent");

        // And the same fact as queryable components, which is what the engine actually reads.
        let room = loaded.entities["room"];
        let lamp = loaded.entities["lamp"];
        let bulb = loaded.entities["bulb"];

        assert_eq!(world.get::<Parent>(lamp).map(|p| p.0), Some(room));
        assert_eq!(world.get::<Parent>(bulb).map(|p| p.0), Some(lamp));
        assert!(
            !world.has::<Parent>(room),
            "a root gets no Parent component"
        );
    }

    #[test]
    fn a_parent_link_survives_being_written_and_read_as_a_component() {
        // Guards the thing that would silently half-work: `Parent` is inserted directly rather than
        // through the registry, so nothing in the scene path would notice if it stopped being a
        // usable component.
        let document =
            parse("scene s\nversion 1\n\nentity a \"A\"\n\n  entity b \"B\"\n").expect("parses");
        let mut world = World::new();
        let loaded = instantiate(&document, &registry(), &mut world).expect("instantiates");

        let children: Vec<_> = world
            .iter::<Parent>()
            .map(|(entity, parent)| (entity, parent.0))
            .collect();
        assert_eq!(children, vec![(loaded.entities["b"], loaded.entities["a"])]);
    }

    #[test]
    fn an_unknown_component_names_the_entity_and_lists_what_is_valid() {
        let document = parse("scene s\nversion 1\n\nentity a \"A\"\n  Postion\n    x 1.0\n")
            .expect("parses -- it is a schema problem, not a syntax one");
        let mut world = World::new();

        let error =
            instantiate(&document, &registry(), &mut world).expect_err("`Postion` is a typo");
        let message = error.to_string();

        assert!(message.contains("entity `a`"), "{message}");
        assert!(
            message.contains("no component named `Postion`"),
            "{message}"
        );
        assert!(message.contains("Player, Position"), "{message}");
    }

    #[test]
    fn a_failed_load_leaves_no_entities_behind() {
        // The property that matters most here: a half-loaded scene looks like it worked, and the
        // missing half turns up much later as a mysteriously absent object.
        let source = "\
scene level
version 1

entity good \"Good\"
  Player

entity bad \"Bad\"
  NotAComponent
    x 1.0
";
        let document = parse(source).expect("parses");
        let mut world = World::new();

        instantiate(&document, &registry(), &mut world).expect_err("the second entity fails");
        assert_eq!(
            world.entity_count(),
            0,
            "the first entity must be rolled back too"
        );
    }

    #[test]
    fn a_bad_value_reports_the_entity_and_the_field_problem() {
        let document =
            parse("scene s\nversion 1\n\nentity a \"A\"\n  Position\n    x here\n    y 1.0\n")
                .expect("parses -- `here` is a valid identifier syntactically");
        let mut world = World::new();

        let error =
            instantiate(&document, &registry(), &mut world).expect_err("`here` is not a number");
        let message = error.to_string();
        assert!(message.contains("entity `a`"), "{message}");
        assert!(message.contains("expected a number"), "{message}");
    }

    #[test]
    fn a_prefab_instance_says_what_is_missing_rather_than_failing_vaguely() {
        let document =
            parse("scene s\nversion 1\n\nentity d \"Door\" from prefabs/door\n").expect("parses");
        let mut world = World::new();

        let error = instantiate(&document, &registry(), &mut world)
            .expect_err("prefabs are not loadable yet");
        assert_eq!(
            error,
            InstantiateError::PrefabNotSupported {
                entity: "d".to_string(),
                prefab: "prefabs/door".to_string(),
            }
        );
        assert!(error.to_string().contains("amadeo-assets"));
    }

    #[test]
    fn duplicate_ids_are_refused_at_load_time() {
        // The parser allows them -- the file is syntactically fine -- so this is where they stop.
        let document =
            parse("scene s\nversion 1\n\nentity a \"A\"\n\nentity a \"Also A\"\n").expect("parses");
        let mut world = World::new();

        let error = instantiate(&document, &registry(), &mut world).expect_err("`a` is used twice");
        assert_eq!(
            error,
            InstantiateError::DuplicateId {
                id: "a".to_string()
            }
        );
        assert_eq!(world.entity_count(), 0, "and nothing is left behind");
    }

    #[test]
    fn loading_the_same_scene_twice_gives_identical_state() {
        // Determinism at the authoring boundary (I3): the same file must produce the same world,
        // which is what makes a scene safe to use as a replay's starting state.
        let source = "\
scene level
version 1

entity a \"A\"
  Position
    x 1.0
    y 2.0

  entity b \"B\"
    Player
";
        let document = parse(source).expect("parses");

        let mut first = World::new();
        instantiate(&document, &registry(), &mut first).expect("instantiates");
        let mut second = World::new();
        instantiate(&document, &registry(), &mut second).expect("instantiates");

        assert_eq!(first.state_hash(), second.state_hash());
    }
}
