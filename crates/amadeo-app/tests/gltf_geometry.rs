//! A `.mesh` file that points into a glTF becomes real geometry — the load half of ADR 0039.
//!
//! `amadeo-cli`'s tests cover the *importer*: that it writes valid canonical text. This covers the
//! other half, which nothing else can: that the text it writes actually loads, and that what comes
//! out is the vertices from the glTF rather than an empty mesh nobody notices.
//!
//! Both halves are needed. An importer whose output does not load would pass every test in the CLI.

use amadeo_app::App;
use amadeo_render::{BoxMesh, GltfPart, Material, Mesh, MeshCache, PlaneMesh};
use amadeo_transform::Transform;
use std::path::PathBuf;

/// A `.glb` with one triangle whose positions are distinctive enough to recognise.
fn triangle_glb() -> Vec<u8> {
    let floats: [f32; 24] = [
        // positions -- deliberately not a unit triangle, so a loader that invented geometry rather
        // than reading it would produce different numbers.
        0.0, 0.0, 0.0, 3.0, 0.0, 0.0, 0.0, 7.0, 0.0, //
        // normals
        0.0, 0.0, 1.0, 0.0, 0.0, 1.0, 0.0, 0.0, 1.0, //
        // uvs
        0.0, 0.0, 1.0, 0.0, 0.0, 1.0,
    ];
    let mut binary = Vec::new();
    for value in floats {
        binary.extend_from_slice(&value.to_le_bytes());
    }
    let indices_offset = binary.len();
    for value in [0u16, 1, 2] {
        binary.extend_from_slice(&value.to_le_bytes());
    }
    while binary.len() % 4 != 0 {
        binary.push(0);
    }

    let json = format!(
        r#"{{
"asset":{{"version":"2.0"}},"scene":0,"scenes":[{{"nodes":[0]}}],
"nodes":[{{"name":"Slab","mesh":0}}],
"meshes":[{{"name":"Slab","primitives":[{{"attributes":{{"POSITION":0,"NORMAL":1,"TEXCOORD_0":2}},"indices":3}}]}}],
"accessors":[
 {{"bufferView":0,"componentType":5126,"count":3,"type":"VEC3","min":[0.0,0.0,0.0],"max":[3.0,7.0,0.0]}},
 {{"bufferView":1,"componentType":5126,"count":3,"type":"VEC3"}},
 {{"bufferView":2,"componentType":5126,"count":3,"type":"VEC2"}},
 {{"bufferView":3,"componentType":5123,"count":3,"type":"SCALAR"}}
],
"bufferViews":[
 {{"buffer":0,"byteOffset":0,"byteLength":36}},
 {{"buffer":0,"byteOffset":36,"byteLength":36}},
 {{"buffer":0,"byteOffset":72,"byteLength":24}},
 {{"buffer":0,"byteOffset":{indices_offset},"byteLength":6}}
],
"buffers":[{{"byteLength":{}}}]
}}"#,
        binary.len()
    );

    let mut json_bytes = json.into_bytes();
    while json_bytes.len() % 4 != 0 {
        json_bytes.push(b' ');
    }

    let mut glb = Vec::new();
    let total = 12 + 8 + json_bytes.len() + 8 + binary.len();
    glb.extend_from_slice(b"glTF");
    glb.extend_from_slice(&2u32.to_le_bytes());
    glb.extend_from_slice(&(total as u32).to_le_bytes());
    glb.extend_from_slice(&(json_bytes.len() as u32).to_le_bytes());
    glb.extend_from_slice(b"JSON");
    glb.extend_from_slice(&json_bytes);
    glb.extend_from_slice(&(binary.len() as u32).to_le_bytes());
    glb.extend_from_slice(&[b'B', b'I', b'N', 0]);
    glb.extend_from_slice(&binary);
    glb
}

/// Writes an asset directory holding the glb, a `.mesh` pointing into it, and their sidecars.
fn asset_directory(name: &str, mesh_index: u32, primitive_index: u32) -> PathBuf {
    let directory = PathBuf::from(env!("CARGO_TARGET_TMPDIR")).join(name);
    let _ = std::fs::remove_dir_all(&directory);
    std::fs::create_dir_all(&directory).expect("a writable directory");

    let write = |file: &str, bytes: &[u8]| {
        std::fs::write(directory.join(file), bytes).expect("writes");
    };

    write("slab.glb", &triangle_glb());
    write("slab.glb.ama-meta", b"id = \"slab_glb\"\n");
    // Exactly the shape `amadeo import-gltf` generates, written by hand here so this test fails if
    // the *format* changes rather than only if the importer does.
    write(
        "slab.mesh",
        format!(
            "scene slab\nversion 1\n\nentity mesh \"Slab\"\n  GltfPart\n    flat false\n    \
             mesh {mesh_index}\n    primitive {primitive_index}\n    source \"slab_glb\"\n"
        )
        .as_bytes(),
    );
    write("slab.mesh.ama-meta", b"id = \"slab\"\n");
    directory
}

