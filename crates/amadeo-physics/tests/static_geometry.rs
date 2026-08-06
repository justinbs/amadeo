//! Static triangle meshes are solid, and the null backend proves the claim is worth something.
//!
//! ADR 0043 found that terrain collision cannot be a `Collider` component: `Shape` is `Copy` and
//! `StableHash`, and a triangle mesh is neither cheap to copy nor something ADR 0042 will allow into
//! the state hash. So it reaches the backend by **id**, the way a texture reaches the GPU, and this
//! file is what says that path actually works.
//!
//! # Every collision claim here is made twice
//!
//! Once against rapier, where it must hold, and once against `NullPhysics`, where it must **fail**.
//! That is the discipline ADR 0037 §5 arrived at for the character controller, applied to terrain: a
//! body that rests on a triangle mesh is only evidence of collision if the same body falls through
//! the same mesh when there is no solver behind it.
//!
//! The rapier half is gated on the feature; the null half runs in every build, so the control case
//! is never the one that gets skipped.

use amadeo_ecs::{Entity, World};
use amadeo_physics::{
    BodyState, NullPhysics, PhysicsBackend, PhysicsError, RigidBody, StaticMesh, StaticMeshId,
    Velocity,
};

/// A flat square of ground as two triangles, centred on the origin at `y = 0`.
///
/// Hand-built rather than meshed from a field, deliberately: this file is about whether the physics
/// boundary carries geometry, and borrowing `amadeo-voxel` here would make a failure ambiguous
/// between the two.
fn ground(id: u64) -> StaticMesh {
    StaticMesh {
        id: StaticMeshId(id),
        translation: [0.0, 0.0, 0.0],
        vertices: vec![
            [-10.0, 0.0, -10.0],
            [10.0, 0.0, -10.0],
            [10.0, 0.0, 10.0],
            [-10.0, 0.0, 10.0],
        ],
        indices: vec![[0, 1, 2], [0, 2, 3]],
        friction: 0.5,
    }
}

/// An entity handle to hang a body on. Physics never creates entities, so any handle will do.
fn an_entity() -> Entity {
    World::new().spawn()
}

/// A ball falling from `height`, with a collider that is not `None`.
fn falling_ball(entity: Entity, height: f32) -> BodyState {
    BodyState {
        entity,
        translation: [0.0, height, 0.0],
        rotation: [0.0, 0.0, 0.0],
        velocity: Velocity::default(),
        body: RigidBody::dynamic(1.0),
        collider: Some(amadeo_physics::Collider::sphere(0.5)),
    }
}

/// Drops a ball for `ticks` and returns the height it ends at.
///
/// The body is handed back in full every tick, because that is what makes `step` a pure function —
/// the backend is never the only record of where anything is.
fn drop_for(backend: &mut dyn PhysicsBackend, ticks: usize, from: f32) -> f32 {
    let entity = an_entity();
    let mut body = falling_ball(entity, from);

    for _ in 0..ticks {
        let results = backend
            .step(std::slice::from_ref(&body), [0.0, -9.81, 0.0])
            .expect("a ball and a floor is a simulable world");
        let result = results.first().expect("one body in, one body out");
        body.translation = result.translation;
        body.rotation = result.rotation;
        body.velocity = result.velocity;
    }

    body.translation[1]
}

#[test]
fn an_empty_mesh_is_refused_by_both_backends_the_same_way() {
    // **Most chunks of a real world are empty** -- entirely air or entirely rock, both of which mesh
    // into nothing. So this is the common case rather than an edge case, and the two backends have
    // to agree about it: a null backend that accepted more than the real one would hide a missing
    // filter until somebody turned rapier on.
    let empty = StaticMesh {
        id: StaticMeshId(1),
        translation: [0.0, 0.0, 0.0],
        vertices: Vec::new(),
        indices: Vec::new(),
        friction: 0.5,
    };
    assert!(empty.is_empty());

    let error = NullPhysics::new()
        .insert_static_mesh(empty.clone())
        .expect_err("an empty mesh has no triangles to collide with");
    match error {
        PhysicsError::BadGeometry { id, .. } => assert_eq!(id, StaticMeshId(1)),
        other => panic!("expected BadGeometry, got {other:?}"),
    }

    #[cfg(feature = "rapier")]
    {
        let error = amadeo_physics::RapierPhysics::new()
            .insert_static_mesh(empty)
            .expect_err("rapier must refuse it too, and for the same reason");
        assert!(matches!(error, PhysicsError::BadGeometry { .. }));
    }
}

