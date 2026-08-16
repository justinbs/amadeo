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
mod query;
#[cfg(feature = "rapier")]
mod rapier;

pub use backend::{
    BodyResult, BodyState, NullPhysics, PhysicsBackend, PhysicsError, StaticMesh, StaticMeshId,
};
pub use components::{BodyKind, Collider, RigidBody, Shape, Velocity};
pub use query::{ShapeCast, ShapeHit, ShapeMotion, ShapeMove};
#[cfg(feature = "rapier")]
pub use rapier::RapierPhysics;

use amadeo_core::StableHash;
use amadeo_ecs::{Entity, Resource, Service, World};
use amadeo_reflect::Reflect;
use amadeo_transform::{GlobalTransform, Mat4, Parent, Transform};

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

    /// Throws away everything the solver was caching — see [`PhysicsBackend::reset`].
    ///
    /// # Call this after restoring a snapshot, or after loading a level
    ///
    /// **What it is measurably for is dropping static geometry.** That is derived data belonging to
    /// a *level* rather than to a body, it travels by id rather than through
    /// [`PhysicsBackend::step`], and keeping it would leave the previous world's ground standing in
    /// the next one. Whatever inserted it puts it back; the terrain streamer does that by noticing
    /// it is gone.
    ///
    /// # The contact-cache argument turns out not to bite, and that is a good thing
    ///
    /// The reason this was expected to matter is that a restore puts the **components** back and not
    /// a solver's contact caches, sleeping islands or warm-start data — ADR 0028's lesson about the
    /// entity allocator's free list, arriving through a second door.
    ///
    /// Measured, it does not happen: a warm solver handed a restored world matches a fresh one
    /// exactly, even with a settled and sleeping stack of dynamic bodies
    /// (`tests/reset_clears_the_solver.rs`). That is **ADR 0036's own contract paying off** — a
    /// backend must not keep state that cannot be rebuilt from the bodies it is given, so a solver
    /// honouring that has nothing to go stale. The decision that makes physics deterministic is what
    /// removes the hazard.
    ///
    /// Still call it. The static geometry half is real, it is the documented contract for replacing
    /// a world, and a backend that caches more than rapier does would need it.
    ///
    /// # Why this pass-through had to be added
    ///
    /// `PhysicsBackend::reset` has existed since ADR 0036 and has been documented since then as the
    /// thing that makes a physics game snapshot-able. **Nothing outside this crate could call it** —
    /// the backend is deliberately private (see [`Physics::insert_static_mesh`] for why), so the
    /// only callers were tests holding a backend directly. Found while building save and load, which
    /// is the first thing that restores a snapshot into a running game.
    pub fn reset(&mut self) {
        self.backend.reset();
        // The last error belonged to a world that no longer exists. Keeping it would make the next
        // `Physics::last_error` report a failure from before the restore, which is exactly the kind
        // of stale diagnostic that sends somebody looking in the wrong place.
        self.last_error = None;
    }

    /// Adds or replaces static collision geometry held between steps — see
    /// [`PhysicsBackend::insert_static_mesh`].
    ///
    /// # Why this is a pass-through rather than an exposed backend
    ///
    /// The backend stays private. Handing out `&mut dyn PhysicsBackend` would let any caller drive
    /// `step` directly, and the whole reason [`step_physics`] exists is that a step must be fed from
    /// the world's components — which are the source of truth (ADR 0036). Static geometry is the one
    /// thing that genuinely cannot travel that way, because it is far too large to hand over every
    /// tick and ADR 0042 will not have vertices in the state hash, so it gets its own door.
    ///
    /// # Errors
    ///
    /// [`PhysicsError::BadGeometry`] if the mesh cannot be built. An empty mesh is rejected —
    /// filter with [`StaticMesh::is_empty`] first.
    pub fn insert_static_mesh(&mut self, mesh: StaticMesh) -> Result<(), PhysicsError> {
        self.backend.insert_static_mesh(mesh)
    }

    /// Removes static collision geometry. Removing something absent is not an error.
    pub fn remove_static_mesh(&mut self, id: StaticMeshId) {
        self.backend.remove_static_mesh(id);
    }

    /// How many pieces of static geometry the backend holds. Diagnostics and tests.
    #[must_use]
    pub fn static_mesh_count(&self) -> usize {
        self.backend.static_mesh_count()
    }

    /// Moves one shape through the world, sliding along what it hits — ADR 0037.
    ///
    /// The gameplay-facing half of [`PhysicsBackend::move_shape`]. Reach it with
    /// [`World::with_service_taken`](amadeo_ecs::World::with_service_taken), which is what lets a
    /// system hold the world and the backend at once.
    ///
    /// **Call this after [`step_physics`] has run this tick.** A backend answers from a spatial
    /// index the step builds, so asking first queries an empty one on tick 1 and the shape passes
    /// through the level exactly once.
    pub fn move_shape(&mut self, request: &ShapeMove) -> ShapeMotion {
        self.backend.move_shape(request)
    }

    /// The gameplay-facing half of [`PhysicsBackend::cast_shape`] — ADR 0054. `None` means clear.
    ///
    /// **Call this after [`step_physics`] has run this tick**, for the reason [`Physics::move_shape`]
    /// gives: a backend answers from an index the step builds, and an empty index finds nothing in
    /// the way anywhere.
    ///
    /// Takes `&self`, so a system can reach it with
    /// [`World::service`](amadeo_ecs::World::service) and ask alongside a query rather than taking
    /// the service mutably to ask a read-only question.
    #[must_use]
    pub fn cast_shape(&self, cast: &ShapeCast) -> Option<ShapeHit> {
        self.backend.cast_shape(cast)
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

    // Which bodies hang off something else. Collected up front because a query cannot ask the world
    // a second question while it is running, and sorted so the lookup below is a binary search.
    let mut parented: Vec<Entity> = world
        .query::<(&RigidBody, &Parent)>()
        .map(|(entity, _)| entity)
        .collect();
    parented.sort_unstable();

    // `GlobalTransform` when propagation has run, the local transform otherwise — the same fallback
    // the renderer uses, and for the same reason: requiring it would mean forgetting one system
    // makes physics silently wrong rather than slightly wrong.
    let bodies: Vec<BodyState> = world
        .query::<(
            &RigidBody,
            &Transform,
            Option<&Velocity>,
            Option<&GlobalTransform>,
            Option<&Collider>,
        )>()
        .map(|(entity, (body, transform, velocity, global, collider))| {
            // **A child's pose comes from the composed matrix; a root's comes from its own.**
            //
            // For a child there is no choice: its `Transform` is relative to its parent, so a door
            // authored square inside a piece that is turned a quarter turn has a local rotation of
            // zero and a world rotation of ninety degrees. Hand the solver the zero and it builds
            // the collider facing the wrong way.
            //
            // For a root the two are the same thing by definition — and the local one is
            // **fresher**. Propagation runs in `PostSimulation`, so a `GlobalTransform` read here
            // is always a tick old, and anything that wrote a `Transform` since is silently undone
            // by the write-back below. That is how a body placed between ticks snaps back to where
            // it was, and it is a real thing that happened the moment scene loading started
            // composing the hierarchy up front.
            let child = parented.binary_search(&entity).is_ok();
            let (placement, orientation) = match (child, global) {
                (true, Some(global)) => {
                    let matrix = global.to_mat4();
                    (matrix.translation(), matrix.to_euler_degrees())
                }
                _ => (transform.translation, transform.rotation),
            };
            BodyState {
                entity,
                translation: placement,
                rotation: orientation,
                velocity: velocity.copied().unwrap_or_default(),
                body: *body,
                // Optional, and absence means "collides with nothing" rather than "give it a
                // default shape" — inventing a one-metre cube for an entity nobody gave a collider
                // would put an invisible obstacle in the world.
                collider: collider.copied(),
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
        // **A static body is authored, not simulated.** The solver was handed its pose and hands
        // the same pose back, so writing it in can only ever be a no-op or a mistake — and it was a
        // mistake, because the pose comes back in *world* space while a `Transform` is in its
        // parent's. Skipping is both cheaper and the only defensible reading of "static".
        if world
            .get::<RigidBody>(result.entity)
            .is_some_and(|body| body.kind == BodyKind::Static)
        {
            continue;
        }

        // A body despawned during the step simply is not here any more. Skipping is right: the
        // alternative is resurrecting a handle, which the generational index would refuse anyway.
        if let Some(transform) = world.get::<Transform>(result.entity) {
            let mut moved = *transform;
            let (translation, rotation) = in_its_own_space(world, result.entity, &result);
            moved.translation = translation;
            moved.rotation = rotation;
            world.insert(result.entity, moved);
        }
        if world.get::<Velocity>(result.entity).is_some() {
            world.insert(result.entity, result.velocity);
        }
    }
}

/// Turns a solver's world-space answer back into the space the entity's own `Transform` is written
/// in — which for a child is its parent's, and for a root is the world.
///
/// # The bug this exists to stop
///
/// A backend is given world poses and returns world poses; a `Transform` on a child is **relative to
/// its parent**. Writing one straight into the other therefore stores the world position as if it
/// were a local one, and `propagate_transforms` then applies the parent on top of it — so every
/// tick, a parented body moves by its parent's offset again. It reads as geometry sliding away from
/// where the file puts it, and it is invisible on tick one because nothing has propagated yet.
///
/// It went unnoticed because until ADR 0071's room pieces there was no reason to put a collider on
/// anything but a prefab root. A piece with more than one collider has no choice: a prefab has
/// exactly one root, so a doorway's two jambs and its lintel are all children.
///
/// # What it does not handle
///
/// A **scaled** parent. The scale is dropped here, as it is everywhere else in this crate — a
/// collider carries its own size and nothing scales a shape — so a body under a scaled parent lands
/// in the right place with an unscaled collider. Reporting that would need an error channel
/// `step_physics` does not have, and a scaled physics body is ill-defined anyway.
fn in_its_own_space(world: &World, entity: Entity, result: &BodyResult) -> ([f32; 3], [f32; 3]) {
    let world_pose = (result.translation, result.rotation);

    // Each `else` is a root, an orphan, or a parent so degenerate it has no inverse. In all three
    // the honest answer is the world pose: it is what the old code always did, and for a root it is
    // exactly right.
    let Some(parent) = world.get::<Parent>(entity).map(|parent| parent.0) else {
        return world_pose;
    };
    let Some(global) = world.get::<GlobalTransform>(parent) else {
        return world_pose;
    };
    let Some(inverse) = global.to_mat4().inverse_rigid() else {
        return world_pose;
    };

    let placed = inverse.mul(&Mat4::from_transform(
        result.translation,
        result.rotation,
        [1.0, 1.0, 1.0],
    ));
    (placed.translation(), placed.to_euler_degrees())
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

    /// A body hanging off a root that has been moved and turned — a prefab piece, in other words.
    ///
    /// The root is placed and rotated so that composing it is not the identity in either respect:
    /// a bug that only added the parent's translation would still pass against an unrotated one.
    fn piece_world() -> (World, amadeo_ecs::Entity, amadeo_ecs::Entity) {
        let mut world = World::new();
        world.insert_service(Physics::new(Box::new(NullPhysics::new())));
        world.insert_resource(Gravity::none());

        let root = world.spawn();
        world.insert(
            root,
            Transform {
                translation: [30.0, 0.0, -12.0],
                rotation: [0.0, 90.0, 0.0],
                scale: [1.0, 1.0, 1.0],
            },
        );

        let child = world.spawn();
        world.insert(
            child,
            Transform {
                translation: [0.0, 1.2, 0.0],
                rotation: [0.0, 0.0, 0.0],
                scale: [1.0, 1.0, 1.0],
            },
        );
        world.insert(child, Parent(root));
        world.insert(child, RigidBody::default());
        world.insert(child, Collider::cuboid(1.6, 2.4, 0.16));

        amadeo_transform::propagate_transforms(&mut world);
        (world, root, child)
    }

    #[test]
    fn a_body_on_a_piece_stays_where_the_piece_puts_it() {
        // **The defect this pins made every generated interior wrong, and hid for a whole session.**
        //
        // A backend is handed world poses and returns world poses. A `Transform` on a child is
        // relative to its parent. Writing one straight into the other stores the world position as
        // if it were local, and `propagate_transforms` then applies the parent *again* — so a
        // parented body walks away from its piece by the piece's own offset, once per tick.
        //
        // It was invisible until ADR 0071's room pieces, because nothing before them had a reason
        // to put a collider on anything but a prefab root. A piece with two colliders has no
        // choice: a prefab has exactly one root, so a doorway's jambs and its lintel are children.
        //
        // And it was invisible on the *first* tick, which is why a capture taken at tick one looked
        // fine. Nothing had propagated yet, so the fallback to the local transform was still in
        // play and the first step was correct.
        let (mut world, _root, child) = piece_world();
        let placed = world
            .get::<GlobalTransform>(child)
            .expect("composed")
            .to_mat4()
            .translation();

        for _ in 0..30 {
            step_physics(&mut world);
            amadeo_transform::propagate_transforms(&mut world);
        }

        let after = world
            .get::<GlobalTransform>(child)
            .expect("still composed")
            .to_mat4()
            .translation();
        assert_eq!(
            placed, after,
            "half a second of physics moved a static body off its piece"
        );
    }

    #[test]
    fn a_moving_body_on_a_piece_is_stored_in_the_pieces_own_space() {
        // The other half, and the one that would still be wrong if `step_physics` merely skipped
        // static bodies. A body the solver *may* move has its answer converted back through the
        // parent, so what lands in the `Transform` is a local offset — and composing it returns the
        // world position the solver actually reported.
        let (mut world, _root, child) = piece_world();
        world.insert(child, RigidBody::dynamic(1.0));
        world.insert(child, Velocity::default());
        world.insert_resource(Gravity::earth());

        step_physics(&mut world);
        let stored = world
            .get::<Transform>(child)
            .expect("still there")
            .translation;
        amadeo_transform::propagate_transforms(&mut world);
        let composed = world
            .get::<GlobalTransform>(child)
            .expect("composed")
            .to_mat4()
            .translation();

        // It fell, so the world position is below where it started and the *local* one is not the
        // world one — which is the whole claim. A local transform that equalled the world position
        // would mean the conversion had not happened.
        assert!(composed[1] < 1.2, "gravity should have pulled it down");
        assert!(
            (stored[0] - composed[0]).abs() > 1.0,
            "a local offset of {stored:?} should not match the world position {composed:?}"
        );
    }

    #[test]
    fn a_body_placed_between_ticks_stays_where_it_was_put() {
        // Moving something by writing its `Transform` is what a test, a teleport and a level
        // transition all do. It used to be undone on the next step whenever the world had a
        // composed `GlobalTransform` to read instead — which, since scene loading started composing
        // the hierarchy up front, is always. The read now prefers a root's own transform, which is
        // the same value one tick fresher.
        let (mut world, entity) = falling_world();
        world.insert(entity, RigidBody::kinematic());
        step_physics(&mut world);
        amadeo_transform::propagate_transforms(&mut world);

        if let Some(transform) = world.get_mut::<Transform>(entity) {
            transform.translation = [50.0, 4.0, -7.0];
        }
        step_physics(&mut world);

        let now = world.get::<Transform>(entity).expect("there").translation;
        assert!(
            (now[0] - 50.0).abs() < 0.01 && (now[2] + 7.0).abs() < 0.01,
            "it was put at 50, 4, -7 and the step moved it to {now:?}"
        );
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
