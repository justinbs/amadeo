//! Constructing components by name, which is what makes a text file able to build a world.

use crate::component::Component;
use crate::entity::Entity;
use crate::world::World;
use amadeo_reflect::{ReflectError, TypeInfo, TypeRegistry, Value};
use std::collections::BTreeMap;

/// Inserts a component of one concrete type, reached through a plain function pointer.
///
/// # Why a function pointer rather than a trait object
///
/// [`amadeo_reflect::Reflect`] is not object-safe — `from_value` returns `Self` — and ADR 0012 chose
/// that deliberately rather than contorting the trait. The way back is this: a generic function that
/// captures nothing monomorphises per component type and coerces to a plain `fn`, so a map of them
/// gives type-erased construction with no `dyn`, no downcast, and no allocation.
type Inserter = fn(&mut World, Entity, &Value) -> Result<(), ReflectError>;

/// The monomorphised body behind each [`Inserter`].
///
/// Non-capturing, so `insert_component::<Health>` is a `fn` value that can live in a map.
fn insert_component<T: Component>(
    world: &mut World,
    entity: Entity,
    value: &Value,
) -> Result<(), ReflectError> {
    let component = T::from_value(value)?;
    world.insert(entity, component);
    Ok(())
}

/// What can go wrong building a component by name.
#[derive(Debug, Clone, PartialEq, Eq, thiserror::Error)]
pub enum RegistryError {
    /// No component is registered under that name.
    #[error(
        "no component named `{name}` is registered. Registered components are: {known}.\n\
         If this is a new component, register it with `registry.register::<{name}>()`; \
         if it belongs to a module, that module may not be loaded"
    )]
    UnknownComponent {
        /// The name that was asked for.
        name: String,
        /// Every name that would have worked, comma separated.
        known: String,
    },

    /// The name resolved, but the value did not fit the component.
    #[error("component `{name}`: {source}")]
    BadValue {
        /// The component being built.
        name: String,
        /// What `from_value` objected to.
        #[source]
        source: ReflectError,
    },

    /// The entity handle was stale by the time the component was inserted.
    #[error("cannot add `{name}`: entity {entity:?} no longer exists")]
    DeadEntity {
        /// The component being built.
        name: String,
        /// The stale handle.
        entity: Entity,
    },
}

/// Every component the engine can build from a name and a [`Value`].
///
/// This is the bridge between reflection and the ECS. `amadeo-reflect` knows what a `Transform2d`
/// *looks like*; this knows how to put one on an entity. A scene file needs both.
///
/// It owns a [`TypeRegistry`] rather than sitting beside one, so [`ComponentRegistry::register`] is
/// a single call that satisfies invariant I8 — there is no way to register the constructor and
/// forget the schema, or the reverse.
#[derive(Debug, Default)]
pub struct ComponentRegistry {
    /// Schemas, for `amadeo describe` and the editor.
    types: TypeRegistry,
    /// Constructors, keyed by the same canonical names. `BTreeMap` so listings are reproducible.
    inserters: BTreeMap<String, Inserter>,
}

impl ComponentRegistry {
    /// An empty registry.
    #[must_use]
    pub fn new() -> Self {
        Self::default()
    }

    /// Registers a component's schema and its constructor together.
    ///
    /// Registering the same component twice is harmless, matching
    /// [`TypeRegistry::register`]'s behaviour — modules will register overlapping sets.
    ///
    /// # Errors
    ///
    /// Returns [`amadeo_reflect::RegistryError`] if a *different* type already claims this name.
    pub fn register<T: Component>(&mut self) -> Result<(), amadeo_reflect::RegistryError> {
        self.types.register::<T>()?;
        self.inserters.insert(T::type_name(), insert_component::<T>);
        Ok(())
    }

    /// The schemas, for description and inspection.
    #[must_use]
    pub fn types(&self) -> &TypeRegistry {
        &self.types
    }

    /// Whether a component is registered under this name.
    #[must_use]
    pub fn contains(&self, name: &str) -> bool {
        self.inserters.contains_key(name)
    }

    /// How many components are registered.
    #[must_use]
    pub fn len(&self) -> usize {
        self.inserters.len()
    }

    /// Whether nothing is registered.
    #[must_use]
    pub fn is_empty(&self) -> bool {
        self.inserters.is_empty()
    }

    /// Every registered component name, sorted.
    pub fn names(&self) -> impl Iterator<Item = &str> {
        self.inserters.keys().map(String::as_str)
    }

    /// One component's schema.
    #[must_use]
    pub fn info(&self, name: &str) -> Option<&TypeInfo> {
        self.types.get(name)
    }

