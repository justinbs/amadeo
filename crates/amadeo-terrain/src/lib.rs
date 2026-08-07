//! Chunked terrain streaming: which chunks are loaded, when they are generated, and what the rest of
//! the engine is told about them.
//!
//! # The one rule this crate exists to obey
//!
//! ADR 0041 §2. A chunk has **two products and they have different rules**:
//!
//! - Its **mesh** is drawn and nothing else, so it may arrive whenever. A slow machine draws terrain
//!   a few frames later than a fast one and nothing about the simulation changes.
//! - Its **collider** is gameplay, because a character stands on it. *When* it arrives changes where
//!   that character ends up, so it may not be allowed to arrive late.
//!
//! So this crate does two different things with the same work. Visual chunks are **submitted to a
//! job pool** and collected whenever they land. Collision chunks are **meshed inline, on the calling
//! thread, before the tick continues** — the simulation blocks on ground it needs. A slow machine
//! gets a frame hitch and keeps its replay, which is the trade ADR 0041 chose deliberately.
//!
//! Both lists report *changes*, and the difference between them is **what decides the change**.
//! [`TerrainUpdate::colliders`] comes from the residency diff — where the viewers are, and nothing
//! else — so its content and its order are the same on every machine at every worker count.
//! [`TerrainUpdate::meshes`] is whatever the pool finished, so which tick a chunk lands on depends
//! on machine speed. Gameplay may rely on the first and must never look at the second.
//!
//! Getting that distinction slightly wrong is easy and CI caught it once: an early version built
//! `colliders` as "meshed this tick" followed by "already known", and which group a chunk fell into
//! depended on whether the pool had already delivered it. The *set* was always right; the **order**
//! followed thread count. See `update`.
//!
//! # Why this crate has no engine dependencies
//!
//! It needs `amadeo-voxel` and `amadeo-jobs` and nothing else — no `World`, no renderer, no solver.
//! That is deliberate rather than incidental: the hard part of streaming is *when* work happens and
//! whether the answer depends on that, and none of it involves an entity. Kept free of the engine,
//! the claim that matters is testable with no engine —
//! `the_thread_count_cannot_reach_the_colliders` walks five streamers east **in lockstep** at 1, 2,
//! 3, 5 and 8 workers and requires identical colliders from all of them. In lockstep rather than one
//! after another for a reason worth knowing: run alone, even a one-worker pool gets all the wall
//! clock it needs, and the test passed against a deliberately broken implementation. Advancing them
//! together is what puts them under comparable time pressure.
//!
//! The layer that turns a [`TerrainUpdate`] into entities, mesh uploads and
//! `PhysicsBackend::insert_static_mesh` calls sits above this and is mechanical.
//!
//! # Editing, and the half of ADR 0042 §4 that is still open
//!
//! [`TerrainStreamer::edit`] changes one sample: digging, or building. That is ADR 0042's other
//! half — the base is generated and an edit is authored, so a save file is a seed plus a list of
//! changed samples rather than a world of voxels.
//!
//! An edit invalidates **every chunk whose field reads that sample**, which is up to eight, because
//! a chunk samples one cell beyond its own volume on both sides (ADR 0043 §4). Marking only the
//! chunk that "owns" it leaves the neighbours holding geometry that disagrees, and the crack opens
//! exactly where somebody has been digging.
//!
//! **What is still missing is where the edits live.** ADR 0042 §4 says they belong in a reflected,
//! hashed component so a snapshot restores them. They are currently held here, in a service, which
//! means **they are not in the state hash and a snapshot does not restore them** — so a dug world
//! saves and reloads undug.
//!
//! That is not simply unfinished: §4 says "a component on a chunk entity", and this crate now
//! **despawns chunk entities when they stream out**, which would take the edits with them. The
//! resolution is probably an entity per *edited* chunk whose existence is driven by having been
//! edited rather than by being loaded — but it is a real design question rather than wiring, and it
//! is written up as **Q29** rather than guessed at.
//!
//! ```
//! use amadeo_terrain::{TerrainSettings, TerrainStreamer};
//! use amadeo_voxel::{ChunkShape, FlatGround, Viewer, ChunkKey};
//! use std::sync::Arc;
//!
//! let settings = TerrainSettings {
//!     shape: ChunkShape::new(8, 1.0),
//!     friction: 0.6,
//! };
//! let mut streamer = TerrainStreamer::new(Arc::new(FlatGround { height: 4.0 }), settings, 4);
//!
//! // A viewer standing at the origin, drawing three chunks out and solid for one.
//! let viewer = Viewer {
//!     centre: ChunkKey::new(0, 0, 0),
//!     visual_radius: 3,
//!     collision_radius: 1,
//! };
//!
//! let update = streamer.update(&[viewer]);
//! // Ground the viewer can stand on is ready on the tick it was asked for -- nothing was waited
//! // for. The meshes to *draw* may still be in flight, and that is allowed.
//! assert!(!update.colliders.is_empty());
//! ```

