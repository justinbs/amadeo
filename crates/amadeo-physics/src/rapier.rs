//! The rapier backend. Only compiled with the `rapier` feature.
//!
//! # What this file is allowed to be, and what it is not
//!
//! It is the **only** place in the engine that names a rapier type. ADR 0036 §4 makes that a rule
//! rather than a habit: no rapier type may appear in a component, a scene file, a snapshot or the
//! state hash, because ADR 0036 §3 pins the rapier version exactly — a leaked type would put an
//! upgrade in the scene format *and* the state hash at once.
//!
//! Everything crossing [`PhysicsBackend`] is a type `amadeo-physics` defines, so that rule is
//! enforced by the module boundary rather than by remembering.
//!
//! # The features are the decision, not a detail
//!
//! `enhanced-determinism` is on and `parallel` and `simd-*` are off, permanently (ADR 0036). That is
//! set in `Cargo.toml` with `default-features = false`, because rapier's *defaults* include the ones
//! this engine forbids — a default build would quietly be the fast, nondeterministic one.
//!
//! # Why the world persists between steps
//!
//! Rapier keeps state a single step's inputs do not describe: contact caches, sleeping islands, the
//! warm-starting that makes a stack of boxes settle instead of jittering. Throwing it away each step
//! would be simpler and would make stacking visibly worse.
//!
//! That state is a deterministic function of history, so it cannot break a replay — but it **can**
//! break a snapshot, in the exact way ADR 0028 already found once with the entity allocator's free
//! list: restoring components without it gives a world that hashes identically and then simulates
//! differently. [`PhysicsBackend::reset`] is the answer, and `RapierPhysics` implements it by
//! throwing the whole world away so the next step rebuilds from the components — which are the
//! source of truth precisely so that this is possible.

use crate::backend::{BodyResult, BodyState, PhysicsBackend, PhysicsError};
use crate::components::{BodyKind, Shape, Velocity};
use amadeo_core::FIXED_DT;
use amadeo_ecs::Entity;
use amadeo_transform::Mat4;
use rapier3d::math::{Matrix, Rotation, Vector};
use rapier3d::prelude::*;
use std::collections::BTreeMap;

/// Rapier, configured for determinism.
pub struct RapierPhysics {
    bodies: RigidBodySet,
    colliders: ColliderSet,
    pipeline: PhysicsPipeline,
    islands: IslandManager,
    broad_phase: DefaultBroadPhase,
    narrow_phase: NarrowPhase,
    impulse_joints: ImpulseJointSet,
    multibody_joints: MultibodyJointSet,
    ccd: CCDSolver,
    parameters: IntegrationParameters,
    /// Which rapier body each entity owns.
    ///
    /// A `BTreeMap` rather than a hash map, like every other registry in this engine: iteration
    /// order reaches the order bodies are inserted into rapier's sets, and iteration order deciding
    /// a simulation result is exactly the nondeterminism trap `CLAUDE.md` lists second.
    handles: BTreeMap<Entity, RigidBodyHandle>,
}

/// Hand-written because **rapier's types do not implement `Debug`**, and [`PhysicsBackend`] requires
/// it so that a world can be printed while diagnosing one.
///
/// Reporting the counts rather than the contents is the right answer anyway: a rapier world printed
/// in full is thousands of lines, and the two questions actually worth asking of it are "is anything
/// in there" and "did it lose track of an entity".
impl std::fmt::Debug for RapierPhysics {
    fn fmt(&self, formatter: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        formatter
            .debug_struct("RapierPhysics")
            .field("bodies", &self.bodies.len())
            .field("colliders", &self.colliders.len())
            .field("tracked_entities", &self.handles.len())
            .finish()
    }
}

impl Default for RapierPhysics {
    fn default() -> Self {
        Self::new()
    }
}

