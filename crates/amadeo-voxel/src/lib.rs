//! Turning a signed-distance field into a mesh — naive surface nets.
//!
//! # What this is for
//!
//! Smooth voxel terrain: ground you can dig into and that comes out rounded rather than blocky. It
//! is the **fourth producer of mesh data** in this engine, after a box, a plane and a glTF
//! primitive, and like the other three nothing above the loader can tell where the geometry came
//! from.
//!
//! # Why surface nets rather than marching cubes
//!
//! Both turn a field into a surface and neither can represent a sharp edge. Surface nets is simpler
//! to reason about, produces **fewer triangles** for the same field, and — importantly for chunked
//! terrain — needs no equivalent of marching cubes' TransVoxel machinery to stitch neighbouring
//! chunks together, because a chunk that samples one cell into its neighbour already agrees with it.
//!
//! What it gives up is sharp features: a 90° corner comes out rounded. That is the correct trade for
//! terrain and the wrong one for architecture, which is why buildings stay `BoxMesh` and glTF.
//!
//! # The apron, which is a data-shape constraint rather than a detail
//!
//! A cell's vertex is decided by its **eight corners**, so meshing the last cell of a chunk needs
//! samples that belong to the next chunk along. A chunk that samples only its own volume produces a
//! mesh with visible cracks at every seam.
//!
//! So [`Field`] is sized in *samples*, not cells, and a caller meshing a chunk of `n` cells fills an
//! `n + 1` sample grid — reaching one step into the neighbour on the high side. Getting this wrong
//! is the single most likely way chunked terrain looks broken, and it looks like a rendering bug.
//!
//! ```
//! use amadeo_voxel::{Field, surface_nets};
//!
//! // A sphere of radius 6, sampled over a 24-cell grid: distance from the centre, negative inside.
//! let mut field = Field::new(24);
//! let centre = 12.0;
//! field.fill(|x, y, z| {
//!     let (dx, dy, dz) = (x - centre, y - centre, z - centre);
//!     (dx * dx + dy * dy + dz * dz).sqrt() - 6.0
//! });
//!
//! let mesh = surface_nets(&field);
//! assert!(!mesh.positions.is_empty());
//! assert_eq!(mesh.indices.len() % 3, 0);
//! ```
//!
//! # Chunks
//!
//! [`chunk`] decides *which* chunks a world keeps loaded, from the positions of its viewers. It is
//! integer arithmetic because residency is gameplay state (ADR 0041 §2), and it turns the apron
//! constraint above into a checked property — [`Residency`] carries a `data` set one chunk larger
//! than its `visual` set, so the outermost drawn chunk always has a neighbour to mesh against.

//! [`terrain`] is where a chunk's samples come from — ADR 0042's generated base plus sparse edits —
//! and where one chunk is filled and meshed so that it meets its neighbours.

pub mod chunk;
pub mod terrain;

pub use chunk::{ChunkKey, Residency, Viewer};
pub use terrain::{ChunkShape, Edits, FlatGround, TerrainSource, fill_chunk, mesh_chunk};

/// A cubic grid of signed-distance samples.
///
/// **Negative is inside the surface, positive is outside**, and the surface is where the value
/// crosses zero. That is the usual convention and it is worth stating because the alternative
/// produces a mesh that is inside out — which reads as "the terrain is invisible" once back-face
/// culling is on.
///
/// Sized in **samples**, one more per axis than the number of cells it meshes. See the module docs
/// on the apron for why.
#[derive(Debug, Clone, PartialEq)]
pub struct Field {
    cells: usize,
    /// `(cells + 1)^3` samples, indexed x-fastest then y then z.
    samples: Vec<f32>,
}

impl Field {
    /// A field covering `cells` cells per axis, every sample far outside the surface.
    ///
    /// Starts positive rather than at zero: a field of zeroes is *entirely on the surface*, which is
    /// a degenerate case that meshes into nonsense. Starting outside means an unfilled field meshes
    /// into nothing, which is the honest answer.
    #[must_use]
    pub fn new(cells: usize) -> Self {
        let side = cells + 1;
        Self {
            cells,
            samples: vec![1.0; side * side * side],
        }
    }

