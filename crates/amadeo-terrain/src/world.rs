//! Turning a [`TerrainUpdate`](crate::TerrainUpdate) into entities, geometry and colliders.
//!
//! Only compiled with the `engine` feature. The streamer in [`crate`] needs no engine at all, and
//! keeping that true is what lets ADR 0041's claim be tested with no `World` in the build.
//!
//! # The ordering that is load-bearing
//!
//! [`stream_terrain`] is registered **before** `step_physics`, because a chunk's collider has to be
//! in the solver on the tick the character walks onto it. The other way round, the character spends
//! one tick standing on ground that does not exist yet — which is a fall through the world at the
//! exact moment a new chunk streams in, and it looks like a physics bug rather than an ordering one.
//!
//! [`install`] sets that up so a game does not have to remember, exactly as
//! `amadeo_character::install` does for its own `.after(STEP_PHYSICS)`.
//!
//! # Where each part of an update goes
//!
//! | Field | Effect |
//! |---|---|
//! | `visible_added` | **spawn** a chunk entity — deterministic, so the state hash is too |
//! | `meshes` | fill the [`MeshCache`] — the entity already exists and starts drawing |
//! | `colliders` | `PhysicsBackend::insert_static_mesh`, and fill the cache too |
//! | `colliders_removed` | `remove_static_mesh`, geometry kept |
//! | `removed` | despawn, drop the cache entry, remove the collider |
//!
//! **`colliders` fills the mesh cache as well**, which is easy to miss: a chunk meshed inline for
//! collision is recorded as already known, so the job pool never meshes it and it never appears in
//! `meshes`. Without this the ground you are standing on is the one piece of terrain that is
//! invisible.

use crate::{ReadyChunk, TerrainSettings, TerrainStreamer, chunk_mesh_id};
use amadeo_app::{App, Stage, system};
use amadeo_core::StableHash;
use amadeo_ecs::{Component, Entity, Service, World};
use amadeo_physics::{Physics, STEP_PHYSICS, StaticMesh, StaticMeshId, step_physics};
use amadeo_reflect::{Reflect, RegistryError};
use amadeo_render::{Mesh, MeshCache, MeshData, Vertex};
use amadeo_transform::Transform;
use amadeo_voxel::{ChunkKey, TerrainSource, Viewer};
use std::sync::Arc;

/// The label [`stream_terrain`] is registered under.
pub const STREAM_TERRAIN: &str = "stream_terrain";

/// How many world units one repeat of a terrain texture covers.
///
/// Terrain UVs are projected straight from world coordinates, so without this a texture would tile
/// once per **metre** — which at any distance is finer than a pixel and, with no mipmaps in the
/// backend yet, shimmers badly. Eight metres is coarse enough to be stable and fine enough to read
/// as ground.
///
/// A constant rather than a field on `Material`, deliberately: a tile size is one number and adding
/// it to the material schema would change every `.material` file in the repository to express
/// something only terrain currently varies. The moment a second surface wants its own, that is the
/// change to make.
const TEXTURE_TILE: f32 = 8.0;

/// An entity that terrain is loaded around — a player, a spectator, a server's area of interest.
///
/// Position comes from the entity's [`Transform`], per ADR 0018's one-transform rule, exactly as a
/// camera's and a light's do.
#[derive(Debug, Clone, Copy, PartialEq, StableHash, Reflect)]
pub struct TerrainViewer {
    /// How many chunks out, in every direction, are **drawn**.
    #[reflect(min = 0.0, max = 64.0, unit = "chunks")]
    pub visual_radius: i32,
    /// How many chunks out are **solid**.
    ///
    /// Normally much smaller than `visual_radius`: every chunk with a collider is one the simulation
    /// may have to block on (ADR 0041 §2), and collision only has to exist where something can
    /// actually touch it.
    #[reflect(min = 0.0, max = 64.0, unit = "chunks")]
    pub collision_radius: i32,
}

impl Default for TerrainViewer {
    fn default() -> Self {
        Self {
            visual_radius: 4,
            collision_radius: 2,
        }
    }
}

impl Component for TerrainViewer {}

/// Marks an entity as one streamed chunk, and says which.
///
/// # Why the key lives on the entity rather than in a map on the service
///
/// A service is not hashed and is not restored by a snapshot (ADR 0009). A `BTreeMap<ChunkKey,
/// Entity>` kept there would be lost on a restore while the entities it named survived, leaving
/// chunks nothing could ever despawn — which is ADR 0028's lesson exactly: hash equality after a
/// restore is necessary and not sufficient.
///
/// Kept on the entity, the mapping is part of the world and comes back with it.
#[derive(Debug, Clone, Copy, PartialEq, Default, StableHash, Reflect)]
pub struct TerrainChunk {
    /// Detail level.
    pub lod: u8,
    /// Chunk coordinate along x.
    pub x: i32,
    /// Chunk coordinate along y.
    pub y: i32,
    /// Chunk coordinate along z.
    pub z: i32,
}

impl Component for TerrainChunk {}

