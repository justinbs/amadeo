//! Streaming reaches entities, geometry and the solver — the layer above `TerrainStreamer`.
//!
//! The streamer's own tests cover *what it decides*. This covers *what happens to a world when it
//! decides it*, which is where the two halves of ADR 0041 §2 turn into different engine subsystems.

#![cfg(feature = "engine")]

use amadeo_ecs::World;
use amadeo_physics::{NullPhysics, Physics};
use amadeo_render::MeshCache;
use amadeo_terrain::{
    Terrain, TerrainChunk, TerrainSettings, TerrainViewer, chunk_mesh_id, stream_terrain,
};
use amadeo_transform::Transform;
use amadeo_voxel::{ChunkShape, FlatGround};
use std::sync::Arc;

fn settings() -> TerrainSettings {
    TerrainSettings {
        shape: ChunkShape::new(8, 1.0),
        friction: 0.6,
    }
}

/// A world with terrain, a null solver, and one viewer standing at the origin.
fn world_with_terrain(workers: usize, visual: i32, collision: i32) -> World {
    let mut world = World::new();
    world.insert_service(Terrain::new(
        Arc::new(FlatGround { height: 4.0 }),
        settings(),
        workers,
    ));
    world.insert_service(MeshCache::new());
    world.insert_service(Physics::new(Box::new(NullPhysics::new())));

    let viewer = world.spawn();
    world.insert(viewer, Transform::default());
    world.insert(
        viewer,
        TerrainViewer {
            visual_radius: visual,
            collision_radius: collision,
        },
    );
    world
}

fn chunk_count(world: &World) -> usize {
    world.query::<(&TerrainChunk,)>().count()
}

#[test]
fn a_world_with_no_terrain_service_is_untouched() {
    // The same posture `drive_characters` takes: a world without the subsystem is left alone rather
    // than half-driven.
    let mut world = World::new();
    let before = world.state_hash();
    stream_terrain(&mut world);
    assert_eq!(world.state_hash(), before);
}

#[test]
fn a_viewer_brings_chunk_entities_into_existence() {
    let mut world = world_with_terrain(4, 2, 1);
    stream_terrain(&mut world);

    // A 5x5x5 visual box around the origin.
    assert_eq!(
        chunk_count(&world),
        125,
        "every chunk in the visual box gets an entity, whether or not its geometry has arrived"
    );
}

#[test]
fn an_entity_exists_before_its_geometry_does() {
    // **The point of spawning from residency rather than from mesh arrival.** An entity whose mesh
    // has not landed draws nothing, which is correct and invisible -- and it means the entity
    // allocator, and therefore the state hash, cannot follow machine speed (ADR 0028).
    let mut world = world_with_terrain(1, 3, 0);
    stream_terrain(&mut world);

    let entities = chunk_count(&world);
    let cached = world
        .service::<MeshCache>()
        .expect("inserted above")
        .ids()
        .count();

    assert!(entities > 0);
    assert!(
        cached < entities,
        "with one worker, most chunks should still be meshing: {cached} cached of {entities}"
    );
}

#[test]
fn the_ground_underfoot_is_cached_even_though_the_pool_never_meshed_it() {
    // The easy thing to miss. A collision chunk is meshed inline and recorded as known, so the job
    // pool never touches it and it never appears in `meshes`. Without the collider path also
    // filling the cache, the one piece of terrain you are standing on is the one you cannot see.
    let mut world = world_with_terrain(4, 3, 0);
    stream_terrain(&mut world);

    let cache = world.service::<MeshCache>().expect("inserted above");
    assert!(
        cache
            .get(&chunk_mesh_id(amadeo_voxel::ChunkKey::new(0, 0, 0)))
            .is_some(),
        "the chunk the viewer stands in must have geometry on the tick it is asked for"
    );
}

#[test]
fn colliders_reach_the_solver() {
    let mut world = world_with_terrain(4, 2, 1);
    stream_terrain(&mut world);

    let solid = world
        .service::<Physics>()
        .expect("inserted above")
        .static_mesh_count();
    assert!(solid > 0, "collision chunks must be handed to the backend");
}

#[test]
fn walking_away_drops_the_collider_and_the_entity() {
    let mut world = world_with_terrain(4, 1, 1);
    stream_terrain(&mut world);
    let before = chunk_count(&world);
    assert!(before > 0);

    // Teleport far enough that none of the original region is required.
    let viewer = world
        .query::<(&TerrainViewer,)>()
        .map(|(entity, _)| entity)
        .next()
        .expect("one viewer");
    world.insert(
        viewer,
        Transform {
            translation: [4000.0, 0.0, 0.0],
            ..Transform::default()
        },
    );

    stream_terrain(&mut world);

    assert_eq!(
        chunk_count(&world),
        before,
        "the old chunks should be gone and a new region of the same size loaded"
    );
    for (_, (chunk,)) in world.query::<(&TerrainChunk,)>() {
        assert!(
            chunk.x > 400,
            "{chunk:?} is from the abandoned region and should have been despawned"
        );
    }
}

#[test]
fn the_world_is_the_same_however_many_threads_meshed_it() {
    // **M2.5's exit gate 2, at the layer where it actually matters.** The streamer's own test covers
    // its outputs; this covers the world those outputs produce -- entity count, chunk identity, and
    // what the solver holds. A mesh arriving a tick later is allowed; a different world is not.
    let mut worlds: Vec<World> = [1_usize, 2, 3, 5, 8]
        .iter()
        .map(|workers| world_with_terrain(*workers, 2, 1))
        .collect();

    for _ in 0..4 {
        for world in &mut worlds {
            stream_terrain(world);
        }
    }

    let reference: Vec<TerrainChunk> = worlds[0]
        .query::<(&TerrainChunk,)>()
        .map(|(_, (chunk,))| *chunk)
        .collect();
    let reference_solid = worlds[0]
        .service::<Physics>()
        .expect("inserted above")
        .static_mesh_count();

    for (index, world) in worlds.iter().enumerate().skip(1) {
        let chunks: Vec<TerrainChunk> = world
            .query::<(&TerrainChunk,)>()
            .map(|(_, (chunk,))| *chunk)
            .collect();
        assert_eq!(chunks, reference, "world {index} has different chunks");
        assert_eq!(
            world
                .service::<Physics>()
                .expect("inserted above")
                .static_mesh_count(),
            reference_solid,
            "world {index} has a different amount of solid ground"
        );
    }
}

#[test]
fn standing_still_does_not_keep_spawning_entities() {
    // A leak that would be invisible for a few seconds and then not: re-spawning the same chunk
    // every tick grows the world without bound and moves the state hash every frame.
    let mut world = world_with_terrain(4, 2, 1);
    stream_terrain(&mut world);
    let after_one = chunk_count(&world);

    for _ in 0..10 {
        stream_terrain(&mut world);
    }
    assert_eq!(chunk_count(&world), after_one);
}
