//! Meshes and materials — ADR 0035 for what a mesh asset is, ADR 0033 for where a material lives.
//!
//! # The two kinds of mesh, and the one type they meet in
//!
//! A mesh asset's file is a scene document with a single root, exactly as a prefab and a material
//! are. It carries **either** a procedural shape ([`BoxMesh`], [`PlaneMesh`]) **or** vertex data from
//! an import. Both produce a [`MeshData`], and nothing above the loader can tell which it came from.
//!
//! That shared type is the load-bearing part of ADR 0035, and it is ADR 0026's `PixelFormat`
//! argument reused: it makes the glTF importer a new *producer* rather than a change to the
//! component, the cache, the backend, and every test that asserts on geometry.
//!
//! ```text
//!   BoxMesh { size }  ──tessellate──┐
//!                                    ├──►  MeshData { positions, normals, uvs, indices }
//!   a .glb            ──import──────┘
//! ```
//!
//! # Why a shape is a reflected component rather than a Rust constructor
//!
//! Invariant I1. A box described as three numbers is hand-writable, diffable text; a `.glb` is
//! opaque bytes. Making shapes assets is what lets a 3D level be authored — by a person or by the
//! agent — with no toolchain and no binary, which is the same reach ADR 0031 gave the camera.

use amadeo_core::StableHash;
use amadeo_ecs::{Component, Service};
use amadeo_reflect::Reflect;
use std::collections::BTreeMap;

/// One vertex, as everything downstream sees it.
///
/// **The layout is fixed** (ADR 0035 §3): position, normal, texture coordinate, and nothing else. A
/// configurable layout would mean a shader per permutation, and ADR 0033 already chose
/// defines-plus-a-pipeline-cache over that kind of generality for the same reason — one person
/// maintains this.
///
/// Tangents are **not** here. When normal mapping needs them they are generated at load from the
/// UVs, which is what glTF itself permits for a model that omits them.
#[derive(Debug, Clone, Copy, PartialEq, Default)]
pub struct Vertex {
    /// Position in the mesh's own space, before any transform.
    pub position: [f32; 3],
    /// Unit-length surface normal, in the same space.
    pub normal: [f32; 3],
    /// Texture coordinate. `[0, 0]` is the **top-left** of an image, matching `sprite.wgsl`.
    pub uv: [f32; 2],
}

/// Geometry, ready for the GPU.
///
/// Indexed triangles: `indices` names vertices three at a time, **counter-clockwise when seen from
/// the front**. That winding is the convention the whole engine uses, and it is what decides which
/// side of a triangle is lit — so it is asserted per shape rather than assumed.
#[derive(Debug, Clone, PartialEq, Default)]
pub struct MeshData {
    /// The vertices, in no particular order — `indices` decides the triangles.
    pub vertices: Vec<Vertex>,
    /// Three per triangle, counter-clockwise from the front.
    pub indices: Vec<u32>,
}

impl MeshData {
    /// How many triangles this holds.
    #[must_use]
    pub fn triangle_count(&self) -> usize {
        self.indices.len() / 3
    }

    /// The smallest axis-aligned box containing every vertex, as `(min, max)`.
    ///
    /// `None` for a mesh with no vertices, because an empty box has no honest corners — returning
    /// zeros would put a degenerate box at the origin and quietly claim something is there.
    ///
    /// # What needs this, and why it lives on the data rather than in a caller
    ///
    /// Two things, and both would otherwise compute it themselves and drift:
    ///
    /// - **Frustum culling** (M2.5 gate 3) tests this box against the camera's frustum to decide
    ///   whether a mesh can be skipped. Every mesh, every frame, so it must be exactly the bounds the
    ///   renderer draws — a box that is too small culls things that are on screen, which is a
    ///   flickering disappearance rather than an error.
    /// - **`render.describe`** (**Q26**) projects it to report what a mesh covers on screen, which is
    ///   how an agent answers "is it visible" for 3D without a picture.
    ///
    /// In mesh space, before any transform: the same space `vertices` is in. A caller with a model
    /// matrix transforms the eight corners, which is correct under rotation where transforming the
    /// two extremes alone is not.
    #[must_use]
    pub fn bounds(&self) -> Option<([f32; 3], [f32; 3])> {
        let first = self.vertices.first()?.position;
        let mut min = first;
        let mut max = first;

        for vertex in &self.vertices[1..] {
            for axis in 0..3 {
                min[axis] = min[axis].min(vertex.position[axis]);
                max[axis] = max[axis].max(vertex.position[axis]);
            }
        }
        Some((min, max))
    }

