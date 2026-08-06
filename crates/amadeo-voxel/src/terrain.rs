//! Where a chunk's samples come from, and how one chunk is meshed so it meets its neighbours.
//!
//! # ADR 0042's data model, as code
//!
//! Terrain is a **generated base plus a sparse overlay of edits**. The base is a pure function of
//! position ([`TerrainSource`]); an edit is a single changed sample ([`Edits`]). Only the edits are
//! authored, so only they are simulation state — the base is re-derivable from the seed and belongs
//! outside the state hash, exactly as `GlobalTransform` does (ADR 0019).
//!
//! Neither type is reflected or hashed here, because this crate has no dependencies and cannot name
//! `Reflect`. The layer that stores edits on an entity is what makes them hashed state.
//!
//! # There are TWO aprons, and ADR 0042 only described one
//!
//! ADR 0042 §2 says a chunk of `n` cells fills an `n + 1` sample grid, "reaching one step into the
//! neighbour on the high side". That is correct and it is not sufficient, which was found here by
//! meshing two adjacent chunks and looking at the result rather than by trusting the ADR.
//!
//! Two different things need neighbour data:
//!
//! 1. **Vertices.** A cell's vertex is decided by its eight corners, so meshing the last *cell* of a
//!    chunk needs the first *sample* of the next one. This is the apron the ADR describes.
//! 2. **Quads.** [`surface_nets`] emits a quad for a grid edge by looking at
//!    the four cells around it, and it can only do that where all four cells have vertices. At a
//!    chunk's low face they do not — the cells on the other side belong to the previous chunk. So
//!    the quads *bridging* two chunks are emitted by neither, and the surface has a one-cell gap all
//!    the way around every chunk.
//!
//! The fix is a **low apron as well**: a chunk meshes one extra cell *below* its own volume, whose
//! vertices exist only so the bridging quads can be emitted. Every quad in the world is then emitted
//! exactly once, by the chunk on the high side of it — no duplicates, no gaps. This is the same
//! convention `fast-surface-nets` documents as "faces are not generated on the positive boundaries
//! of a chunk".
//!
//! So a chunk of `n` cells fills an `n + 2` sample grid covering `n + 1` cells, running from one
//! cell below its origin. [`ChunkShape::samples_per_axis`] is that number, and
//! `two_adjacent_chunks_have_no_gap_between_them` is what holds it in place.

use crate::chunk::ChunkKey;
use crate::{Field, VoxelMesh, surface_nets};
use std::collections::BTreeMap;

/// The generated half of terrain: a pure function from a world position to a signed distance.
///
/// **Negative is inside the surface**, matching [`Field`]. Getting the sign backwards produces a
/// mesh that is inside out, which reads as invisible terrain rather than as a sign error.
///
/// # It must be a pure function, and that is not a style request
///
/// The same coordinate must give the same value on every machine and at every moment, because a
/// chunk's collider is gameplay state (ADR 0041 §2) and two machines that disagree about the ground
/// disagree about where the player is standing. In practice that means: no wall clock, no `HashMap`
/// iteration, no cached mutable state, and any randomness seeded from the coordinate rather than
/// drawn from a stream.
///
/// `Send + Sync` because meshing a chunk is the job `amadeo-jobs` was built for, and a job owns its
/// inputs (ADR 0041).
pub trait TerrainSource: Send + Sync {
    /// The signed distance at a world position, in world units.
    fn sample(&self, x: f32, y: f32, z: f32) -> f32;
}

/// A flat ground plane at a given height. The simplest source that produces something walkable.
///
/// Useful on its own for tests and for a game that wants terrain machinery without terrain shape
/// yet, and useful as a worked example of how short a [`TerrainSource`] is.
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct FlatGround {
    /// The world height of the surface.
    pub height: f32,
}

impl TerrainSource for FlatGround {
    fn sample(&self, _x: f32, y: f32, _z: f32) -> f32 {
        // Positive above the surface, negative below it: the ground is the solid half.
        y - self.height
    }
}

/// The authored half of terrain: samples a player has changed.
///
/// # Why a `BTreeMap` and why keyed by integer sample coordinate
///
/// Sparse, so an untouched world costs nothing to store and nothing to hash — ADR 0042's whole
/// point. Ordered, because `CLAUDE.md` trap 2 names unordered iteration as a determinism leak and
/// anything that serialises or hashes this walks it.
///
/// Keyed by **world** sample coordinate rather than per chunk, deliberately. An edit made near a
/// boundary has to be visible to both chunks that read that sample, or the two would mesh the same
/// point differently and the seam would open exactly where somebody had been digging.
#[derive(Debug, Clone, Default, PartialEq)]
pub struct Edits {
    changed: BTreeMap<[i32; 3], f32>,
}

