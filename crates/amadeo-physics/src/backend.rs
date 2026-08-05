//! The physics backend abstraction, and the null backend every build must have.
//!
//! # Why there is a trait here at all
//!
//! ADR 0002 put rapier behind engine-owned traits, and **ADR 0036 §4 made that boundary testable
//! rather than aspirational**: no rapier type may appear in a component, a scene file, a snapshot or
//! the state hash. This module is where that is enforced — everything crossing it is a type this
//! crate defines, so a rapier type physically cannot reach the world.
//!
//! That matters more than the usual "swap the implementation" argument, because ADR 0036 pins the
//! rapier version exactly: if rapier types leaked into components, an upgrade would move the state
//! hash *and* the scene format at once.
//!
//! # And why there is a null backend
//!
//! Invariant I7. The whole engine must run with no window and no GPU, and a dedicated server (ADR
//! 0006) will want the simulation without the drawing — but it will still want physics, so "null"
//! here means *no solver*, not *no engine*. [`NullPhysics`] integrates velocity and nothing else,
//! which is enough to keep a headless test meaningful and cheap enough to be the default.

use crate::components::{BodyKind, Collider, RigidBody, Velocity};
use amadeo_core::FIXED_DT;
use amadeo_ecs::Entity;

/// What can go wrong in a physics step.
#[derive(Debug, thiserror::Error, PartialEq, Eq)]
pub enum PhysicsError {
    /// The backend could not be created.
    #[error("could not initialise the {backend} physics backend: {reason}")]
    InitFailed {
        /// Which backend failed.
        backend: &'static str,
        /// Why.
        reason: String,
    },

    /// A body was described in a way the backend cannot represent.
    #[error(
        "entity {entity:?} cannot be simulated: {reason}. Check its `RigidBody` and `Collider`"
    )]
    BadBody {
        /// Which entity.
        entity: Entity,
        /// What was wrong with it, in terms the author can act on.
        reason: String,
    },
}

/// One body handed to the backend for a step.
///
/// A flat, owned snapshot rather than a borrow of the world, for the same reason
/// [`FrameData`](../../amadeo_render/struct.FrameData.html) is: a backend should be given everything
/// it needs and never reach back. It also means the *only* types crossing the boundary are ones this
/// crate defines, which is what ADR 0036 §4 asks for.
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct BodyState {
    /// Which entity this is.
    pub entity: Entity,
    /// World position.
    pub translation: [f32; 3],
    /// World rotation, Euler degrees (ADR 0018).
    pub rotation: [f32; 3],
    /// How fast it is moving.
    pub velocity: Velocity,
    /// How the body is driven, its mass, and its damping.
    pub body: RigidBody,
    /// Its shape and surface properties, if it has any.
    ///
    /// `None` is a body that participates in the simulation but collides with nothing — a marker
    /// that moves, or something whose collision is handled by gameplay. It is a real state rather
    /// than an oversight, which is why it is an `Option` rather than a default shape: inventing a
    /// one-metre cube for an entity nobody gave a collider would put an invisible obstacle in the
    /// world.
    pub collider: Option<Collider>,
}

/// What a step produced for one body.
///
/// Only what physics is allowed to change. Notably **not** the shape, the mass or the body kind —
/// those are authored, and a backend that could write them back would make the components stop being
/// the source of truth.
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct BodyResult {
    /// Which entity this is.
    pub entity: Entity,
    /// Where it ended up.
    pub translation: [f32; 3],
    /// How it ended up oriented.
    pub rotation: [f32; 3],
    /// How fast it is now moving.
    pub velocity: Velocity,
}

/// Something that can advance a set of bodies by one fixed tick.
///
/// # The step is a pure function, and that is the whole design
///
/// `step` takes the bodies and returns their new states. It is handed the *complete* input and hands
/// back the *complete* output, so nothing about a backend's internal state can be the only record of
/// anything — which is what makes a snapshot able to restore a physics simulation at all.
///
/// A real backend still keeps internal state between steps (contact caches, sleeping islands), and
/// that is fine and necessary for both speed and stability. What it must not do is keep state that
/// cannot be **rebuilt** from the bodies it is given.
pub trait PhysicsBackend: std::fmt::Debug + Send + Sync {
    /// A short name for diagnostics.
    fn name(&self) -> &'static str;

    /// Advances every body by one fixed tick.
    ///
    /// `gravity` is in world units per second squared, already negated for "down" by the caller —
    /// so a backend applies it rather than deciding which way down is.
    ///
    /// # Errors
    ///
    /// [`PhysicsError`] if a body cannot be simulated. A failing step leaves the world unchanged
    /// rather than partially advanced, because half a tick of physics is worse than none.
    fn step(
        &mut self,
        bodies: &[BodyState],
        gravity: [f32; 3],
    ) -> Result<Vec<BodyResult>, PhysicsError>;

