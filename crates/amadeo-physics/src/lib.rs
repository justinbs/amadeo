//! Rigid-body physics, behind engine-owned traits — ADR 0002, ADR 0036.
//!
//! # Deterministic before it is fast
//!
//! ADR 0036 settled the trade this subsystem could not avoid: rapier can be bit-identical across
//! machines, or it can be multi-threaded and vectorised, and **not both**. This engine takes
//! determinism, permanently, because invariant I3 is what every verification mechanism here is built
//! on — golden replays, separate-process replay, snapshots, the determinism CI job. Physics that did
//! not honour it would not weaken those a little; it would make them silent about the largest thing
//! in the simulation.
//!
//! Two consequences run through this crate:
//!
//! - **Physics state is in the state hash**, carried by ordinary reflected components
//!   ([`Velocity`] and the entity's `Transform`) rather than inside a solver. That is what makes a
//!   physics-driven game snapshot-able and replayable with nothing extra built.
//! - **No rapier type crosses [`PhysicsBackend`]**, so the version pin ADR 0036 requires cannot leak
//!   into the scene format or the state hash.
//!
//! ```
//! use amadeo_ecs::World;
//! use amadeo_physics::{Collider, Gravity, NullPhysics, Physics, RigidBody, Velocity, step_physics};
//! use amadeo_transform::Transform;
//!
//! let mut world = World::new();
//! world.insert_service(Physics::new(Box::new(NullPhysics::new())));
//! world.insert_resource(Gravity::earth());
//!
//! let crate_ = world.spawn();
//! world.insert(crate_, Transform::at(0.0, 10.0));
//! world.insert(crate_, RigidBody::dynamic(20.0));
//! world.insert(crate_, Collider::cuboid(1.0, 1.0, 1.0));
//! world.insert(crate_, Velocity::default());
//!
//! step_physics(&mut world);
//!
//! // Gravity pulled it down, and the world's own components are the record of that.
//! assert!(world.get::<Velocity>(crate_).expect("still there").linear[1] < 0.0);
//! ```

mod backend;
mod components;

pub use backend::{BodyResult, BodyState, NullPhysics, PhysicsBackend, PhysicsError};
pub use components::{BodyKind, Collider, RigidBody, Shape, Velocity};

use amadeo_core::StableHash;
use amadeo_ecs::{Resource, Service, World};
use amadeo_reflect::Reflect;
use amadeo_transform::{GlobalTransform, Transform};

/// The label the app layer registers [`step_physics`] under.
pub const STEP_PHYSICS: &str = "step_physics";

/// How strongly, and in which direction, everything is pulled.
///
/// A [`Resource`] rather than a service, so it is **in the state hash** — changing gravity changes
/// the simulation, and two runs that disagree about it have genuinely diverged. It is also authorable
/// in a scene file for the same reason every other reflected type is.
#[derive(Debug, Clone, Copy, PartialEq, StableHash, Reflect)]
pub struct Gravity {
    /// Acceleration in world units per second squared. Negative Y is down.
    #[reflect(unit = "units/s^2")]
    pub acceleration: [f32; 3],
}

impl Default for Gravity {
    fn default() -> Self {
        Self::earth()
    }
}

impl Resource for Gravity {}

impl Gravity {
    /// Earth's, near enough: 9.81 units per second squared downward.
    ///
    /// Worth knowing that games rarely use this. Real gravity makes a jump feel floaty at human
    /// scale, so most platformers run at two or three times it — which is why this is a named
    /// constructor rather than a hidden default nobody can see to change.
    #[must_use]
    pub fn earth() -> Self {
        Self {
            acceleration: [0.0, -9.81, 0.0],
        }
    }

    /// No gravity at all — for a top-down game, or space.
    #[must_use]
    pub fn none() -> Self {
        Self {
            acceleration: [0.0, 0.0, 0.0],
        }
    }
}