    /// Whether every index names a vertex that exists.
    ///
    /// Cheap, and worth having: an out-of-range index is a GPU validation error at draw time, which
    /// is a long way from the tessellation or import that produced it.
    #[must_use]
    pub fn is_well_formed(&self) -> bool {
        self.indices.len().is_multiple_of(3)
            && self
                .indices
                .iter()
                .all(|index| (*index as usize) < self.vertices.len())
    }
}

/// A rectangular box, centred on its entity's origin.
///
/// The workhorse of a blocked-out level: floors, walls, crates, platforms. Three numbers in a text
/// file, which is the whole argument ADR 0035 turned on.
#[derive(Debug, Clone, Copy, PartialEq, StableHash, Reflect)]
pub struct BoxMesh {
    /// Full width, height and depth in the mesh's own space.
    #[reflect(min = 0.0, max = 10000.0, unit = "world units")]
    pub size: [f32; 3],
}

impl Default for BoxMesh {
    fn default() -> Self {
        Self {
            size: [1.0, 1.0, 1.0],
        }
    }
}

impl Component for BoxMesh {}

impl BoxMesh {
    /// Turns the parameters into geometry.
    ///
    /// Twenty-four vertices rather than eight, because each corner needs a **different normal** on
    /// each of the three faces meeting there. Sharing eight would average them and turn a box into
    /// something that shades like a sphere — the classic first mistake, and the reason
    /// `a_box_has_flat_faces` asserts on normals rather than on vertex count.
    #[must_use]
    pub fn tessellate(&self) -> MeshData {
        let [x, y, z] = [self.size[0] / 2.0, self.size[1] / 2.0, self.size[2] / 2.0];

        // Each face as (normal, four corners counter-clockwise seen from outside).
        let faces: [([f32; 3], [[f32; 3]; 4]); 6] = [
            // +Z, front
            (
                [0.0, 0.0, 1.0],
                [[-x, -y, z], [x, -y, z], [x, y, z], [-x, y, z]],
            ),
            // -Z, back
            (
                [0.0, 0.0, -1.0],
                [[x, -y, -z], [-x, -y, -z], [-x, y, -z], [x, y, -z]],
            ),
            // +X, right
            (
                [1.0, 0.0, 0.0],
                [[x, -y, z], [x, -y, -z], [x, y, -z], [x, y, z]],
            ),
            // -X, left
            (
                [-1.0, 0.0, 0.0],
                [[-x, -y, -z], [-x, -y, z], [-x, y, z], [-x, y, -z]],
            ),
            // +Y, top
            (
                [0.0, 1.0, 0.0],
                [[-x, y, z], [x, y, z], [x, y, -z], [-x, y, -z]],
            ),
            // -Y, bottom
            (
                [0.0, -1.0, 0.0],
                [[-x, -y, -z], [x, -y, -z], [x, -y, z], [-x, -y, z]],
            ),
        ];

        let mut data = MeshData::default();
        for (normal, corners) in faces {
            let first = data.vertices.len() as u32;
            // UVs run [0,0] at the first corner round to [0,1], so each face shows the whole image.
            for (corner, uv) in corners
                .iter()
                .zip([[0.0, 1.0], [1.0, 1.0], [1.0, 0.0], [0.0, 0.0]])
            {
                data.vertices.push(Vertex {
                    position: *corner,
                    normal,
                    uv,
                });
            }
            // Two triangles per face, both counter-clockwise from outside.
            data.indices
                .extend([first, first + 1, first + 2, first, first + 2, first + 3]);
        }
        data
    }
}