    /// Throws away everything cached between steps, so the next step rebuilds from the bodies.
    ///
    /// # This exists because of ADR 0028, not because of physics
    ///
    /// Restoring a snapshot puts the *components* back. It does not put back a solver's contact
    /// caches, sleeping islands or warm-start data — so without this, a restored world would hash
    /// identically to the one it came from and then **simulate differently**.
    ///
    /// That is precisely the failure ADR 0028 found with the entity allocator's free list, and its
    /// conclusion applies unchanged: hash equality after a restore is necessary and **not
    /// sufficient**. Anything that replaces the world wholesale — a snapshot restore, loading a new
    /// level — calls this.
    ///
    /// The default does nothing, which is correct for a backend that caches nothing.
    fn reset(&mut self) {}
}

/// A backend with no solver: it integrates velocity and detects nothing.
///
/// # This is not a stub, and the distinction matters
///
/// It is what a headless run gets by default, and it does real work — a body with a velocity moves,
/// and gravity accelerates it. What it does not do is collision detection or response, so nothing
/// ever stops.
///
/// That is deliberately useful rather than merely present. A determinism test, a replay, or an agent
/// checking that a projectile is where it should be all work against this, run in milliseconds, and
/// need no rapier at all. The same argument as
/// [`NullBackend`](../../amadeo_render/struct.NullBackend.html) for rendering: a null backend that
/// records or computes something is worth far more than one that does nothing.
#[derive(Debug, Clone, Default)]
pub struct NullPhysics {
    steps: u64,
}

impl NullPhysics {
    /// A fresh null backend.
    #[must_use]
    pub fn new() -> Self {
        Self::default()
    }

    /// How many steps have run. For tests that want to know physics is actually being driven.
    #[must_use]
    pub fn steps(&self) -> u64 {
        self.steps
    }
}

