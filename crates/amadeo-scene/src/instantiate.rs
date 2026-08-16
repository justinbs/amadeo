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

    /// The entity instances a prefab nothing has supplied.
    #[error(
        "entity `{entity}` instances the prefab `{prefab}`, and no prefab by that id was loaded.\n\
         A prefab is an asset (ADR 0029), so it needs a `.ama-meta` sidecar and a mention in the \
         scene's `assets` block. Run `amadeo assets` for the ids that exist"
    )]
    UnknownPrefab {
        /// The authoring id of the instance.
        entity: String,
        /// The prefab id it asked for.
        prefab: String,
    },

    /// A prefab document does not have exactly one root entity.
    #[error(
        "the prefab `{prefab}` has {found} root entities, and a prefab needs exactly one.\n\
         An instance *is* its prefab's root, so with none there is nothing to be and with several \
         there is no way to say which one the overrides apply to"
    )]
    PrefabRootCount {
        /// The prefab id.
        prefab: String,
        /// How many roots it actually has.
        found: usize,
    },

    /// A prefab instances itself, directly or through a chain.
    #[error(
        "the prefab `{prefab}` instances itself: {chain}.\n\
         Expanding that never finishes, so it is refused rather than attempted"
    )]
    PrefabCycle {
        /// The prefab that closed the loop.
        prefab: String,
        /// The chain, outermost first, joined by arrows.
        chain: String,
    },

    /// An override names a component the prefab's root does not have.
    ///
    /// **Deliberately fatal.** See the note at the override site for why dropping it instead is the
    /// behaviour this engine is specifically avoiding.
    #[error(
        "entity `{entity}` overrides `{component}`, but the prefab `{prefab}` does not put that \
         component on its root.\n\
         Either the prefab changed and this override is stale, or the name is a typo. Remove the \
         override, or use a plain `{component}` block to add the component instead of replacing it"
    )]
    DanglingOverride {
        /// The authoring id of the instance.
        entity: String,
        /// The component that was overridden.
        component: String,
        /// The prefab it instances.
        prefab: String,
    },

    /// A bare component block would replace something the prefab already supplied.
    #[error(
        "entity `{entity}` declares `{component}`, but its prefab already puts that component on \
         the root. Write `override {component}` to replace it \u{2014} spelling it out is what keeps \
         an override visible in the file (invariant I1)"
    )]
    ComponentAlreadyFromPrefab {
        /// The authoring id of the instance.
        entity: String,
        /// The contested component.
        component: String,
    },
}

/// Prefab documents, by asset id.
///
/// # Why the caller supplies this rather than the loader finding it
///
/// Resolving a prefab id means reading a file, and `amadeo-scene` deliberately does no I/O — the
/// same split that keeps `scene.check` able to validate text it was handed. The app layer builds
/// the library from `Assets`, which also makes ADR 0021's load barrier apply to prefabs for free: a
/// prefab is resident before the first tick, like every other asset.
#[derive(Debug, Default, Clone, PartialEq)]
pub struct PrefabLibrary {
    /// Ordered, so anything derived from it is reproducible (invariant I3).
    documents: BTreeMap<String, SceneDocument>,
}

impl PrefabLibrary {
    /// An empty library. A scene with no `from` lines needs nothing else.
    #[must_use]
    pub fn new() -> PrefabLibrary {
        PrefabLibrary::default()
    }

    /// Adds a prefab under an asset id, replacing any earlier one.
    pub fn insert(&mut self, id: impl Into<String>, document: SceneDocument) -> &mut Self {
        self.documents.insert(id.into(), document);
        self
    }

    /// The document for an id.
    #[must_use]
    pub fn get(&self, id: &str) -> Option<&SceneDocument> {
        self.documents.get(id)
    }

    /// Every prefab id, in order.
    pub fn ids(&self) -> impl Iterator<Item = &str> {
        self.documents.keys().map(String::as_str)
    }

    /// How many prefabs are loaded.
    #[must_use]
    pub fn len(&self) -> usize {
        self.documents.len()
    }

    /// Whether nothing is loaded.
    #[must_use]
    pub fn is_empty(&self) -> bool {
        self.documents.is_empty()
    }
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
    instantiate_with(document, registry, &PrefabLibrary::new(), world)
}