/// A flat rectangle lying in the XZ plane, facing up.
///
/// The other half of blocking out a level: ground, ceilings, water. Lying flat rather than standing
/// up because a floor is what it is nearly always used for, and a wall is a [`BoxMesh`] with a small
/// depth — which has thickness, and therefore does not vanish when seen edge-on.
#[derive(Debug, Clone, Copy, PartialEq, StableHash, Reflect)]
pub struct PlaneMesh {
    /// Full extent along X and Z.
    #[reflect(min = 0.0, max = 10000.0, unit = "world units")]
    pub size: [f32; 2],
}

impl Default for PlaneMesh {
    fn default() -> Self {
        Self { size: [1.0, 1.0] }
    }
}

impl Component for PlaneMesh {}

impl PlaneMesh {
    /// Turns the parameters into geometry: two triangles, normal pointing up.
    #[must_use]
    pub fn tessellate(&self) -> MeshData {
        let (x, z) = (self.size[0] / 2.0, self.size[1] / 2.0);
        let up = [0.0, 1.0, 0.0];

        // Counter-clockwise seen from above, which is from +Y looking down.
        let corners = [[-x, 0.0, z], [x, 0.0, z], [x, 0.0, -z], [-x, 0.0, -z]];
        let uvs = [[0.0, 1.0], [1.0, 1.0], [1.0, 0.0], [0.0, 0.0]];

        MeshData {
            vertices: corners
                .iter()
                .zip(uvs)
                .map(|(position, uv)| Vertex {
                    position: *position,
                    normal: up,
                    uv,
                })
                .collect(),
            indices: vec![0, 1, 2, 0, 2, 3],
        }
    }
}

/// What a surface is made of — ADR 0033.
///
/// **An asset named by an id**, because a material is shared by construction: the Vault's forty-four
/// walls use one, and inline data would be forty-four copies in every state hash and every snapshot.
///
/// The field list is the **metallic-roughness** model, which is what glTF 2.0 defines — chosen so
/// that the importer ADR 0035 anticipates maps onto it directly rather than through a translation
/// nobody can predict the losses of.
#[derive(Debug, Clone, PartialEq, StableHash, Reflect)]
pub struct Material {
    /// The surface colour, linear RGBA. Multiplied with [`Material::base_colour_texture`].
    #[reflect(min = 0.0, max = 1.0)]
    pub base_colour: [f32; 4],
    /// `0.0` is a dielectric — wood, plastic, stone. `1.0` is bare metal. In-between is rare and
    /// usually means a blend mask rather than a real surface.
    #[reflect(min = 0.0, max = 1.0)]
    pub metallic: f32,
    /// `0.0` is a mirror, `1.0` is completely diffuse.
    #[reflect(min = 0.0, max = 1.0)]
    pub roughness: f32,
    /// Light this surface emits on its own, linear RGB. Black means none.
    ///
    /// Not clamped to 1.0: a value above it is what makes something register as a *source* once
    /// bloom is drawing, which is the point of the HDR target ADR 0034 introduced.
    #[reflect(min = 0.0, max = 100.0)]
    pub emissive: [f32; 3],
    /// Declared asset id of the base colour texture. **Empty means none**, matching
    /// [`Camera::environment`](crate::Camera) and `Sprite::texture`.
    pub base_colour_texture: String,
}

impl Default for Material {
    /// A plain, mid-rough, non-metallic white surface.
    ///
    /// Hand-written rather than derived for the same reason [`Environment`](crate::Environment)'s is:
    /// a derived default would be black, fully smooth and fully transparent, which is not a material
    /// so much as an absence of one.
    fn default() -> Self {
        Self {
            base_colour: [1.0, 1.0, 1.0, 1.0],
            metallic: 0.0,
            roughness: 0.5,
            emissive: [0.0, 0.0, 0.0],
            base_colour_texture: String::new(),
        }
    }
}

impl Component for Material {}

/// A thing to draw in 3D: which geometry, and what it is made of.
///
/// Both are **asset ids** (ADR 0033), so the component reads the same in a `.scene`, in a
/// `world.entity` dump and in memory. An id that has not loaded is not an error and not a branch —
/// ADR 0021 again, and the renderer draws nothing for a mesh it does not have while saying so.
/// Both fields default to empty, which is how every asset-id field in the engine spells "none" —
/// so unlike [`Material`] and [`Environment`](crate::Environment), whose "off" values are the
/// identities of their operations rather than zero, this one is genuinely derivable.
#[derive(Debug, Clone, Default, PartialEq, StableHash, Reflect)]
pub struct Mesh {
    /// Declared asset id of the geometry.
    pub mesh: String,
    /// Declared asset id of the material. Empty means [`Material::default`].
    pub material: String,
}

