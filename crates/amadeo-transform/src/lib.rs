//! Transforms and hierarchy — the spatial vocabulary shared by everything above the ECS.
//!
//! ADR 0015 put these here rather than in `amadeo-ecs` (which defines what a component *is*, not
//! which components exist) or `amadeo-scene` (which sits above the renderer, physics, and animation,
//! all of which need transforms — invariant I6 makes that impossible).
//!
//! ```
//! use amadeo_ecs::{ComponentRegistry, World};
//! use amadeo_transform::{Parent, Transform2d};
//!
//! let mut registry = ComponentRegistry::new();
//! registry.register::<Transform2d>().expect("registers");
//! registry.register::<Parent>().expect("registers");
//!
//! let mut world = World::new();
//! let room = world.spawn();
//! world.insert(room, Transform2d::at(0.0, 0.0));
//!
//! let lamp = world.spawn();
//! world.insert(lamp, Transform2d::at(1.0, 2.0));
//! world.insert(lamp, Parent(room));
//!
//! assert_eq!(world.get::<Parent>(lamp).map(|parent| parent.0), Some(room));
//! ```
//!
//! # What is not here yet
//!
//! **`GlobalTransform` and the propagation system.** Composing a child's transform with its parent's
//! is easy; deciding what the renderer reads is entangled with open question Q3 (how 2D and 3D
//! coexist), which is settled before M1's 2D renderer work. Until then [`Parent`] is structure
//! without propagation — a real limitation, and still enough for a scene to round-trip its tree
//! rather than lose it.
//!
//! **`Children`.** A denormalised cache of what [`Parent`] already says, and keeping two
//! representations consistent is a well-known source of dangling references. It arrives when a
//! system needs fast child iteration, not before.
//!
//! **`Transform3d`.** M2.

use amadeo_core::StableHash;
use amadeo_ecs::{Component, Entity};
use amadeo_reflect::Reflect;

/// Where an entity is in 2D space.
///
/// Position is in world units, rotation in radians counter-clockwise, and scale multiplies each
/// axis independently.
// Not a doc comment: `///` on a reflected type is the description `amadeo describe` prints, so it is
// read by an agent that has never seen this file and wants to know what the type is *for*.
// Implementation history is noise there. Moved here from `amadeo-render` by ADR 0015 — the renderer
// was its first consumer, not its owner, and physics, animation, and the scene format all need it
// too.
#[derive(Debug, Clone, Copy, PartialEq, StableHash, Reflect)]
pub struct Transform2d {
    /// Position in world units.
    #[reflect(unit = "world units", sync = "on_change", interpolate = "linear")]
    pub position: [f32; 2],
    /// Rotation in radians, counter-clockwise.
    #[reflect(unit = "rad", sync = "on_change", interpolate = "angular")]
    pub rotation: f32,
    /// Scale multiplier on each axis.
    #[reflect(sync = "on_change", interpolate = "linear")]
    pub scale: [f32; 2],
}

impl Default for Transform2d {
    fn default() -> Self {
        Self {
            position: [0.0, 0.0],
            rotation: 0.0,
            scale: [1.0, 1.0],
        }
    }
}

impl Transform2d {
    /// A transform at a position, unrotated and unscaled.
    #[must_use]
    pub fn at(x: f32, y: f32) -> Self {
        Self {
            position: [x, y],
            ..Self::default()
        }
    }
}

impl Component for Transform2d {}

/// The entity this one is a child of.
///
/// A scene file expresses nesting by indentation; loading one turns that nesting into `Parent`
/// links. There is no `Children` component — iterate `Parent` to find an entity's children.
///
/// **A `Parent` can go stale.** Despawning a parent does not despawn or fix up its children, so the
/// handle simply stops resolving: looking a component up through it returns `None` rather than
/// misbehaving.
// Design notes, kept out of the reflected description because an agent reading `amadeo describe`
// wants to know how to use the type, not why it is shaped this way:
//
// ADR 0004 chose a scene tree for authoring and an ECS for runtime, with the tree persisting as
// components rather than as a separate structure. This is that component.
//
// `Children` is absent because it would be a denormalised cache of what this already says, and two
// representations of one fact drift apart — a despawn updating one and not the other leaves a
// dangling reference. It arrives when a system needs fast child iteration, with a story for keeping
// the two in step. Cascade-despawn belongs with the propagation system (ADR 0015).
#[derive(Debug, Clone, Copy, PartialEq, StableHash, Reflect)]
pub struct Parent(pub Entity);

