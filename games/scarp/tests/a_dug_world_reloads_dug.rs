//! **Q29's promise, end to end through a real game.**
//!
//! ADR 0042 said a save file for a terrain world is *a seed plus a diff*. Until session 13 the diff
//! lived in a `Service`, which is outside the state hash and untouched by a snapshot (ADR 0009) — so
//! digging a tunnel, saving, and loading gave the tunnel back filled in.
//!
//! `amadeo-terrain`'s own tests cover the mechanism: that the resource is the truth, that it is
//! hashed, and that the streamer follows it in both directions. This covers the thing a player would
//! notice, through the actual game: press F, save, reload, and the hole is still there.

use amadeo_app::App;
use amadeo_core::Tick;
use amadeo_input::{InputDriver, ScriptedSource};
use amadeo_render::{MeshCache, MeshData};
use amadeo_terrain::{TerrainEdits, chunk_mesh_id};
use amadeo_voxel::ChunkKey;
use scarp::{DIG, build_simulation};

/// The game with a scripted dig at tick 130, once the character has settled onto the ground.
fn digging_game() -> App {
    let mut app = build_simulation().expect("the game builds");
    let mut source = ScriptedSource::new();
    source.press(Tick(130), DIG, true);
    source.press(Tick(132), DIG, false);
    amadeo_input::install(&mut app.world, InputDriver::new(Box::new(source)));
    app
}

/// The geometry of the chunk the player is standing in.
fn ground(app: &App) -> Option<MeshData> {
    app.world
        .service::<MeshCache>()
        .expect("a mesh cache")
        .get(&chunk_mesh_id(ChunkKey::new(0, 0, 0)))
        .cloned()
}

#[test]
fn a_hole_dug_in_the_game_survives_a_save_and_a_reload() {
    let mut dug = digging_game();
    dug.run_ticks(120).expect("the world advances");
    let before = ground(&dug).expect("the ground under the player is meshed");

    // Through the dig and a few ticks past it, so the chunk has been re-meshed.
    dug.run_ticks(40).expect("the world advances");
    let after = ground(&dug).expect("still meshed after digging");
    assert_ne!(before, after, "pressing dig changed nothing to save");

    let edits = dug.world.resource::<TerrainEdits>().expect("installed");
    assert!(
        !edits.is_empty(),
        "the dig did not reach the authored edits, so there is nothing for a save to carry"
    );
    let snapshot = dug.capture_snapshot();

    // A brand new game. Its streamer knows nothing about any of this, and a snapshot cannot tell it
    // — a streamer is a Service (ADR 0009). The resource is what carries the hole across, and
    // `stream_terrain` notices its revision is stale and re-digs before the next frame.
    let mut reloaded = build_simulation().expect("the game builds");
    amadeo_input::install(&mut reloaded.world, InputDriver::null());
    reloaded.restore_snapshot(&snapshot).expect("restores");
    reloaded.run_ticks(4).expect("the world advances");

    assert_eq!(
        ground(&reloaded).expect("meshed after reloading"),
        after,
        "the reloaded world is not the world that was saved; the hole was filled back in"
    );
}

#[test]
fn a_save_of_an_untouched_world_carries_no_edits() {
    // ADR 0042's other half, and the reason the edits are sparse rather than a grid: a world nobody
    // has dug costs nothing to store. A save file for it is the seed alone.
    let mut app = build_simulation().expect("the game builds");
    amadeo_input::install(&mut app.world, InputDriver::null());
    app.run_ticks(160).expect("the world advances");

    let edits = app.world.resource::<TerrainEdits>().expect("installed");
    assert!(
        edits.is_empty(),
        "an untouched world recorded {} edits",
        edits.len()
    );
    assert_eq!(edits.revision, 0);
}

#[test]
fn digging_moves_the_state_hash() {
    // What makes a dig replayable as well as saveable. Two runs of the same game that differ only in
    // whether somebody dug must not agree about the state of the world.
    let mut dug = digging_game();
    let mut untouched = build_simulation().expect("the game builds");
    amadeo_input::install(&mut untouched.world, InputDriver::null());

    for _ in 0..160 {
        dug.run_ticks(1).expect("advances");
        untouched.run_ticks(1).expect("advances");
    }

    assert_ne!(
        dug.world.state_hash(),
        untouched.world.state_hash(),
        "a dug world and an untouched one hash the same, so a replay would not reproduce the hole"
    );
}