/// Holds the active physics backend.
///
/// A [`Service`]: engine machinery, never simulation state (ADR 0009). The *results* it produces go
/// into components, which are hashed; the solver's own caches are not, and must never be the only
/// record of anything — see [`PhysicsBackend`].
#[derive(Debug)]
pub struct Physics {
    backend: Box<dyn PhysicsBackend>,
    /// Set when the last step failed. Cleared on the next success.
    last_error: Option<PhysicsError>,
}

impl Service for Physics {}

impl Physics {
    /// Wraps a backend.
    #[must_use]
    pub fn new(backend: Box<dyn PhysicsBackend>) -> Self {
        Self {
            backend,
            last_error: None,
        }
    }

    /// The backend's name, for diagnostics.
    #[must_use]
    pub fn backend_name(&self) -> &'static str {
        self.backend.name()
    }

    /// The error from the last failed step, if it failed.
    ///
    /// Surfaced rather than logged-and-forgotten, so a game whose bodies have stopped moving can be
    /// diagnosed by asking rather than by guessing — the same standard `Renderer::last_error` sets.
    #[must_use]
    pub fn last_error(&self) -> Option<&PhysicsError> {
        self.last_error.as_ref()
    }
}

/// Advances every rigid body by one fixed tick.
///
/// Registered in the app layer's simulation stage. Does nothing if no [`Physics`] service is
/// installed, so a game that wants none pays nothing.
///
/// # Why static bodies are collected too
///
/// They never move, so stepping them is wasted — but a solver needs to *know about* them, because
/// they are what dynamic bodies collide with. [`NullPhysics`] has no solver and so genuinely wastes
/// the work; a real backend does not. Filtering them here would mean the null backend and the real
/// one were handed different worlds, which is exactly the asymmetry that makes a headless test stop
/// predicting a windowed run.
///
/// # Why the results are written back by entity rather than by position
///
/// A backend returns one result per body and the ordering test in `backend.rs` pins that it keeps
/// them in order — but writing back by *handle* means a backend that ever reordered would produce a
/// visibly wrong world rather than a silently swapped one.
pub fn step_physics(world: &mut World) {
    if !world.has_service::<Physics>() {
        return;
    }

    let gravity = world
        .resource::<Gravity>()
        .copied()
        .unwrap_or_else(Gravity::none)
        .acceleration;

    // `GlobalTransform` when propagation has run, the local transform otherwise — the same fallback
    // the renderer uses, and for the same reason: requiring it would mean forgetting one system
    // makes physics silently wrong rather than slightly wrong.
    let bodies: Vec<BodyState> = world
        .query::<(
            &RigidBody,
            &Transform,
            Option<&Velocity>,
            Option<&GlobalTransform>,
        )>()
        .map(|(entity, (_body, transform, velocity, global))| {
            let placement = match global {
                Some(global) => global.to_mat4().translation(),
                None => transform.translation,
            };
            BodyState {
                entity,
                translation: placement,
                rotation: transform.rotation,
                velocity: velocity.copied().unwrap_or_default(),
            }
        })
        .collect();

    if bodies.is_empty() {
        return;
    }

    let stepped =
        world.with_service_taken::<Physics, Option<Vec<BodyResult>>>(|_world, physics| {
            match physics.backend.step(&bodies, gravity) {
                Ok(results) => {
                    physics.last_error = None;
                    Some(results)
                }
                // A failed step leaves the world untouched rather than partially advanced: half a tick
                // of physics is worse than none, and much harder to recognise.
                Err(error) => {
                    physics.last_error = Some(error);
                    None
                }
            }
        });

    let Some(Some(results)) = stepped else {
        return;
    };

    for result in results {
        // A body despawned during the step simply is not here any more. Skipping is right: the
        // alternative is resurrecting a handle, which the generational index would refuse anyway.
        if let Some(transform) = world.get::<Transform>(result.entity) {
            let mut moved = *transform;
            moved.translation = result.translation;
            moved.rotation = result.rotation;
            world.insert(result.entity, moved);
        }
        if world.get::<Velocity>(result.entity).is_some() {
            world.insert(result.entity, result.velocity);
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn falling_world() -> (World, amadeo_ecs::Entity) {
        let mut world = World::new();
        world.insert_service(Physics::new(Box::new(NullPhysics::new())));
        world.insert_resource(Gravity::earth());

        let entity = world.spawn();
        world.insert(entity, Transform::at(0.0, 10.0));
        world.insert(entity, RigidBody::dynamic(1.0));
        world.insert(entity, Velocity::default());
        (world, entity)
    }

    #[test]
    fn a_body_falls_and_the_world_records_it() {
        let (mut world, entity) = falling_world();
        step_physics(&mut world);

        let velocity = world.get::<Velocity>(entity).expect("still there");
        assert!(velocity.linear[1] < 0.0, "gravity should pull down");
        let transform = world.get::<Transform>(entity).expect("still there");
        assert!(transform.translation[1] < 10.0, "and it should have moved");
    }

    #[test]
    fn physics_is_part_of_the_state_hash() {
        // ADR 0036 §2, asserted rather than assumed: a world that has stepped is a different world.
        // This is what makes gate 3 mean anything.
        let (mut world, _) = falling_world();
        let before = world.state_hash();
        step_physics(&mut world);
        assert_ne!(before, world.state_hash());
    }

    #[test]
    fn stepping_is_reproducible_across_worlds() {
        // I3 at the level a replay actually cares about.
        let run = || {
            let (mut world, entity) = falling_world();
            for _ in 0..90 {
                step_physics(&mut world);
            }
            (world.state_hash(), world.get::<Transform>(entity).copied())
        };
        assert_eq!(run(), run());
    }

    #[test]
    fn a_world_with_no_physics_service_is_untouched() {
        // A game that wants no physics pays nothing, and does not silently gain gravity.
        let mut world = World::new();
        world.insert_resource(Gravity::earth());
        let entity = world.spawn();
        world.insert(entity, Transform::at(0.0, 10.0));
        world.insert(entity, RigidBody::dynamic(1.0));
        world.insert(entity, Velocity::default());

        let before = world.state_hash();
        step_physics(&mut world);
        assert_eq!(before, world.state_hash());
    }

    #[test]
    fn a_world_with_no_gravity_resource_does_not_fall() {
        // Missing gravity means *none* rather than a default pull. Inventing 9.81 for a world that
        // never asked would make a top-down game mysteriously drift downward.
        let mut world = World::new();
        world.insert_service(Physics::new(Box::new(NullPhysics::new())));
        let entity = world.spawn();
        world.insert(entity, Transform::at(0.0, 10.0));
        world.insert(entity, RigidBody::dynamic(1.0));
        world.insert(entity, Velocity::default());

        step_physics(&mut world);
        assert_eq!(world.get::<Velocity>(entity).expect("there").linear[1], 0.0);
    }

    #[test]
    fn an_entity_without_a_velocity_still_simulates() {
        // `Velocity` is optional, like `SortOrder` and `GlobalTransform` on the render path — a body
        // that forgot one should fall rather than vanish from the simulation.
        let mut world = World::new();
        world.insert_service(Physics::new(Box::new(NullPhysics::new())));
        world.insert_resource(Gravity::earth());

        let entity = world.spawn();
        world.insert(entity, Transform::at(0.0, 10.0));
        world.insert(entity, RigidBody::dynamic(1.0));

        step_physics(&mut world);
        let transform = world.get::<Transform>(entity).expect("there");
        assert!(transform.translation[1] < 10.0);
        // But no `Velocity` was invented for it: physics writes back only what is already there.
        assert!(world.get::<Velocity>(entity).is_none());
    }

    #[test]
    fn gravity_is_authorable_and_hashed() {
        let earth = Gravity::earth();
        assert_eq!(
            Gravity::from_value(&earth.to_value()).expect("round trips"),
            earth
        );
        assert_ne!(
            amadeo_core::stable_hash_of(&earth),
            amadeo_core::stable_hash_of(&Gravity::none())
        );
    }
}
