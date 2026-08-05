//! What physics reads off entities, and what it writes back.
//!
//! # These components are the source of truth, not rapier
//!
//! ADR 0036 puts physics state **in the state hash**, and ADR 0036 §4 forbids any rapier type from
//! appearing in a component, a scene file, a snapshot or the state hash. Both are satisfied the same
//! way: the authoritative state of a body is its [`Transform`](amadeo_transform::Transform) and its
//! [`Velocity`], which are ordinary reflected components, and the backend's own world is a *cache*
//! rebuilt from and written back to them.
//!
//! That is what makes a physics-driven game snapshot-able, replayable and describable with nothing
//! extra built — and it is why the trait in [`crate::backend`] speaks in these types rather than in
//! anything rapier defines.

use amadeo_core::StableHash;
use amadeo_ecs::Component;
use amadeo_reflect::Reflect;

/// How a body is driven.
///
/// The three-way split every physics engine has, and the names are worth being precise about because
/// choosing the wrong one is the most common way a character ends up unable to move or unable to
/// stop.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default, StableHash, Reflect)]
pub enum BodyKind {
    /// Never moves, and nothing can push it. Walls, floors, terrain.
    ///
    /// The default, because it is the safe one: a body that should have been dynamic and is static
    /// visibly does nothing, where a body that should have been static and is dynamic falls through
    /// the world on the first tick.
    #[default]
    Static,
    /// Moved by forces, gravity and collisions. A crate, a ragdoll, a thrown object.
    Dynamic,
    /// Moved by gameplay code, and pushes dynamic bodies without being pushed back.
    ///
    /// What a character controller, a moving platform or a lift is. The distinction from `Dynamic`
    /// is that gameplay decides where it goes and physics does not argue.
    Kinematic,
}

/// Marks an entity as something physics simulates.
///
/// Position and orientation are **not here** — they come from the
/// [`Transform`](amadeo_transform::Transform) on the same entity, per ADR 0018's one-transform rule,
/// exactly as a camera's and a light's do.
#[derive(Debug, Clone, Copy, PartialEq, StableHash, Reflect)]
pub struct RigidBody {
    /// How this body is driven.
    pub kind: BodyKind,
    /// How much it resists being pushed, in kilograms. Ignored for static bodies.
    #[reflect(min = 0.0, max = 100000.0, unit = "kg")]
    pub mass: f32,
    /// How quickly linear motion bleeds away, `0.0` for none.
    ///
    /// Not friction — this is resistance from the medium the body moves *through*, which is what
    /// stops a frictionless object drifting forever.
    #[reflect(min = 0.0, max = 100.0)]
    pub linear_damping: f32,
    /// The same for rotation.
    #[reflect(min = 0.0, max = 100.0)]
    pub angular_damping: f32,
    /// Whether gravity applies. Off for a flying character or a projectile with its own arc.
    pub gravity: bool,
}

impl Default for RigidBody {
    fn default() -> Self {
        Self {
            kind: BodyKind::Static,
            mass: 1.0,
            linear_damping: 0.0,
            angular_damping: 0.0,
            gravity: true,
        }
    }
}

impl Component for RigidBody {}

impl RigidBody {
    /// A body moved by forces and collisions.
    #[must_use]
    pub fn dynamic(mass: f32) -> Self {
        Self {
            kind: BodyKind::Dynamic,
            mass,
            ..Self::default()
        }
    }

    /// A body gameplay moves, which pushes dynamic bodies without being pushed back.
    #[must_use]
    pub fn kinematic() -> Self {
        Self {
            kind: BodyKind::Kinematic,
            ..Self::default()
        }
    }
}

/// The shape physics uses for an entity.
///
/// **Deliberately not the mesh.** A collision shape is nearly always simpler than what is drawn — a
/// character is a capsule, a crate is a box, a level is a handful of boxes — because collision cost
/// scales with shape complexity and because a simple shape behaves more predictably. Tying the two
/// together is a decision that reads as convenient and is expensive to undo, which is why they are
/// separate components naming separate things.
///
/// A fieldless-variant-plus-payload enum, which ADR 0032 made expressible in a scene file.
#[derive(Debug, Clone, Copy, PartialEq, StableHash, Reflect)]
pub enum Shape {
    /// A rectangular box, centred on the entity's origin.
    Cuboid {
        /// Full width, height and depth.
        #[reflect(min = 0.0, max = 10000.0, unit = "world units")]
        size: [f32; 3],
    },
    /// A ball, centred on the entity's origin.
    Sphere {
        /// Distance from the centre to the surface.
        #[reflect(min = 0.0, max = 10000.0, unit = "world units")]
        radius: f32,
    },
    /// A cylinder with hemispherical caps, standing upright.
    ///
    /// What a character is, essentially always: it slides over small steps rather than catching on
    /// them, and it cannot tip over on a corner the way a box can.
    Capsule {
        /// Distance from the axis to the surface.
        #[reflect(min = 0.0, max = 10000.0, unit = "world units")]
        radius: f32,
        /// The straight section between the two caps. Total height is this plus twice the radius.
        #[reflect(min = 0.0, max = 10000.0, unit = "world units")]
        height: f32,
    },
}

impl Default for Shape {
    fn default() -> Self {
        Shape::Cuboid {
            size: [1.0, 1.0, 1.0],
        }
    }
}

