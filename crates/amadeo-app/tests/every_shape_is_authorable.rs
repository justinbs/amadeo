//! Every shape the engine can read out of a `.mesh` file can also be checked and described.
//!
//! # The defect this closes
//!
//! Session 21's engine review found that **no game registered any shape but `BoxMesh` and
//! `PlaneMesh`.** `ArchMesh` — one session old — and the whole of ADR 0074's parametric set were
//! invisible to `amadeo check` and `amadeo describe` in every game that shipped them:
//!
//! ```text
//! > describe CylinderMesh
//! {"error":{"message":"no type named `CylinderMesh` is in this game's schema"}}
//! ```
//!
//! And a `.mesh` holding one still **loaded**, because `App::load_meshes` reads shapes with
//! `from_value` directly and never consults the registry. That is the worse of the two failure modes:
//! it works when tried and reports as broken when checked, so an agent authoring content had to read
//! Rust source to learn the noun existed. ADR 0030 exists to prevent precisely that.
//!
//! # Why the test is shaped like this
//!
//! The engine has two hand-written lists of shape kinds — the branches in `App::load_meshes` and the
//! calls in `App::register_asset_components` — and nothing in the type system ties them together. So
//! the test writes a real `.mesh` asset **per shape**, loads it, and asserts both halves at once:
//! geometry came out, and no asset problem was reported. Adding a shape to the loader without
//! registering it fails the second half, because `read_component_assets` reports an unregistered
//! shape rather than staying quiet about it.

use amadeo_app::App;
use amadeo_render::{Mesh, MeshCache};
use amadeo_transform::Transform;
use std::path::{Path, PathBuf};

/// Every shape kind the engine ships, with a valid body for each.
///
/// Deliberately spelled out rather than generated: this list is the test's whole point, and a shape
/// missing from it is a shape nothing here checks.
///
/// Most name every field, which exercises the full parse. **`SphereMesh` deliberately names one**,
/// so the pipeline is checked end to end against ADR 0075's defaults rather than only in a unit test:
/// a `.mesh` file on disk, read through `read_component_assets`, with three of four fields absent.
///
/// Writing these from memory got four of the seven wrong — `ArchMesh` has a `length` rather than a
/// `depth`, `WedgeMesh` has `width`/`depth` rather than a `size` — which is itself the argument for
/// `amadeo describe <Shape> --example` being how a shape is learned rather than by reading Rust.
const SHAPES: &[(&str, &str)] = &[
    ("BoxMesh", "  BoxMesh\n    size 1.0 1.0 1.0\n"),
    ("PlaneMesh", "  PlaneMesh\n    size 4.0 4.0\n"),
    (
        "ArchMesh",
        "  ArchMesh\n    floor false\n    height 3.0\n    length 0.4\n    segments 8\n    \
         width 2.0\n",
    ),
    (
        "CylinderMesh",
        "  CylinderMesh\n    capped true\n    height 2.0\n    radius 0.5\n    sides 12\n    \
         top_radius 0.5\n",
    ),
    ("SphereMesh", "  SphereMesh\n    radius 0.75\n"),
    (
        "WedgeMesh",
        "  WedgeMesh\n    depth 1.0\n    height_back 0.25\n    height_front 1.0\n    width 1.0\n",
    ),
    (
        "StairMesh",
        "  StairMesh\n    rise 0.18\n    run 0.28\n    steps 6\n    width 1.2\n",
    ),
    // ADR 0074 §2. A list whose items have named fields, which is ADR 0067's shape, and an enum with
    // a payload, which is ADR 0032's — so this fixture is also the first file to use both at once.
    (
        "CompoundMesh",
        "  CompoundMesh\n    parts\n      - position 0.0 0.4 0.0\n        solid Cylinder\n          \
         shape\n            height 0.8\n            radius 0.1\n",
    ),
    // ADR 0074 §4. One triangle, which is the smallest thing this can hold, and no normals — so it
    // also exercises the derive-them-from-the-triangles path.
    (
        "VertexMesh",
        "  VertexMesh\n    indices 0 1 2\n    positions 0.0 0.0 0.0 1.0 0.0 0.0 0.0 1.0 0.0\n",
    ),
];

/// Writes an asset root holding one `.mesh` file whose body is `shape`.
fn directory_with_shape(name: &str, shape: &str) -> PathBuf {
    let directory = PathBuf::from(env!("CARGO_TARGET_TMPDIR")).join(name);
    let _ = std::fs::remove_dir_all(&directory);
    std::fs::create_dir_all(&directory).expect("a writable directory");

    let file = format!("scene part\nversion 1\n\nentity mesh \"Part\"\n{shape}");
    std::fs::write(directory.join("part.mesh"), file).expect("writes");
    std::fs::write(directory.join("part.mesh.ama-meta"), b"id = \"part\"\n").expect("writes");
    // The marker that makes this an asset root (ADR 0022).
    std::fs::write(directory.join("amadeo-assets.toml"), b"").expect("writes");
    directory
}