impl Component for Mesh {}

impl Mesh {
    /// A mesh drawing one geometry with one material.
    #[must_use]
    pub fn new(mesh: impl Into<String>, material: impl Into<String>) -> Self {
        Self {
            mesh: mesh.into(),
            material: material.into(),
        }
    }
}

/// One piece of geometry inside a glTF file — ADR 0039.
///
/// **The third producer of [`MeshData`]**, alongside [`BoxMesh`] and [`PlaneMesh`], and exactly the
/// additive change ADR 0035 was written to buy: nothing above the loader knows where a mesh came
/// from.
///
/// # Why this indirection exists rather than a field on `Mesh`
///
/// A glTF file holds many meshes and a `Mesh` component draws one. Something has to say *which*.
/// The alternatives were a compound id string (`"level#3"`), which hides structure inside a name —
/// the exact defect ADR 0030 called out when an array's length existed only inside its type name —
/// or a new field on `Mesh`, which every existing scene file would have had to grow.
///
/// This is a third option and a better one: a `.mesh` asset file already *is* the indirection, so it
/// carries this instead of a `BoxMesh`. Mesh ids stay flat, `Mesh` is untouched, and the mapping
/// from a name to a piece of a file is a two-line text file anyone can read:
///
/// ```text
/// scene chair_seat
/// version 1
///
/// entity mesh "Chair seat"
///   GltfPart
///     mesh 0
///     primitive 0
///     source "chair_glb"
/// ```
#[derive(Debug, Clone, PartialEq, Default, StableHash, Reflect)]
pub struct GltfPart {
    /// The declared asset id of the `.glb` or `.gltf` file (ADR 0020).
    pub source: String,
    /// Which mesh of that file, by index.
    ///
    /// An index rather than a name because glTF does not require names to exist or to be unique,
    /// and an importer that invented them would be inventing the thing the file is addressed by.
    /// The generated file's own *scene name* carries the readable version.
    #[reflect(min = 0.0, max = 1000000.0)]
    pub mesh: u32,
    /// Which primitive within that mesh, by index.
    ///
    /// A glTF mesh holds one primitive per material. This is the level Amadeo's `Mesh` corresponds
    /// to — treating a whole glTF mesh as one Amadeo mesh silently loses every material but the
    /// first.
    #[reflect(min = 0.0, max = 1000000.0)]
    pub primitive: u32,
}

impl Component for GltfPart {}

/// How a light casts shadows, or whether it does — ADR 0038.
///
/// # An enum rather than a `bool`, and why it ships with two variants
///
/// The same argument [`PixelFormat`](amadeo_image::PixelFormat) shipped with under ADR 0026, and the
/// render graph's own internal `TargetFormat` under ADR 0034: **the tag is the load-bearing part.**
/// The
/// expensive thing about shadows is not the algorithm — `RenderBackend` isolates that completely —
/// it is the field on an authored, hashed component that scene files carry. Getting that shape right
/// once means cascades arrive as a new variant rather than as a change to every scene that has a sun
/// in it.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default, StableHash, Reflect)]
pub enum ShadowMode {
    /// No shadows. Everything is lit purely by how it faces the light.
    ///
    /// The default, and not only to save the work: a scene lit with no shadows looks flat but
    /// *correct*, where a scene with a badly-fitted shadow map looks broken. Opting in is the
    /// safer direction.
    #[default]
    Off,
    /// One shadow map covering a box centred on the camera.
    ///
    /// Godot calls this "Orthogonal" and ships it as a real mode rather than a stepping stone, which
    /// is what this name follows. It is the cheapest directional shadow and the right one for
    /// interiors and small scenes, where everything that matters is close by.
    ///
    /// Its limitation is inherent and worth stating plainly: one map stretched over a large outdoor
    /// scene gives every shadow-map pixel a lot of ground to cover, and edges go visibly blocky.
    /// That is what cascades — splitting the camera's range into slices with a map each — exist to
    /// fix, and where they will arrive as a third variant.
    Orthogonal,
}

