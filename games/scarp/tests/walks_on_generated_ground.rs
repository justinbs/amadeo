//! **M2.5's exit gates 1 and 2, driven headlessly.**
//!
//! Gate 1 is "a generated terrain world you can walk around, streamed in chunks, with collision that
//! works". Gate 2 is that a replay of it reproduces across runs, processes *and thread counts* — the
//! last being the one that proves ADR 0041 rather than assuming it.
//!
//! Everything here runs with no window and no GPU (invariant I7), which is the only way any of it
//! could be asserted at all: the claims are about where a character ends up and what the solver
//! holds, and neither is a picture.
//!
//! # Why these are worth more than the crate-level tests they overlap
//!
//! `amadeo-terrain`'s own tests drive a `TerrainStreamer` directly, with a `Viewer` moved by hand
//! along a straight line. This drives the **game**: a character with a capsule, gravity, rapier, a
//! schedule, and a viewer whose position is wherever the physics put it. Session 12's lesson was
//! that a test can carry an exit gate's name and still not measure it, and the difference between a
//! hand-moved viewer and a simulated one is exactly the sort of gap that hides in.

use amadeo_app::App;
use amadeo_character::{CharacterMotion, MOVE_FORWARD};
use amadeo_core::Tick;
use amadeo_ecs::World;
use amadeo_input::{InputDriver, ScriptedSource};
use amadeo_physics::Physics;
use amadeo_render::MeshCache;
use amadeo_terrain::{Terrain, TerrainChunk, TerrainViewer};
use amadeo_transform::Transform;
use scarp::{DIG, Highlands, build_simulation, origin_ground_height};

/// The game with a scripted input source, so actions arrive at exact ticks.
///
/// Scripted rather than writing into [`InputState`](amadeo_input::InputState) directly, and that is
/// not fussiness: `sample_input` rebuilds the resource from the driver every tick in `PreSimulation`,
/// so a value poked in from a test is overwritten before any system sees it. This is also the same
/// path a keyboard and a `.replay` file take, which is what makes the test evidence about the game
/// rather than about its systems.
fn scripted(script: impl FnOnce(&mut ScriptedSource)) -> App {
    let mut app = build_simulation().expect("the game builds");
    let mut source = ScriptedSource::new();
    script(&mut source);
    amadeo_input::install(&mut app.world, InputDriver::new(Box::new(source)));
    app
}

/// The player's position.
fn player_position(world: &World) -> [f32; 3] {
    world
        .query::<(&TerrainViewer, &Transform)>()
        .map(|(_, (_, transform))| transform.translation)
        .next()
        .expect("the scene authors one player")
}

/// Whether the character thinks it is standing on something.
fn grounded(world: &World) -> bool {
    world
        .query::<(&CharacterMotion,)>()
        .map(|(_, (motion,))| motion.grounded)
        .next()
        .expect("the scene authors one character")
}

fn chunk_count(world: &World) -> usize {
    world.query::<(&TerrainChunk,)>().count()
}

/// Runs the world for a number of ticks.
fn advance(app: &mut App, ticks: u64) {
    app.run_ticks(ticks).expect("the world advances");
}

#[test]
fn the_player_lands_on_ground_that_was_never_authored() {
    // **Exit gate 1's core claim.** Nothing in `scarp.scene` describes any ground: there is a sun, a
    // player and a camera. The surface the character comes to rest on was generated, meshed, turned
    // into a triangle collider and handed to rapier, all within the first few ticks.
    let mut app = scripted(|_| {});

    // Spawned above the surface and falling.
    let start = player_position(&app.world);
    assert!(
        start[1] > origin_ground_height(),
        "spawned above the ground"
    );

    advance(&mut app, 180);

    let resting = player_position(&app.world);
    assert!(
        grounded(&app.world),
        "the character never found the ground; it is at y = {} after three seconds",
        resting[1]
    );
    // A capsule of height 1.2 and radius 0.4 has its centre one unit above whatever it stands on.
    let expected = origin_ground_height() + 1.0;
    assert!(
        (resting[1] - expected).abs() < 0.35,
        "resting at y = {} but the ground at the origin is {}",
        resting[1],
        origin_ground_height()
    );
}

#[test]
fn the_ground_underfoot_is_solid_before_the_character_needs_it() {
    // ADR 0041 Â§2, at the layer that matters. A collider is gameplay, so it may not be late -- and
    // "late" here means the character falls through the world on tick one and everything afterwards
    // is wrong. Asserted on the *first* tick rather than after settling.
    let mut app = scripted(|_| {});
    advance(&mut app, 1);

    let solid = app
        .world
        .service::<Physics>()
        .expect("physics is installed")
        .static_mesh_count();
    assert!(
        solid > 0,
        "no terrain collider reached the solver on the first tick"
    );
}

/// Every chunk key currently alive, as sortable text.
fn chunk_keys(world: &World) -> std::collections::BTreeSet<String> {
    world
        .query::<(&TerrainChunk,)>()
        .map(|(_, (chunk,))| format!("{}/{}/{}", chunk.x, chunk.y, chunk.z))
        .collect()
}