    /// How many cells per axis this field meshes.
    #[must_use]
    pub fn cells(&self) -> usize {
        self.cells
    }

    /// How many samples per axis: one more than [`Field::cells`].
    #[must_use]
    pub fn side(&self) -> usize {
        self.cells + 1
    }

    /// Fills every sample from a function of its grid coordinate.
    ///
    /// Coordinates are `f32` and in **sample units**, so a caller placing a chunk in the world
    /// scales and offsets them itself. Keeping that out of here is what lets the same code mesh a
    /// chunk at the origin and one a kilometre away with no special case.
    ///
    /// Iterates z, then y, then x — a fixed order, so a field built from a function is built the
    /// same way on every machine (invariant I3).
    pub fn fill(&mut self, mut sample: impl FnMut(f32, f32, f32) -> f32) {
        let side = self.side();
        for z in 0..side {
            for y in 0..side {
                for x in 0..side {
                    self.samples[x + side * (y + side * z)] = sample(x as f32, y as f32, z as f32);
                }
            }
        }
    }

    /// One sample, or `None` if the coordinate is outside the grid.
    #[must_use]
    pub fn get(&self, x: usize, y: usize, z: usize) -> Option<f32> {
        let side = self.side();
        if x >= side || y >= side || z >= side {
            return None;
        }
        self.samples.get(x + side * (y + side * z)).copied()
    }

    /// Sets one sample. Out-of-range coordinates are ignored rather than panicking.
    pub fn set(&mut self, x: usize, y: usize, z: usize, value: f32) {
        let side = self.side();
        if x >= side || y >= side || z >= side {
            return;
        }
        self.samples[x + side * (y + side * z)] = value;
    }

    /// Reads a sample without bounds checking the caller has already done.
    fn at(&self, x: usize, y: usize, z: usize) -> f32 {
        let side = self.side();
        self.samples[x + side * (y + side * z)]
    }
}

/// A mesh produced from a field.
///
/// Plain arrays rather than an engine type: this crate sits below `amadeo-render` and cannot name
/// `MeshData` (invariant I6), exactly as `amadeo-gltf` cannot. The layer that can see both converts.
#[derive(Debug, Clone, Default, PartialEq)]
pub struct VoxelMesh {
    /// One per surface cell, in grid units.
    pub positions: Vec<[f32; 3]>,
    /// Unit normals, from the field's gradient.
    pub normals: Vec<[f32; 3]>,
    /// Three per triangle, counter-clockwise from outside — the winding the whole engine uses.
    pub indices: Vec<u32>,
}

impl VoxelMesh {
    /// Whether anything was produced. An empty mesh is the correct answer for a field that never
    /// crosses zero, not a failure.
    #[must_use]
    pub fn is_empty(&self) -> bool {
        self.indices.is_empty()
    }

    /// How many triangles.
    #[must_use]
    pub fn triangles(&self) -> usize {
        self.indices.len() / 3
    }
}

/// The twelve edges of a cell, as pairs of corner indices.
///
/// Corner `i` is at offset `(i & 1, (i >> 1) & 1, (i >> 2) & 1)`, so this table and the offset
/// arithmetic below have to agree — which is why the offsets are derived from the index rather than
/// written out as a second table that could drift from this one.
const EDGES: [(usize, usize); 12] = [
    (0, 1),
    (2, 3),
    (4, 5),
    (6, 7), // along x
    (0, 2),
    (1, 3),
    (4, 6),
    (5, 7), // along y
    (0, 4),
    (1, 5),
    (2, 6),
    (3, 7), // along z
];