/// A light shining from a direction rather than from a place — the sun, or the moon.
///
/// **An entity, following ADR 0031's precedent for the camera**: a world holds any number, a scene
/// file authors them, and parenting one to something is how it follows. Its *direction* comes from
/// the [`Transform`](amadeo_transform::Transform) on the same entity, exactly as a camera's position
/// does — a light points along its own **negative Z**, which is the same convention a camera looks
/// along, so "aim it like a camera" is literally true.
///
/// Point lights, which fall off with distance and need a position, are still to come — M3's horror
/// slice is what actually needs them.
#[derive(Debug, Clone, Copy, PartialEq, StableHash, Reflect)]
pub struct DirectionalLight {
    /// Linear RGB. White is the neutral choice; warm and cool are what sell a time of day.
    #[reflect(min = 0.0, max = 1.0)]
    pub colour: [f32; 3],
    /// How bright, multiplied into the colour.
    ///
    /// Not capped at 1.0, because the scene target is high dynamic range since ADR 0034 — a value
    /// above it is what gives tonemapping something to compress.
    #[reflect(min = 0.0, max = 100.0)]
    pub intensity: f32,
    /// Whether and how this light casts shadows (ADR 0038).
    pub shadows: ShadowMode,
    /// How far from the camera shadows are drawn, in world units.
    ///
    /// **The single most important shadow setting**, because it is a direct trade against quality:
    /// the shadow map is a fixed number of pixels stretched over a box this big, so doubling the
    /// distance halves the detail. Set it to roughly the distance a player can actually see
    /// shadows at, rather than to the size of the level.
    #[reflect(min = 0.1, max = 10000.0, unit = "world units")]
    pub shadow_distance: f32,
    /// How many pixels across the shadow map is.
    ///
    /// Powers of two, and the memory cost is the square of it — 4096 is four times 2048, not twice.
    #[reflect(min = 16.0, max = 8192.0, unit = "px")]
    pub shadow_resolution: u32,
    /// How much to push a shadow test away from the surface, in world units.
    ///
    /// Fixes **shadow acne**: a lit surface shadowing itself in stripes, because the shadow map's
    /// resolution means one stored depth stands for a small patch of a sloped surface and half that
    /// patch is behind it. Too little leaves the stripes; too much makes a shadow detach from
    /// whatever cast it, which is called peter-panning and looks exactly like it sounds.
    #[reflect(min = 0.0, max = 10.0, unit = "world units")]
    pub shadow_bias: f32,
}

impl Default for DirectionalLight {
    fn default() -> Self {
        Self {
            colour: [1.0, 1.0, 1.0],
            intensity: 1.0,
            shadows: ShadowMode::Off,
            // Enough to cover a room or a small outdoor area at a 2048 map, which works out at
            // about 33 shadow-map pixels per world unit.
            shadow_distance: 30.0,
            shadow_resolution: 2048,
            shadow_bias: 0.02,
        }
    }
}

impl DirectionalLight {
    /// A light that casts shadows, with the default distance, resolution and bias.
    #[must_use]
    pub fn casting_shadows() -> Self {
        Self {
            shadows: ShadowMode::Orthogonal,
            ..Self::default()
        }
    }
}

impl Component for DirectionalLight {}

/// Every mesh the game has loaded, by asset id.
///
/// A [`Service`], so it can never move a replay (ADR 0009) — the *parameters* of a procedural shape
/// are authored data and live in the asset file, while the vertices they tessellate into are
/// derived. That asymmetry is the same one ADR 0019 drew for `GlobalTransform`.
///
/// Filled from above, like [`TextureCache`](crate::TextureCache) and
/// [`EnvironmentCache`](crate::EnvironmentCache): a mesh asset's file is a *scene* file and
/// `amadeo-scene` sits above this crate, so by I6 the renderer cannot parse it.
///
/// # Geometry can change and can go away, and the version is how anyone finds out
///
/// Until terrain streaming, every mesh in this engine loaded once at startup and stayed forever, so
/// "does the backend have this id" was a complete question. Streaming breaks both halves of that:
/// a chunk that is dug re-meshes under the **same id**, and a chunk walked away from should stop
/// costing video memory.
///
/// So each entry carries a **version**, bumped on every write. A backend that has uploaded version 3
/// and sees version 4 knows to replace its copy — where `has_mesh` alone would answer "yes, I have
/// that" and keep the pre-dig geometry on screen forever, over a collider that had already changed.
#[derive(Debug, Clone, Default)]
pub struct MeshCache {
    loaded: BTreeMap<String, (u64, MeshData)>,
    /// Bumped for every write, so no two versions of anything collide even across an id being
    /// removed and re-added. One counter rather than one per entry: a per-entry counter would restart
    /// at zero when an id came back, and a backend still holding the old version would decide it was
    /// already up to date.
    writes: u64,
}