impl Edits {
    /// No edits — a world exactly as generated.
    #[must_use]
    pub fn new() -> Self {
        Self::default()
    }

    /// Changes one sample.
    pub fn set(&mut self, sample: [i32; 3], value: f32) {
        self.changed.insert(sample, value);
    }

    /// Returns a sample to whatever the generator says it should be.
    pub fn clear(&mut self, sample: [i32; 3]) {
        self.changed.remove(&sample);
    }

    /// The edited value at a sample, if it has been edited.
    #[must_use]
    pub fn get(&self, sample: [i32; 3]) -> Option<f32> {
        self.changed.get(&sample).copied()
    }

    /// How many samples have been changed. What a save file's terrain section costs.
    #[must_use]
    pub fn len(&self) -> usize {
        self.changed.len()
    }

    /// Whether the world is exactly as generated.
    #[must_use]
    pub fn is_empty(&self) -> bool {
        self.changed.is_empty()
    }

    /// Every edit, in sample-coordinate order.
    pub fn iter(&self) -> impl Iterator<Item = ([i32; 3], f32)> + '_ {
        self.changed.iter().map(|(key, value)| (*key, *value))
    }
}

/// How big a chunk is, in cells and in world units.
///
/// One value shared by generation, meshing and residency, so the three cannot disagree about what a
/// chunk is — which they would silently, since each of them would otherwise take a size parameter.
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct ChunkShape {
    /// Cells along each axis of a chunk, at any detail level.
    ///
    /// A chunk always meshes this many cells; a coarser chunk covers more world with them.
    pub cells: usize,
    /// The world size of one cell at detail level 0.
    pub cell_size: f32,
}

impl ChunkShape {
    /// A chunk of `cells` cells, each `cell_size` world units across.
    #[must_use]
    pub fn new(cells: usize, cell_size: f32) -> Self {
        Self { cells, cell_size }
    }

    /// The world size of one cell at a detail level. Doubles per level.
    ///
    /// Level 0 is full resolution. Nothing produces a level above 0 yet (Q25), but the arithmetic is
    /// here because every other function in this module would otherwise have to be changed to add
    /// it.
    #[must_use]
    pub fn cell_size_at(&self, lod: u8) -> f32 {
        // `2f32.powi` rather than a bit shift: the exponent is small, this is exact for every level
        // that matters, and it does not have to worry about overflowing a shift.
        self.cell_size * 2.0f32.powi(i32::from(lod))
    }

    /// The world size of a whole chunk at a detail level.
    #[must_use]
    pub fn chunk_size_at(&self, lod: u8) -> f32 {
        self.cell_size_at(lod) * self.cells as f32
    }

    /// How many samples a chunk's field needs per axis: `cells + 2`.
    ///
    /// One more than [`Field::new`] would suggest, because a chunk needs an apron on **both** sides
    /// — see the module docs. A chunk that fills only `cells + 1` cracks along every low face.
    #[must_use]
    pub fn samples_per_axis(&self) -> usize {
        self.cells + 2
    }

    /// The world position of a chunk's low corner.
    #[must_use]
    pub fn origin_of(&self, key: ChunkKey) -> [f32; 3] {
        let size = self.chunk_size_at(key.lod);
        [
            key.x as f32 * size,
            key.y as f32 * size,
            key.z as f32 * size,
        ]
    }
}

/// Fills a chunk's field from the generated base and the edits over it.
///
/// The returned [`Field`] covers `cells + 1` cells starting **one cell below** the chunk's origin,
/// which is what makes neighbouring chunks meet. See the module docs for why there are two aprons.
///
/// # Determinism
///
/// Samples are visited in a fixed order and every one is a pure function of its own coordinate, so
/// two machines fill a chunk identically and a chunk filled twice is filled the same way. Nothing
/// here reads another chunk's *state* — only the same source and the same edits — which is what lets
/// chunks be generated in any order, in parallel, without their results depending on that order.
#[must_use]
pub fn fill_chunk(
    source: &dyn TerrainSource,
    edits: &Edits,
    shape: ChunkShape,
    key: ChunkKey,
) -> Field {
    let cell_size = shape.cell_size_at(key.lod);
    // The chunk's own low corner in *sample* coordinates. A chunk of `n` cells owns samples
    // [k * n, (k + 1) * n], and the field below starts one earlier for the low apron.
    let base = [
        key.x * shape.cells as i32,
        key.y * shape.cells as i32,
        key.z * shape.cells as i32,
    ];

    let mut field = Field::new(shape.cells + 1);
    field.fill(|fx, fy, fz| {
        // Field coordinate 0 is one sample *below* the chunk's own first sample.
        let sample = [
            base[0] + fx as i32 - 1,
            base[1] + fy as i32 - 1,
            base[2] + fz as i32 - 1,
        ];
        if let Some(edited) = edits.get(sample) {
            return edited;
        }
        source.sample(
            sample[0] as f32 * cell_size,
            sample[1] as f32 * cell_size,
            sample[2] as f32 * cell_size,
        )
    });
    field
}

