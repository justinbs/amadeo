//! Transforms and hierarchy — the spatial vocabulary shared by everything above the ECS.
//!
//! ADR 0015 put these here rather than in `amadeo-ecs` (which defines what a component *is*, not
//! which components exist) or `amadeo-scene` (which sits above the renderer, physics, and animation,
//! all of which need transforms — invariant I6 makes that impossible).
//!
//! ```
//! use amadeo_ecs::{ComponentRegistry, World};
//! use amadeo_transform::{Parent, Transform};
//!
//! let mut registry = ComponentRegistry::new();
//! registry.register::<Transform>().expect("registers");
//! registry.register::<Parent>().expect("registers");
//!
//! let mut world = World::new();
//! let room = world.spawn();
//! world.insert(room, Transform::at(0.0, 0.0));
//!
//! let lamp = world.spawn();
//! world.insert(lamp, Transform::at(1.0, 2.0));
//! world.insert(lamp, Parent(room));
//!
//! assert_eq!(world.get::<Parent>(lamp).map(|parent| parent.0), Some(room));
//! ```
//!
//! # What is not here yet
//!
//! **`GlobalTransform` and the propagation system.** Composing a child's transform with its parent's
//! is easy; what blocked it was deciding what a transform *is*, which ADR 0018 has now settled. Until
//! propagation lands, [`Parent`] is structure without effect — a real limitation, and still enough
//! for a scene to round-trip its tree rather than lose it.
//!
//! When it arrives, `GlobalTransform` is a **computed 4×4 matrix**: never authored, never written to
//! a scene file, and therefore free to be whatever the maths wants rather than whatever a human can
//! type (ADR 0018).
//!
//! **`Children`.** A denormalised cache of what [`Parent`] already says, and keeping two
//! representations consistent is a well-known source of dangling references. It arrives when a
//! system needs fast child iteration, not before.

mod matrix;
mod propagate;

pub use matrix::Mat4;
pub use propagate::{GlobalTransform, MAX_DEPTH, PROPAGATE_TRANSFORMS, propagate_transforms};

use amadeo_core::StableHash;
use amadeo_ecs::{Component, Entity};
use amadeo_reflect::Reflect;

/// Where an entity is, how it is turned, and how big it is.
///
/// Always 3D. A 2D game leaves `z` at zero and rotates only about it — there is no separate 2D
/// transform, because all three target games are 3D and two transform types would mean two
/// hierarchies in any world that mixes them.
///
/// Rotation is Euler angles in **degrees**, applied Z, then X, then Y.
// Not a doc comment: `///` on a reflected type is the description `amadeo describe` prints, so it is
// read by an agent that has never seen this file and wants to know what the type is *for*.
// Implementation history is noise there.
//
// ADR 0015 moved this out of `amadeo-render`; ADR 0018 made it 3D and retired `Transform2d`.
//
// Euler degrees rather than a quaternion is the deliberate, contested choice -- see ADR 0018 for the
// full argument. The short version: a quaternion cannot be hand-written, and I1 says the file is the
// source of truth and is hand-editable. Storing Euler and authoring Euler is also the only option
// that round-trips byte-identically (I2), because quaternion-to-Euler has no single answer.
// Gimbal lock and interpolation are handled where they arise -- `GlobalTransform` is a computed
// matrix, animation holds its own quaternions, and the camera rig is a separate module.
#[derive(Debug, Clone, Copy, PartialEq, StableHash, Reflect)]
pub struct Transform {
    /// Position in world units.
    #[reflect(unit = "world units", sync = "on_change", interpolate = "linear")]
    pub translation: [f32; 3],
    /// Rotation in degrees, applied Z then X then Y.
    #[reflect(unit = "deg", sync = "on_change", interpolate = "angular")]
    pub rotation: [f32; 3],
    /// Scale multiplier on each axis.
    #[reflect(sync = "on_change", interpolate = "linear")]
    pub scale: [f32; 3],
}