impl TerrainChunk {
    /// The key this component names.
    #[must_use]
    pub fn key(&self) -> ChunkKey {
        ChunkKey::at_lod(self.lod, self.x, self.y, self.z)
    }

    /// The component for a key.
    #[must_use]
    pub fn of(key: ChunkKey) -> Self {
        Self {
            lod: key.lod,
            x: key.x,
            y: key.y,
            z: key.z,
        }
    }
}

/// The streamer, as a service.
///
/// A **service** rather than a resource, and that is ADR 0009 doing real work: everything in here is
/// derived from the seed, the edits and where the viewers are, so none of it belongs in the state
/// hash. It also holds a job pool, and a thread pool inside hashed state would be nonsense.
#[derive(Debug)]
pub struct Terrain {
    /// The streamer itself.
    pub streamer: TerrainStreamer,
    /// The declared asset id of the material every terrain chunk is drawn with.
    ///
    /// An **id** rather than a [`Material`](amadeo_render::Material), matching how every other mesh
    /// in the engine names one (ADR 0033): the material itself is an asset file a game authors and
    /// `amadeo check` validates, and inlining a copy here would be a second place for it to live.
    /// Empty means a plain white surface — which is what an unconfigured terrain draws as.
    pub material: String,
}

impl Service for Terrain {}

impl Terrain {
    /// A terrain service over a generated world, drawn with the default material.
    #[must_use]
    pub fn new(source: Arc<dyn TerrainSource>, settings: TerrainSettings, workers: usize) -> Self {
        Self {
            streamer: TerrainStreamer::new(source, settings, workers),
            material: String::new(),
        }
    }

    /// The same, drawn with a named material asset.
    #[must_use]
    pub fn with_material(mut self, id: impl Into<String>) -> Self {
        self.material = id.into();
        self
    }
}

/// Registers the terrain components and the streaming system.
///
/// # Call this before loading a scene
///
/// It is what registers [`TerrainViewer`] and [`TerrainChunk`], and a scene file naming a component
/// the registry has not been told about refuses to load. Same ordering `amadeo_character::install`
/// needs, and for the same reason.
///
/// # Errors
///
/// [`RegistryError`] if a game has already registered a different type under one of these names.
pub fn install(app: &mut App, terrain: Terrain) -> Result<(), RegistryError> {
    app.register_component::<TerrainViewer>()?;
    app.register_component::<TerrainChunk>()?;

    // **Loaded here, because nothing will ever name it in time.** `App::load_materials` scans the
    // `Mesh` components that exist when a scene finishes loading, and a terrain chunk is spawned at
    // runtime -- so the material would never be read and every chunk would draw plain white over an
    // otherwise correct world. Found by looking at the first capture of `games/scarp`.
    app.load_material(&terrain.material);

    app.insert_service(terrain);
    if !app.world.has_service::<MeshCache>() {
        app.insert_service(MeshCache::new());
    }

    // Only if nobody else has -- `amadeo_character::install` needs the same system, and an open
    // world is precisely the case that calls both. See that function for the failure this avoids.
    if !app.has_system(Stage::Simulation, STEP_PHYSICS) {
        app.add_system(Stage::Simulation, system(STEP_PHYSICS, step_physics));
    }
    // **Before** physics, not after: a collider has to be in the solver on the tick a character
    // walks onto it. See the module docs.
    app.add_system(
        Stage::Simulation,
        system(STREAM_TERRAIN, stream_terrain).before(STEP_PHYSICS),
    );
    Ok(())
}