#[test]
fn the_null_backend_records_geometry_and_collides_with_none_of_it() {
    // **The control case, and it is the important half of this file.** Without it, a ball resting at
    // 0.5 against rapier could just as well be a ball that never moved.
    let mut backend = NullPhysics::new();
    backend
        .insert_static_mesh(ground(7))
        .expect("a two-triangle floor is valid geometry");
    assert_eq!(backend.static_mesh_count(), 1, "it is recorded");

    let height = drop_for(&mut backend, 120, 5.0);
    assert!(
        height < -1.0,
        "with no solver the ball must fall straight through the floor, ended at {height}"
    );
}

#[test]
fn inserting_the_same_id_replaces_rather_than_accumulates() {
    // Digging into a chunk re-meshes it under the same id. Accumulating would leave the old surface
    // behind, so the tunnel you just dug would still be solid.
    let mut backend = NullPhysics::new();
    for _ in 0..5 {
        backend
            .insert_static_mesh(ground(3))
            .expect("valid geometry");
    }
    assert_eq!(backend.static_mesh_count(), 1);
}

#[test]
fn removing_geometry_that_was_never_there_is_not_an_error() {
    // An empty chunk never got a collider, and the streamer does not track which ones it skipped.
    let mut backend = NullPhysics::new();
    backend.remove_static_mesh(StaticMeshId(999));
    assert_eq!(backend.static_mesh_count(), 0);
}

#[test]
fn a_reset_drops_static_geometry() {
    // ADR 0028's rule: restoring a snapshot must not leave the old world's ground standing in the
    // new one. Terrain is derived, so throwing it away loses nothing.
    let mut backend = NullPhysics::new();
    backend.insert_static_mesh(ground(1)).expect("valid");
    backend.insert_static_mesh(ground(2)).expect("valid");
    assert_eq!(backend.static_mesh_count(), 2);

    backend.reset();
    assert_eq!(backend.static_mesh_count(), 0);
}

#[cfg(feature = "rapier")]
mod with_a_real_solver {
    use super::*;
    use amadeo_physics::RapierPhysics;

    #[test]
    fn a_ball_lands_on_a_triangle_mesh_and_rests_there() {
        // **The claim this whole path exists to make.** A trimesh with no rigid body behind it is
        // solid ground, which is what a streamed terrain chunk is.
        let mut backend = RapierPhysics::new();
        backend
            .insert_static_mesh(ground(1))
            .expect("a two-triangle floor is valid geometry");

        let height = drop_for(&mut backend, 240, 5.0);
        assert!(
            (height - 0.5).abs() < 0.1,
            "a ball of radius 0.5 should rest at 0.5 on a floor at y = 0, ended at {height}"
        );
    }

    #[test]
    fn removing_the_ground_lets_the_ball_through() {
        // Streaming a chunk out is exactly this, and it is the operation most likely to be got
        // wrong quietly -- a collider that is removed from the map but not from the solver leaves
        // invisible ground behind.
        let mut backend = RapierPhysics::new();
        backend.insert_static_mesh(ground(1)).expect("valid");
        assert_eq!(backend.static_mesh_count(), 1);

        backend.remove_static_mesh(StaticMeshId(1));
        assert_eq!(backend.static_mesh_count(), 0);

        let height = drop_for(&mut backend, 120, 5.0);
        assert!(
            height < -1.0,
            "with the ground removed the ball must fall, ended at {height}"
        );
    }

    #[test]
    fn the_same_geometry_produces_the_same_result_every_time() {
        // I3. A terrain collider is gameplay state (ADR 0041 §2), so two runs of the same drop onto
        // the same chunk must agree bit for bit rather than approximately.
        let mut first = RapierPhysics::new();
        first.insert_static_mesh(ground(1)).expect("valid");
        let mut second = RapierPhysics::new();
        second.insert_static_mesh(ground(1)).expect("valid");

        assert_eq!(
            drop_for(&mut first, 200, 5.0).to_bits(),
            drop_for(&mut second, 200, 5.0).to_bits(),
            "the same fall onto the same geometry must reproduce exactly"
        );
    }

    #[test]
    fn replacing_the_ground_with_a_lower_one_lets_the_ball_settle_lower() {
        // Editing terrain: same id, different surface. If replacement left the old trimesh in the
        // solver the ball would rest on ground that is no longer there.
        let mut backend = RapierPhysics::new();
        backend.insert_static_mesh(ground(1)).expect("valid");

        let mut lowered = ground(1);
        lowered.translation = [0.0, -3.0, 0.0];
        backend
            .insert_static_mesh(lowered)
            .expect("replacing is valid");
        assert_eq!(backend.static_mesh_count(), 1, "replaced, not accumulated");

        let height = drop_for(&mut backend, 240, 5.0);
        assert!(
            (height - -2.5).abs() < 0.1,
            "the ball should rest on the lowered floor at -2.5, ended at {height}"
        );
    }
}