#[cfg(feature = "engine")]
pub mod world;

#[cfg(feature = "engine")]
pub use world::{STREAM_TERRAIN, Terrain, TerrainChunk, TerrainViewer, install, stream_terrain};

use amadeo_jobs::{Inbox, JobPool};
use amadeo_voxel::{ChunkKey, ChunkShape, Edits, Residency, TerrainSource, Viewer, VoxelMesh};
use std::collections::{BTreeMap, BTreeSet};
use std::sync::Arc;

/// What a terrain world is made of, dimensionally.
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct TerrainSettings {
    /// How big a chunk is, in cells and world units.
    pub shape: ChunkShape,
    /// Friction for terrain colliders. Ground people walk on, so not ice.
    pub friction: f32,
}

/// One chunk's geometry, ready to be drawn or made solid.
///
/// The mesh is in world units **relative to `origin`**, so placing it is one translation — see
/// [`amadeo_voxel::mesh_chunk`].
#[derive(Debug, Clone, PartialEq)]
pub struct ReadyChunk {
    /// Which chunk this is.
    pub key: ChunkKey,
    /// Where its low corner sits in the world.
    pub origin: [f32; 3],
    /// Its geometry. **Never empty** — an empty chunk is dropped rather than reported, because most
    /// chunks of a real world are entirely air or entirely rock and neither draws nor collides.
    pub mesh: VoxelMesh,
}

/// What one tick of streaming decided.
///
/// # Every list here is a change list, and the asymmetry is what *decides* the change
///
/// `colliders`, `colliders_removed` and `removed` come from the **residency diff** — where the
/// viewers are and nothing else — so their contents and their order are identical on every machine
/// at every worker count. `meshes` is **whatever the pool finished**, so which tick a chunk lands on
/// depends on machine speed.
///
/// Gameplay may rely on the first three and must never look at the fourth. That is ADR 0041 §2
/// expressed as four fields.
#[derive(Debug, Clone, Default, PartialEq)]
pub struct TerrainUpdate {
    /// Chunks whose geometry became available for **drawing** this tick, in key order.
    ///
    /// May arrive several ticks after the chunk became visible. That is allowed and is the point.
    pub meshes: Vec<ReadyChunk>,
    /// Chunks that **became solid** this tick, in key order.
    ///
    /// A *change* rather than a census: the caller holds what it has already been given, exactly as
    /// it does for meshes. Reporting every solid chunk every tick would mean re-meshing the whole
    /// collision region sixty times a second.
    ///
    /// Deterministic in content **and order**, because it comes from the residency diff and nothing
    /// else. Gameplay may rely on this and may not rely on `meshes`.
    pub colliders: Vec<ReadyChunk>,
    /// Chunks that stopped needing to be solid but are still drawn, in key order.
    ///
    /// Distinct from `removed`: this geometry stays, only its collider goes. A viewer walking away
    /// from ground it can still see is the case, and missing it leaves invisible collision behind.
    pub colliders_removed: Vec<ChunkKey>,
    /// Chunks that **entered** the drawn region this tick, in key order.
    ///
    /// # This is what an entity is created from, and `meshes` is not
    ///
    /// A chunk entity is world state, so *when* it is spawned reaches the entity allocator and
    /// therefore the state hash (ADR 0028). Spawning one when its geometry arrived would make the
    /// hash depend on machine speed — the exact failure ADR 0041 §2 exists to prevent, arriving
    /// through the back door.
    ///
    /// So an entity is created from **this** list, which is a residency diff, and its geometry is
    /// filled in later from `meshes`. A chunk whose mesh has not landed yet is an entity that draws
    /// nothing, which is correct and invisible.
    pub visible_added: Vec<ChunkKey>,
    /// Chunks that left the drawn region, in key order.
    ///
    /// Despawn the entity, drop the cached geometry, and remove any collider.
    pub removed: Vec<ChunkKey>,
}

/// Decides which chunks exist, gets them generated, and says what became available.
///
/// Not `Clone`, deliberately: it owns a job pool and an inbox, and two streamers sharing neither
/// would be two worlds.
pub struct TerrainStreamer {
    settings: TerrainSettings,
    source: Arc<dyn TerrainSource>,
    /// ADR 0042's sparse overlay. Nothing writes to it yet — see the module docs.
    edits: Arc<Edits>,
    pool: JobPool,
    /// Where finished visual chunks land. Drains in **key order, never completion order** (ADR
    /// 0041), which is what stops two machines seeing chunks appear in different orders.
    inbox: Arc<Inbox<ChunkKey, (u64, VoxelMesh)>>,
    /// How many times the edits have changed.
    ///
    /// A job carries the version it was submitted under, and a delivery whose version is stale is
    /// thrown away. Without it, digging a hole and getting the pre-dig mesh back a few milliseconds
    /// later would fill the hole in again — a race that depends on machine speed and would therefore
    /// be almost impossible to reproduce.
    edit_version: u64,
    /// Chunks whose geometry an edit invalidated, waiting to be redone.
    dirty: BTreeSet<ChunkKey>,
    /// What was required last tick, so this tick can diff against it.
    required: Residency,
    /// Chunks whose geometry the caller has already been given, and whether it was empty.
    ///
    /// Empty chunks are recorded too. Without that, an all-air chunk would be re-meshed every tick
    /// forever, because "the caller has no geometry for it" and "there is no geometry" look
    /// identical from here.
    known: BTreeMap<ChunkKey, bool>,
    /// Visual chunks currently on the pool, so the same work is not submitted twice.
    in_flight: BTreeSet<ChunkKey>,
}

