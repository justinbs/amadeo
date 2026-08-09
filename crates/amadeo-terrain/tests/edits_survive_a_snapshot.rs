//! **Q29**: a dug world saves and reloads dug.
//!
//! Until session 13 the edits lived in the streamer, which is a `Service` — outside the state hash
//! and untouched by a snapshot (ADR 0009). So digging a tunnel, saving, and loading gave the tunnel
//! back filled in, which is ADR 0042's central promise — *a save file is a seed plus a diff* —
//! quietly unkept.
//!
//! ADR 0046 moved the authored edits into a hashed [`TerrainEdits`] resource and made the streamer a
//! cache of it. These are the claims that makes.
//!
//! # The save/reload round trip is tested in `games/scarp`, not here
//!
//! `amadeo-snapshot` sits **above** this crate — it is built on `amadeo-scene`, which is built on
//! `amadeo-render`. Reaching for it from a dev-dependency here would invert the crate graph that
//! invariant I6 keeps a strict DAG, even though cargo would allow it.
//!
//! So this file tests the *mechanism* — that the resource is the source of truth, that it is hashed,
//! and that the streamer follows it in both directions — and `games/scarp` tests the *promise*, that
//! a dug world saves and reloads dug. A game is the right place for that anyway: it is the layer
//! where every other defect this session found actually surfaced.

#![cfg(feature = "engine")]

use amadeo_ecs::World;
use amadeo_physics::{NullPhysics, Physics};
use amadeo_render::MeshCache;
use amadeo_terrain::{
    Terrain, TerrainEdits, TerrainSettings, TerrainViewer, chunk_mesh_id, stream_terrain,
};
use amadeo_transform::Transform;
use amadeo_voxel::{ChunkKey, ChunkShape, FlatGround};
use std::sync::Arc;

fn world_with_terrain() -> World {
    let mut world = World::new();
    world.insert_service(Terrain::new(
        Arc::new(FlatGround { height: 4.0 }),
        TerrainSettings {
            shape: ChunkShape::new(8, 1.0),
            friction: 0.6,
        },
        2,
    ));
    world.insert_service(MeshCache::new());
    world.insert_service(Physics::new(Box::new(NullPhysics::new())));
    world.insert_resource(TerrainEdits::default());

    let viewer = world.spawn();
    world.insert(viewer, Transform::default());
    world.insert(
        viewer,
        TerrainViewer {
            visual_radius: 1,
            collision_radius: 1,
        },
    );
    world
}

/// The geometry of the chunk under the viewer.
fn ground(world: &World) -> Option<amadeo_render::MeshData> {
    world
        .service::<MeshCache>()
        .expect("a mesh cache")
        .get(&chunk_mesh_id(ChunkKey::new(0, 0, 0)))
        .cloned()
}

/// Digs a hole through the surface, the way a game does — by writing the **resource**.
fn dig(world: &mut World) {
    let edits = world
        .resource_mut::<TerrainEdits>()
        .expect("inserted above");
    for y in 3..6 {
        edits.set([4, y, 4], 4.0);
    }
}

#[test]
fn digging_through_the_resource_reaches_the_ground() {
    // Gameplay writes the authored edits and never touches the streamer. `stream_terrain` carries
    // them across, which is what makes the resource the source of truth rather than a copy of one.
    let mut world = world_with_terrain();
    stream_terrain(&mut world);
    let before = ground(&world).expect("the chunk under the viewer is solid");

    dig(&mut world);
    stream_terrain(&mut world);

    let after = ground(&world).expect("still solid after digging");
    assert_ne!(
        before, after,
        "writing TerrainEdits did not reach the meshed ground"
    );
}

#[test]
fn edits_are_in_the_state_hash() {
    // **The half a Service could never provide.** A replay that reproduces the world must reproduce
    // the holes in it, and a hash that ignores digging says two visibly different worlds are the
    // same.
    let mut world = world_with_terrain();
    stream_terrain(&mut world);
    let before = world.state_hash();

    dig(&mut world);
    assert_ne!(
        world.state_hash(),
        before,
        "digging left the state hash unchanged, so a replay would not reproduce it"
    );
}

#[test]
fn a_world_whose_streamer_knows_nothing_catches_up() {
    // **What a snapshot restore looks like from this crate's side.** A restore replaces the resource
    // and cannot touch the streamer, which is a service (ADR 0009) — so the streamer comes back
    // knowing nothing while the resource holds every edit. Simulated exactly by building a fresh
    // world and handing it an already-dug resource.
    let mut dug_world = world_with_terrain();
    stream_terrain(&mut dug_world);
    dig(&mut dug_world);
    stream_terrain(&mut dug_world);
    let dug = ground(&dug_world).expect("solid after digging");

    let authored = dug_world
        .resource::<TerrainEdits>()
        .expect("inserted")
        .clone();

    let mut fresh = world_with_terrain();
    fresh.insert_resource(authored);
    stream_terrain(&mut fresh);

    assert_eq!(
        ground(&fresh).expect("solid after catching up"),
        dug,
        "a streamer handed an already-dug edit set did not reproduce the hole"
    );
}

#[test]
fn going_back_to_fewer_edits_fills_the_hole_back_in() {
    // **The direction a diff is easy to get wrong**, and the reason `replace_edits` looks at what
    // the streamer holds as well as at what it is given. Restoring an *earlier* save means the
    // authored set has **fewer** edits than the streamer does, and a sync that only walked the new
    // set would leave the extra digging in place forever — a world that cannot be undone.
    let mut world = world_with_terrain();
    stream_terrain(&mut world);
    let undug = ground(&world).expect("solid to begin with");

    dig(&mut world);
    stream_terrain(&mut world);
    assert_ne!(
        ground(&world).as_ref(),
        Some(&undug),
        "the dig has to change something to be a test"
    );

    // What restoring a pre-dig save does: the resource goes back to empty, with a revision that has
    // still moved on.
    let edits = world
        .resource_mut::<TerrainEdits>()
        .expect("inserted above");
    *edits = TerrainEdits {
        revision: edits.revision + 1,
        samples: Vec::new(),
    };
    stream_terrain(&mut world);

    assert_eq!(
        ground(&world).expect("solid again"),
        undug,
        "going back to an empty edit set left the digging in the world"
    );
}

#[test]
fn a_tick_where_nobody_dug_costs_nothing() {
    // The sync compares one integer and stops. Worth a test because the obvious implementation --
    // re-apply every edit every tick -- would mark every edited chunk dirty forever, and a world
    // with a thousand holes would re-mesh all of them sixty times a second.
    let mut world = world_with_terrain();
    stream_terrain(&mut world);
    dig(&mut world);
    stream_terrain(&mut world);

    // Let the re-meshing settle, then confirm a quiet tick produces no new work.
    for _ in 0..4 {
        stream_terrain(&mut world);
    }
    let terrain = world.service::<Terrain>().expect("installed");
    assert_eq!(
        terrain.streamer.in_flight(),
        0,
        "a tick where nobody dug is still queueing meshing work"
    );
}
