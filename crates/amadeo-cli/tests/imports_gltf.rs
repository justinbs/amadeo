//! `amadeo import-gltf` produces text the rest of the engine already understands — ADR 0039.
//!
//! The claim worth checking is not "it wrote some files". It is that **what it wrote is valid,
//! canonical scene text** — because everything downstream (the parser, `amadeo check`,
//! `amadeo fmt --check`, and CI) assumes that, and a generator whose output its own formatter would
//! rewrite is a generator that will fail CI the first time anyone uses it.
//!
//! The `.glb` is built here rather than committed for the reason `amadeo-gltf`'s own tests give: a
//! committed binary fixture is one nobody can read or review. The builder is deliberately a cut-down
//! copy of that one — a shared crate for one test helper would be a worse trade than thirty lines.

use std::path::PathBuf;

/// A `.glb` with one triangle, one material, and two nodes, one under the other.
fn fixture_glb() -> Vec<u8> {
    let floats: [f32; 24] = [
        // positions
        0.0, 0.0, 0.0, 1.0, 0.0, 0.0, 0.0, 1.0, 0.0, //
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
"nodes":[
 {{"name":"Room Root","children":[1],"translation":[1.5,0.0,-2.0]}},
 {{"name":"Wall.001","mesh":0,"scale":[2.0,1.0,1.0]}}
],
"meshes":[{{"name":"Wall.001","primitives":[{{"attributes":{{"POSITION":0,"NORMAL":1,"TEXCOORD_0":2}},"indices":3,"material":0}}]}}],
"materials":[{{"name":"Grey Stone","pbrMetallicRoughness":{{"baseColorFactor":[0.5,0.5,0.5,1.0],"metallicFactor":0.0,"roughnessFactor":0.9}},"emissiveFactor":[0.0,0.0,0.0]}}],
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

/// Writes the fixture into a fresh directory under the target dir and imports it.
///
/// `target/` rather than the system temp directory, so a failed run leaves the output somewhere
/// findable and a `cargo clean` sweeps it up.
fn import_into(name: &str) -> (PathBuf, Vec<PathBuf>) {
    let directory = PathBuf::from(env!("CARGO_TARGET_TMPDIR")).join(name);
    let _ = std::fs::remove_dir_all(&directory);
    std::fs::create_dir_all(&directory).expect("a writable directory");

    let source = directory.join("level.glb");
    std::fs::write(&source, fixture_glb()).expect("writes");

    let output = std::process::Command::new(env!("CARGO_BIN_EXE_amadeo"))
        .args(["import-gltf", source.to_str().expect("utf-8")])
        .output()
        .expect("the CLI runs");
    assert!(
        output.status.success(),
        "import failed: {}",
        String::from_utf8_lossy(&output.stderr)
    );

    let mut written: Vec<PathBuf> = std::fs::read_dir(&directory)
        .expect("readable")
        .filter_map(|entry| entry.ok().map(|entry| entry.path()))
        .collect();
    written.sort();
    (directory, written)
}

fn read(path: &PathBuf) -> String {
    std::fs::read_to_string(path).expect("written")
}

#[test]
fn an_import_produces_a_scene_materials_and_a_mesh_file_per_primitive() {
    let (directory, written) = import_into("shape");
    let names: Vec<String> = written
        .iter()
        .filter_map(|path| path.file_name()?.to_str().map(str::to_string))
        .collect();

    assert!(names.contains(&"level.scene".to_string()), "{names:?}");
    // Ids are prefixed with the file's stem, so importing two models cannot make them collide.
    assert!(
        names.contains(&"level_grey_stone.material".to_string()),
        "{names:?}"
    );
    assert!(
        names.contains(&"level_wall_001.mesh".to_string()),
        "{names:?}"
    );
    // Every generated asset needs a sidecar or the catalogue cannot find it (ADR 0020) — including
    // the source file, which is what the mesh files point at.
    assert!(
        names.contains(&"level.glb.ama-meta".to_string()),
        "{names:?}"
    );
    assert!(
        names.contains(&"level_wall_001.mesh.ama-meta".to_string()),
        "{names:?}"
    );

    // Geometry stayed in the .glb: the mesh file is a pointer, not vertex data. This is the whole
    // of ADR 0039's decision in one assertion.
    let mesh = read(&directory.join("level_wall_001.mesh"));
    assert!(mesh.contains("GltfPart"), "{mesh}");
    assert!(mesh.contains("source \"level_glb\""), "{mesh}");
    assert!(
        !mesh.contains("0.0 1.0 0.0"),
        "vertex data must not be written into the mesh file: {mesh}"
    );
}

#[test]
fn everything_it_writes_parses_as_a_scene_and_is_already_canonical() {
    // **The claim that matters.** CI runs `amadeo fmt --check` and `amadeo check` over the project's
    // text files, so output this tool's own formatter would rewrite is output that breaks CI the
    // first time anyone imports anything.
    let (_, written) = import_into("canonical");

    for path in &written {
        let Some(extension) = path.extension().and_then(|extension| extension.to_str()) else {
            continue;
        };
        if !matches!(extension, "scene" | "mesh" | "material") {
            continue;
        }
        let text = read(path);
        let document = amadeo_scene::parse(&text)
            .unwrap_or_else(|error| panic!("{} did not parse: {error}", path.display()));
        assert_eq!(
            amadeo_scene::to_text(&document),
            text,
            "{} is not canonically formatted",
            path.display()
        );
    }
}

#[test]
fn the_scene_keeps_the_hierarchy_and_converts_the_transforms() {
    let (directory, _) = import_into("hierarchy");
    let text = read(&directory.join("level.scene"));
    let document = amadeo_scene::parse(&text).expect("parses");

    assert_eq!(document.entities.len(), 1, "one root: {text}");
    let root = &document.entities[0];
    assert_eq!(root.name, "Room Root");
    // Nesting is how the scene format expresses parenting, and it is what makes the imported layout
    // an actual hierarchy rather than a flat list at baked world positions.
    assert_eq!(root.children.len(), 1, "{text}");
    assert_eq!(root.children[0].name, "Wall.001");

    // glTF stores metres and quaternions; a scene file stores metres and Euler degrees (ADR 0018).
    assert!(text.contains("translation 1.5 0.0 -2.0"), "{text}");
    assert!(text.contains("rotation 0.0 0.0 0.0"), "{text}");
    assert!(text.contains("scale 2.0 1.0 1.0"), "{text}");

    // The scene declares what it needs, including the source file — a `.mesh` pointing into a `.glb`
    // whose bytes never loaded would draw nothing (ADR 0021).
    assert!(text.contains("assets\n"), "{text}");
    assert!(text.contains("  level_glb\n"), "{text}");
}

#[test]
fn a_dry_run_writes_nothing() {
    let directory = PathBuf::from(env!("CARGO_TARGET_TMPDIR")).join("dry");
    let _ = std::fs::remove_dir_all(&directory);
    std::fs::create_dir_all(&directory).expect("a writable directory");
    let source = directory.join("level.glb");
    std::fs::write(&source, fixture_glb()).expect("writes");

    let output = std::process::Command::new(env!("CARGO_BIN_EXE_amadeo"))
        .args(["import-gltf", source.to_str().expect("utf-8"), "--check"])
        .output()
        .expect("the CLI runs");
    assert!(output.status.success());
    assert!(
        String::from_utf8_lossy(&output.stdout).contains("would write"),
        "a dry run should say what it would do"
    );

    let left: Vec<_> = std::fs::read_dir(&directory)
        .expect("readable")
        .filter_map(|entry| entry.ok())
        .map(|entry| entry.file_name())
        .collect();
    assert_eq!(
        left.len(),
        1,
        "only the source should be there, got {left:?}"
    );
}

#[test]
fn a_file_that_is_not_gltf_is_refused_with_something_actionable() {
    let directory = PathBuf::from(env!("CARGO_TARGET_TMPDIR")).join("bad");
    let _ = std::fs::remove_dir_all(&directory);
    std::fs::create_dir_all(&directory).expect("a writable directory");
    let source = directory.join("notes.glb");
    std::fs::write(&source, b"this is not a model").expect("writes");

    let output = std::process::Command::new(env!("CARGO_BIN_EXE_amadeo"))
        .args(["import-gltf", source.to_str().expect("utf-8")])
        .output()
        .expect("the CLI runs");
    assert!(!output.status.success());
    let message = String::from_utf8_lossy(&output.stderr);
    assert!(
        message.contains("glTF 2.0") || message.contains(".glb"),
        "the message should say what a usable file looks like: {message}"
    );
}