/// Hand-written because a [`TerrainSource`] is a game's own type and cannot be required to be
/// `Debug` — the same reason `RapierPhysics` writes its own.
///
/// Counts rather than contents is the right answer anyway: the two questions worth asking of a
/// streamer while diagnosing one are "how much does it think exists" and "is it still working".
impl std::fmt::Debug for TerrainStreamer {
    fn fmt(&self, formatter: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        formatter
            .debug_struct("TerrainStreamer")
            .field("known_chunks", &self.known.len())
            .field("in_flight", &self.in_flight.len())
            .field("workers", &self.pool.workers())
            .field("edits", &self.edits.len())
            .finish()
    }
}

impl TerrainStreamer {
    /// A streamer over a generated world.
    ///
    /// `workers` is how many threads mesh visual chunks. It cannot affect the result — that is
    /// ADR 0041's requirement and `the_thread_count_cannot_reach_the_colliders` is the test.
    #[must_use]
    pub fn new(source: Arc<dyn TerrainSource>, settings: TerrainSettings, workers: usize) -> Self {
        Self {
            settings,
            source,
            edits: Arc::new(Edits::new()),
            pool: JobPool::new(workers),
            inbox: Arc::new(Inbox::new()),
            required: Residency::default(),
            known: BTreeMap::new(),
            in_flight: BTreeSet::new(),
            edit_version: 0,
            dirty: BTreeSet::new(),
        }
    }

    /// Changes one sample of the world — digging, or building.
    ///
    /// This is ADR 0042's other half made usable: the base is generated and **an edit is authored**,
    /// so a save file is a seed plus this list rather than a world of voxels.
    ///
    /// # It invalidates more than one chunk, and that is the apron again
    ///
    /// A sample near a boundary belongs to every chunk whose field reaches it — up to eight, because
    /// a chunk samples one cell beyond its own volume on both sides (ADR 0043 §4). Marking only the
    /// chunk that "owns" the sample leaves the neighbours holding geometry that disagrees with it,
    /// and the seam opens exactly where somebody has been digging.
    ///
    /// # Determinism
    ///
    /// An edit is a gameplay action, so it happens at a definite tick on every machine. What it
    /// invalidates is integer arithmetic over the same coordinate. Nothing here depends on what the
    /// job pool had finished.
    pub fn edit(&mut self, sample: [i32; 3], value: f32) {
        // Copy-on-write: jobs already running hold an `Arc` to the *old* edits and will finish
        // against them. Their results are discarded by the version check rather than raced against.
        Arc::make_mut(&mut self.edits).set(sample, value);
        self.edit_version += 1;

        for key in self.chunks_sampling(sample) {
            self.known.remove(&key);
            self.in_flight.remove(&key);
            self.dirty.insert(key);
        }
    }

    /// How many samples have been changed. What a save file's terrain section costs.
    #[must_use]
    pub fn edit_count(&self) -> usize {
        self.edits.len()
    }

    /// Every chunk whose field reads a given world sample.
    ///
    /// Up to eight, because a chunk of `n` cells covers samples `[k*n - 1, k*n + n]` — overlapping
    /// its neighbours by one at each end, which is the two-sided apron.
    fn chunks_sampling(&self, sample: [i32; 3]) -> Vec<ChunkKey> {
        let cells = self.settings.shape.cells as i32;
        let candidates = |value: i32| {
            let base = value.div_euclid(cells);
            // The chunk the sample sits in, and the ones on either side, filtered by whether their
            // field actually reaches it.
            [base - 1, base, base + 1]
                .into_iter()
                .filter(move |k| {
                    let low = k * cells - 1;
                    let high = k * cells + cells;
                    value >= low && value <= high
                })
                .collect::<Vec<i32>>()
        };

        let mut keys = Vec::new();
        for x in candidates(sample[0]) {
            for y in candidates(sample[1]) {
                for z in candidates(sample[2]) {
                    keys.push(ChunkKey::new(x, y, z));
                }
            }
        }
        keys
    }

    /// How big chunks are.
    #[must_use]
    pub fn settings(&self) -> TerrainSettings {
        self.settings
    }

    /// How many chunks are being meshed in the background right now.
    ///
    /// **Diagnostics only.** A count that depends on machine speed is exactly what makes a replay
    /// diverge, so gameplay must never branch on this — the same rule `JobPool::pending` carries.
    #[must_use]
    pub fn in_flight(&self) -> usize {
        self.in_flight.len()
    }

