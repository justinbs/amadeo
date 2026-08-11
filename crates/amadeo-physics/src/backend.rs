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
use crate::query::{ShapeCast, ShapeHit, ShapeMotion, ShapeMove};
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

    /// Static geometry could not be turned into a collision shape.
    #[error(
        "static mesh {id:?} cannot be used for collision: {reason} \
         ({vertices} vertices, {triangles} triangles)"
    )]
    BadGeometry {
        /// Which mesh.
        id: StaticMeshId,
        /// What was wrong with it.
        reason: String,
        /// How many vertices it had, which is usually the first thing worth knowing.
        vertices: usize,
        /// How many triangles it had.
        triangles: usize,
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

/// Names one piece of static collision geometry held by a backend between steps.
///
/// Opaque on purpose. The caller decides what the number means — the terrain streamer derives it
/// from a chunk key — and this crate never interprets it, exactly as `move_shape` knows nothing
/// about characters (ADR 0037).
#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Hash)]
pub struct StaticMeshId(pub u64);

/// A triangle mesh that does not move, held by the backend until it is removed.
///
/// # Why this does not go through `step` like everything else
///
/// [`BodyState`] is `Copy` and is handed over in full every tick, which is what makes a step a pure
/// function. Terrain cannot work that way: a chunk is thousands of triangles, there are hundreds of
/// chunks, and copying all of it sixty times a second to say "still there" would dominate the frame.
///
/// # And why it is not a [`Collider`] component
///
/// [`Shape`](crate::Shape) is `Copy` and `StableHash`. A triangle mesh is neither cheap to copy nor
/// something ADR 0042 will allow into the state hash — its whole point is that an untouched world
/// costs nothing to hash, and walking a world's worth of vertices is the opposite of that.
///
/// So the geometry travels the way a texture travels to the GPU: **by id, uploaded once**. It is
/// *derived* data — regenerable from a seed and a sparse edit list — so ADR 0019's rule applies and
/// it belongs outside the hash. What *is* hashed is the seed and the edits, which are what produced
/// it.
///
/// # This is a mechanism, not terrain
///
/// Nothing here knows about chunks, voxels or ground. A static trimesh is equally what an imported
/// level's collision geometry is, or a bridge, or a piece of scenery too concave for a box.
#[derive(Debug, Clone, PartialEq)]
pub struct StaticMesh {
    /// What to call this geometry, so it can be replaced or removed later.
    pub id: StaticMeshId,
    /// Where the mesh's origin sits in the world. Vertices are relative to it.
    ///
    /// Separate from the vertices so that a chunk's mesh can be generated in its own local space and
    /// placed by one translation — which is also what stops a chunk a kilometre out losing precision
    /// in its vertex coordinates.
    pub translation: [f32; 3],
    /// Vertex positions, relative to `translation`.
    pub vertices: Vec<[f32; 3]>,
    /// Triangles, as indices into `vertices`.
    pub indices: Vec<[u32; 3]>,
    /// How much sliding contact is resisted, as on [`Collider`].
    pub friction: f32,
}

impl StaticMesh {
    /// Whether there is any geometry here at all.
    ///
    /// **Worth checking before inserting.** Most chunks of a real world are entirely air or entirely
    /// rock, and both mesh into nothing — that is the honest answer rather than a failure. A backend
    /// asked to build a triangle mesh from no triangles has every right to refuse, so the empty case
    /// is filtered rather than handed over.
    #[must_use]
    pub fn is_empty(&self) -> bool {
        self.indices.is_empty() || self.vertices.is_empty()
    }
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

    /// Moves one shape through the world, sliding along whatever it hits — ADR 0037.
    ///
    /// # Why this is a second operation rather than part of `step`
    ///
    /// A `Kinematic` body handed to `step` goes exactly where gameplay put it, walls included. That
    /// is what kinematic means, and it is what a moving platform wants. A character wants the other
    /// question: *given this much desired motion, where do I actually end up?*
    ///
    /// # It must run after `step`, and the caller is responsible for that
    ///
    /// A real backend answers this from a spatial index that `step` builds. Asking before the first
    /// step means asking an **empty** index, and the shape passes through the level — once, on tick
    /// one, which is close to the hardest kind of bug to notice. `modules/amadeo-character`
    /// registers its system `.after(STEP_PHYSICS)` for exactly this reason.
    ///
    /// # No `Result`, deliberately
    ///
    /// Every [`Shape`](crate::Shape) variant is representable by every backend, so there is no
    /// failure to report. A backend that cannot detect anything returns
    /// [`ShapeMotion::unobstructed`], which is an honest answer rather than an error.
    fn move_shape(&mut self, request: &ShapeMove) -> ShapeMotion;

    /// Sweeps one shape along a straight line and reports the first thing in the way — ADR 0054.
    ///
    /// `None` means the whole motion is clear.
    ///
    /// # Why this is a third operation rather than a flag on `move_shape`
    ///
    /// [`move_shape`](Self::move_shape) answers *"where does this body end up?"* and slides to do it.
    /// This answers *"how far along this line before something blocks it?"* and does not. Half of
    /// `ShapeMove`'s fields — `step_height`, `snap_distance`, `max_slope_degrees`, `up` — are
    /// meaningless to the second question, and a `slide: bool` that silently voided four of them was
    /// the alternative **Q34** rejected.
    ///
    /// # It must run after `step`, for `move_shape`'s reason
    ///
    /// Same spatial index, same failure: asking before the first step queries an empty world and
    /// finds everything clear.
    ///
    /// # `&self`, unlike its neighbours
    ///
    /// A cast is a question. It reads the index `step` built and changes nothing, and saying so in
    /// the signature is what lets a caller hold it alongside a world query rather than taking the
    /// service mutably to ask something read-only.
    fn cast_shape(&self, cast: &ShapeCast) -> Option<ShapeHit>;

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
    ///
    /// **It also drops every [`StaticMesh`].** That is deliberate: static geometry is derived data,
    /// so throwing it away loses nothing that cannot be rebuilt, and keeping it across a snapshot
    /// restore would leave the old world's terrain standing in the new one. Whatever inserted the
    /// geometry is responsible for putting it back, which the terrain streamer does by noticing it
    /// is missing.
    fn reset(&mut self) {}

    /// Adds or replaces one piece of static collision geometry, keeping it until it is removed.
    ///
    /// Inserting an id that already exists **replaces** it, which is what editing terrain does — a
    /// chunk that has been dug into is the same chunk with a different surface.
    ///
    /// # This is a gameplay-visible operation, and its timing matters
    ///
    /// ADR 0041 §2: a chunk's collider is what a character stands on, so *when* it arrives changes
    /// where the character ends up. A caller must therefore insert the geometry it needs **before**
    /// the step that needs it, and block if it is not ready yet — a frame hitch that keeps its
    /// replay, rather than a character falling through a world that had not finished loading.
    ///
    /// # Errors
    ///
    /// [`PhysicsError::BadGeometry`] if the mesh cannot be built — degenerate triangles, or an index
    /// pointing past the end of the vertices. An empty mesh is *not* an error to construct but is
    /// rejected here, because most chunks of a real world are empty and the caller should be
    /// filtering them with [`StaticMesh::is_empty`] rather than asking.
    fn insert_static_mesh(&mut self, mesh: StaticMesh) -> Result<(), PhysicsError>;

    /// Removes static geometry. Removing something that is not there is not an error — a chunk that
    /// never had a collider because it was empty is the common case, not a mistake.
    fn remove_static_mesh(&mut self, id: StaticMeshId);

    /// How many pieces of static geometry are held. Diagnostics and tests.
    fn static_mesh_count(&self) -> usize;
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
    /// Static geometry it has been given, tracked but never collided against.
    ///
    /// A `BTreeSet` rather than a count so that inserting the same id twice is a replacement here
    /// too — otherwise this backend and the real one would disagree about how many pieces exist, and
    /// a test asserting that count would pass against one and fail against the other.
    static_meshes: std::collections::BTreeSet<StaticMeshId>,
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

    /// Applies the motion in full and reports mid-air, because nothing here can detect otherwise.
    ///
    /// # This being useless is the point
    ///
    /// It is the same posture `step` takes: a backend without a *solver*, not a stub. And it is
    /// what makes the character tests evidence rather than decoration — pointed at this backend, a
    /// character walks straight through a wall, which is what proves the passing rapier test is
    /// measuring collision response and not an accidentally-correct constant. ADR 0037 §5.
    fn move_shape(&mut self, request: &ShapeMove) -> ShapeMotion {
        ShapeMotion::unobstructed(request)
    }

    /// Finds nothing, ever, which is the same honest uselessness as everything else here.
    ///
    /// Pointed at this backend a follow camera keeps its full arm length inside solid rock, which is
    /// what makes the passing rapier test evidence that the sweep is consulted rather than evidence
    /// that the authored distance happened to be right.
    fn cast_shape(&self, _cast: &ShapeCast) -> Option<ShapeHit> {
        None
    }

    /// Records the geometry and collides with none of it — the same posture as `step` and
    /// `move_shape`.
    ///
    /// **This being useless is what makes the terrain tests evidence.** Pointed at this backend, a
    /// body dropped onto a meshed chunk falls straight through it, which is what proves the passing
    /// rapier test is measuring real collision against real triangles rather than a body that
    /// happened to start at rest. ADR 0037 §5, applied to terrain.
    fn insert_static_mesh(&mut self, mesh: StaticMesh) -> Result<(), PhysicsError> {
        // Rejected here as well as in the real backend, so that a caller which forgets to filter
        // empty chunks fails the same way against both. A null backend that accepted more than the
        // real one would hide the bug until someone turned rapier on.
        if mesh.is_empty() {
            return Err(PhysicsError::BadGeometry {
                id: mesh.id,
                reason:
                    "the mesh has no triangles; filter empty chunks with `StaticMesh::is_empty`"
                        .to_string(),
                vertices: mesh.vertices.len(),
                triangles: mesh.indices.len(),
            });
        }
        self.static_meshes.insert(mesh.id);
        Ok(())
    }

    fn remove_static_mesh(&mut self, id: StaticMeshId) {
        self.static_meshes.remove(&id);
    }

    fn static_mesh_count(&self) -> usize {
        self.static_meshes.len()
    }

    /// Drops the recorded geometry, matching what a real backend does on a reset.
    fn reset(&mut self) {
        self.static_meshes.clear();
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::components::Shape;

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
    fn the_null_backend_moves_a_shape_straight_through_everything() {
        // Not a limitation being tolerated — it is the control case ADR 0037 §5 relies on. A
        // character test that passes against *this* backend is not testing collision.
        let mut physics = NullPhysics::new();
        let request = ShapeMove::new(
            Shape::Capsule {
                radius: 0.4,
                height: 1.2,
            },
            [0.0, 1.0, 0.0],
            [5.0, 0.0, 0.0],
        );
        let motion = physics.move_shape(&request);
        assert_eq!(motion.translation, [5.0, 1.0, 0.0]);
        assert!(!motion.grounded, "nothing here can detect ground");
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