/// An app with the components a mesh needs registered, pointed at `directory`.
fn app_with(directory: &PathBuf) -> App {
    let mut app = App::new();
    app.register_component::<Transform>().expect("fresh");
    app.register_component::<Mesh>().expect("fresh");
    app.register_component::<GltfPart>().expect("fresh");
    app.register_component::<BoxMesh>().expect("fresh");
    app.register_component::<PlaneMesh>().expect("fresh");
    app.register_component::<Material>().expect("fresh");
    app.scan_assets(directory).expect("scans");
    app
}

/// Spawns an entity drawing `slab` and runs the loaders.
fn load_slab(app: &mut App) {
    let entity = app.world.spawn();
    app.world.insert(entity, Transform::default());
    app.world.insert(entity, Mesh::new("slab", ""));
    app.load_meshes();
}

#[test]
fn geometry_comes_out_of_the_gltf_rather_than_from_nowhere() {
    // **The whole of the load half in one assertion.** The positions are distinctive, so a loader
    // that produced an empty mesh, or a default cube, or the right number of wrong vertices, fails
    // here rather than drawing something plausible.
    let directory = asset_directory("gltf_load", 0, 0);
    let mut app = app_with(&directory);
    load_slab(&mut app);

    let cache = app.world.service::<MeshCache>().expect("installed on use");
    let data = cache.get("slab").expect("the glTF's geometry loaded");
    assert_eq!(data.vertices.len(), 3);
    assert_eq!(data.indices, vec![0, 1, 2]);
    assert_eq!(data.vertices[1].position, [3.0, 0.0, 0.0]);
    assert_eq!(data.vertices[2].position, [0.0, 7.0, 0.0]);
    // The fixed layout ADR 0035 pins, all three parts.
    assert_eq!(data.vertices[0].normal, [0.0, 0.0, 1.0]);
    assert_eq!(data.vertices[1].uv, [1.0, 0.0]);
}

#[test]
fn a_part_naming_an_index_the_file_does_not_have_is_skipped_rather_than_fatal() {
    // ADR 0021 requires a missing asset to be survivable. A model re-exported with fewer meshes
    // should leave one thing undrawn, not take the game down on load.
    let directory = asset_directory("gltf_missing_index", 9, 0);
    let mut app = app_with(&directory);
    load_slab(&mut app);

    let missing = app
        .world
        .service::<MeshCache>()
        .is_none_or(|cache| cache.get("slab").is_none());
    assert!(missing, "an out-of-range part should load nothing");
}

#[test]
fn a_gltf_mesh_is_still_just_a_mesh_to_everything_above_the_loader() {
    // **The property ADR 0035 was written early to buy**, and the reason glTF import could wait
    // until M2's last session: the `Mesh` component, the cache and everything downstream cannot
    // tell where the geometry came from. A box and a glTF primitive arrive at the same place.
    let directory = asset_directory("gltf_uniform", 0, 0);
    std::fs::write(
        directory.join("crate_box.mesh"),
        b"scene crate_box\nversion 1\n\nentity mesh \"Crate\"\n  BoxMesh\n    size 1.0 1.0 1.0\n",
    )
    .expect("writes");
    std::fs::write(
        directory.join("crate_box.mesh.ama-meta"),
        b"id = \"crate_box\"\n",
    )
    .expect("writes");

    let mut app = app_with(&directory);
    for id in ["slab", "crate_box"] {
        let entity = app.world.spawn();
        app.world.insert(entity, Transform::default());
        app.world.insert(entity, Mesh::new(id, ""));
    }
    app.load_meshes();

    let cache = app.world.service::<MeshCache>().expect("installed");
    // Both loaded, through one call, into one cache, with no caller distinguishing them.
    assert!(cache.get("slab").is_some(), "the glTF primitive");
    assert!(cache.get("crate_box").is_some(), "the procedural box");
}