    /// Builds a component from a value and puts it on an entity.
    ///
    /// # Errors
    ///
    /// - [`RegistryError::UnknownComponent`] if the name is not registered, **listing every name
    ///   that is** — which turns a typo from a mystery into a one-glance fix.
    /// - [`RegistryError::BadValue`] if the value does not match the component's shape.
    /// - [`RegistryError::DeadEntity`] if the handle is stale.
    pub fn insert(
        &self,
        world: &mut World,
        entity: Entity,
        name: &str,
        value: &Value,
    ) -> Result<(), RegistryError> {
        let Some(inserter) = self.inserters.get(name) else {
            return Err(RegistryError::UnknownComponent {
                name: name.to_string(),
                known: self.names().collect::<Vec<_>>().join(", "),
            });
        };

        if !world.contains(entity) {
            return Err(RegistryError::DeadEntity {
                name: name.to_string(),
                entity,
            });
        }

        inserter(world, entity, value).map_err(|source| RegistryError::BadValue {
            name: name.to_string(),
            source,
        })
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use amadeo_core::StableHash;
    use amadeo_reflect::Reflect;

    /// How much damage something can take.
    #[derive(Debug, Clone, Copy, PartialEq, StableHash, Reflect)]
    struct Health {
        /// Current hit points.
        current: f32,
    }
    impl Component for Health {}

    /// Marks the player.
    #[derive(Debug, Clone, Copy, PartialEq, StableHash, Reflect)]
    struct Player;
    impl Component for Player {}

    fn registry() -> ComponentRegistry {
        let mut registry = ComponentRegistry::new();
        registry.register::<Health>().expect("registers");
        registry.register::<Player>().expect("registers");
        registry
    }

    #[test]
    fn a_component_can_be_built_from_its_name_and_a_value() {
        let registry = registry();
        let mut world = World::new();
        let entity = world.spawn();

        let value = Value::structure([("current", Value::F32(75.0))]);
        registry
            .insert(&mut world, entity, "Health", &value)
            .expect("Health is registered and the value fits");

        assert_eq!(world.get::<Health>(entity), Some(&Health { current: 75.0 }));
    }

    #[test]
    fn a_scene_files_integer_still_lands_in_a_float_field() {
        // The parser has no schema, so `current 75` arrives as an integer. It must still work --
        // this is the leniency amadeo-reflect's float impls exist for.
        let registry = registry();
        let mut world = World::new();
        let entity = world.spawn();

        let value = Value::structure([("current", Value::I64(75))]);
        registry
            .insert(&mut world, entity, "Health", &value)
            .expect("an integer is a fine way to write a float");

        assert_eq!(world.get::<Health>(entity), Some(&Health { current: 75.0 }));
    }

    #[test]
    fn an_unknown_component_lists_the_ones_that_exist() {
        // The failure this shape exists to prevent: a typo that reads as "computer says no".
        let registry = registry();
        let mut world = World::new();
        let entity = world.spawn();

        let error = registry
            .insert(&mut world, entity, "Helth", &Value::Unit)
            .expect_err("`Helth` is a typo");

        assert_eq!(
            error,
            RegistryError::UnknownComponent {
                name: "Helth".to_string(),
                known: "Health, Player".to_string(),
            }
        );
        let message = error.to_string();
        assert!(message.contains("Health, Player"), "{message}");
        assert!(message.contains("module may not be loaded"), "{message}");
    }

    #[test]
    fn a_bad_value_reports_the_component_and_the_reason() {
        let registry = registry();
        let mut world = World::new();
        let entity = world.spawn();

        let error = registry
            .insert(
                &mut world,
                entity,
                "Health",
                &Value::structure([("current", Value::String("lots".into()))]),
            )
            .expect_err("`lots` is not a number");

        assert_eq!(
            error.to_string(),
            "component `Health`: f32: expected a number, found string"
        );
    }

    #[test]
    fn a_stale_handle_is_refused_rather_than_silently_ignored() {
        // `World::insert` returns false for a dead entity, which a caller can easily drop on the
        // floor. Going through the registry, it is an error with the component's name in it.
        let registry = registry();
        let mut world = World::new();
        let entity = world.spawn();
        world.despawn(entity);

        let error = registry
            .insert(&mut world, entity, "Player", &Value::Unit)
            .expect_err("the entity is gone");

        assert!(matches!(error, RegistryError::DeadEntity { .. }));
        assert!(error.to_string().contains("no longer exists"));
    }

    #[test]
    fn the_registry_carries_schemas_as_well_as_constructors() {
        // One call registers both, so I8 cannot be half-satisfied.
        let registry = registry();
        assert_eq!(registry.len(), 2);
        assert_eq!(
            registry.names().collect::<Vec<_>>(),
            vec!["Health", "Player"]
        );

        let info = registry.info("Health").expect("schema is there too");
        assert_eq!(info.docs, "How much damage something can take.");
        assert_eq!(info.field("current").expect("reflected").type_name, "f32");
    }

    #[test]
    fn registering_twice_is_harmless() {
        let mut registry = registry();
        registry.register::<Health>().expect("second is a no-op");
        assert_eq!(registry.len(), 2);
    }

    #[test]
    fn a_marker_component_round_trips_through_the_registry() {
        let registry = registry();
        let mut world = World::new();
        let entity = world.spawn();

        registry
            .insert(&mut world, entity, "Player", &Value::Unit)
            .expect("a unit struct takes a unit value");
        assert!(world.has::<Player>(entity));
    }
}