/// Meshes one chunk, in world units relative to the chunk's own origin.
///
/// The caller places the result by setting a transform to [`ChunkShape::origin_of`] and nothing
/// else. Positions may be slightly negative, because the low apron's cell sits below the origin —
/// that is correct and is what closes the seam with the previous chunk.
#[must_use]
pub fn mesh_chunk(
    source: &dyn TerrainSource,
    edits: &Edits,
    shape: ChunkShape,
    key: ChunkKey,
) -> VoxelMesh {
    let field = fill_chunk(source, edits, shape, key);
    let cell_size = shape.cell_size_at(key.lod);
    let mut mesh = surface_nets(&field);

    // Grid units to world units, and back off by the one cell of low apron. Done here rather than
    // by the caller so that "where is this chunk" is a single translation and cannot be got subtly
    // wrong per call site.
    for position in &mut mesh.positions {
        for value in position.iter_mut() {
            *value = (*value - 1.0) * cell_size;
        }
    }
    mesh
}

#[cfg(test)]
mod tests {
    use super::*;

    /// A ground plane with a bump in it, so the surface is not axis-aligned and a seam would show.
    #[derive(Debug)]
    struct RollingGround;

    impl TerrainSource for RollingGround {
        fn sample(&self, x: f32, y: f32, z: f32) -> f32 {
            y - (4.0 + (x * 0.35).sin() * 1.5 + (z * 0.27).cos() * 1.2)
        }
    }

    fn shape() -> ChunkShape {
        ChunkShape::new(8, 1.0)
    }

    /// Every vertex of a chunk, in absolute world coordinates.
    fn world_vertices(key: ChunkKey) -> Vec<[f32; 3]> {
        let origin = shape().origin_of(key);
        mesh_chunk(&RollingGround, &Edits::new(), shape(), key)
            .positions
            .iter()
            .map(|p| [p[0] + origin[0], p[1] + origin[1], p[2] + origin[2]])
            .collect()
    }

    #[test]
    fn a_flat_source_is_solid_below_and_empty_above() {
        // The sign convention, stated as a test because getting it backwards produces terrain that
        // is inside out and reads as invisible rather than as inverted.
        let ground = FlatGround { height: 10.0 };
        assert!(
            ground.sample(0.0, 9.0, 0.0) < 0.0,
            "below ground must be inside"
        );
        assert!(
            ground.sample(0.0, 11.0, 0.0) > 0.0,
            "above ground must be outside"
        );
    }

    #[test]
    fn a_chunks_field_carries_an_apron_on_both_sides() {
        // `cells + 2` samples, not `cells + 1`. The extra one is the low apron, and without it the
        // quads bridging this chunk to the previous one are emitted by nobody.
        let shape = ChunkShape::new(16, 1.0);
        assert_eq!(shape.samples_per_axis(), 18);

        let field = fill_chunk(
            &FlatGround { height: 4.0 },
            &Edits::new(),
            shape,
            ChunkKey::new(0, 0, 0),
        );
        assert_eq!(field.side(), shape.samples_per_axis());
        assert_eq!(field.cells(), shape.cells + 1);
    }

    #[test]
    fn two_adjacent_chunks_have_no_gap_between_them() {
        // **The claim this module exists to make.** Chunk 0 and chunk 1 are meshed independently,
        // in different jobs, possibly on different threads -- and their surfaces have to meet.
        //
        // A gap is invisible in a vertex count and invisible in a per-chunk test. It shows up only
        // when the two are put in the same coordinate system and something looks across the join.
        let left = world_vertices(ChunkKey::new(0, 0, 0));
        let right = world_vertices(ChunkKey::new(1, 0, 0));
        assert!(!left.is_empty() && !right.is_empty());

        let boundary = shape().chunk_size_at(0);

        // Vertices from the left chunk that sit in the last cell before the join, and vertices from
        // the right chunk in the first cell after it. If the surface is continuous these two bands
        // are one cell apart; if the bridging quads are missing, the geometry still *exists* but
        // nothing joins it -- so what this actually checks is that each side reaches the join.
        let left_reach = left
            .iter()
            .filter(|p| p[0] >= boundary - 1.0 && p[0] <= boundary)
            .count();
        let right_reach = right
            .iter()
            .filter(|p| p[0] >= boundary - 1.0 && p[0] <= boundary)
            .count();

        assert!(
            left_reach > 0,
            "the left chunk does not reach its own high boundary at x = {boundary}"
        );
        assert!(
            right_reach > 0,
            "the right chunk does not reach back to the join at x = {boundary}; \
             its low apron is missing and the bridging quads belong to nobody"
        );
    }