impl RapierPhysics {
    /// A fresh, empty simulation.
    #[must_use]
    pub fn new() -> Self {
        let parameters = IntegrationParameters {
            // The engine's tick, not rapier's default. ADR 0007 fixes the rate at 60 Hz, and a
            // solver stepping by a different amount than the simulation believes would make every
            // velocity wrong by a constant factor -- which reads as "gravity is too weak" rather
            // than as a timestep bug.
            dt: FIXED_DT,
            ..IntegrationParameters::default()
        };

        Self {
            bodies: RigidBodySet::new(),
            colliders: ColliderSet::new(),
            pipeline: PhysicsPipeline::new(),
            islands: IslandManager::new(),
            broad_phase: DefaultBroadPhase::new(),
            narrow_phase: NarrowPhase::new(),
            impulse_joints: ImpulseJointSet::new(),
            multibody_joints: MultibodyJointSet::new(),
            ccd: CCDSolver::new(),
            parameters,
            handles: BTreeMap::new(),
        }
    }

    /// Turns Amadeo's Euler degrees into a rotation rapier understands.
    ///
    /// Goes through [`Mat4::from_euler_degrees`] rather than nalgebra's `from_euler_angles`, because
    /// **the two use different orders**: ADR 0018 composes Z, then X, then Y, and nalgebra composes
    /// roll-pitch-yaw. Using nalgebra's would be right for one axis and subtly wrong for the others,
    /// which reads as physics drifting rather than as a conversion bug.
    fn rotation_of(degrees: [f32; 3]) -> Rotation {
        let matrix = Mat4::from_euler_degrees(degrees);
        // Both are column-major and both store the basis as three axis vectors, so the columns
        // transfer across directly with no transpose.
        let column = |index: usize| {
            Vector::new(
                matrix.columns[index][0],
                matrix.columns[index][1],
                matrix.columns[index][2],
            )
        };
        Rotation::from_mat3(&Matrix::from_cols(column(0), column(1), column(2)))
    }

    /// And back again, through the same convention.
    fn degrees_of(rotation: Rotation) -> [f32; 3] {
        let basis = Matrix::from_quat(rotation);
        let mut matrix = Mat4::IDENTITY;
        for index in 0..3 {
            let axis = basis.col(index);
            matrix.columns[index][0] = axis.x;
            matrix.columns[index][1] = axis.y;
            matrix.columns[index][2] = axis.z;
        }
        matrix.to_euler_degrees()
    }

    /// Builds the rapier collider for one of this engine's shapes.
    ///
    /// Rapier takes **half**-extents where Amadeo authors full sizes, which is the single most
    /// likely place for a collider to end up twice the size it should be.
    fn collider_for(shape: Shape) -> Option<ColliderBuilder> {
        match shape {
            Shape::Cuboid { size } => Some(ColliderBuilder::cuboid(
                size[0] / 2.0,
                size[1] / 2.0,
                size[2] / 2.0,
            )),
            Shape::Sphere { radius } => Some(ColliderBuilder::ball(radius)),
            // Rapier's capsule is also described by a half-height, and by the *straight* section
            // only -- the same convention `Shape::Capsule` documents, so this is a halving and not a
            // change of meaning.
            Shape::Capsule { radius, height } => {
                Some(ColliderBuilder::capsule_y(height / 2.0, radius))
            }
        }
    }
}