/// Meshes a field with naive surface nets.
///
/// One vertex per cell the surface passes through, placed at the average of where the surface cuts
/// that cell's edges. Neighbouring vertices are joined into quads across every grid edge whose two
/// ends straddle the surface.
///
/// # Determinism
///
/// Cells are visited in a fixed order and every sum is accumulated in that order, so the same field
/// produces byte-identical output on every machine — which matters because a terrain collider is
/// gameplay state (ADR 0041). `the_same_field_always_meshes_identically` pins it.
#[must_use]
pub fn surface_nets(field: &Field) -> VoxelMesh {
    let cells = field.cells();
    let mut mesh = VoxelMesh::default();
    if cells == 0 {
        return mesh;
    }

    // Which vertex each cell produced, or `usize::MAX` for a cell the surface misses. A flat array
    // rather than a map: it is indexed once per quad corner and a hash lookup there would dominate.
    const NONE: usize = usize::MAX;
    let mut vertex_at = vec![NONE; cells * cells * cells];
    let cell_index = |x: usize, y: usize, z: usize| x + cells * (y + cells * z);

    // --- Pass one: a vertex for every cell the surface crosses. ---
    for z in 0..cells {
        for y in 0..cells {
            for x in 0..cells {
                let mut corner = [0.0_f32; 8];
                for (index, value) in corner.iter_mut().enumerate() {
                    *value = field.at(
                        x + (index & 1),
                        y + ((index >> 1) & 1),
                        z + ((index >> 2) & 1),
                    );
                }

                // Entirely inside or entirely outside: no surface here. `is_sign_negative` rather
                // than `< 0.0` so that a sample of exactly -0.0 counts as inside, which keeps a
                // field built by subtraction from having a seam at exactly zero.
                let inside = corner[0].is_sign_negative();
                if corner.iter().all(|v| v.is_sign_negative() == inside) {
                    continue;
                }

                // Average of the edge crossings. Summed in the fixed order of `EDGES`, because
                // floating-point addition is not associative and a different order is a different
                // vertex.
                let mut sum = [0.0_f32; 3];
                let mut crossings = 0.0_f32;
                for (a, b) in EDGES {
                    let (va, vb) = (corner[a], corner[b]);
                    if va.is_sign_negative() == vb.is_sign_negative() {
                        continue;
                    }
                    // Where along the edge the value passes through zero. Guarded because two
                    // samples that differ in sign but not in value would divide by zero.
                    let span = va - vb;
                    let t = if span.abs() < f32::EPSILON {
                        0.5
                    } else {
                        va / span
                    };
                    let start = [(a & 1) as f32, ((a >> 1) & 1) as f32, ((a >> 2) & 1) as f32];
                    let end = [(b & 1) as f32, ((b >> 1) & 1) as f32, ((b >> 2) & 1) as f32];
                    for axis in 0..3 {
                        sum[axis] += start[axis] + (end[axis] - start[axis]) * t;
                    }
                    crossings += 1.0;
                }

                if crossings == 0.0 {
                    continue;
                }

                vertex_at[cell_index(x, y, z)] = mesh.positions.len();
                mesh.positions.push([
                    x as f32 + sum[0] / crossings,
                    y as f32 + sum[1] / crossings,
                    z as f32 + sum[2] / crossings,
                ]);
                mesh.normals.push(gradient_at(field, x, y, z));
            }
        }
    }

    // --- Pass two: join neighbouring vertices into quads. ---
    //
    // A quad exists across every grid edge whose two ends straddle the surface, joining the four
    // cells that share it. Only edges starting at a cell with both other coordinates at least 1 have
    // all four neighbours inside the grid, which is what the range below encodes.
    for z in 0..cells {
        for y in 0..cells {
            for x in 0..cells {
                if vertex_at[cell_index(x, y, z)] == NONE {
                    continue;
                }
                let here = field.at(x, y, z);
                let inside = here.is_sign_negative();

                // The edge along +x, and the four cells around it.
                if y > 0 && z > 0 && field.at(x + 1, y, z).is_sign_negative() != inside {
                    push_quad(
                        &mut mesh.indices,
                        &vertex_at,
                        [
                            cell_index(x, y - 1, z - 1),
                            cell_index(x, y, z - 1),
                            cell_index(x, y, z),
                            cell_index(x, y - 1, z),
                        ],
                        !inside,
                    );
                }
                if x > 0 && z > 0 && field.at(x, y + 1, z).is_sign_negative() != inside {
                    push_quad(
                        &mut mesh.indices,
                        &vertex_at,
                        [
                            cell_index(x - 1, y, z - 1),
                            cell_index(x, y, z - 1),
                            cell_index(x, y, z),
                            cell_index(x - 1, y, z),
                        ],
                        inside,
                    );
                }
                if x > 0 && y > 0 && field.at(x, y, z + 1).is_sign_negative() != inside {
                    push_quad(
                        &mut mesh.indices,
                        &vertex_at,
                        [
                            cell_index(x - 1, y - 1, z),
                            cell_index(x, y - 1, z),
                            cell_index(x, y, z),
                            cell_index(x - 1, y, z),
                        ],
                        !inside,
                    );
                }
            }
        }
    }

    mesh
}