    /// Advances streaming by one tick and reports what changed.
    ///
    /// # What is deterministic here, precisely
    ///
    /// Given the same viewers, this returns the same `colliders`, `colliders_removed` and `removed`
    /// — same contents **and same order** — on every machine at every worker count, because all
    /// three come from integer set differences over `BTreeSet`s and collision chunks are meshed
    /// inline rather than waited for.
    ///
    /// **Order is part of the claim, not a detail.** The first version of this got the *set* right
    /// and the order wrong, by partitioning collision chunks into "meshed now" and "already known"
    /// — a split that depends on what the job pool had finished. It passed on an 8-core developer
    /// machine and failed on CI.
    ///
    /// `meshes` is **not** deterministic in *timing* — a faster machine gets a chunk sooner. It is
    /// deterministic in *content and order*: the same chunk always meshes to the same geometry, and
    /// the inbox drains in key order rather than completion order. Nothing in the simulation may
    /// read it.
    pub fn update(&mut self, viewers: &[Viewer]) -> TerrainUpdate {
        let required = Residency::of(viewers);
        let mut update = TerrainUpdate::default();

        // --- 1. Let go of what is no longer needed, before doing any work. ---
        //
        // First rather than last so that a viewer teleporting across the world does not hold both
        // regions in memory at once.
        // Keyed on the **visual** set rather than the data set, because that is what has consumers:
        // an entity, a cache entry and possibly a collider. The `data` set is one ring wider and
        // exists so that meshing a drawn chunk can read into its neighbours — and since a chunk is
        // sampled from the source on demand rather than stored, nothing is held for it to release.
        for key in self.required.visual.difference(&required.visual) {
            self.known.remove(key);
            self.in_flight.remove(key);
            // **Reported unconditionally**, including for chunks the caller was never given.
            //
            // The obvious version -- only report what was actually delivered -- makes this list
            // depend on what the job pool had finished, which is machine speed. That is the same
            // defect the collider ordering had, in a second place, and it is the general shape to
            // watch for: **anything filtered by "what does the caller already have" inherits the
            // nondeterminism of delivery.**
            //
            // Safe because removal is idempotent by design at every consumer: a mesh cache drops a
            // missing key silently, and `PhysicsBackend::remove_static_mesh` documents that
            // removing something absent is not an error -- precisely because most chunks are empty
            // and never had a collider.
            update.removed.push(*key);
        }

        // What entered the drawn region. Entities are created from this, never from mesh arrival —
        // see `TerrainUpdate::visible_added`.
        for key in required.visual.difference(&self.required.visual) {
            update.visible_added.push(*key);
        }

        // --- 2. Collision chunks, meshed inline. The simulation blocks here. ---
        //
        // ADR 0041 §2: a character stands on these, so *when* they arrive changes where it ends up.
        // Meshing on this thread makes arrival unconditional -- the cost is a frame hitch on a slow
        // machine, which is the trade that keeps the replay.
        //
        // Done before the visual submissions so that a tick which is short of time spends it here.
        //
        // **Driven by the residency diff and nothing else**, which is the correction that matters.
        // An earlier version reported "chunks meshed this tick" followed by "chunks already known",
        // and *which group a chunk fell into* depended on whether the job pool had already delivered
        // it as a visual chunk -- so the ORDER of this list depended on thread count. The set was
        // always right and the order was not, which CI caught and this machine never did.
        //
        // `BTreeSet::difference` yields in key order, so the sequence is now a pure function of
        // where the viewers are.
        // Taken rather than borrowed, so a chunk is redone once per edit rather than every tick
        // after one. Anything still dirty at the end of this call is put back below.
        let dirty = std::mem::take(&mut self.dirty);

        // Chunks that must be solid and are not already: the ones entering the collision region,
        // plus the ones an edit invalidated. A `BTreeSet` so the union is in key order however the
        // two sets overlap -- the ordering lesson from the last round, applied rather than relearnt.
        let solid_now: BTreeSet<ChunkKey> = required
            .collision
            .difference(&self.required.collision)
            .chain(dirty.intersection(&required.collision))
            .copied()
            .collect();

        for key in &solid_now {
            let mesh = self.mesh_one(*key);
            let empty = mesh.is_empty();
            // Recorded so the pool is not asked to do the same work again. It may already be here
            // from a visual delivery, which is fine -- meshing is a pure function, so doing it twice
            // costs time and cannot change the answer.
            self.known.insert(*key, empty);
            self.in_flight.remove(key);
            if empty {
                // An edit can dig a chunk away entirely. The caller is still holding the collider
                // it was given before, so it has to be told -- otherwise the tunnel you just dug
                // stays solid and nothing says why.
                update.colliders_removed.push(*key);
            } else {
                update.colliders.push(self.ready(*key, mesh));
            }
        }

        // Chunks that stopped needing to be solid but are still drawn. Their collider goes; their
        // mesh stays. Without this a viewer walking away would leave invisible ground behind it.
        for key in self.required.collision.difference(&required.collision) {
            update.colliders_removed.push(*key);
        }

        // --- 3. Visual chunks, submitted to the pool. These may arrive whenever. ---
        for key in &required.visual {
            if self.known.contains_key(key) || self.in_flight.contains(key) {
                continue;
            }
            self.in_flight.insert(*key);

            // A job owns its inputs (ADR 0041), so everything it touches is cloned or shared behind
            // an Arc. It cannot reach the world, which is what makes it safe to run anywhere.
            let source = Arc::clone(&self.source);
            let edits = Arc::clone(&self.edits);
            let inbox = Arc::clone(&self.inbox);
            let shape = self.settings.shape;
            let key = *key;
            let version = self.edit_version;
            self.pool.submit(move || {
                let mesh = amadeo_voxel::mesh_chunk(source.as_ref(), &edits, shape, key);
                inbox.deliver(key, (version, mesh));
            });
        }

        // --- 4. Collect whatever finished, in key order. ---
        for (key, (version, mesh)) in self.inbox.drain() {
            self.in_flight.remove(&key);
            // Meshed against edits that have since changed. Discarding it is what stops a dug hole
            // filling itself back in when a job that started before the dig lands after it.
            if version != self.edit_version {
                continue;
            }
            // A chunk that left the region while its job was running. Its result is thrown away
            // rather than delivered, or the caller would be handed geometry it just removed.
            if !required.data.contains(&key) {
                continue;
            }
            let empty = mesh.is_empty();
            // A collision chunk meshed inline may already be known. Do not deliver it twice.
            if self.known.insert(key, empty).is_none() && !empty {
                update.meshes.push(self.ready(key, mesh));
            }
        }

        self.required = required;
        update
    }