/// What an entity collides with, and how.
#[derive(Debug, Clone, Copy, PartialEq, StableHash, Reflect)]
pub struct Collider {
    /// The shape used for collision.
    pub shape: Shape,
    /// How much sliding contact is resisted. `0.0` is ice, `1.0` is rubber on rough concrete.
    #[reflect(min = 0.0, max = 2.0)]
    pub friction: f32,
    /// How much of an impact is returned. `0.0` does not bounce, `1.0` bounces to its own height.
    #[reflect(min = 0.0, max = 1.0)]
    pub restitution: f32,
    /// Whether this reports overlaps instead of blocking movement.
    ///
    /// A trigger volume: a doorway that notices you passed through it, a pickup, a damage zone.
    /// The Vault's sigils are exactly this shape of thing, done by hand today.
    pub sensor: bool,
}

impl Default for Collider {
    fn default() -> Self {
        Self {
            shape: Shape::default(),
            friction: 0.5,
            restitution: 0.0,
            sensor: false,
        }
    }
}

impl Component for Collider {}

impl Collider {
    /// A box collider of a given size.
    #[must_use]
    pub fn cuboid(width: f32, height: f32, depth: f32) -> Self {
        Self {
            shape: Shape::Cuboid {
                size: [width, height, depth],
            },
            ..Self::default()
        }
    }

    /// A ball collider.
    #[must_use]
    pub fn sphere(radius: f32) -> Self {
        Self {
            shape: Shape::Sphere { radius },
            ..Self::default()
        }
    }

    /// An upright capsule — what a character usually is.
    #[must_use]
    pub fn capsule(radius: f32, height: f32) -> Self {
        Self {
            shape: Shape::Capsule { radius, height },
            ..Self::default()
        }
    }

    /// The same collider, reporting overlaps rather than blocking movement.
    #[must_use]
    pub fn as_sensor(mut self) -> Self {
        self.sensor = true;
        self
    }
}

/// How fast a body is moving, in world units per second.
///
/// **Hashed, and authoritative.** ADR 0036 puts physics state in the state hash, and this is half of
/// it — the other half is the entity's `Transform`. A backend reads these at the start of a step and
/// writes them back at the end, so the world's own components are always the record rather than
/// something inside a physics engine that a snapshot cannot see.
#[derive(Debug, Clone, Copy, PartialEq, Default, StableHash, Reflect)]
pub struct Velocity {
    /// Movement, in world units per second.
    #[reflect(unit = "units/s")]
    pub linear: [f32; 3],
    /// Rotation, in degrees per second about each axis — degrees to match
    /// [`Transform`](amadeo_transform::Transform)'s rotation, which ADR 0018 keeps in degrees so it
    /// stays hand-writable.
    #[reflect(unit = "deg/s")]
    pub angular: [f32; 3],
}

impl Component for Velocity {}

impl Velocity {
    /// A body moving at a given speed with no spin.
    #[must_use]
    pub fn linear(x: f32, y: f32, z: f32) -> Self {
        Self {
            linear: [x, y, z],
            angular: [0.0, 0.0, 0.0],
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use amadeo_reflect::Reflect;

    #[test]
    fn a_body_defaults_to_static() {
        // The safe default: a body that should have been dynamic visibly does nothing, where one
        // that should have been static falls through the world on the first tick.
        assert_eq!(RigidBody::default().kind, BodyKind::Static);
    }

    #[test]
    fn the_physics_components_round_trip_through_the_value_tree() {
        // I8, and it is load-bearing here rather than decorative: ADR 0036 puts physics state in the
        // state hash, which means a snapshot has to be able to write and read it back.
        let body = RigidBody::dynamic(80.0);
        assert_eq!(
            RigidBody::from_value(&body.to_value()).expect("round trips"),
            body
        );

        let collider = Collider::capsule(0.4, 1.2).as_sensor();
        assert_eq!(
            Collider::from_value(&collider.to_value()).expect("round trips"),
            collider
        );

        let velocity = Velocity::linear(1.0, -9.8, 0.5);
        assert_eq!(
            Velocity::from_value(&velocity.to_value()).expect("round trips"),
            velocity
        );
    }

    #[test]
    fn every_shape_survives_the_value_tree() {
        // An enum carrying a payload, which ADR 0032 made expressible — so a collider is authorable
        // in a scene file rather than only in code.
        for shape in [
            Shape::Cuboid {
                size: [2.0, 1.0, 3.0],
            },
            Shape::Sphere { radius: 0.75 },
            Shape::Capsule {
                radius: 0.4,
                height: 1.2,
            },
        ] {
            let collider = Collider {
                shape,
                ..Collider::default()
            };
            assert_eq!(
                Collider::from_value(&collider.to_value()).expect("round trips"),
                collider
            );
        }
    }

    #[test]
    fn velocity_changes_the_hash() {
        // The property ADR 0036 turned on: physics state is in the state hash, so two worlds whose
        // bodies are moving differently must not agree.
        let still = amadeo_core::stable_hash_of(&Velocity::default());
        let moving = amadeo_core::stable_hash_of(&Velocity::linear(0.0, 1.0, 0.0));
        assert_ne!(still, moving);
    }

    #[test]
    fn a_sensor_is_distinguishable_from_a_solid_collider() {
        // Sensors report overlaps rather than blocking, so mixing the two up is the difference
        // between walking through a wall and being unable to pick anything up.
        let solid = Collider::sphere(1.0);
        let sensor = Collider::sphere(1.0).as_sensor();
        assert!(!solid.sensor && sensor.sensor);
        assert_ne!(
            amadeo_core::stable_hash_of(&solid),
            amadeo_core::stable_hash_of(&sensor)
        );
    }
}