/// Creates every entity in `document`, resolving `from` lines against `prefabs`.
///
/// The prefab-aware form of [`instantiate`]. The library is supplied by the caller rather than
/// looked up here, which keeps this crate free of file I/O and puts the load at the app layer,
/// where ADR 0021's barrier already lives — a prefab is an asset, so it is resident before the
/// first tick like any other.
///
/// # It composes the hierarchy before it returns, and that is not a convenience
///
/// A loaded scene has a `GlobalTransform` on everything, immediately, without waiting for a tick.
///
/// [`propagate_transforms`](amadeo_transform::propagate_transforms) is a `PostSimulation` system, so
/// without this a freshly loaded world has none at all — and **every consumer falls back to the
/// local transform when one is missing**. For a root those are the same thing. For a child they are
/// not, so between a scene loading and the first tick *finishing*, every parented camera, light,
/// collider and mesh sits at its own local coordinates rather than where the file puts it.
///
/// That has now cost two sessions, in two different disguises. Session 18 lost most of one to a
/// torch beam authored at `y = -0.1` under a camera: at tick 0 it was drawn a tenth of a metre
/// *underground*, inside a floor slab, correctly shadowing the whole room with the floor it was
/// buried in — and the conclusion drawn was that the renderer was broken. Session 19 hit the same
/// fault one tick later, because `step_physics` runs in `Simulation` and propagation does not run
/// until `PostSimulation`: a scene whose props are prefab instances therefore has *all of their
/// colliders stacked at the piece origin for the whole of tick 1*, and the symptom was a door
/// reported as within arm's reach from ninety metres away.
///
/// Doing it here rather than asking every caller to remember is the choice this project keeps
/// making: the step that would be forgotten is the step whose absence has no symptom.
///
/// **Safe by construction.** `GlobalTransform` is DERIVED, so ADR 0019 keeps it out of the state
/// hash and computing it earlier cannot move a replay or a golden recording. A world that never
/// runs the system at all now gets one correct composition instead of none, which is strictly better
/// than the fallback it had.
///
/// # Errors
///
/// [`InstantiateError`], having despawned everything it created.
pub fn instantiate_with(
    document: &SceneDocument,
    registry: &ComponentRegistry,
    prefabs: &PrefabLibrary,
    world: &mut World,
) -> Result<Instantiated, InstantiateError> {
    let mut result = Instantiated::default();
    let mut created: Vec<Entity> = Vec::new();
    let mut context = Context {
        registry,
        prefabs,
        stack: Vec::new(),
    };

    let outcome = build(document, &mut context, world, &mut result, &mut created);

    if outcome.is_err() {
        // Unwind in reverse creation order. Not strictly necessary -- despawn handles any order --
        // but it keeps entity slot reuse predictable, which keeps state hashes predictable.
        for entity in created.into_iter().rev() {
            world.despawn(entity);
        }
        return outcome.map(|()| result);
    }

    // Only on success, so a scene that failed to load leaves nothing composed behind it.
    amadeo_transform::propagate_transforms(world);
    outcome.map(|()| result)
}

/// Expands a prefab onto an entity that already exists.
///
/// The instance entity has already been spawned and registered under its **own** authoring id, so
/// this only has to give it the prefab root's components and children.
///
/// # Why the prefab's own ids are not registered
///
/// A prefab's internal entities do not go into [`Instantiated::entities`]. Two reasons, and the
/// second is the load-bearing one:
///
/// - two instances of one prefab would collide on every internal id, and
/// - **nothing can refer to them anyway.** An override reaches only the instance root (ADR 0029),
///   so there is no syntax that could name something inside. Not registering them makes that
///   structural rather than a rule to remember.
fn instantiate_prefab(
    prefab_id: &str,
    source: &SceneEntity,
    entity: Entity,
    context: &mut Context<'_>,
    world: &mut World,
    created: &mut Vec<Entity>,
) -> Result<(), InstantiateError> {
    if context.stack.iter().any(|seen| seen == prefab_id) {
        let mut chain = context.stack.clone();
        chain.push(prefab_id.to_string());
        return Err(InstantiateError::PrefabCycle {
            prefab: prefab_id.to_string(),
            chain: chain.join(" -> "),
        });
    }

    let document =
        context
            .prefabs
            .get(prefab_id)
            .ok_or_else(|| InstantiateError::UnknownPrefab {
                entity: source.id.clone(),
                prefab: prefab_id.to_string(),
            })?;

    if document.entities.len() != 1 {
        return Err(InstantiateError::PrefabRootCount {
            prefab: prefab_id.to_string(),
            found: document.entities.len(),
        });
    }
    // Cloned because `context` is borrowed mutably below and the document lives inside it. A prefab
    // is small and this happens once per instance, at load.
    let root = document.entities[0].clone();

    context.stack.push(prefab_id.to_string());

    // The root's own components land on the instance entity, which is what makes an instance *be*
    // its prefab's root rather than merely contain one.
    let outcome = expand_root(&root, source, entity, context, world, created);

    context.stack.pop();
    outcome
}

