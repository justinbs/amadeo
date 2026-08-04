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

use amadeo_render::{Camera, EnvironmentCache, Tonemap};
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