/// An app that has registered the engine's asset vocabulary and wants the mesh id `part`.
fn app_wanting_the_part(directory: &Path) -> App {
    let mut app = App::new();
    app.register_component::<Transform>().expect("fresh");
    app.register_component::<Mesh>().expect("fresh");
    app.register_asset_components().expect("fresh");
    app.scan_assets(directory.to_str().expect("utf-8"))
        .expect("scans");

    let entity = app.world.spawn();
    app.world.insert(entity, Transform::at(0.0, 0.0));
    app.world.insert(entity, Mesh::new("part", ""));
    app
}

#[test]
fn every_shape_the_loader_reads_is_registered_and_tessellates() {
    for (name, body) in SHAPES {
        let directory = directory_with_shape(&format!("shape_{name}"), body);
        let mut app = app_wanting_the_part(&directory);
        app.load_meshes();

        // Half one: the registry can see it, so `amadeo check` accepts a file naming it and
        // `amadeo describe <name>` answers. This is the half that was false for five of the seven.
        assert!(
            app.components().info(name).is_some(),
            "`{name}` is not registered, so `amadeo check` would refuse a .mesh holding one and \
             `amadeo describe {name}` would say the type does not exist"
        );

        // Half two: it actually became geometry. A registered shape the loader cannot read would be
        // the opposite failure and is just as bad.
        let cache = app
            .world
            .service::<MeshCache>()
            .unwrap_or_else(|| panic!("`{name}` produced no mesh cache at all"));
        let data = cache
            .get("part")
            .unwrap_or_else(|| panic!("`{name}` loaded no geometry for the id `part`"));
        assert!(
            !data.indices.is_empty(),
            "`{name}` tessellated to no triangles"
        );

        // Half three, and the drift catcher: adding a kind to `load_meshes` without adding it to
        // `register_asset_components` reports here rather than going unnoticed.
        let problems: Vec<String> = app
            .asset_problems()
            .map(|(id, reason)| format!("{id}: {reason}"))
            .collect();
        assert!(
            problems.is_empty(),
            "`{name}` loaded but was complained about: {problems:?}"
        );
    }
}

#[test]
fn a_shape_that_loads_unregistered_says_so_rather_than_staying_quiet() {
    // The net under `register_asset_components`, tested directly — because the whole defect was that
    // this case is *silent*, and a net nothing exercises is a net nobody knows is missing.
    //
    // Deliberately does NOT call `register_asset_components`, which is the mistake being simulated.
    let directory = directory_with_shape(
        "shape_unregistered",
        "  CylinderMesh\n    capped true\n    height 2.0\n    radius 0.5\n    sides 12\n    \
         top_radius 0.5\n",
    );

    let mut app = App::new();
    app.register_component::<Transform>().expect("fresh");
    app.register_component::<Mesh>().expect("fresh");
    app.scan_assets(directory.to_str().expect("utf-8"))
        .expect("scans");
    let entity = app.world.spawn();
    app.world.insert(entity, Transform::at(0.0, 0.0));
    app.world.insert(entity, Mesh::new("part", ""));
    app.load_meshes();

    // It still loads. ADR 0021's posture: a game is visibly wrong rather than refusing to run, and
    // this shape is not even wrong — it is merely uncheckable.
    assert!(
        app.world
            .service::<MeshCache>()
            .and_then(|cache| cache.get("part"))
            .is_some(),
        "an unregistered shape should still tessellate — the complaint is about checking, not loading"
    );

    let problems: Vec<String> = app
        .asset_problems()
        .map(|(id, reason)| format!("{id}: {reason}"))
        .collect();
    assert_eq!(problems.len(), 1, "one unregistered shape, one complaint");
    let reason = &problems[0];
    // The three things that make it actionable: which asset, which type, and what to do about it.
    assert!(
        reason.contains("part"),
        "must name the asset, got: {reason}"
    );
    assert!(
        reason.contains("CylinderMesh"),
        "and the shape, got: {reason}"
    );
    assert!(
        reason.contains("register_asset_components"),
        "and the fix, got: {reason}"
    );
}

#[test]
fn a_game_may_keep_its_own_registrations_alongside_this_one() {
    // Written expecting the opposite — that a double registration would be refused, on ADR 0017's
    // rule that two components under one canonical name is a real ambiguity. It is not, and the
    // distinction is the right one: `TypeRegistry::register` returns `Ok` when the name is already
    // present *and the schema is identical*, and errors only when two genuinely different types
    // collide (`amadeo-reflect/src/registry.rs:69`).
    //
    // Which makes this a real guarantee worth pinning rather than an accident: a game can adopt
    // `register_asset_components` without deleting anything, and a game that registers `BoxMesh`
    // itself because an entity carries one is not punished for it.
    let mut app = App::new();
    app.register_component::<amadeo_render::BoxMesh>()
        .expect("a game's own line");
    app.register_asset_components()
        .expect("the engine's vocabulary on top of it");
    app.register_asset_components()
        .expect("and again, because idempotence is the property");

    assert!(app.components().info("BoxMesh").is_some());
    assert!(app.components().info("CylinderMesh").is_some());
}