impl Service for MeshCache {}

impl MeshCache {
    /// An empty cache.
    #[must_use]
    pub fn new() -> Self {
        Self::default()
    }

    /// Records geometry under an id, replacing any earlier one and giving it a fresh version.
    pub fn insert(&mut self, id: impl Into<String>, data: MeshData) {
        self.writes += 1;
        self.loaded.insert(id.into(), (self.writes, data));
    }

    /// Forgets an id's geometry. Removing something absent is not an error.
    ///
    /// Idempotent deliberately, and that is load-bearing rather than lenient: a terrain streamer
    /// reports every chunk that leaves the drawn region, including ones whose geometry never arrived
    /// or turned out to be empty. Filtering that list by "did the caller ever receive this" is the
    /// mistake `docs/07` documents at length — it makes the output depend on what a thread pool
    /// happened to finish. Removal being harmless is what lets the list stay honest.
    pub fn remove(&mut self, id: &str) {
        self.loaded.remove(id);
    }

    /// Which version of an id the cache currently holds, if it holds one.
    ///
    /// What a backend compares against to decide whether its copy is stale.
    #[must_use]
    pub fn version_of(&self, id: &str) -> Option<u64> {
        self.loaded.get(id).map(|(version, _)| *version)
    }

    /// The geometry an id names, if it has loaded.
    ///
    /// Unlike a texture there is **no placeholder**, and that is deliberate: a missing texture has an
    /// obvious stand-in that is the right size, and a missing *mesh* has no honest one — a
    /// substitute cube would be a shape nobody authored sitting in the world, which is worse than a
    /// gap you can see through. The renderer draws nothing and reports the id.
    #[must_use]
    pub fn get(&self, id: &str) -> Option<&MeshData> {
        self.loaded.get(id).map(|(_, data)| data)
    }

    /// Every loaded id, in order.
    pub fn ids(&self) -> impl Iterator<Item = &str> {
        self.loaded.keys().map(String::as_str)
    }
}

/// Every material the game has loaded, by asset id.
///
/// Same shape and same reasoning as [`MeshCache`], except that a missing material **does** have an
/// honest stand-in — [`Material::default`], a plain white surface — because a material describes how
/// to shade geometry that exists rather than whether anything is there at all.
#[derive(Debug, Clone, Default)]
pub struct MaterialCache {
    loaded: BTreeMap<String, Material>,
}

impl Service for MaterialCache {}

impl MaterialCache {
    /// An empty cache.
    #[must_use]
    pub fn new() -> Self {
        Self::default()
    }

    /// Records a material under an id, replacing any earlier one.
    pub fn insert(&mut self, id: impl Into<String>, material: Material) {
        self.loaded.insert(id.into(), material);
    }

    /// The material an id names, or the default for an empty or unloaded id.
    #[must_use]
    pub fn get(&self, id: &str) -> Material {
        if id.is_empty() {
            return Material::default();
        }
        self.loaded.get(id).cloned().unwrap_or_default()
    }

    /// Whether an id has actually been loaded.
    #[must_use]
    pub fn is_loaded(&self, id: &str) -> bool {
        self.loaded.contains_key(id)
    }

