//! **M2.5's exit gate 2**: a replay of a streamed world reproduces across runs, processes *and
//! thread counts*.
//!
//! The third of those is the one this milestone exists to prove. ADR 0041's entire claim is that
//! parallelism is a **pure speedup that nothing downstream can observe**, and a terrain streamer is
//! the first thing in this engine where that could plausibly be false: chunks are meshed on a job
//! pool, and a chunk's collider decides where a character standing on it ends up.
//!
//! # Why this is not covered by the tests it looks like it duplicates
//!
//! `amadeo-terrain` already runs five streamers at five worker counts and requires identical
//! colliders, and `streams_into_a_world.rs` does the same for the entities and the solver's contents.
//! Both drive a `Viewer` along a straight line **by setting its coordinate**.
//!
//! This drives a *game*: input, a character controller, rapier, gravity, a capsule on a slope, and a
//! viewer whose position is wherever all of that put it. That closes the loop the others leave open
//! — in them, a divergence in the terrain could not feed back into where the viewer goes next,
//! because the viewer's path was scripted. **Here it can.** A collider that differed by one chunk
//! would move the character, which would move the viewer, which would load different chunks, and the
//! two worlds would separate for good.
//!
//! Session 12's lesson was that a test can carry an exit gate's name and measure nothing. The
//! specific failure there was running each configuration to completion separately, which gives a
//! one-worker pool all the wall clock it needs. So these advance **in lockstep**.

use amadeo_app::App;
use amadeo_character::{MOVE_FORWARD, TURN};
use amadeo_core::Tick;
use amadeo_input::{InputDriver, ScriptedSource};
use amadeo_physics::Physics;
use amadeo_terrain::{TerrainChunk, TerrainViewer};
use amadeo_transform::Transform;
use scarp::{DIG, build_with_workers};

/// Worker counts to compare.
///
/// The odd ones are deliberate, and the reason is the same one `par_for_each_mut`'s test gives: an
/// off-by-one in how work is divided hides completely when the work divides evenly.
const WORKER_COUNTS: [usize; 5] = [1, 2, 3, 5, 8];

/// The same scripted walk for every world: settle, walk, turn, walk, dig.
///
/// Deliberately not a straight line. A turn puts the character on a different part of the terrain and
/// therefore over different chunks, and a dig changes the world *while jobs are in flight* — which is
/// the case the streamer's edit version exists for, and the one most likely to expose a mesh landing
/// from before the edit.
fn walk(source: &mut ScriptedSource) {
    source.axis(Tick(90), MOVE_FORWARD, 1.0);
    source.axis(Tick(240), TURN, 1.0);
    source.axis(Tick(300), TURN, 0.0);
    source.press(Tick(360), DIG, true);
    source.press(Tick(362), DIG, false);
    source.axis(Tick(420), MOVE_FORWARD, 1.0);
}

fn world_with(workers: usize) -> App {
    let mut app = build_with_workers(workers).expect("the game builds");
    let mut source = ScriptedSource::new();
    walk(&mut source);
    amadeo_input::install(&mut app.world, InputDriver::new(Box::new(source)));
    app
}

/// Everything about a world that is supposed to be identical everywhere.
#[derive(Debug, PartialEq)]
struct Observation {
    /// The whole simulation, in one number. This is the claim; the rest is diagnosis.
    state_hash: u64,
    /// Where the character actually is, so a failure says how far apart the worlds drifted rather
    /// than only that two hashes differ.
    player: [f32; 3],
    /// Which chunks exist. Entity identity is world state (ADR 0028), so this must match exactly.
    chunks: Vec<TerrainChunk>,
    /// How much solid ground the solver is holding.
    colliders: usize,
}

