//! A camera's look reaches the frame — ADR 0034.
//!
//! The chain this pins, which crosses three crates and a file format:
//!
//! ```text
//!   assets/looks/corridor_dark.environment      a scene file with one root
//!         |  amadeo-app: parse, then Environment::from_value
//!         v
//!   EnvironmentCache["corridor_dark"] = Environment { .. }
//!         |  amadeo-render: render_quads resolves the camera's id
//!         v
//!   View { environment, .. }  ->  FrameData::look()  ->  the post pass
//! ```
//!
//! Everything here is headless. The look is *data* precisely so that it can be checked without a
//! GPU (invariant I7), which is half of why ADR 0034 chose data over a code extension point; the
//! pixels it produces are checked separately in `capture.rs`.

use amadeo_ecs::World;
use amadeo_render::{
    Camera, Environment, EnvironmentCache, NullBackend, Renderer, Tonemap, render_quads,
};
use amadeo_transform::Transform;

/// A world with one camera, optionally naming an environment.
fn world_with_camera(environment: &str) -> World {
    let mut world = World::new();
    let eye = world.spawn();
    world.insert(eye, Transform::at(0.0, 0.0));
    world.insert(
        eye,
        Camera {
            environment: environment.to_string(),
            ..Camera::orthographic(10.0)
        },
    );
    world.insert_service(Renderer::new(Box::new(NullBackend::new(64, 64))));
    world
}

/// The look the frame ended up with.
fn rendered_look(world: &mut World) -> Environment {
    render_quads(world);
    let renderer = world.service::<Renderer>().expect("installed above");
    let backend = renderer.null_backend().expect("a null backend");
    backend.last_frame().expect("a frame was drawn").look()
}

/// The look a camera asked for, once the cache holds it.
fn dark_corridor() -> Environment {
    Environment {
        exposure: 1.5,
        tonemap: Tonemap::AcesFilmic,
        ..Environment::default()
    }
}

#[test]
fn a_camera_gets_the_look_its_id_names() {
    let mut world = world_with_camera("corridor_dark");
    let mut cache = EnvironmentCache::new();
    cache.insert("corridor_dark", dark_corridor());
    world.insert_service(cache);

    assert_eq!(rendered_look(&mut world), dark_corridor());
}

#[test]
fn a_camera_naming_nothing_gets_the_look_that_does_nothing() {
    // The property that let ADR 0034 ship without changing either game's confirmed appearance.
    let mut world = world_with_camera("");
    world.insert_service(EnvironmentCache::new());

    let look = rendered_look(&mut world);
    assert_eq!(look, Environment::default());
    assert!(!look.changes_the_picture());
}

#[test]
fn an_id_that_never_loaded_renders_rather_than_failing() {
    // ADR 0021: the simulation never observes asset state, so a missing environment is a look and
    // not an error. The camera still draws; it simply draws unprocessed.
    let mut world = world_with_camera("a_look_nobody_shipped");
    world.insert_service(EnvironmentCache::new());

    assert_eq!(rendered_look(&mut world), Environment::default());
}

#[test]
fn a_game_with_no_environment_cache_at_all_still_draws() {
    // Every game that has not asked for post-processing, which is both of the ones in this repo.
    // The service is installed on first use rather than at construction, so its absence has to be
    // an ordinary case rather than a panic.
    let mut world = world_with_camera("corridor_dark");
    assert_eq!(rendered_look(&mut world), Environment::default());
}

#[test]
fn the_frames_look_comes_from_the_camera_that_draws_first() {
    // ADR 0031 has every camera compose into one image, so post-processing has one picture to work
    // on and the cameras are no longer separable by then. `FrameData::look` takes the first view's,
    // which is the same rule ADR 0031 gave `render.describe`. Recorded as Q23: a HUD camera cannot
    // yet have a different grade from the world beneath it.
    let mut world = World::new();

    let world_camera = world.spawn();
    world.insert(world_camera, Transform::at(0.0, 0.0));
    world.insert(
        world_camera,
        Camera {
            environment: "corridor_dark".to_string(),
            order: 0,
            ..Camera::orthographic(10.0)
        },
    );

    let hud = world.spawn();
    world.insert(hud, Transform::at(0.0, 0.0));
    world.insert(
        hud,
        Camera {
            environment: "hud_flat".to_string(),
            order: 10,
            ..Camera::orthographic(10.0)
        },
    );

    let mut cache = EnvironmentCache::new();
    cache.insert("corridor_dark", dark_corridor());
    cache.insert(
        "hud_flat",
        Environment {
            exposure: 0.25,
            ..Environment::default()
        },
    );
    world.insert_service(cache);
    world.insert_service(Renderer::new(Box::new(NullBackend::new(64, 64))));

    render_quads(&mut world);
    let renderer = world.service::<Renderer>().expect("installed above");
    let frame = renderer
        .null_backend()
        .expect("a null backend")
        .last_frame()
        .expect("a frame was drawn");

    // Both views carry their own camera's look — the information is not lost...
    assert_eq!(frame.views.len(), 2);
    assert_eq!(frame.views[0].environment, dark_corridor());
    assert_eq!(frame.views[1].environment.exposure, 0.25);
    // ...but the frame applies the first camera's, which is the documented limitation.
    assert_eq!(frame.look(), dark_corridor());
}

#[test]
fn a_look_is_not_part_of_the_state_hash() {
    // `EnvironmentCache` is a `Service`, which ADR 0009 excludes from `state_hash` by trait bound.
    // Asserted by running the same world with and without a look installed rather than by reading
    // the trait bounds, because that is the claim that actually matters for replays.
    let mut without = world_with_camera("corridor_dark");
    render_quads(&mut without);

    let mut with = world_with_camera("corridor_dark");
    let mut cache = EnvironmentCache::new();
    cache.insert("corridor_dark", dark_corridor());
    with.insert_service(cache);
    render_quads(&mut with);

    assert_eq!(
        without.state_hash(),
        with.state_hash(),
        "loading an environment must not be able to move a replay"
    );
}
