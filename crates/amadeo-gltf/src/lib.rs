//! Reading glTF 2.0 files into plain data — ADR 0039.
//!
//! # Why this is its own crate
//!
//! The same reason [`amadeo-image`](../amadeo_image/index.html) is: it holds a heavy external
//! parser and nothing else, so the dependency lives in exactly one place and **no type the `gltf`
//! crate defines is visible above this boundary**. Everything crossing out of here is defined here.
//!
//! That matters more than tidiness. glTF is a large specification with a large crate behind it, and
//! an engine whose renderer named `gltf::Primitive` in its own API would have made an interchange
//! format part of its public surface.
//!
//! # It has no opinion about engines
//!
//! There is no `MeshData` here, no `Material`, no `Transform`. Those live in `amadeo-render` and
//! `amadeo-transform`, both of which sit *above* this crate — so this crate cannot name them (I6),
//! and the conversion happens in the layer that can see both. The same split `amadeo-image` has, and
//! for the same reason.
//!
//! Rotations come out as **quaternions**, deliberately, even though ADR 0018 authors Euler degrees:
//! converting is `amadeo-transform`'s job and it already owns that convention. A conversion done
//! here would be a second implementation of it.
//!
//! ```no_run
//! # fn main() -> Result<(), amadeo_gltf::GltfError> {
//! let bytes = std::fs::read("level.glb").expect("a file");
//! let document = amadeo_gltf::read(&bytes)?;
//!
//! // Everything a scene needs: what shapes exist, what they are made of, and where they go.
//! println!("{} meshes, {} materials", document.meshes.len(), document.materials.len());
//! for node in &document.nodes {
//!     println!("{} at {:?}", node.name, node.translation);
//! }
//! # Ok(())
//! # }
//! ```

/// What can go wrong reading a glTF file.
#[derive(Debug, thiserror::Error)]
pub enum GltfError {
    /// The bytes are not a glTF file, or are a damaged one.
    #[error(
        "could not read the glTF file: {0}. Check it is a .gltf or .glb saved by a tool that exports glTF 2.0"
    )]
    Malformed(String),

    /// The file is valid glTF but refers to data that is not in it.
    ///
    /// A `.gltf` may keep its buffers and images in sibling files. Only self-contained files are
    /// read here — `.glb`, or `.gltf` with embedded base64 — because reading siblings would mean
    /// this crate touching the filesystem, and the asset layer is what decides where bytes come
    /// from (ADR 0021).
    #[error(
        "the glTF file refers to external data ({what}) that is not embedded in it. Re-export as \
         .glb, or as .gltf with embedded buffers"
    )]
    NotSelfContained {
        /// Which external resource was missing.
        what: String,
    },

    /// A primitive is missing something every mesh in this engine needs.
    #[error(
        "mesh {mesh} primitive {primitive} has no {missing}. Amadeo's vertex layout is fixed at \
         position, normal and UV (ADR 0035), so a mesh without them cannot be drawn"
    )]
    IncompletePrimitive {
        /// Which mesh, by index.
        mesh: usize,
        /// Which primitive within it.
        primitive: usize,
        /// What was absent.
        missing: &'static str,
    },
}

/// One vertex, in the fixed layout ADR 0035 pins.
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct GltfVertex {
    /// Position in the mesh's own space.
    pub position: [f32; 3],
    /// Unit normal.
    pub normal: [f32; 3],
    /// Texture coordinate.
    pub uv: [f32; 2],
    /// The file's `TANGENT` attribute — `xyz` direction, `w` handedness — or `None` if it has none.
    ///
    /// **Optional where the others are required**, and that asymmetry is the point (ADR 0047). A
    /// tangent frame can be computed from the other three; a position cannot be computed from
    /// anything. So a file without one is ordinary rather than broken, and the engine generates a
    /// frame at load.
    ///
    /// Reading it matters because the alternative is guessing. A normal map is *baked* against a
    /// particular tangent frame — usually MikkTSpace's — and a renderer that computes a different one
    /// lights the bumps slightly wrong everywhere. Blender and Substance can export the frame they
    /// baked against, so taking it from the file when it is there is how Amadeo gets MikkTSpace
    /// correctness without implementing MikkTSpace.
    pub tangent: Option<[f32; 4]>,
}

/// One drawable piece of geometry.
///
/// **A glTF *mesh* may hold several primitives**, one per material, and Amadeo's `Mesh` component
/// draws exactly one thing with exactly one material — so a primitive, not a mesh, is what
/// corresponds to an Amadeo mesh asset. Getting that backwards produces a model that silently loses
/// every material but the first.
#[derive(Debug, Clone, PartialEq)]
pub struct GltfPrimitive {
    /// Vertices, in no particular order — `indices` decides the triangles.
    pub vertices: Vec<GltfVertex>,
    /// Three per triangle.
    pub indices: Vec<u32>,
    /// Which entry of [`GltfDocument::materials`] this uses, if it names one.
    pub material: Option<usize>,
}