/// Applies a prefab root's components and children to the instance entity.
fn expand_root(
    root: &SceneEntity,
    source: &SceneEntity,
    entity: Entity,
    context: &mut Context<'_>,
    world: &mut World,
    created: &mut Vec<Entity>,
) -> Result<(), InstantiateError> {
    // A prefab root may itself instance another prefab. Safe to recurse because an override can
    // never reach inside one, so there is no cross-level resolution to get wrong -- which is the
    // exact thing that makes Unity's nested prefabs lose overrides.
    if let Some(inner) = &root.prefab {
        instantiate_prefab(inner, root, entity, context, world, created)?;
    }

    for (name, value) in &root.components {
        context
            .registry
            .insert(world, entity, name, value)
            .map_err(|error| InstantiateError::Component {
                entity: source.id.clone(),
                source: Box::new(error),
            })?;
    }

    // A prefab root's own overrides, which apply to whatever it instanced in turn.
    for (name, value) in &root.overrides {
        let Some(existing) = context.registry.get(world, entity, name) else {
            return Err(InstantiateError::DanglingOverride {
                entity: root.id.clone(),
                component: name.clone(),
                prefab: root.prefab.clone().unwrap_or_default(),
            });
        };
        context
            .registry
            .insert(world, entity, name, &merge_over(&existing, value))
            .map_err(|error| InstantiateError::Component {
                entity: source.id.clone(),
                source: Box::new(error),
            })?;
    }

    for child in &root.children {
        spawn_prefab_child(child, entity, context, world, created)?;
    }
    Ok(())
}

/// Creates one entity inside a prefab, and everything beneath it.
///
/// Unlike [`spawn_entity`] this registers nothing in [`Instantiated`] — see the note on
/// [`instantiate_prefab`] for why prefab internals are deliberately unaddressable.
fn spawn_prefab_child(
    source: &SceneEntity,
    parent: Entity,
    context: &mut Context<'_>,
    world: &mut World,
    created: &mut Vec<Entity>,
) -> Result<(), InstantiateError> {
    let entity = world.spawn();
    created.push(entity);
    world.insert(entity, Parent(parent));

    if let Some(inner) = &source.prefab {
        instantiate_prefab(inner, source, entity, context, world, created)?;
    }

    for (name, value) in &source.components {
        context
            .registry
            .insert(world, entity, name, value)
            .map_err(|error| InstantiateError::Component {
                entity: source.id.clone(),
                source: Box::new(error),
            })?;
    }

    for child in &source.children {
        spawn_prefab_child(child, entity, context, world, created)?;
    }
    Ok(())
}

/// Lays an override's fields over the value the prefab supplied.
///
/// # Why an override is a patch rather than a replacement
///
/// A `Transform` has three fields, and moving a prefab instance should not mean restating its
/// rotation and scale to leave them alone. Merging means
///
/// ```text
/// entity w1 "Wall" from wall_tile
///   override Transform
///     translation 3.0 0.0 0.0
/// ```
///
/// says exactly what it looks like it says. Restating everything would still work — a full
/// override is just a patch that happens to cover every field — so this is strictly more permissive
/// than replacement, not different.
///
/// **Only the top level merges.** A field whose value is itself a struct is replaced whole, because
/// merging recursively would make `override Foo` with one deeply nested field ambiguous about how
/// much it was meant to touch. One level is the rule that stays explainable.
fn merge_over(
    base: &amadeo_reflect::Value,
    patch: &amadeo_reflect::Value,
) -> amadeo_reflect::Value {
    use amadeo_reflect::Value;

    let (Value::Struct(base_fields), Value::Struct(patch_fields)) = (base, patch) else {
        // Not both structs, so there is nothing to merge field-wise: the override wins outright.
        // Reached by a component that reflects as a scalar.
        return patch.clone();
    };

    let mut merged = base_fields.clone();
    for (name, value) in patch_fields {
        merged.insert(name.clone(), value.clone());
    }
    Value::Struct(merged)
}