    #[test]
    fn neighbouring_chunks_agree_about_the_samples_they_share() {
        // The reason the seam closes at all: both chunks compute the shared sample plane from the
        // same source and the same edits, so they agree bit for bit rather than approximately.
        let shape = shape();
        let left = fill_chunk(&RollingGround, &Edits::new(), shape, ChunkKey::new(0, 0, 0));
        let right = fill_chunk(&RollingGround, &Edits::new(), shape, ChunkKey::new(1, 0, 0));

        // Field x index i in `left` is world sample (0 * 8 + i - 1); in `right` it is (8 + i - 1).
        // So left's index `8 + 1` and right's index `1` are the same world sample.
        for y in 0..shape.samples_per_axis() {
            for z in 0..shape.samples_per_axis() {
                let from_left = left.get(shape.cells + 1, y, z);
                let from_right = right.get(1, y, z);
                assert_eq!(
                    from_left, from_right,
                    "chunks disagree about the shared sample at y={y}, z={z}"
                );
            }
        }
    }

    #[test]
    fn an_edit_near_a_boundary_is_seen_by_both_chunks() {
        // Digging next to a seam is the case where a per-chunk edit store would open a hole, which
        // is why `Edits` is keyed by world sample coordinate rather than by chunk.
        let shape = shape();
        let mut edits = Edits::new();
        // The sample plane shared by chunk 0 and chunk 1.
        edits.set([8, 4, 3], -5.0);

        let left = fill_chunk(&RollingGround, &edits, shape, ChunkKey::new(0, 0, 0));
        let right = fill_chunk(&RollingGround, &edits, shape, ChunkKey::new(1, 0, 0));

        assert_eq!(left.get(shape.cells + 1, 5, 4), Some(-5.0));
        assert_eq!(right.get(1, 5, 4), Some(-5.0));
    }

    #[test]
    fn the_same_chunk_meshes_identically_every_time() {
        // I3, at chunk granularity. A terrain collider is gameplay state, so two machines meshing
        // the same chunk must agree bit for bit.
        let key = ChunkKey::new(-2, 0, 3);
        assert_eq!(
            mesh_chunk(&RollingGround, &Edits::new(), shape(), key),
            mesh_chunk(&RollingGround, &Edits::new(), shape(), key)
        );
    }

    #[test]
    fn a_chunk_is_placed_by_its_origin_and_nothing_else() {
        let shape = ChunkShape::new(8, 2.0);
        assert_eq!(shape.chunk_size_at(0), 16.0);
        assert_eq!(shape.origin_of(ChunkKey::new(3, 0, -1)), [48.0, 0.0, -16.0]);
    }

    #[test]
    fn a_coarser_chunk_covers_more_world_with_the_same_cell_count() {
        // The LOD hook, asserted so that it is a working piece of arithmetic rather than an unused
        // field. Q25 is still open; this is what it will be decided on top of.
        let shape = ChunkShape::new(16, 1.0);
        assert_eq!(shape.cell_size_at(0), 1.0);
        assert_eq!(shape.cell_size_at(1), 2.0);
        assert_eq!(shape.chunk_size_at(0), 16.0);
        assert_eq!(shape.chunk_size_at(1), 32.0);
    }

    #[test]
    fn an_untouched_world_stores_nothing() {
        // ADR 0042's central claim: a world nobody has dug costs nothing to store or to hash.
        assert!(Edits::new().is_empty());
        let mut edits = Edits::new();
        edits.set([1, 2, 3], -1.0);
        assert_eq!(edits.len(), 1);
        edits.clear([1, 2, 3]);
        assert!(
            edits.is_empty(),
            "clearing an edit returns the sample to the generator"
        );
    }

    #[test]
    fn edits_iterate_in_a_fixed_order() {
        // Anything that serialises or hashes edits walks this, and trap 2 names unordered iteration
        // as a determinism leak.
        let mut edits = Edits::new();
        for sample in [[5, 0, 0], [-3, 2, 1], [0, 0, 0], [5, 0, -1]] {
            edits.set(sample, 1.0);
        }
        let order: Vec<[i32; 3]> = edits.iter().map(|(key, _)| key).collect();
        assert_eq!(order, vec![[-3, 2, 1], [0, 0, 0], [5, 0, -1], [5, 0, 0]]);
    }
}