impl Component for Parent {}

#[cfg(test)]
mod tests {
    use super::*;
    use amadeo_core::stable_hash_of;
    use amadeo_ecs::{ComponentRegistry, World};
    use amadeo_reflect::Value;

    #[test]
    fn a_transform_defaults_to_the_origin_unscaled() {
        let transform = Transform2d::default();
        assert_eq!(transform.position, [0.0, 0.0]);
        assert_eq!(transform.rotation, 0.0);
        assert_eq!(
            transform.scale,
            [1.0, 1.0],
            "a default scale of zero would make everything invisible, which is a memorable first bug"
        );
    }

    #[test]
    fn at_places_a_transform_without_touching_rotation_or_scale() {
        let transform = Transform2d::at(3.0, -4.0);
        assert_eq!(transform.position, [3.0, -4.0]);
        assert_eq!(transform.scale, [1.0, 1.0]);
    }

    #[test]
    fn parent_links_one_entity_to_another() {
        let mut world = World::new();
        let room = world.spawn();
        let lamp = world.spawn();
        world.insert(lamp, Parent(room));

        assert_eq!(world.get::<Parent>(lamp).map(|parent| parent.0), Some(room));
        assert!(!world.has::<Parent>(room), "a root has no Parent");
    }

    #[test]
    fn a_parent_handle_goes_stale_rather_than_dangling() {
        // Nothing cascades a despawn yet, so this is the behaviour to know about: the link survives
        // and the lookup through it fails detectably, which generational indices are what buy.
        let mut world = World::new();
        let room = world.spawn();
        world.insert(room, Transform2d::at(1.0, 1.0));
        let lamp = world.spawn();
        world.insert(lamp, Parent(room));

        world.despawn(room);

        let parent = world
            .get::<Parent>(lamp)
            .expect("the link is still there")
            .0;
        assert_eq!(
            world.get::<Transform2d>(parent),
            None,
            "the stale handle must resolve to nothing rather than to whoever reused the slot"
        );
    }

    #[test]
    fn both_components_build_from_the_registry_by_name() {
        // The path a scene file takes. If either failed here, a scene could not load it.
        let mut registry = ComponentRegistry::new();
        registry.register::<Transform2d>().expect("registers");
        registry.register::<Parent>().expect("registers");

        let mut world = World::new();
        let entity = world.spawn();

        registry
            .insert(
                &mut world,
                entity,
                "Transform2d",
                &Transform2d::at(2.0, 3.0).to_value(),
            )
            .expect("builds");
        assert_eq!(
            world.get::<Transform2d>(entity),
            Some(&Transform2d::at(2.0, 3.0))
        );

        assert_eq!(
            registry.names().collect::<Vec<_>>(),
            vec!["Parent", "Transform2d"]
        );
    }

    #[test]
    fn a_parent_round_trips_through_the_value_tree() {
        // Entity reflects as { generation, index }. Meaningless in a saved file -- ADR 0015 -- but
        // it has to work, because Component: Reflect admits no exceptions.
        let mut world = World::new();
        let room = world.spawn();
        let parent = Parent(room);

        let value = parent.to_value();
        assert_eq!(Parent::from_value(&value).expect("round trips"), parent);

        // And the shape is the introspectable one, not an opaque integer.
        assert!(matches!(value, Value::Struct(_)), "got {value}");
    }

    #[test]
    fn transforms_hash_by_value() {
        assert_eq!(
            stable_hash_of(&Transform2d::at(1.0, 2.0)),
            stable_hash_of(&Transform2d::at(1.0, 2.0))
        );
        assert_ne!(
            stable_hash_of(&Transform2d::at(1.0, 2.0)),
            stable_hash_of(&Transform2d::at(1.0, 2.5))
        );
    }

    #[test]
    fn the_schema_carries_units_for_the_agent() {
        let info = Transform2d::type_info();
        assert_eq!(
            info.field("rotation").expect("reflected").unit.as_deref(),
            Some("rad"),
            "the unit is what stops degrees being passed to a radians field"
        );
        assert_eq!(info.replicated_fields().count(), 3);
    }
}