/// Everything the recursion carries that is not the world.
///
/// A struct rather than four more parameters: `spawn_entity` already takes six, and a prefab makes
/// it recursive through two more levels.
struct Context<'a> {
    registry: &'a ComponentRegistry,
    prefabs: &'a PrefabLibrary,
    /// Prefab ids currently being expanded, outermost first. The cycle guard.
    stack: Vec<String>,
}

/// The fallible half of [`instantiate`], split out so the caller can roll back on failure.
fn build(
    document: &SceneDocument,
    context: &mut Context<'_>,
    world: &mut World,
    result: &mut Instantiated,
    created: &mut Vec<Entity>,
) -> Result<(), InstantiateError> {
    // Roots pass `None` as their parent, so nothing at the top level gets a `Parent` component.
    for entity in &document.entities {
        spawn_entity(entity, None, context, world, result, created)?;
    }
    Ok(())
}

/// Creates one entity and everything beneath it.
fn spawn_entity(
    source: &SceneEntity,
    parent: Option<(&str, Entity)>,
    context: &mut Context<'_>,
    world: &mut World,
    result: &mut Instantiated,
    created: &mut Vec<Entity>,
) -> Result<(), InstantiateError> {
    if result.entities.contains_key(&source.id) {
        return Err(InstantiateError::DuplicateId {
            id: source.id.clone(),
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

    // A prefab instance takes its components from the prefab first, then has this entity's own
    // blocks applied on top. Doing it in that order is what makes an `override` an override.
    if let Some(prefab_id) = &source.prefab {
        instantiate_prefab(prefab_id, source, entity, context, world, created)?;
    }

    for (name, value) in &source.components {
        // On a plain entity there is nothing to collide with. On an instance, a bare component block
        // *adds* something the prefab does not have -- and if the prefab does have it, the author
        // meant `override` and saying so is better than silently picking one.
        if source.prefab.is_some() && context.registry.get(world, entity, name).is_some() {
            return Err(InstantiateError::ComponentAlreadyFromPrefab {
                entity: source.id.clone(),
                component: name.clone(),
            });
        }
        context
            .registry
            .insert(world, entity, name, value)
            .map_err(|error| InstantiateError::Component {
                entity: source.id.clone(),
                source: Box::new(error),
            })?;
    }

    // Overrides last, so they win over everything the prefab supplied.
    for (name, value) in &source.overrides {
        // An override with no prefab is refused by the *parser* (`ParseErrorKind::OverrideWithoutPrefab`),
        // so it cannot reach here -- which is the right place for it: it is a syntax rule, and
        // catching it before a world is touched means a bad file never half-loads.
        // **The dangling-override rule.** Unity drops one of these silently, and it is a documented
        // source of lost work: the value reverts to the prefab's and nobody finds out until
        // something behaves wrong much later. Refusing means a prefab edit breaks every scene using
        // it at once, loudly, which is friction bought deliberately.
        let Some(existing) = context.registry.get(world, entity, name) else {
            return Err(InstantiateError::DanglingOverride {
                entity: source.id.clone(),
                component: name.clone(),
                prefab: source.prefab.clone().unwrap_or_default(),
            });
        };
        context
            .registry
            .insert(world, entity, name, &merge_over(&existing, value))
            .map_err(|error| InstantiateError::Component {
                entity: source.id.clone(),
                source: Box::new(error),
            })?;
    }

    for child in &source.children {
        spawn_entity(
            child,
            Some((&source.id, entity)),
            context,
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
    fn instantiate_without_a_library_says_the_prefab_is_not_loaded() {
        // The plain `instantiate` uses an empty library, so a scene with a `from` line reports the
        // prefab as missing rather than silently producing an entity with nothing on it. Callers
        // that want prefabs use `instantiate_with`; `App::load_scene` always does.
        let document = parse(
            "scene s
version 1

entity d \"Door\" from door_metal
",
        )
        .expect("parses");
        let mut world = World::new();

        let error = instantiate(&document, &registry(), &mut world).expect_err("no library");
        assert!(error.to_string().contains("door_metal"), "{error}");
        assert!(error.to_string().contains("amadeo assets"), "{error}");
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