impl Default for Transform {
    fn default() -> Self {
        Self {
            translation: [0.0, 0.0, 0.0],
            rotation: [0.0, 0.0, 0.0],
            scale: [1.0, 1.0, 1.0],
        }
    }
}

impl Transform {
    /// A transform at a point on the XY plane, unrotated and unscaled.
    ///
    /// The 2D convenience, kept because 2D scenes and tests use it constantly. `z` is zero.
    #[must_use]
    pub fn at(x: f32, y: f32) -> Self {
        Self {
            translation: [x, y, 0.0],
            ..Self::default()
        }
    }

    /// A transform at a point in space, unrotated and unscaled.
    #[must_use]
    pub fn at_xyz(x: f32, y: f32, z: f32) -> Self {
        Self {
            translation: [x, y, z],
            ..Self::default()
        }
    }

    /// Rotation about the Z axis, in degrees — the only rotation a 2D game needs.
    #[must_use]
    pub fn spin(self) -> f32 {
        self.rotation[2]
    }
}

impl Component for Transform {}

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
        let transform = Transform::default();
        assert_eq!(transform.translation, [0.0, 0.0, 0.0]);
        assert_eq!(transform.rotation, [0.0, 0.0, 0.0]);
        assert_eq!(
            transform.scale,
            [1.0, 1.0, 1.0],
            "a default scale of zero would make everything invisible, which is a memorable first bug"
        );
    }

    #[test]
    fn at_places_a_transform_on_the_xy_plane_without_touching_rotation_or_scale() {
        let transform = Transform::at(3.0, -4.0);
        assert_eq!(transform.translation, [3.0, -4.0, 0.0]);
        assert_eq!(transform.scale, [1.0, 1.0, 1.0]);
    }

    #[test]
    fn spin_reads_the_only_rotation_a_2d_game_has() {
        // ADR 0018: 2D is the degenerate case of one 3D transform, not a separate type. A 2D game
        // turns about Z and leaves the other two alone.
        let mut transform = Transform::at_xyz(1.0, 2.0, 3.0);
        transform.rotation[2] = 90.0;

        assert_eq!(transform.translation, [1.0, 2.0, 3.0]);
        assert_eq!(transform.spin(), 90.0);
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
        world.insert(room, Transform::at(1.0, 1.0));
        let lamp = world.spawn();
        world.insert(lamp, Parent(room));

        world.despawn(room);

        let parent = world
            .get::<Parent>(lamp)
            .expect("the link is still there")
            .0;
        assert_eq!(
            world.get::<Transform>(parent),
            None,
            "the stale handle must resolve to nothing rather than to whoever reused the slot"
        );
    }

    #[test]
    fn both_components_build_from_the_registry_by_name() {
        // The path a scene file takes. If either failed here, a scene could not load it.
        let mut registry = ComponentRegistry::new();
        registry.register::<Transform>().expect("registers");
        registry.register::<Parent>().expect("registers");

        let mut world = World::new();
        let entity = world.spawn();

        registry
            .insert(
                &mut world,
                entity,
                "Transform",
                &Transform::at(2.0, 3.0).to_value(),
            )
            .expect("builds");
        assert_eq!(
            world.get::<Transform>(entity),
            Some(&Transform::at(2.0, 3.0))
        );

        assert_eq!(
            registry.names().collect::<Vec<_>>(),
            vec!["Parent", "Transform"]
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
            stable_hash_of(&Transform::at(1.0, 2.0)),
            stable_hash_of(&Transform::at(1.0, 2.0))
        );
        assert_ne!(
            stable_hash_of(&Transform::at(1.0, 2.0)),
            stable_hash_of(&Transform::at(1.0, 2.5))
        );
    }

    #[test]
    fn the_schema_carries_units_for_the_agent() {
        let info = Transform::type_info();
        assert_eq!(
            info.field("rotation").expect("reflected").unit.as_deref(),
            Some("deg"),
            "degrees, not radians -- ADR 0018 chose the one a human writes correctly by hand"
        );
        assert_eq!(info.replicated_fields().count(), 3);
    }
}