/// Advances terrain streaming by one tick and applies the result to the world.
///
/// Does nothing if there is no [`Terrain`] service, so a world without terrain is untouched rather
/// than half-driven — the same posture `drive_characters` takes.
pub fn stream_terrain(world: &mut World) {
    if !world.has_service::<Terrain>() {
        return;
    }

    // --- Where the viewers are. ---
    //
    // `Residency::of` is asserted order-independent, so the order this query happens to yield
    // cannot reach the answer -- which is why there is no sort here.
    let Some(terrain) = world.service::<Terrain>() else {
        return;
    };
    let shape = terrain.streamer.settings().shape;
    let viewers: Vec<Viewer> = world
        .query::<(&TerrainViewer, &Transform)>()
        .map(|(_, (viewer, transform))| Viewer {
            centre: ChunkKey::containing(transform.translation, shape.chunk_size_at(0)),
            visual_radius: viewer.visual_radius,
            collision_radius: viewer.collision_radius,
        })
        .collect();

    // Nothing to load terrain around. Leaving what is loaded alone rather than unloading the world
    // is the kinder failure: a game that forgot the component sees terrain that never grows, not
    // terrain that vanishes.
    if viewers.is_empty() {
        return;
    }

    let (update, material) = {
        let Some(terrain) = world.service_mut::<Terrain>() else {
            return;
        };
        (terrain.streamer.update(&viewers), terrain.material.clone())
    };

    // --- Geometry into the cache. ---
    //
    // Colliders as well as meshes: a chunk meshed inline for collision never reaches the pool, so it
    // never appears in `meshes`, and without this the ground underfoot is the one invisible thing.
    if let Some(cache) = world.service_mut::<MeshCache>() {
        for chunk in update.meshes.iter().chain(&update.colliders) {
            cache.insert(chunk_mesh_id(chunk.key), mesh_data_of(chunk));
        }
        // **Dropped as well as added**, which this system's own documentation claimed and its code
        // did not do. Without it, geometry accumulates for every chunk ever visited: walk in one
        // direction long enough and the cache holds the whole world, one chunk at a time, with
        // nothing referring to any of it. The renderer frees the matching video memory by noticing
        // the id has gone from here.
        for key in &update.removed {
            cache.remove(&chunk_mesh_id(*key));
        }
    }

    // --- Colliders into the solver. ---
    if world.has_service::<Physics>() {
        let friction = update_friction(world);
        if let Some(physics) = world.service_mut::<Physics>() {
            for chunk in &update.colliders {
                let mesh = static_mesh_of(chunk, friction);
                // An empty chunk never reaches here -- the streamer drops those -- but a degenerate
                // one could, and a failed insert must not take the game down mid-tick. Terrain that
                // is not solid is survivable; a panic is not.
                let _ = physics.insert_static_mesh(mesh);
            }
            for key in update.colliders_removed.iter().chain(&update.removed) {
                physics.remove_static_mesh(collider_id(*key));
            }
        }
    }

    // --- Entities. ---
    //
    // Spawned from `visible_added`, which is a residency diff, so the entity allocator -- and
    // therefore the state hash -- cannot follow machine speed. See `TerrainUpdate::visible_added`.
    for key in &update.visible_added {
        let entity = world.spawn();
        world.insert(
            entity,
            Transform {
                translation: shape.origin_of(*key),
                ..Transform::default()
            },
        );
        world.insert(entity, TerrainChunk::of(*key));
        // The geometry may not exist yet. `MeshCache::get` returning `None` draws nothing and says
        // which id, which is exactly the right behaviour for a chunk still being meshed.
        world.insert(entity, Mesh::new(chunk_mesh_id(*key), material.clone()));
    }

    if !update.removed.is_empty() {
        despawn_chunks(world, &update.removed);
    }
}

/// The friction terrain colliders are given.
fn update_friction(world: &World) -> f32 {
    world
        .service::<Terrain>()
        .map_or(0.5, |terrain| terrain.streamer.settings().friction)
}

/// The id a chunk's collider is held under.
///
/// A hash of the key rather than the key packed into bits: `StaticMeshId` is one `u64` and four
/// fields do not fit in it without a range assumption that would silently wrap on a large world.
fn collider_id(key: ChunkKey) -> StaticMeshId {
    StaticMeshId(amadeo_core::stable_hash_of(&TerrainChunk::of(key)))
}

/// Converts a chunk's geometry into what the renderer holds.
///
/// UVs are a flat planar projection from x and z. Terrain has no authored texture coordinates —
/// there is no artist to make them — and a planar mapping is what every voxel terrain starts with.
/// It stretches on vertical faces, which is a known limitation of the approach rather than a defect
/// here; triplanar mapping is the usual fix and belongs with the material work.
///
/// **Projected from the chunk's *world* position, not its local one.** Vertices are stored relative
/// to the chunk origin, so using them directly would restart the pattern at every chunk boundary and
/// print a grid of seams across the landscape the moment a material carried a texture — visible
/// nowhere today, since materials are colours only, and tedious to attribute once they are not.
fn mesh_data_of(chunk: &ReadyChunk) -> MeshData {
    MeshData {
        vertices: chunk
            .mesh
            .positions
            .iter()
            .zip(&chunk.mesh.normals)
            .map(|(position, normal)| Vertex {
                position: *position,
                normal: *normal,
                uv: [
                    (position[0] + chunk.origin[0]) / TEXTURE_TILE,
                    (position[2] + chunk.origin[2]) / TEXTURE_TILE,
                ],
            })
            .collect(),
        indices: chunk.mesh.indices.clone(),
    }
}

/// Converts a chunk's geometry into what the solver holds.
fn static_mesh_of(chunk: &ReadyChunk, friction: f32) -> StaticMesh {
    StaticMesh {
        id: collider_id(chunk.key),
        translation: chunk.origin,
        vertices: chunk.mesh.positions.clone(),
        indices: chunk
            .mesh
            .indices
            .chunks_exact(3)
            .map(|triangle| [triangle[0], triangle[1], triangle[2]])
            .collect(),
        friction,
    }
}

/// Despawns the entities for a set of chunks.
///
/// Collected before despawning rather than despawned during the query, because mutating the world
/// while iterating it is what `Commands` exists to avoid.
fn despawn_chunks(world: &mut World, removed: &[ChunkKey]) {
    let doomed: Vec<Entity> = world
        .query::<(&TerrainChunk,)>()
        .filter(|(_, (chunk,))| removed.contains(&chunk.key()))
        .map(|(entity, _)| entity)
        .collect();

    for entity in doomed {
        world.despawn(entity);
    }
}