/// A glTF mesh: a name and the primitives it is made of.
#[derive(Debug, Clone, PartialEq)]
pub struct GltfMesh {
    /// The name the authoring tool gave it, or a generated one.
    pub name: String,
    /// One or more pieces, each with its own material.
    pub primitives: Vec<GltfPrimitive>,
}

/// A surface description, in the subset Amadeo's `Material` can hold.
///
/// glTF's metallic-roughness model is the same one ADR 0033 chose, so this is a rename rather than a
/// conversion. What is dropped is what Amadeo cannot yet express: texture references beyond base
/// colour, normal maps, occlusion, and every `KHR_materials_*` extension.
#[derive(Debug, Clone, PartialEq)]
pub struct GltfMaterial {
    /// The name the authoring tool gave it, or a generated one.
    pub name: String,
    /// Linear RGBA.
    pub base_colour: [f32; 4],
    /// `0.0` dielectric, `1.0` bare metal.
    pub metallic: f32,
    /// `0.0` mirror, `1.0` fully diffuse.
    pub roughness: f32,
    /// Linear RGB of light the surface emits on its own.
    pub emissive: [f32; 3],
}

/// One node of the scene graph: where something is, and what is under it.
#[derive(Debug, Clone, PartialEq)]
pub struct GltfNode {
    /// The name the authoring tool gave it, or a generated one.
    pub name: String,
    /// Position, relative to its parent.
    pub translation: [f32; 3],
    /// Rotation as a quaternion `[x, y, z, w]`, relative to its parent.
    ///
    /// **A quaternion rather than Euler degrees**, even though ADR 0018 authors Euler: the
    /// conversion belongs to `amadeo-transform`, which owns that convention and its exact axis
    /// order. Converting here would be a second implementation of it, and a subtly different one is
    /// the kind of bug that reads as "the imported model is rotated slightly wrong".
    pub rotation: [f32; 4],
    /// Scale on each axis, relative to its parent.
    pub scale: [f32; 3],
    /// Which entry of [`GltfDocument::meshes`] this node draws, if any.
    ///
    /// `None` is common and correct: most nodes in a real file are pure transforms grouping other
    /// nodes.
    pub mesh: Option<usize>,
    /// Indices into [`GltfDocument::nodes`] of this node's children.
    pub children: Vec<usize>,
}

/// Everything read out of one glTF file.
#[derive(Debug, Clone, PartialEq, Default)]
pub struct GltfDocument {
    /// Every mesh, in file order. Indices into this are stable and are what a `.mesh` asset names.
    pub meshes: Vec<GltfMesh>,
    /// Every material, in file order.
    pub materials: Vec<GltfMaterial>,
    /// Every node, in file order.
    pub nodes: Vec<GltfNode>,
    /// Indices into [`GltfDocument::nodes`] of the default scene's top-level nodes.
    pub roots: Vec<usize>,
}

/// Reads a self-contained glTF file — `.glb`, or `.gltf` with embedded buffers.
///
/// # Errors
///
/// [`GltfError::Malformed`] if the bytes are not glTF 2.0, [`GltfError::NotSelfContained`] if the
/// file points at buffers it does not carry, and [`GltfError::IncompletePrimitive`] if a primitive
/// lacks positions, normals or UVs — which the fixed vertex layout requires.
pub fn read(bytes: &[u8]) -> Result<GltfDocument, GltfError> {
    // `import_slice` reads the JSON *and* resolves the buffers, which for a .glb means the binary
    // chunk that follows. It refuses anything needing a sibling file, which is exactly the boundary
    // this crate wants -- the asset layer owns where bytes come from (ADR 0021), not this.
    let (document, buffers, _images) = gltf::import_slice(bytes).map_err(|error| match error {
        gltf::Error::Io(_) => GltfError::NotSelfContained {
            what: "a buffer or image in a sibling file".to_string(),
        },
        other => GltfError::Malformed(other.to_string()),
    })?;

    let meshes = read_meshes(&document, &buffers)?;
    let materials = document.materials().map(read_material).collect();
    let nodes = document.nodes().map(read_node).collect();

    // The default scene, or the first one. A file with no scene at all still has meshes worth
    // importing, so this is empty rather than an error.
    let roots = document
        .default_scene()
        .or_else(|| document.scenes().next())
        .map(|scene| scene.nodes().map(|node| node.index()).collect())
        .unwrap_or_default();

    Ok(GltfDocument {
        meshes,
        materials,
        nodes,
        roots,
    })
}