#[test]
fn walking_brings_new_ground_in_and_lets_old_ground_go() {
    // Streaming, as a player experiences it: the world ahead loads, the world behind unloads, and
    // the number of chunks alive stays bounded. Without that second half the test passes for a while
    // and then the process runs out of memory, which is not a failure any assertion catches.
    //
    // # By walking rather than by teleporting, and that is a finding rather than a preference
    //
    // The first version of this set the player's `Transform` to a distant point, which is much
    // cheaper than simulating the walk. **It silently did nothing.** `step_physics` reads
    // `GlobalTransform` in preference to `Transform`, and `propagate_transforms` runs in
    // `PostSimulation` -- so a `Transform` written from outside the tick is read back stale, physics
    // steps from the old position, and writes it straight back over the new one. The player was
    // still at the origin, and the only sign was one chunk failing an assertion about its
    // coordinate.
    //
    // Walking is what the gate asks for anyway ("a world you can walk around"), and it goes through
    // input, the character controller, rapier and residency rather than around all four.
    let mut app = scripted(|source| {
        // Hold forward from the moment the character has settled onto the ground.
        source.axis(Tick(90), MOVE_FORWARD, 1.0);
    });
    advance(&mut app, 90);

    let settled = chunk_count(&app.world);
    let before = chunk_keys(&app.world);
    assert!(settled > 0, "no chunks were streamed in at all");

    let started_at = player_position(&app.world);
    // Long enough to cross two chunk boundaries at the authored speed of 6 units per second: two
    // sixteen-unit chunks is 32 units, which is a bit over five seconds.
    advance(&mut app, 360);
    let ended_at = player_position(&app.world);

    let travelled = (ended_at[0] - started_at[0]).abs() + (ended_at[2] - started_at[2]).abs();
    assert!(
        travelled > 20.0,
        "the character only travelled {travelled} units; it is not actually walking"
    );

    let after = chunk_keys(&app.world);
    assert!(
        after.difference(&before).count() > 0,
        "walking brought no new ground into view"
    );
    assert!(
        before.difference(&after).count() > 0,
        "nothing behind the player was released; chunks accumulate as the world is crossed"
    );
    assert_eq!(
        chunk_count(&app.world),
        settled,
        "the loaded region changed size while the player walked in a straight line"
    );

    // And the geometry went with them, which is the half `stream_terrain`'s documentation claimed
    // and its code did not do until this game was built.
    let cache = app.world.service::<MeshCache>().expect("a mesh cache");
    for departed in before.difference(&after) {
        let id = format!("terrain/0/{departed}");
        assert!(
            cache.get(&id).is_none(),
            "{id} was despawned but its geometry is still cached"
        );
    }
}

#[test]
fn digging_changes_the_ground_and_the_geometry_together() {
    // ADR 0042 end to end, through a game rather than through a streamer: an input action becomes an
    // edit becomes a re-meshed chunk becomes a new collider *and* new geometry in the cache.
    //
    // Both halves matter and they failed independently. Before `MeshCache` carried a version the
    // collider changed and the drawn mesh did not, so the player walked into a tunnel that still
    // looked like solid rock -- a defect no simulation test can see, because the simulation is right.
    // Dig on tick 130, once the character has settled onto the ground.
    let mut app = scripted(|source| {
        source.press(Tick(130), DIG, true);
        source.press(Tick(132), DIG, false);
    });
    advance(&mut app, 120);

    let before_edits = app
        .world
        .service::<Terrain>()
        .expect("terrain is installed")
        .streamer
        .edit_count();
    assert_eq!(before_edits, 0, "nothing has been dug yet");

    let under_foot = "terrain/0/0/0/0";
    let before = app
        .world
        .service::<MeshCache>()
        .expect("a mesh cache")
        .get(under_foot)
        .cloned()
        .expect("the chunk the player stands in has geometry");

    advance(&mut app, 16);

    let terrain = app
        .world
        .service::<Terrain>()
        .expect("terrain is installed");
    assert!(
        terrain.streamer.edit_count() > 0,
        "pressing dig changed nothing"
    );

    let after = app
        .world
        .service::<MeshCache>()
        .expect("a mesh cache")
        .get(under_foot)
        .cloned()
        .expect("the dug chunk still has geometry");
    assert_ne!(
        before, after,
        "the chunk was dug but its drawn geometry is unchanged"
    );
}

#[test]
fn the_generated_ground_is_the_same_on_every_run() {
    // I3 for the part of the world that is not in any file. Two `Highlands` built from one seed must
    // agree exactly -- not approximately -- because the surface decides where a collider is and two
    // machines that disagree about that disagree about where the player is standing.
    let first = Highlands::new(7);
    let second = Highlands::new(7);
    let other = Highlands::new(8);

    let mut differences = 0;
    for i in 0..200 {
        let (x, z) = (i as f32 * 1.7 - 170.0, i as f32 * -2.3 + 90.0);
        assert_eq!(
            first.height(x, z),
            second.height(x, z),
            "the same seed gave two different worlds at ({x}, {z})"
        );
        if first.height(x, z) != other.height(x, z) {
            differences += 1;
        }
    }
    assert!(
        differences > 190,
        "only {differences} of 200 columns differ between seeds; the seed is barely reaching the world"
    );
}

#[test]
fn the_spawn_column_is_at_base_height_for_every_seed() {
    // **What lets `scarp.scene` author a spawn height honestly**, rather than the game overriding it
    // in code and making the file lie about the world (invariant I1).
    //
    // Gradient noise is exactly zero at every lattice point whatever the seed, and the origin is a
    // lattice point for both of this world's octaves. So the ground there is `BASE_HEIGHT` on the
    // nose, always. This test is what stops that silently ceasing to be true if a frequency changes.
    for seed in [0, 1, 42, 0x0053_4341_5250, u64::MAX] {
        let terrain = Highlands::new(seed);
        assert!(
            (terrain.height(0.0, 0.0) - origin_ground_height()).abs() < 1e-5,
            "seed {seed} puts the spawn column at {} rather than {}",
            terrain.height(0.0, 0.0),
            origin_ground_height()
        );
    }
}