fn observe(app: &App) -> Observation {
    Observation {
        state_hash: app.world.state_hash(),
        player: app
            .world
            .query::<(&TerrainViewer, &Transform)>()
            .map(|(_, (_, transform))| transform.translation)
            .next()
            .expect("one player"),
        chunks: app
            .world
            .query::<(&TerrainChunk,)>()
            .map(|(_, (chunk,))| *chunk)
            .collect(),
        colliders: app
            .world
            .service::<Physics>()
            .expect("physics is installed")
            .static_mesh_count(),
    }
}

#[test]
fn a_walk_reproduces_at_every_thread_count() {
    // **The exit gate.** Five worlds, five worker counts, one scripted walk, advanced together.
    let mut worlds: Vec<App> = WORKER_COUNTS.iter().map(|w| world_with(*w)).collect();

    // In lockstep, a tick at a time. Running each to completion in turn is what made session 12's
    // equivalent test pass against an implementation that was deliberately broken: a streamer with
    // the machine to itself always finishes its meshing in time, so the slow configuration never
    // gets to be slow. Interleaving is what puts them under comparable time pressure.
    for tick in 0..480u64 {
        for world in &mut worlds {
            world.run_ticks(1).expect("the world advances");
        }

        // Compared every tick rather than at the end, so a divergence is attributed to the tick it
        // began on rather than to the 480th.
        let reference = observe(&worlds[0]);
        for (index, world) in worlds.iter().enumerate().skip(1) {
            let observed = observe(world);
            assert_eq!(
                observed.state_hash, reference.state_hash,
                "tick {tick}: {} workers diverged from 1 worker.\n  {} workers: {observed:?}\n  \
                 1 worker:  {reference:?}",
                WORKER_COUNTS[index], WORKER_COUNTS[index]
            );
        }
    }

    // And the walk has to have been worth doing. A character that never moved would reproduce
    // perfectly and prove nothing -- the same weakness as a determinism test over a world where
    // nothing happens.
    //
    // **Measured in chunks crossed rather than in units travelled.** Distance was the first version
    // and it is the wrong quantity: the walk turns halfway through, so the straight-line displacement
    // understates the path and a threshold on it is really a threshold on the turn. What streaming
    // needs is that the player *left the chunk they started in*, which is what makes new ground load
    // and old ground unload, and that is a question with an exact answer.
    let end = observe(&worlds[0]);
    let chunk_of = |position: f32| (position / 16.0).floor() as i32;
    let (x, z) = (chunk_of(end.player[0]), chunk_of(end.player[2]));
    assert!(
        (x, z) != (0, 0),
        "the character finished in the chunk it started in, at {:?}; \
         this walk never crossed a boundary and so never exercised streaming",
        end.player
    );
    assert!(
        end.chunks.len() > 100,
        "only {} chunks were ever loaded",
        end.chunks.len()
    );
    assert!(end.colliders > 0, "no ground was ever made solid");
}

#[test]
fn the_same_world_twice_in_one_process_is_identical() {
    // Run-to-run reproduction, which is the cheap half of gate 2 and the one that catches a stray
    // `HashMap` or an uninitialised value long before the thread-count test would.
    let mut first = world_with(4);
    let mut second = world_with(4);

    first.run_ticks(300).expect("the world advances");
    second.run_ticks(300).expect("the world advances");

    assert_eq!(observe(&first), observe(&second));
}

#[test]
fn a_different_seed_is_a_different_world() {
    // The control case. Without it, everything above would pass just as well against a generator
    // that ignored its input entirely and produced the same flat plain every time -- which would
    // reproduce beautifully at every thread count and be worthless.
    //
    // Not a seed *parameter* on the builder, because `requested_seed` reads the command line and a
    // test process has none. Compared through the generator instead, which is where the seed lands.
    let default_world = scarp::Highlands::new(0x0053_4341_5250);
    let other_world = scarp::Highlands::new(1);

    let differences = (0..100)
        .filter(|i| {
            let x = *i as f32 * 3.1;
            default_world.height(x, x * 0.7) != other_world.height(x, x * 0.7)
        })
        .count();
    assert!(
        differences > 95,
        "only {differences} of 100 columns differ; the seed is barely reaching the world"
    );
}