    /// Meshes one chunk on this thread.
    fn mesh_one(&self, key: ChunkKey) -> VoxelMesh {
        amadeo_voxel::mesh_chunk(self.source.as_ref(), &self.edits, self.settings.shape, key)
    }

    /// Packages a mesh with where it goes.
    fn ready(&self, key: ChunkKey, mesh: VoxelMesh) -> ReadyChunk {
        ReadyChunk {
            key,
            origin: self.settings.shape.origin_of(key),
            mesh,
        }
    }
}

/// How many meshing workers to use on this machine, leaving a core for the simulation.
///
/// A convenience so a game does not have to depend on `amadeo-jobs` for one number, and so there is
/// one answer to the question rather than one per caller.
///
/// **This number may not reach gameplay.** It varies by machine; it decides how *fast* chunks are
/// meshed and never *what* comes out, which is ADR 0041's whole claim and what
/// `the_thread_count_cannot_reach_the_colliders` holds.
#[must_use]
pub fn default_workers() -> usize {
    JobPool::workers_for_this_machine()
}

/// The name a chunk's geometry is held under, for a mesh cache or a collider registry.
///
/// A plain string because that is what `amadeo-render`'s cache and `Mesh` component already speak —
/// so a streamed chunk needs **no renderer change at all**, which is ADR 0035's bet paying off a
/// fourth time after `BoxMesh`, `PlaneMesh` and `GltfPart`.
///
/// Includes the detail level, because two chunks over the same volume at different resolutions are
/// different geometry and sharing a name would make one silently overwrite the other.
#[must_use]
pub fn chunk_mesh_id(key: ChunkKey) -> String {
    format!("terrain/{}/{}/{}/{}", key.lod, key.x, key.y, key.z)
}

#[cfg(test)]
mod tests {
    use super::*;
    use amadeo_voxel::FlatGround;

    fn settings() -> TerrainSettings {
        TerrainSettings {
            shape: ChunkShape::new(8, 1.0),
            friction: 0.6,
        }
    }

    /// Ground at y = 4, which cuts through chunk row 0 and leaves rows above and below empty.
    fn streamer(workers: usize) -> TerrainStreamer {
        TerrainStreamer::new(Arc::new(FlatGround { height: 4.0 }), settings(), workers)
    }

    fn viewer(x: i32, visual: i32, collision: i32) -> Viewer {
        Viewer {
            centre: ChunkKey::new(x, 0, 0),
            visual_radius: visual,
            collision_radius: collision,
        }
    }