fn read_meshes(
    document: &gltf::Document,
    buffers: &[gltf::buffer::Data],
) -> Result<Vec<GltfMesh>, GltfError> {
    let mut meshes = Vec::new();
    for mesh in document.meshes() {
        let mut primitives = Vec::new();
        for primitive in mesh.primitives() {
            primitives.push(read_primitive(
                &primitive,
                buffers,
                mesh.index(),
                primitive.index(),
            )?);
        }
        meshes.push(GltfMesh {
            name: mesh
                .name()
                .map(str::to_string)
                .unwrap_or_else(|| format!("mesh_{}", mesh.index())),
            primitives,
        });
    }
    Ok(meshes)
}

fn read_primitive(
    primitive: &gltf::Primitive,
    buffers: &[gltf::buffer::Data],
    mesh_index: usize,
    primitive_index: usize,
) -> Result<GltfPrimitive, GltfError> {
    let reader = primitive.reader(|buffer| buffers.get(buffer.index()).map(|data| &data.0[..]));

    let missing = |what: &'static str| GltfError::IncompletePrimitive {
        mesh: mesh_index,
        primitive: primitive_index,
        missing: what,
    };

    let positions: Vec<[f32; 3]> = reader
        .read_positions()
        .ok_or_else(|| missing("positions"))?
        .collect();
    let normals: Vec<[f32; 3]> = reader
        .read_normals()
        .ok_or_else(|| missing("normals"))?
        .collect();
    // `read_tex_coords(0)` is the first UV set. A model with several is using the others for
    // lightmaps or masks, neither of which this engine reads yet.
    let uvs: Vec<[f32; 2]> = reader
        .read_tex_coords(0)
        .ok_or_else(|| missing("texture coordinates"))?
        .into_f32()
        .collect();

    // glTF permits the attribute arrays to be different lengths only if they come from different
    // accessors by mistake; a valid file has them equal. Truncating to the shortest is what keeps a
    // slightly-wrong file drawable rather than making it a hard error, and it cannot index past the
    // end of anything.
    // Tangents, if the exporter wrote them. Unlike the three above this is not `ok_or_else`: a file
    // without tangents is normal, and the engine generates them at load (ADR 0047).
    let tangents: Vec<[f32; 4]> = reader
        .read_tangents()
        .map(Iterator::collect)
        .unwrap_or_default();

    let count = positions.len().min(normals.len()).min(uvs.len());
    let vertices = (0..count)
        .map(|index| GltfVertex {
            position: positions[index],
            normal: normals[index],
            uv: uvs[index],
            // `get` rather than indexing: a file with tangents for only some vertices is malformed,
            // and truncating the way `count` does above would silently drop good geometry. Missing
            // entries become `None` and are generated with the rest.
            tangent: tangents.get(index).copied(),
        })
        .collect();

    // Indices are optional in glTF: without them the vertices are already in triangle order, which
    // this turns into an explicit list so everything above sees one shape.
    let indices = match reader.read_indices() {
        Some(indices) => indices.into_u32().collect(),
        None => (0..count as u32).collect(),
    };

    Ok(GltfPrimitive {
        vertices,
        indices,
        material: primitive.material().index(),
    })
}

fn read_material(material: gltf::Material) -> GltfMaterial {
    let pbr = material.pbr_metallic_roughness();
    GltfMaterial {
        name: material
            .name()
            .map(str::to_string)
            .unwrap_or_else(|| match material.index() {
                Some(index) => format!("material_{index}"),
                None => "default".to_string(),
            }),
        base_colour: pbr.base_color_factor(),
        metallic: pbr.metallic_factor(),
        roughness: pbr.roughness_factor(),
        emissive: material.emissive_factor(),
    }
}

fn read_node(node: gltf::Node) -> GltfNode {
    // `decomposed` gives translation, rotation and scale separately. glTF also allows a node to
    // carry a full matrix instead, and this is what turns that case into the same three parts --
    // which is what Amadeo's `Transform` holds (ADR 0018).
    let (translation, rotation, scale) = node.transform().decomposed();
    GltfNode {
        name: node
            .name()
            .map(str::to_string)
            .unwrap_or_else(|| format!("node_{}", node.index())),
        translation,
        rotation,
        scale,
        mesh: node.mesh().map(|mesh| mesh.index()),
        children: node.children().map(|child| child.index()).collect(),
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn nonsense_bytes_are_refused_with_something_actionable() {
        let error = read(b"this is not a glTF file").expect_err("should refuse");
        let message = error.to_string();
        assert!(
            message.contains(".glb"),
            "the message should say what a valid file looks like: {message}"
        );
    }

    #[test]
    fn an_empty_document_is_not_an_error() {
        // A file with no scene still has meshes worth importing, so `roots` being empty is a fact
        // about the file rather than a failure to read it.
        let document = GltfDocument::default();
        assert!(document.roots.is_empty());
        assert!(document.meshes.is_empty());
    }
}