/// Emits two triangles for one quad, skipping it if any corner cell has no vertex.
///
/// `flip` reverses the winding. Which way round a quad goes depends on which side of the surface the
/// edge's start is on.
///
/// # This was wrong on all three axes, and the symptom was not "holes"
///
/// The comment that used to sit here predicted the failure as *half* the terrain going missing. It
/// was worse and stranger than that: the flip was inverted on every axis, so **every mesh was
/// uniformly inside-out** — visible from underneath and invisible from above. On a heightfield that
/// reads as an empty world with a faint wisp at the horizon, which looks like a *streaming* bug, and
/// three sessions of work went past it.
///
/// It survived because the mesher's own tests checked the **normals**, which come from the field's
/// gradient and were always right, and the GPU decides which face you are seeing from the
/// **winding**, which nothing checked. Nothing had ever drawn a surface-nets mesh either — the
/// collider path has no winding at all. `triangles_are_wound_to_match_their_own_normals` compares
/// the two against each other and is what closes that gap.
fn push_quad(indices: &mut Vec<u32>, vertex_at: &[usize], cells: [usize; 4], flip: bool) {
    let mut corners = [0_u32; 4];
    for (slot, cell) in cells.iter().enumerate() {
        match vertex_at.get(*cell).copied() {
            Some(index) if index != usize::MAX => corners[slot] = index as u32,
            // A neighbour with no vertex means the grid edge straddles the surface but one of the
            // cells around it does not — which happens at the field's boundary. Dropping the quad
            // leaves a hole there, and the apron is what stops that hole being visible.
            _ => return,
        }
    }

    let order = if flip {
        [[0, 3, 2], [0, 2, 1]]
    } else {
        [[0, 1, 2], [0, 2, 3]]
    };
    for triangle in order {
        for corner in triangle {
            indices.push(corners[corner]);
        }
    }
}

