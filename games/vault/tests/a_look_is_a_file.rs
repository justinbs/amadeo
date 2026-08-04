//! An `Environment` is an asset like any other — ADR 0034, proved against real files.
//!
//! # Why this test is in the game rather than in `amadeo-render`
//!
//! ADR 0034's claim is not "the renderer can hold a look". It is that **a look is an asset, its file
//! is a scene file, and the whole existing toolchain therefore applies to it for nothing**. That
//! claim spans a sidecar, the asset catalogue, the load barrier, the scene parser, reflection, and
//! the render crate's cache — and the only place all six exist together is a real game.
//!
//! `crates/amadeo-render/tests/environment.rs` covers what happens once the look is in the cache.
//! This covers how it gets there, which is the half that involves a file on disk.
//!
//! # The Vault does not actually use this look
//!
//! `corridor_dark.environment` ships in the Vault's assets and its camera deliberately does *not*
//! name it. Two reasons: the game's appearance has been confirmed on screen and is what the exit
//! gate was judged against, and pointing the camera at a look would move `collect-three.replay` for
//! a purely cosmetic reason. It is here as a worked example and as this test's fixture — the same
//! role quad-demo's deliberately-missing sprite plays for the placeholder path.

use amadeo_render::{Camera, EnvironmentCache, Mesh, MeshCache, Tonemap};
use vault::build_simulation;

/// The Vault, with its camera pointed at the shipped look.
fn app_with_the_look() -> amadeo_app::App {
    let mut app = build_simulation().expect("the game builds");

    // In code rather than in `vault.scene`, for the reason in the module docs. This is also the
    // path a game that spawns its camera in code takes, so it is worth exercising.
    let cameras: Vec<_> = app
        .world
        .entities()
        .into_iter()
        .filter(|entity| app.world.get::<Camera>(*entity).is_some())
        .collect();
    assert!(!cameras.is_empty(), "the Vault authors a camera");

    for entity in cameras {
        // Cloned because `World::get` hands back a borrow and `insert` needs the value — the world
        // cannot be borrowed and written at once.
        let mut camera = app
            .world
            .get::<Camera>(entity)
            .expect("just checked")
            .clone();
        camera.environment = "corridor_dark".to_string();
        app.world.insert(entity, camera);
    }

    app.load_environments();
    app
}

#[test]
fn a_look_travels_from_a_file_into_the_renderers_cache() {
    let app = app_with_the_look();
    let cache = app
        .world
        .service::<EnvironmentCache>()
        .expect("load_environments installs it on first use");

    assert!(
        cache.is_loaded("corridor_dark"),
        "loaded ids: {:?}",
        cache.ids().collect::<Vec<_>>()
    );

    // The values in `assets/looks/corridor_dark.environment`, having been through a sidecar, the
    // catalogue, the load barrier, the scene parser and `Reflect::from_value`.
    let look = cache.get("corridor_dark");
    assert_eq!(look.tonemap, Tonemap::AcesFilmic);
    assert!((look.exposure - 1.1).abs() < 1e-6, "{}", look.exposure);
    assert!((look.grade.saturation - 0.7).abs() < 1e-6);
    assert!((look.vignette.intensity - 0.55).abs() < 1e-6);
    assert!(look.changes_the_picture());
}

#[test]
fn a_shape_written_as_three_numbers_becomes_geometry() {
    // ADR 0035's whole claim, against a real file: `assets/meshes/wall_panel.mesh` is six lines of
    // text describing a box, and it comes out the other end as tessellated geometry with no
    // toolchain, no binary and no import step. That is invariant I1 reaching 3D.
    //
    // The Vault is a 2D game and does not draw this. It ships as a worked example and this
    // fixture, exactly as `corridor_dark.environment` does.
    let mut app = build_simulation().expect("the game builds");

    let entity = app.world.spawn();
    app.world.insert(entity, Mesh::new("wall_panel", ""));
    app.load_meshes();

    let cache = app
        .world
        .service::<MeshCache>()
        .expect("load_meshes installs it on first use");
    let data = cache.get("wall_panel").expect("the file tessellated");

    assert_eq!(data.triangle_count(), 12, "a box is six faces of two");
    assert!(data.is_well_formed());

    // The declared size, having been through a sidecar, the catalogue, the load barrier, the scene
    // parser, reflection and tessellation.
    let height = |axis: usize| {
        let values: Vec<f32> = data.vertices.iter().map(|v| v.position[axis]).collect();
        values.iter().copied().fold(f32::NEG_INFINITY, f32::max)
            - values.iter().copied().fold(f32::INFINITY, f32::min)
    };
    assert!((height(0) - 1.0).abs() < 1e-5);
    assert!((height(1) - 2.5).abs() < 1e-5);
    assert!((height(2) - 0.2).abs() < 1e-5);
}

#[test]
fn a_mesh_id_that_never_loaded_is_absent_rather_than_substituted() {
    // Deliberately unlike a texture, which always has a stand-in. A substitute cube would be a shape
    // nobody authored sitting in the world, which is worse than a gap you can see through.
    let mut app = build_simulation().expect("the game builds");
    let entity = app.world.spawn();
    app.world.insert(entity, Mesh::new("no_such_mesh", ""));
    app.load_meshes();

    // Either no cache at all or a cache without it — both are "absent", and neither is an error.
    let missing = app
        .world
        .service::<MeshCache>()
        .is_none_or(|cache| cache.get("no_such_mesh").is_none());
    assert!(missing, "an unresolved mesh id must not be substituted");
}

#[test]
fn the_shipped_game_still_asks_for_no_look() {
    // Guards the decision above rather than the mechanism: if `vault.scene` ever gains an
    // environment id, the game's appearance and its replay both move, and that should be a
    // deliberate act with this test updated alongside — not something that drifts in.
    let app = build_simulation().expect("the game builds");
    for entity in app.world.entities() {
        if let Some(camera) = app.world.get::<Camera>(entity) {
            assert_eq!(
                camera.environment, "",
                "the Vault's camera is deliberately unprocessed"
            );
        }
    }
}

#[test]
fn a_look_cannot_move_the_state_hash() {
    // The property that made it safe to ship the asset at all. `EnvironmentCache` is a `Service`,
    // which ADR 0009 excludes from `state_hash` by trait bound — but the claim worth testing is the
    // one about *this* game: loading a real look off disk leaves the simulation identical.
    let plain = build_simulation().expect("the game builds");
    let with_look = app_with_the_look();

    // The camera's `environment` field is authored data and *is* in the hash, so it is put back
    // before comparing — what is being tested is that loading the file changed nothing else.
    let mut restored = with_look;
    for entity in restored.world.entities() {
        let Some(camera) = restored.world.get::<Camera>(entity) else {
            continue;
        };
        let camera = Camera {
            environment: String::new(),
            ..camera.clone()
        };
        restored.world.insert(entity, camera);
    }

    assert_eq!(
        plain.world.state_hash(),
        restored.world.state_hash(),
        "loading an environment must be invisible to the simulation"
    );
}
