//! Reading an actual glTF binary, rather than asserting on types.
//!
//! # The fixture is built here rather than committed
//!
//! A `.glb` is opaque bytes, and a committed one would be a test fixture nobody can read or review —
//! so this constructs one, which means the *format* is written down in the test rather than hidden
//! in a file. It is also the only way to be sure the fixture exercises exactly what is claimed: a
//! node hierarchy, a named mesh, a material with non-default numbers, and indices.
//!
//! The GLB container is simple enough to write by hand: a twelve-byte header, then a JSON chunk,
//! then a binary chunk, each padded to four bytes.

/// Builds a `.glb` holding one triangle, one material, and a two-node hierarchy.
fn triangle_glb() -> Vec<u8> {
    // --- The binary chunk: positions, normals, UVs, then indices. ---
    let positions: [[f32; 3]; 3] = [[0.0, 0.0, 0.0], [1.0, 0.0, 0.0], [0.0, 1.0, 0.0]];
    let normals: [[f32; 3]; 3] = [[0.0, 0.0, 1.0]; 3];
    let uvs: [[f32; 2]; 3] = [[0.0, 0.0], [1.0, 0.0], [0.0, 1.0]];
    let indices: [u16; 3] = [0, 1, 2];

    let mut binary = Vec::new();
    for value in positions.iter().flatten().chain(normals.iter().flatten()) {
        binary.extend_from_slice(&value.to_le_bytes());
    }
    for value in uvs.iter().flatten() {
        binary.extend_from_slice(&value.to_le_bytes());
    }
    let indices_offset = binary.len();
    for value in indices {
        binary.extend_from_slice(&value.to_le_bytes());
    }
    // glTF requires buffer views to be four-byte aligned, and the whole chunk padded.
    while binary.len() % 4 != 0 {
        binary.push(0);
    }

    // --- The JSON chunk. ---
    //
    // Written out literally rather than built with a serialiser: this crate has no JSON dependency
    // and should not grow one for a test, and a literal is what makes the fixture readable.
    let json = format!(
        r#"{{
"asset":{{"version":"2.0"}},
"scene":0,
"scenes":[{{"nodes":[0]}}],
"nodes":[
  {{"name":"Root","children":[1],"translation":[1.0,2.0,3.0]}},
  {{"name":"Wall Segment","mesh":0,"scale":[2.0,2.0,2.0]}}
],
"meshes":[{{"name":"Wall Segment","primitives":[{{"attributes":{{"POSITION":0,"NORMAL":1,"TEXCOORD_0":2}},"indices":3,"material":0}}]}}],
"materials":[{{"name":"Red Paint","pbrMetallicRoughness":{{"baseColorFactor":[1.0,0.25,0.125,1.0],"metallicFactor":0.5,"roughnessFactor":0.75}},"emissiveFactor":[0.1,0.0,0.0]}}],
"accessors":[
  {{"bufferView":0,"componentType":5126,"count":3,"type":"VEC3","min":[0.0,0.0,0.0],"max":[1.0,1.0,0.0]}},
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
    // The JSON chunk pads with spaces rather than zeroes, per the GLB specification.
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

#[test]
fn a_glb_yields_its_geometry() {
    let document = amadeo_gltf::read(&triangle_glb()).expect("a valid glb");

    assert_eq!(document.meshes.len(), 1);
    let primitive = &document.meshes[0].primitives[0];
    assert_eq!(primitive.vertices.len(), 3);
    assert_eq!(primitive.indices, vec![0, 1, 2]);

    // The fixed vertex layout ADR 0035 pins, all three parts of it — a reader that dropped UVs
    // would still produce something that draws, just untextured forever.
    assert_eq!(primitive.vertices[1].position, [1.0, 0.0, 0.0]);
    assert_eq!(primitive.vertices[1].normal, [0.0, 0.0, 1.0]);
    assert_eq!(primitive.vertices[1].uv, [1.0, 0.0]);
}

#[test]
fn a_glb_yields_its_materials() {
    let document = amadeo_gltf::read(&triangle_glb()).expect("a valid glb");

    assert_eq!(document.materials.len(), 1);
    let material = &document.materials[0];
    assert_eq!(material.name, "Red Paint");
    assert_eq!(material.base_colour, [1.0, 0.25, 0.125, 1.0]);
    // Non-default numbers on purpose: a reader that returned the defaults would pass a test written
    // against a default-looking material and be completely wrong.
    assert_eq!(material.metallic, 0.5);
    assert_eq!(material.roughness, 0.75);
    assert_eq!(material.emissive, [0.1, 0.0, 0.0]);
}

#[test]
fn a_glb_yields_its_node_hierarchy() {
    // The half that makes this an *imported level* rather than an imported model: where things are
    // and what is under what.
    let document = amadeo_gltf::read(&triangle_glb()).expect("a valid glb");

    assert_eq!(document.roots, vec![0]);
    assert_eq!(document.nodes.len(), 2);

    let root = &document.nodes[0];
    assert_eq!(root.name, "Root");
    assert_eq!(root.translation, [1.0, 2.0, 3.0]);
    assert_eq!(root.children, vec![1]);
    // A pure transform grouping other nodes, which is most of a real file.
    assert!(root.mesh.is_none());

    let child = &document.nodes[1];
    assert_eq!(child.name, "Wall Segment");
    assert_eq!(child.scale, [2.0, 2.0, 2.0]);
    assert_eq!(child.mesh, Some(0));
}

#[test]
fn a_rotation_comes_back_as_a_quaternion() {
    // Deliberately *not* converted here: ADR 0018's Euler order belongs to `amadeo-transform`, and
    // a second implementation of it is the bug that reads as "the imported model is rotated
    // slightly wrong". An unrotated node is the identity quaternion, not zeroes.
    let document = amadeo_gltf::read(&triangle_glb()).expect("a valid glb");
    assert_eq!(document.nodes[0].rotation, [0.0, 0.0, 0.0, 1.0]);
}

#[test]
fn a_truncated_file_is_refused_rather_than_read_as_nonsense() {
    let mut glb = triangle_glb();
    glb.truncate(glb.len() / 2);
    let error = amadeo_gltf::read(&glb).expect_err("half a file is not a file");
    assert!(
        error.to_string().contains("glTF"),
        "the message should say what was being read: {error}"
    );
}