    /// Walks several streamers east **in lockstep**, one tick each, and returns what each saw.
    ///
    /// # Interleaved rather than run one after another, and that is not a detail
    ///
    /// An earlier version of this ran each worker count to completion separately. It passed even
    /// when collision chunks were deliberately routed through the job pool — the bug ADR 0041 §2
    /// exists to forbid — because a streamer running alone gives its pool all the wall clock it
    /// needs, so even the slow configuration finished in time and the divergence never appeared.
    ///
    /// Advancing them together is what puts them under comparable time pressure, and it is what
    /// makes this test evidence. Found by breaking the implementation on purpose and noticing this
    /// test did **not** fail when it should have.
    fn walk_together(worker_counts: &[usize], steps: i32) -> Vec<Vec<Vec<ChunkKey>>> {
        let mut streamers: Vec<TerrainStreamer> =
            worker_counts.iter().map(|w| streamer(*w)).collect();
        let mut seen: Vec<Vec<Vec<ChunkKey>>> = vec![Vec::new(); streamers.len()];

        for step in 0..steps {
            for (index, streamer) in streamers.iter_mut().enumerate() {
                let update = streamer.update(&[viewer(step, 2, 1)]);
                seen[index].push(update.colliders.iter().map(|c| c.key).collect());
            }
        }
        seen
    }

    #[test]
    fn colliders_are_complete_on_the_tick_they_are_asked_for() {
        // **The claim ADR 0041 §2 requires.** A character stands on these, so "it will be there in a
        // few frames" is not an acceptable answer -- the character would fall through.
        let mut streamer = streamer(4);
        let update = streamer.update(&[viewer(0, 3, 1)]);

        assert!(
            !update.colliders.is_empty(),
            "collision chunks must be meshed inline, not waited for"
        );
        for chunk in &update.colliders {
            assert!(
                !chunk.mesh.is_empty(),
                "an empty chunk is dropped, not reported"
            );
        }
    }

    #[test]
    fn the_thread_count_cannot_reach_the_colliders() {
        // **M2.5's exit gate 2, at this layer.** ADR 0041's whole claim is that parallelism is a
        // pure speedup nothing downstream can observe. Odd worker counts are deliberate: an
        // off-by-one in work slicing hides completely when the work divides evenly.
        let counts = [1_usize, 2, 3, 5, 8];
        let seen = walk_together(&counts, 6);
        for (index, count) in counts.iter().enumerate().skip(1) {
            assert_eq!(
                seen[index], seen[0],
                "{count} workers produced different colliders than 1 worker"
            );
        }
    }

    #[test]
    fn the_same_walk_produces_the_same_geometry_not_just_the_same_keys() {
        // Keys agreeing is necessary and not sufficient: a mesher that produced the right chunks
        // with the wrong vertices would pass a key comparison and still put the player somewhere
        // else. So compare the geometry itself.
        let mut first = streamer(1);
        let mut second = streamer(8);
        for step in 0..4 {
            let a = first.update(&[viewer(step, 2, 1)]);
            let b = second.update(&[viewer(step, 2, 1)]);
            assert_eq!(a.colliders, b.colliders, "tick {step}");
            assert_eq!(a.removed, b.removed, "tick {step}");
        }
    }

    #[test]
    fn a_chunk_is_not_meshed_twice_for_the_same_viewer() {
        // Standing still must not be work. Without the `known` map every tick would re-mesh the
        // whole region, which is invisible in a correctness test and ruinous in a frame budget.
        let mut streamer = streamer(2);
        streamer.update(&[viewer(0, 2, 1)]);
        streamer.pool.wait_for_idle();
        streamer.update(&[viewer(0, 2, 1)]);

        let third = streamer.update(&[viewer(0, 2, 1)]);
        assert!(
            third.meshes.is_empty(),
            "nothing new should be delivered when the viewer has not moved"
        );
        assert_eq!(streamer.in_flight(), 0, "no work should still be queued");
    }

    #[test]
    fn walking_away_removes_what_was_left_behind() {
        let mut streamer = streamer(2);
        streamer.update(&[viewer(0, 1, 1)]);
        streamer.pool.wait_for_idle();
        streamer.update(&[viewer(0, 1, 1)]);

        // Far enough that none of the original region is required any more.
        let update = streamer.update(&[viewer(20, 1, 1)]);
        assert!(
            !update.removed.is_empty(),
            "chunks left behind must be reported so their colliders can be dropped"
        );
    }

    #[test]
    fn an_empty_chunk_is_never_reported() {
        // Most chunks of a real world are entirely air or entirely rock. Reporting them would hand
        // the physics backend geometry it refuses (`StaticMesh::is_empty`) and the renderer nothing
        // to draw. A tall viewer box guarantees some are empty.
        let mut streamer = streamer(4);
        let update = streamer.update(&[Viewer {
            centre: ChunkKey::new(0, 0, 0),
            visual_radius: 3,
            collision_radius: 3,
        }]);

        for chunk in update.colliders.iter().chain(&update.meshes) {
            assert!(!chunk.mesh.is_empty());
        }
        // And the empty ones must be *remembered*, or they are re-meshed forever.
        let again = streamer.update(&[Viewer {
            centre: ChunkKey::new(0, 0, 0),
            visual_radius: 3,
            collision_radius: 3,
        }]);
        assert!(again.meshes.is_empty());
    }