impl PhysicsBackend for RapierPhysics {
    fn name(&self) -> &'static str {
        "rapier"
    }

    fn reset(&mut self) {
        // Everything, not just the bodies: the caches are what has to go. The next step rebuilds
        // from the components, which is why they are the source of truth.
        *self = Self::new();
    }

    fn step(
        &mut self,
        bodies: &[BodyState],
        gravity: [f32; 3],
    ) -> Result<Vec<BodyResult>, PhysicsError> {
        // Sync in. Bodies are handed over every step, so a new entity appears here and a despawned
        // one is removed below -- the caller never has to tell this backend about lifetimes.
        let mut seen = std::collections::BTreeSet::new();
        for state in bodies {
            seen.insert(state.entity);
            // `Vector::new` rather than rapier's `vector![]` macro: the macro still builds an
            // nalgebra vector, while rapier 0.34's own API takes glam's. They are both "a vector"
            // and the compiler is the only thing that notices.
            let translation = Vector::new(
                state.translation[0],
                state.translation[1],
                state.translation[2],
            );
            let rotation = Self::rotation_of(state.rotation);
            let linvel = Vector::new(
                state.velocity.linear[0],
                state.velocity.linear[1],
                state.velocity.linear[2],
            );
            // Amadeo authors angular velocity in degrees per second to match `Transform`'s degrees
            // (ADR 0018); rapier works in radians.
            let angvel = Vector::new(
                state.velocity.angular[0].to_radians(),
                state.velocity.angular[1].to_radians(),
                state.velocity.angular[2].to_radians(),
            );

            if let Some(handle) = self.handles.get(&state.entity) {
                // Already known: push the authored values in rather than rebuilding, so rapier keeps
                // the contact state that makes stacking stable.
                let Some(body) = self.bodies.get_mut(*handle) else {
                    continue;
                };
                body.set_translation(translation, true);
                body.set_rotation(rotation, true);
                body.set_linvel(linvel, true);
                body.set_angvel(angvel, true);
                continue;
            }

            let builder = match state.body.kind {
                BodyKind::Static => RigidBodyBuilder::fixed(),
                BodyKind::Dynamic => RigidBodyBuilder::dynamic(),
                BodyKind::Kinematic => RigidBodyBuilder::kinematic_position_based(),
            };
            let body = builder
                .translation(translation)
                .linvel(linvel)
                .angvel(angvel)
                .linear_damping(state.body.linear_damping)
                .angular_damping(state.body.angular_damping)
                // Per-body gravity, so a flying character opts out without the world changing.
                .gravity_scale(if state.body.gravity { 1.0 } else { 0.0 })
                .build();
            let handle = self.bodies.insert(body);
            // Set after insertion rather than on the builder, because the builder takes a rotation
            // as a scaled axis while everything else here speaks quaternions — one conversion is
            // one fewer place for the Euler convention to be got wrong.
            if let Some(inserted) = self.bodies.get_mut(handle) {
                inserted.set_rotation(rotation, false);
            }

            if let Some(collider) = state.collider {
                let Some(builder) = Self::collider_for(collider.shape) else {
                    return Err(PhysicsError::BadBody {
                        entity: state.entity,
                        reason: "its collider shape cannot be represented".to_string(),
                    });
                };
                let built = builder
                    .friction(collider.friction)
                    .restitution(collider.restitution)
                    .sensor(collider.sensor)
                    // Mass comes from the `RigidBody`, so the collider must not also contribute one
                    // -- otherwise the authored mass is silently added to a density-derived one.
                    .mass(state.body.mass)
                    .build();
                self.colliders
                    .insert_with_parent(built, handle, &mut self.bodies);
            }
            self.handles.insert(state.entity, handle);
        }

        // Anything that stopped being handed over has been despawned or lost its `RigidBody`.
        let gone: Vec<Entity> = self
            .handles
            .keys()
            .filter(|entity| !seen.contains(entity))
            .copied()
            .collect();
        for entity in gone {
            if let Some(handle) = self.handles.remove(&entity) {
                self.bodies.remove(
                    handle,
                    &mut self.islands,
                    &mut self.colliders,
                    &mut self.impulse_joints,
                    &mut self.multibody_joints,
                    true,
                );
            }
        }

        self.pipeline.step(
            Vector::new(gravity[0], gravity[1], gravity[2]),
            &self.parameters,
            &mut self.islands,
            &mut self.broad_phase,
            &mut self.narrow_phase,
            &mut self.bodies,
            &mut self.colliders,
            &mut self.impulse_joints,
            &mut self.multibody_joints,
            &mut self.ccd,
            &(),
            &(),
        );

        // Read back in the order the caller gave, so results line up with bodies positionally as
        // well as by handle -- the property `a_step_returns_one_result_per_body_in_the_same_order`
        // pins for every backend.
        Ok(bodies
            .iter()
            .filter_map(|state| {
                let handle = self.handles.get(&state.entity)?;
                let body = self.bodies.get(*handle)?;
                let translation = body.translation();
                let linvel = body.linvel();
                let angvel = body.angvel();
                Some(BodyResult {
                    entity: state.entity,
                    translation: [translation.x, translation.y, translation.z],
                    rotation: Self::degrees_of(*body.rotation()),
                    velocity: Velocity {
                        linear: [linvel.x, linvel.y, linvel.z],
                        angular: [
                            angvel.x.to_degrees(),
                            angvel.y.to_degrees(),
                            angvel.z.to_degrees(),
                        ],
                    },
                })
            })
            .collect())
    }
}