/// The field's gradient at a cell, normalised — which is the surface normal.
///
/// Central differences across the cell rather than a face normal: it gives a smooth normal per
/// vertex for free, which is the whole visual point of surface nets over blocky voxels.
fn gradient_at(field: &Field, x: usize, y: usize, z: usize) -> [f32; 3] {
    // Differences between the two opposite faces of the cell, so this reads only samples the cell
    // already owns and never leaves the grid.
    let mut low = [0.0_f32; 3];
    let mut high = [0.0_f32; 3];
    for index in 0..8 {
        let value = field.at(
            x + (index & 1),
            y + ((index >> 1) & 1),
            z + ((index >> 2) & 1),
        );
        for (axis, shift) in [0_usize, 1, 2].into_iter().enumerate() {
            if (index >> shift) & 1 == 1 {
                high[axis] += value;
            } else {
                low[axis] += value;
            }
        }
    }

    let raw = [high[0] - low[0], high[1] - low[1], high[2] - low[2]];
    let length = (raw[0] * raw[0] + raw[1] * raw[1] + raw[2] * raw[2]).sqrt();
    if length < 1e-6 {
        // A perfectly flat patch of field has no gradient to normalise. Up is an arbitrary but
        // stable answer, and far better than the NaN that dividing would produce — one NaN normal
        // spreads through every lighting calculation it touches.
        [0.0, 1.0, 0.0]
    } else {
        [raw[0] / length, raw[1] / length, raw[2] / length]
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    /// A sphere of `radius` centred in a field of `cells` cells.
    fn sphere(cells: usize, radius: f32) -> Field {
        let mut field = Field::new(cells);
        let centre = cells as f32 / 2.0;
        field.fill(|x, y, z| {
            let (dx, dy, dz) = (x - centre, y - centre, z - centre);
            (dx * dx + dy * dy + dz * dz).sqrt() - radius
        });
        field
    }

    #[test]
    fn a_field_that_never_crosses_zero_meshes_into_nothing() {
        // The honest answer for empty space or solid rock, and the common case for most chunks of a
        // real world — so it has to be cheap and it has to be empty, not degenerate.
        let field = Field::new(8);
        assert!(surface_nets(&field).is_empty());

        let mut solid = Field::new(8);
        solid.fill(|_, _, _| -1.0);
        assert!(surface_nets(&solid).is_empty());
    }

    #[test]
    fn a_sphere_meshes_into_a_sphere() {
        // **The claim that matters.** Every vertex should sit close to the radius — a meshing bug
        // that produced the right *number* of vertices in the wrong places would pass a count check
        // and fail this.
        let cells = 32;
        let radius = 10.0;
        let mesh = surface_nets(&sphere(cells, radius));
        let centre = cells as f32 / 2.0;

        assert!(!mesh.is_empty());
        for position in &mesh.positions {
            let (dx, dy, dz) = (
                position[0] - centre,
                position[1] - centre,
                position[2] - centre,
            );
            let distance = (dx * dx + dy * dy + dz * dz).sqrt();
            assert!(
                (distance - radius).abs() < 1.0,
                "vertex at {position:?} is {distance:.2} from the centre, not {radius}"
            );
        }
    }

    #[test]
    fn normals_point_away_from_the_inside() {
        // Negative is inside, so a sphere's normals must point outward. Inverted normals are the
        // classic sign-convention bug and they read as "the terrain is unlit" rather than as a
        // normal problem.
        let cells = 24;
        let mesh = surface_nets(&sphere(cells, 8.0));
        let centre = cells as f32 / 2.0;

        for (position, normal) in mesh.positions.iter().zip(&mesh.normals) {
            let outward = [
                position[0] - centre,
                position[1] - centre,
                position[2] - centre,
            ];
            let dot = outward[0] * normal[0] + outward[1] * normal[1] + outward[2] * normal[2];
            assert!(dot > 0.0, "normal {normal:?} at {position:?} points inward");
        }
    }

    #[test]
    fn triangles_are_wound_to_match_their_own_normals() {
        // **The test that was missing, and the defect it was written against was total.**
        //
        // `normals_point_away_from_the_inside` above checks the *normals*, which come from the
        // field's gradient and were always right. The GPU does not use them to decide which side of
        // a triangle you are looking at — it uses the **winding**, the order the three corners are
        // listed in. Those were inverted on all three axes, so every surface-nets mesh ever produced
        // was inside-out.
        //
        // Nothing caught it for two sessions because nothing had ever *drawn* one: the mesher's own
        // tests assert on vertices and normals, the streamer's on which chunks exist, and the
        // physics ones on a collider, which has no winding. It surfaced the first time terrain
        // reached a camera -- as ground that is invisible from above and faintly visible at the
        // horizon, which reads as a streaming bug rather than a geometry one.
        //
        // Comparing the geometric winding against the stored normal is what makes this checkable
        // without a GPU, and it is the same check `amadeo-render`'s `every_box_triangle_faces_outward`
        // makes for a box.
        let mesh = surface_nets(&sphere(24, 8.0));
        assert!(!mesh.indices.is_empty());

        let mut wrong = 0;
        for triangle in mesh.indices.chunks_exact(3) {
            let corner = |slot: usize| mesh.positions[triangle[slot] as usize];
            let (a, b, c) = (corner(0), corner(1), corner(2));
            let u = [b[0] - a[0], b[1] - a[1], b[2] - a[2]];
            let v = [c[0] - a[0], c[1] - a[1], c[2] - a[2]];
            // The cross product is the direction the GPU considers "front" for this winding.
            let facing = [
                u[1] * v[2] - u[2] * v[1],
                u[2] * v[0] - u[0] * v[2],
                u[0] * v[1] - u[1] * v[0],
            ];
            let normal = mesh.normals[triangle[0] as usize];
            let dot = facing[0] * normal[0] + facing[1] * normal[1] + facing[2] * normal[2];
            if dot < 0.0 {
                wrong += 1;
            }
        }

        assert_eq!(
            wrong,
            0,
            "{wrong} of {} triangles are wound against their own normals; \
             the surface is inside-out and back-face culling will hide it",
            mesh.indices.len() / 3
        );
    }

    #[test]
    fn flat_ground_faces_upward() {
        // The heightfield case stated on its own, because it is the one a terrain game depends on
        // and because it is unambiguous: ground is solid below and air above, so every triangle must
        // face up. A sphere has every orientation at once, which makes a *uniform* inversion easy to
        // misread there; here it is a single sign.
        let cells = 12;
        let mut field = Field::new(cells);
        // Solid below the halfway line, air above it. Same convention as `FlatGround`.
        field.fill(|_, y, _| y - cells as f32 / 2.0);
        let mesh = surface_nets(&field);
        assert!(!mesh.indices.is_empty(), "a plane through the field meshes");

        for triangle in mesh.indices.chunks_exact(3) {
            let corner = |slot: usize| mesh.positions[triangle[slot] as usize];
            let (a, b, c) = (corner(0), corner(1), corner(2));
            let u = [b[0] - a[0], b[1] - a[1], b[2] - a[2]];
            let v = [c[0] - a[0], c[1] - a[1], c[2] - a[2]];
            let facing_up = u[2] * v[0] - u[0] * v[2];
            assert!(
                facing_up > 0.0,
                "a ground triangle at {a:?} faces downward; \
                 seen from above it is a back face and is culled, so the world looks empty"
            );
        }
    }

    #[test]
    fn every_triangle_is_whole_and_indexes_a_real_vertex() {
        let mesh = surface_nets(&sphere(20, 7.0));
        assert_eq!(mesh.indices.len() % 3, 0, "indices must be whole triangles");
        assert_eq!(mesh.positions.len(), mesh.normals.len());
        for index in &mesh.indices {
            assert!(
                (*index as usize) < mesh.positions.len(),
                "index {index} is past the end of {} vertices",
                mesh.positions.len()
            );
        }
    }

    #[test]
    fn the_same_field_always_meshes_identically() {
        // I3. A terrain collider is gameplay state (ADR 0041), so two machines meshing the same
        // chunk must agree bit for bit — not merely produce similar surfaces.
        let field = sphere(24, 9.0);
        assert_eq!(surface_nets(&field), surface_nets(&field));
    }

    #[test]
    fn a_plane_meshes_into_a_flat_sheet() {
        // A horizontal surface is what most terrain mostly is, and it is the case where an off-by-one
        // in the quad loops shows up as a mesh half the size it should be.
        let cells = 16;
        let mut field = Field::new(cells);
        field.fill(|_, y, _| y - 8.0);
        let mesh = surface_nets(&field);

        assert!(!mesh.is_empty());
        for position in &mesh.positions {
            assert!(
                (position[1] - 8.0).abs() < 0.6,
                "a flat field should mesh flat, got {position:?}"
            );
        }
        // One vertex per cell of the 16x16 sheet, give or take the boundary.
        assert!(
            mesh.positions.len() >= cells * cells / 2,
            "expected roughly a sheet of vertices, got {}",
            mesh.positions.len()
        );
    }

    #[test]
    fn editing_a_sample_changes_the_mesh() {
        // The property destructible terrain rests on: the field is data, and changing it changes the
        // surface. Cheap to assert now and expensive to discover missing later.
        let mut field = sphere(16, 5.0);
        let before = surface_nets(&field);
        field.set(8, 8, 8, 1.0);
        assert_ne!(surface_nets(&field), before);
    }

    #[test]
    fn a_zero_cell_field_is_not_a_panic() {
        assert!(surface_nets(&Field::new(0)).is_empty());
    }
}