    /// Every loaded id, in order.
    pub fn ids(&self) -> impl Iterator<Item = &str> {
        self.loaded.keys().map(String::as_str)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    /// Dot product, for checking a normal points where it should.
    fn dot(a: [f32; 3], b: [f32; 3]) -> f32 {
        a[0] * b[0] + a[1] * b[1] + a[2] * b[2]
    }

    /// A triangle's geometric normal, from the winding of its three corners.
    ///
    /// This is what the GPU computes to decide which way a face points, so comparing it against the
    /// stored normal is what catches a face wound the wrong way round.
    fn winding_normal(data: &MeshData, triangle: usize) -> [f32; 3] {
        let corner = |offset: usize| data.vertices[data.indices[triangle * 3 + offset] as usize];
        let (a, b, c) = (corner(0).position, corner(1).position, corner(2).position);
        let u = [b[0] - a[0], b[1] - a[1], b[2] - a[2]];
        let v = [c[0] - a[0], c[1] - a[1], c[2] - a[2]];
        [
            u[1] * v[2] - u[2] * v[1],
            u[2] * v[0] - u[0] * v[2],
            u[0] * v[1] - u[1] * v[0],
        ]
    }

    #[test]
    fn a_box_is_well_formed() {
        let data = BoxMesh::default().tessellate();
        assert_eq!(data.triangle_count(), 12, "six faces, two triangles each");
        assert_eq!(data.vertices.len(), 24, "four per face, not shared");
        assert!(data.is_well_formed());
    }

    #[test]
    fn a_box_has_flat_faces_rather_than_averaged_corners() {
        // The classic first mistake: sharing eight corner vertices averages three normals at each
        // one, and the box shades like a sphere. Asserting on *normals* rather than on vertex count
        // is what actually catches it, since a clever tessellation could use a different count.
        let data = BoxMesh::default().tessellate();
        for vertex in &data.vertices {
            // Every normal must be axis-aligned: exactly one component is ±1 and the rest are zero.
            let magnitude = dot(vertex.normal, vertex.normal);
            assert!(
                (magnitude - 1.0).abs() < 1e-5,
                "normals must be unit length, got {:?}",
                vertex.normal
            );
            let axes = vertex
                .normal
                .iter()
                .filter(|component| component.abs() > 1e-5)
                .count();
            assert_eq!(
                axes, 1,
                "a box face normal is axis-aligned: {:?}",
                vertex.normal
            );
        }
    }

    #[test]
    fn every_box_triangle_faces_outward() {
        // ADR 0035 records this as the cost of tessellating in engine code: a wrong winding is a
        // subtly wrong picture rather than an error. A face wound the other way is invisible from
        // outside once backface culling is on, and lit from the wrong side before that.
        let data = BoxMesh::default().tessellate();
        for triangle in 0..data.triangle_count() {
            let geometric = winding_normal(&data, triangle);
            let stored = data.vertices[data.indices[triangle * 3] as usize].normal;
            assert!(
                dot(geometric, stored) > 0.0,
                "triangle {triangle} is wound against its own normal: \
                 winding {geometric:?} vs stored {stored:?}"
            );
        }
    }

    #[test]
    fn a_box_covers_its_declared_size() {
        let data = BoxMesh {
            size: [2.0, 4.0, 6.0],
        }
        .tessellate();
        let extent = |axis: usize| {
            let values: Vec<f32> = data.vertices.iter().map(|v| v.position[axis]).collect();
            let low = values.iter().copied().fold(f32::INFINITY, f32::min);
            let high = values.iter().copied().fold(f32::NEG_INFINITY, f32::max);
            high - low
        };
        // Centred on the origin, so the extent is the full declared size.
        assert!((extent(0) - 2.0).abs() < 1e-5);
        assert!((extent(1) - 4.0).abs() < 1e-5);
        assert!((extent(2) - 6.0).abs() < 1e-5);
    }

    #[test]
    fn a_plane_lies_flat_and_faces_up() {
        let data = PlaneMesh { size: [3.0, 5.0] }.tessellate();
        assert_eq!(data.triangle_count(), 2);
        assert!(data.is_well_formed());

        for vertex in &data.vertices {
            assert!(vertex.position[1].abs() < 1e-6, "a plane lies in XZ");
            assert_eq!(vertex.normal, [0.0, 1.0, 0.0]);
        }
        for triangle in 0..data.triangle_count() {
            assert!(
                winding_normal(&data, triangle)[1] > 0.0,
                "triangle {triangle} should be wound counter-clockwise seen from above"
            );
        }
    }

    #[test]
    fn bounds_cover_every_vertex_and_nothing_more() {
        // A box of a known size, so the answer is arithmetic rather than an approximation. Too small
        // a box culls things that are on screen — a flicker rather than an error — and too large a
        // box culls nothing and makes the whole exercise pointless.
        let data = BoxMesh {
            size: [2.0, 4.0, 6.0],
        }
        .tessellate();
        let (min, max) = data.bounds().expect("a box has vertices");

        // `BoxMesh` is centred on its own origin, so the extremes are half the size either way.
        assert_eq!(min, [-1.0, -2.0, -3.0]);
        assert_eq!(max, [1.0, 2.0, 3.0]);

        for vertex in &data.vertices {
            for axis in 0..3 {
                assert!(
                    vertex.position[axis] >= min[axis] && vertex.position[axis] <= max[axis],
                    "{:?} falls outside {min:?}..{max:?}",
                    vertex.position
                );
            }
        }
    }

    #[test]
    fn an_empty_mesh_has_no_bounds_rather_than_a_box_at_the_origin() {
        // Zeros would be a degenerate box quietly claiming something is at the origin — which a
        // frustum test would then dutifully decide is on screen. Most chunks of a streamed world are
        // empty, so this is the common case rather than an edge one.
        assert_eq!(MeshData::default().bounds(), None);
    }

    #[test]
    fn bounds_are_not_fooled_by_a_vertex_order() {
        // Whichever vertex happens to be first must not decide the answer. Written because the
        // implementation seeds min and max from `vertices[0]`, which is correct and is exactly the
        // shape that is wrong if the loop skips it or starts in the wrong place.
        let mut data = MeshData::default();
        for position in [[5.0, 5.0, 5.0], [-3.0, 0.0, 1.0], [0.0, 9.0, -7.0]] {
            data.vertices.push(Vertex {
                position,
                ..Vertex::default()
            });
        }
        assert_eq!(data.bounds(), Some(([-3.0, 0.0, -7.0], [5.0, 9.0, 5.0])));
    }

    #[test]
    fn a_degenerate_size_still_produces_something_well_formed() {
        // A zero-sized box is content being odd rather than an error, and it must not produce
        // out-of-range indices or a partial triangle for the GPU to choke on later.
        let data = BoxMesh {
            size: [0.0, 0.0, 0.0],
        }
        .tessellate();
        assert!(data.is_well_formed());
        assert_eq!(data.triangle_count(), 12);
    }

    #[test]
    fn an_unloaded_mesh_is_absent_rather_than_substituted() {
        // Deliberately unlike `TextureCache`, which always returns something. A stand-in cube would
        // be a shape nobody authored sitting in the world.
        let cache = MeshCache::new();
        assert!(cache.get("wall_panel").is_none());
    }

    #[test]
    fn an_unloaded_material_falls_back_to_a_plain_surface() {
        // And this one *is* like `TextureCache`, because a material describes how to shade geometry
        // that exists rather than whether anything is there.
        let cache = MaterialCache::new();
        assert_eq!(cache.get(""), Material::default());
        assert_eq!(cache.get("stone_rough"), Material::default());
        assert!(!cache.is_loaded("stone_rough"));
    }

    #[test]
    fn a_material_round_trips_through_the_value_tree() {
        use amadeo_reflect::Reflect;
        let material = Material {
            base_colour: [0.8, 0.2, 0.1, 1.0],
            metallic: 1.0,
            roughness: 0.25,
            emissive: [2.0, 0.0, 0.0],
            base_colour_texture: "rust_plate".to_string(),
        };
        let back = Material::from_value(&material.to_value()).expect("round trips");
        assert_eq!(back, material);
    }

    #[test]
    fn a_shape_round_trips_through_the_value_tree() {
        // The property ADR 0035 turned on: a shape is authorable data, so it has to survive the
        // journey a scene file puts it through.
        use amadeo_reflect::Reflect;
        let shape = BoxMesh {
            size: [1.0, 2.5, 0.2],
        };
        assert_eq!(
            BoxMesh::from_value(&shape.to_value()).expect("round trips"),
            shape
        );
    }
}