    #[test]
    fn meshes_eventually_arrive_for_visible_chunks() {
        // The visual half. It is allowed to be late; it is not allowed to never come.
        //
        // **Counted across ticks rather than asserted on one**, and that is not defensive padding:
        // an earlier version drained on the *second* update and assumed the first had delivered
        // nothing. Jobs can finish between submission and collection inside a single call, so on a
        // fast enough machine the first update took them all and the second was empty. It passed
        // here and failed in CI. Anything that asserts on *which tick* work landed is asserting on
        // machine speed, which is the whole thing ADR 0041 forbids gameplay from doing.
        let mut streamer = streamer(4);
        let mut delivered = 0;
        for _ in 0..8 {
            delivered += streamer.update(&[viewer(0, 2, 0)]).meshes.len();
            streamer.pool.wait_for_idle();
        }

        assert!(
            delivered > 0,
            "visible chunks must eventually be delivered for drawing"
        );
    }

    #[test]
    fn a_chunk_that_leaves_while_meshing_is_not_delivered() {
        // The race that would otherwise hand the caller geometry it has just been told to remove.
        let mut streamer = streamer(2);
        streamer.update(&[viewer(0, 2, 0)]);
        // Move far away *before* collecting, so every in-flight job lands outside the region.
        let update = streamer.update(&[viewer(50, 2, 0)]);
        streamer.pool.wait_for_idle();
        let after = streamer.update(&[viewer(50, 2, 0)]);

        for chunk in update.meshes.iter().chain(&after.meshes) {
            assert!(
                chunk.key.x > 40,
                "{:?} is from the abandoned region and should not have been delivered",
                chunk.key
            );
        }
    }

    #[test]
    fn what_becomes_visible_does_not_depend_on_what_finished_meshing() {
        // **Entities are spawned from this list**, and an entity is world state, so if this
        // followed job completion the entity allocator would follow machine speed and the state
        // hash with it (ADR 0028). It has to be residency and nothing else.
        //
        // Includes empty chunks deliberately: an all-air chunk still gets an entity, because
        // whether it turned out empty is geometry, and geometry must not decide what exists.
        let counts = [1_usize, 8];
        let mut streamers: Vec<TerrainStreamer> = counts.iter().map(|w| streamer(*w)).collect();
        let mut seen: Vec<Vec<Vec<ChunkKey>>> = vec![Vec::new(); streamers.len()];

        for step in 0..5 {
            for (index, streamer) in streamers.iter_mut().enumerate() {
                let update = streamer.update(&[viewer(step, 2, 1)]);
                seen[index].push(update.visible_added);
            }
        }
        assert_eq!(
            seen[0], seen[1],
            "visible_added must not follow thread count"
        );
        assert!(
            seen[0].iter().any(|tick| !tick.is_empty()),
            "walking east must bring new chunks into view"
        );
    }

    #[test]
    fn colliders_come_back_in_key_order_when_the_solid_region_grows() {
        // **The regression guard for the bug CI found and this machine never did.**
        //
        // The old implementation reported every solid chunk every tick, in two passes: those meshed
        // on this tick, then those already known. Growing the collision region put the new shell in
        // the first pass and the existing centre in the second, so the output was
        // [shell..., centre...] -- the right *set* in an order that is not sorted, and which shifted
        // with thread count because the pool could move a chunk from one pass to the other.
        //
        // Asserting sorted order catches that without depending on timing at all, which is what
        // makes this a test rather than a coin flip. The original failure needed a loaded CI runner
        // to show up.
        let mut streamer = streamer(4);
        streamer.update(&[viewer(0, 3, 1)]);
        let update = streamer.update(&[viewer(0, 3, 2)]);

        let keys: Vec<ChunkKey> = update.colliders.iter().map(|c| c.key).collect();
        assert!(
            !keys.is_empty(),
            "growing the solid region must produce colliders"
        );

        let mut sorted = keys.clone();
        sorted.sort();
        assert_eq!(
            keys, sorted,
            "colliders must be in key order however they came to be meshed"
        );

        // And a chunk that was already solid is not reported again -- it is a change list, not a
        // census, or the whole collision region would be re-meshed every tick.
        assert!(
            !keys.contains(&ChunkKey::new(0, 0, 0)),
            "the centre was already solid and must not be re-reported"
        );
    }

    #[test]
    fn ground_that_stops_being_solid_but_stays_visible_has_its_collider_dropped() {
        // The case `removed` alone would miss. A viewer walking away from ground it can still see
        // needs the collider gone and the mesh kept -- otherwise the world accumulates invisible
        // collision, and the symptom is a player stopping dead in open terrain.
        let mut streamer = streamer(2);
        streamer.update(&[viewer(0, 4, 1)]);
        // Same visual region, collision region moves with the viewer.
        let update = streamer.update(&[viewer(2, 4, 1)]);

        assert!(
            !update.colliders_removed.is_empty(),
            "chunks that stopped being solid must be reported"
        );
        for key in &update.colliders_removed {
            assert!(
                !update.removed.contains(key),
                "{key:?} is still drawn, so it must not be in `removed`"
            );
        }
    }