impl PhysicsBackend for NullPhysics {
    fn name(&self) -> &'static str {
        "null"
    }

    fn step(
        &mut self,
        bodies: &[BodyState],
        gravity: [f32; 3],
    ) -> Result<Vec<BodyResult>, PhysicsError> {
        self.steps += 1;
        let dt = FIXED_DT;

        Ok(bodies
            .iter()
            .map(|body| {
                // A static body never moves, whatever its velocity says. Honoured here rather than
                // filtered out by the caller, because a solver still needs to *know about* static
                // bodies — they are what dynamic ones collide with — so every backend is handed all
                // of them and each decides what to do.
                if body.body.kind == BodyKind::Static {
                    return BodyResult {
                        entity: body.entity,
                        translation: body.translation,
                        rotation: body.rotation,
                        velocity: body.velocity,
                    };
                }

                // Gravity is per body, so a flying character or a projectile with its own arc can
                // opt out without the world's gravity changing for everything else.
                let pull = if body.body.gravity { gravity } else { [0.0; 3] };

                // Semi-implicit Euler: apply the acceleration to the velocity *first*, then move by
                // the new velocity. The explicit form (move, then accelerate) loses energy on every
                // step and makes a bouncing object sink through the floor over time. This is the
                // form every game physics engine uses, and it costs nothing extra.
                //
                // Damping is applied as a per-tick factor rather than an exponential, which is what
                // rapier does too — exact enough at a fixed 60 Hz and cheaper.
                let damp = (1.0 - body.body.linear_damping * dt).clamp(0.0, 1.0);
                let angular_damp = (1.0 - body.body.angular_damping * dt).clamp(0.0, 1.0);
                let velocity = Velocity {
                    linear: [
                        (body.velocity.linear[0] + pull[0] * dt) * damp,
                        (body.velocity.linear[1] + pull[1] * dt) * damp,
                        (body.velocity.linear[2] + pull[2] * dt) * damp,
                    ],
                    angular: [
                        body.velocity.angular[0] * angular_damp,
                        body.velocity.angular[1] * angular_damp,
                        body.velocity.angular[2] * angular_damp,
                    ],
                };

                BodyResult {
                    entity: body.entity,
                    translation: [
                        body.translation[0] + velocity.linear[0] * dt,
                        body.translation[1] + velocity.linear[1] * dt,
                        body.translation[2] + velocity.linear[2] * dt,
                    ],
                    rotation: [
                        body.rotation[0] + velocity.angular[0] * dt,
                        body.rotation[1] + velocity.angular[1] * dt,
                        body.rotation[2] + velocity.angular[2] * dt,
                    ],
                    velocity,
                }
            })
            .collect())
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn body(entity: Entity, velocity: Velocity) -> BodyState {
        BodyState {
            entity,
            translation: [0.0, 0.0, 0.0],
            rotation: [0.0, 0.0, 0.0],
            velocity,
            body: RigidBody::dynamic(1.0),
            collider: Some(Collider::default()),
        }
    }

    /// An entity handle to attach results to. Physics never creates entities, so any handle does.
    fn some_entity() -> Entity {
        let mut world = amadeo_ecs::World::new();
        world.spawn()
    }

    #[test]
    fn a_moving_body_moves() {
        let mut physics = NullPhysics::new();
        let entity = some_entity();
        let out = physics
            .step(&[body(entity, Velocity::linear(2.0, 0.0, 0.0))], [0.0; 3])
            .expect("no solver, no failure");

        assert_eq!(out.len(), 1);
        assert_eq!(out[0].entity, entity);
        // One tick at 60 Hz, so two units per second is 1/30 of a unit.
        assert!((out[0].translation[0] - 2.0 * FIXED_DT).abs() < 1e-6);
        assert_eq!(physics.steps(), 1);
    }

    #[test]
    fn gravity_accelerates_before_it_moves() {
        // Semi-implicit Euler: the velocity is updated first, so the first step already moves. The
        // explicit form would move by the *old* velocity — zero — and lose energy every step, which
        // is how a bouncing object slowly sinks through a floor.
        let mut physics = NullPhysics::new();
        let out = physics
            .step(
                &[body(some_entity(), Velocity::default())],
                [0.0, -10.0, 0.0],
            )
            .expect("no failure");

        assert!(
            out[0].translation[1] < 0.0,
            "a body under gravity should have moved on the first step, got {:?}",
            out[0].translation
        );
        assert!((out[0].velocity.linear[1] + 10.0 * FIXED_DT).abs() < 1e-6);
    }

    #[test]
    fn stepping_is_reproducible() {
        // The property everything else rests on (I3), asserted at the smallest level it exists at.
        let run = || {
            let mut physics = NullPhysics::new();
            let mut state = body(some_entity(), Velocity::linear(1.0, 0.0, -0.5));
            for _ in 0..120 {
                let out = physics
                    .step(&[state], [0.0, -9.81, 0.0])
                    .expect("no failure");
                state.translation = out[0].translation;
                state.rotation = out[0].rotation;
                state.velocity = out[0].velocity;
            }
            state.translation
        };
        assert_eq!(run(), run());
    }

    #[test]
    fn a_step_returns_one_result_per_body_in_the_same_order() {
        // The caller matches results back to entities positionally as well as by handle, and a
        // backend that reordered or dropped one would produce a world where two bodies swapped
        // places — reproducibly, which is the worst kind.
        let mut physics = NullPhysics::new();
        let mut world = amadeo_ecs::World::new();
        let entities: Vec<Entity> = (0..4).map(|_| world.spawn()).collect();
        let bodies: Vec<BodyState> = entities
            .iter()
            .map(|entity| body(*entity, Velocity::default()))
            .collect();

        let out = physics.step(&bodies, [0.0; 3]).expect("no failure");
        assert_eq!(
            out.iter().map(|result| result.entity).collect::<Vec<_>>(),
            entities
        );
    }

    #[test]
    fn a_static_body_does_not_move_however_hard_it_is_pushed() {
        // Static is the default, so this is what most bodies in a level do. A backend that moved
        // them would send the floor falling out from under everything on the first tick.
        let mut physics = NullPhysics::new();
        let mut state = body(some_entity(), Velocity::linear(100.0, 0.0, 0.0));
        state.body = RigidBody::default();
        assert_eq!(state.body.kind, BodyKind::Static);

        let out = physics
            .step(&[state], [0.0, -9.81, 0.0])
            .expect("no failure");
        assert_eq!(out[0].translation, [0.0, 0.0, 0.0]);
    }

    #[test]
    fn a_body_can_opt_out_of_gravity() {
        // A flying character or a projectile with its own arc, without the world's gravity changing
        // for everything else.
        let mut physics = NullPhysics::new();
        let mut state = body(some_entity(), Velocity::default());
        state.body.gravity = false;

        let out = physics
            .step(&[state], [0.0, -9.81, 0.0])
            .expect("no failure");
        assert_eq!(out[0].velocity.linear[1], 0.0);
        assert_eq!(out[0].translation[1], 0.0);
    }

    #[test]
    fn damping_bleeds_speed_away() {
        let mut physics = NullPhysics::new();
        let mut state = body(some_entity(), Velocity::linear(10.0, 0.0, 0.0));
        state.body.gravity = false;
        state.body.linear_damping = 2.0;

        let out = physics.step(&[state], [0.0; 3]).expect("no failure");
        assert!(
            out[0].velocity.linear[0] < 10.0,
            "damping should slow it, got {:?}",
            out[0].velocity
        );
    }

    #[test]
    fn a_body_with_no_collider_is_still_simulated() {
        // `None` is a real state — a marker that moves, or something whose collision gameplay
        // handles — rather than an oversight to substitute a default cube for.
        let mut physics = NullPhysics::new();
        let mut state = body(some_entity(), Velocity::linear(1.0, 0.0, 0.0));
        state.collider = None;

        let out = physics.step(&[state], [0.0; 3]).expect("no failure");
        assert!(out[0].translation[0] > 0.0);
    }

    #[test]
    fn no_bodies_is_not_an_error() {
        let mut physics = NullPhysics::new();
        assert!(
            physics
                .step(&[], [0.0, -9.81, 0.0])
                .expect("fine")
                .is_empty()
        );
        // Still counted, because "is physics running at all" is a question worth being able to ask
        // of a world that happens to be empty.
        assert_eq!(physics.steps(), 1);
    }
}