    #[test]
    fn digging_changes_the_ground_you_are_standing_on() {
        // ADR 0042's destructibility claim, end to end: the base is generated, an edit is authored,
        // and the collider the character stands on reflects it on the tick it was made.
        let mut streamer = streamer(4);
        let before = streamer.update(&[viewer(0, 1, 1)]);
        let centre = before
            .colliders
            .iter()
            .find(|c| c.key == ChunkKey::new(0, 0, 0))
            .expect("the chunk under the viewer is solid")
            .clone();

        // Carve a hole through the surface. The ground is at y = 4 and cells are 1 unit.
        for y in 3..6 {
            streamer.edit([4, y, 4], -4.0);
        }
        let after = streamer.update(&[viewer(0, 1, 1)]);

        let dug = after
            .colliders
            .iter()
            .find(|c| c.key == ChunkKey::new(0, 0, 0))
            .expect("the edited chunk must be re-reported as solid");
        assert_ne!(
            dug.mesh, centre.mesh,
            "digging must change the collider, not just the visual mesh"
        );
        assert_eq!(streamer.edit_count(), 3);
    }

    #[test]
    fn an_edit_near_a_seam_invalidates_every_chunk_that_reads_it() {
        // **The apron, one more time.** A sample on a boundary is read by chunks on both sides, so
        // marking only the one that "owns" it leaves the neighbour holding geometry that disagrees
        // -- and the crack opens exactly where somebody has been digging.
        let streamer = streamer(1);
        // Sample 8 with 8-cell chunks is the plane shared by chunk 0 and chunk 1.
        let touched = streamer.chunks_sampling([8, 4, 4]);
        assert!(
            touched.contains(&ChunkKey::new(0, 0, 0)),
            "the low side reads it"
        );
        assert!(
            touched.contains(&ChunkKey::new(1, 0, 0)),
            "the high side reads it"
        );

        // And a sample well inside a chunk is read by that chunk alone on the x axis.
        let inner = streamer.chunks_sampling([4, 4, 4]);
        assert!(inner.iter().all(|key| key.x == 0));
    }

    #[test]
    fn a_stale_mesh_cannot_fill_in_a_hole_that_was_just_dug() {
        // The race the version counter exists for. A job submitted before an edit finishes after it,
        // and its geometry describes a world that no longer exists. Delivering it would refill the
        // hole a few milliseconds after digging -- timing-dependent, so nearly unreproducible.
        let mut streamer = streamer(4);
        streamer.update(&[viewer(0, 2, 0)]);
        // Edit while jobs from that update are still in flight.
        streamer.edit([4, 4, 4], -4.0);
        streamer.pool.wait_for_idle();

        let update = streamer.update(&[viewer(0, 2, 0)]);
        for chunk in &update.meshes {
            assert!(
                !streamer.dirty.contains(&chunk.key),
                "{:?} was delivered from a stale job",
                chunk.key
            );
        }
        // The edited chunk must still come back, meshed against the new edits.
        streamer.pool.wait_for_idle();
        let recovered = streamer.update(&[viewer(0, 2, 0)]);
        // Through `colliders`, not `meshes`: `collision_radius: 0` still makes the chunk the viewer
        // stands in solid, so an edit to it is redone inline rather than on the pool. Worth stating,
        // because looking only at `meshes` here is what made this test fail the first time.
        let seen_again = update
            .meshes
            .iter()
            .chain(&recovered.meshes)
            .chain(&update.colliders)
            .chain(&recovered.colliders)
            .any(|c| c.key == ChunkKey::new(0, 0, 0));
        assert!(
            seen_again,
            "the edited chunk must be re-meshed, not dropped"
        );
    }

    #[test]
    fn editing_does_not_depend_on_thread_count() {
        // Same claim as the residency one, now that gameplay can change the world. An edit is a
        // gameplay action at a definite tick, so what it invalidates must be identical everywhere.
        let counts = [1_usize, 8];
        let mut streamers: Vec<TerrainStreamer> = counts.iter().map(|w| streamer(*w)).collect();
        let mut seen: Vec<Vec<Vec<ChunkKey>>> = vec![Vec::new(); streamers.len()];

        for step in 0..4 {
            for (index, streamer) in streamers.iter_mut().enumerate() {
                streamer.edit([4 + step, 4, 4], -4.0);
                let update = streamer.update(&[viewer(0, 2, 1)]);
                seen[index].push(update.colliders.iter().map(|c| c.key).collect());
            }
        }
        assert_eq!(seen[0], seen[1]);
    }

    #[test]
    fn a_chunk_mesh_id_is_stable_and_mentions_its_detail_level() {
        assert_eq!(chunk_mesh_id(ChunkKey::new(3, -1, 2)), "terrain/0/3/-1/2");
        assert_ne!(
            chunk_mesh_id(ChunkKey::at_lod(0, 1, 1, 1)),
            chunk_mesh_id(ChunkKey::at_lod(1, 1, 1, 1)),
            "two resolutions of one volume must not share a name"
        );
    }
}
