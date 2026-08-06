//! `par_for_each_mut` gives the same answer as the sequential version, whatever the thread count.
//!
//! ```text
//! cargo test -p amadeo-ecs --release --test parallel_iteration -- --nocapture
//! ```
//!
//! # What is asserted, and what is measured
//!
//! **Equality is asserted**, and it is the whole claim ADR 0041 rests on: the number of threads is a
//! property of the machine, so it must not be able to reach the answer. Checked by running the same
//! work at 1, 2, 3, 5 and 8 threads and requiring byte-identical output — odd counts included,
//! because an off-by-one in chunk slicing hides completely when the row count divides evenly.
//!
//! **The crossover is measured, not asserted.** `PARALLEL_THRESHOLD` is a real number about real
//! hardware, and `CLAUDE.md` §6 forbids tests that depend on wall-clock. The table it prints is what
//! justifies the constant; a timing assertion here would just be flaky.

use amadeo_core::StableHash;
use amadeo_ecs::{Component, PARALLEL_THRESHOLD, World};
use amadeo_reflect::Reflect;

#[derive(Debug, Clone, Copy, PartialEq, StableHash, Reflect)]
struct Sample {
    value: f32,
}

impl Component for Sample {}

fn world_with(rows: usize) -> World {
    let mut world = World::new();
    for index in 0..rows {
        let entity = world.spawn();
        world.insert(
            entity,
            Sample {
                value: index as f32,
            },
        );
    }
    world
}

/// Deliberately expensive per row, so a parallel run has something to actually divide.
///
/// Transcendental functions rather than a multiply: a multiply is so cheap that the whole loop is
/// memory-bound and no thread count changes anything, which would make the measurement below say
/// nothing at all.
fn heavy(value: f32) -> f32 {
    let mut out = value;
    for _ in 0..24 {
        out = (out * 0.5).sin() + (out * 0.25).cos();
    }
    out
}

fn values(world: &World) -> Vec<f32> {
    let mut out: Vec<f32> = world
        .query::<(&Sample,)>()
        .map(|(_, (sample,))| sample.value)
        .collect();
    // Sorted so the comparison is about the *arithmetic*, not about archetype iteration order --
    // which is already deterministic and tested elsewhere.
    out.sort_by(f32::total_cmp);
    out
}

#[test]
fn the_thread_count_cannot_reach_the_answer() {
    // **The claim ADR 0041 rests on.** Odd counts are in the list on purpose: with 8192 rows and 2,
    // 4 or 8 threads every chunk is the same size, and an off-by-one in the last chunk would never
    // show up. 3 and 5 divide unevenly and would catch it.
    let rows = PARALLEL_THRESHOLD * 4;

    let run = |threads: usize| {
        let mut world = world_with(rows);
        world.par_for_each_mut::<Sample>(threads, |_entity, sample| {
            sample.value = heavy(sample.value);
        });
        values(&world)
    };

    let sequential = run(1);
    for threads in [2, 3, 5, 8] {
        assert_eq!(
            run(threads),
            sequential,
            "{threads} threads produced a different answer than 1"
        );
    }
}

#[test]
fn it_agrees_with_for_each_mut_exactly() {
    // Not merely self-consistent across thread counts — it must match the sequential API it is the
    // parallel version *of*, or a system that switched between them would move a replay.
    let rows = PARALLEL_THRESHOLD * 2;

    let mut sequential = world_with(rows);
    sequential.for_each_mut::<Sample>(|_entity, sample| sample.value = heavy(sample.value));

    let mut parallel = world_with(rows);
    parallel.par_for_each_mut::<Sample>(8, |_entity, sample| sample.value = heavy(sample.value));

    assert_eq!(values(&parallel), values(&sequential));
}

#[test]
fn every_row_is_visited_exactly_once() {
    // A chunking bug that skipped or double-counted the boundary row would be invisible in an
    // idempotent operation, so this uses one that is not: adding one.
    let rows = PARALLEL_THRESHOLD * 3 + 7; // deliberately not a round number
    let mut world = world_with(rows);
    world.par_for_each_mut::<Sample>(6, |_entity, sample| sample.value += 1.0);

    let after = values(&world);
    assert_eq!(after.len(), rows);
    for (index, value) in after.iter().enumerate() {
        assert_eq!(
            *value,
            index as f32 + 1.0,
            "row {index} was not visited once"
        );
    }
}

#[test]
fn a_small_world_still_gets_visited() {
    // Below the threshold it runs sequentially and spawns nothing. The behaviour must be identical
    // — the threshold is an optimisation, not a mode.
    let mut world = world_with(10);
    world.par_for_each_mut::<Sample>(8, |_entity, sample| sample.value += 1.0);
    assert_eq!(
        values(&world),
        (1..=10).map(|n| n as f32).collect::<Vec<_>>()
    );
}

#[test]
fn an_empty_world_is_not_an_error() {
    let mut world = World::new();
    world.par_for_each_mut::<Sample>(4, |_entity, sample| sample.value += 1.0);
    assert_eq!(world.for_each_count::<Sample>(), 0);
}

#[test]
fn where_parallel_starts_paying_for_itself() {
    // **Measured, not asserted** — this is what justifies `PARALLEL_THRESHOLD` rather than a guess.
    //
    // `std::thread::scope` spawns fresh threads per call, which is the cost being weighed. The
    // alternative is a persistent pool, which needs either `rayon` or `unsafe` — and ADR 0008
    // forbids the second. This table is the evidence for whether that dependency is worth taking.
    println!("\n--- par_for_each_mut, 24 transcendental ops per row ---");
    println!(
        "(rows below {PARALLEL_THRESHOLD} run the *same* sequential code in both columns, so any \n\
         difference there is measurement noise rather than a speedup.)"
    );
    println!(
        "{:>10} {:>12} {:>12} {:>10}",
        "rows", "1 thread", "8 threads", "speedup"
    );

    for rows in [256_usize, 2_048, 16_384, 131_072] {
        let time = |threads: usize| {
            let mut world = world_with(rows);
            // One untimed pass so allocation and page faults are not in the measurement.
            world.par_for_each_mut::<Sample>(threads, |_e, s| s.value = heavy(s.value));

            let started = std::time::Instant::now();
            for _ in 0..20 {
                world.par_for_each_mut::<Sample>(threads, |_e, s| s.value = heavy(s.value));
            }
            started.elapsed().as_secs_f64() * 1e6 / 20.0
        };

        let one = time(1);
        let many = time(8);
        println!(
            "{rows:>10} {one:>11.1}µs {many:>11.1}µs {:>9.2}x",
            one / many
        );
    }
    println!(
        "\nPARALLEL_THRESHOLD is {PARALLEL_THRESHOLD}. Below it this runs sequentially and spawns \
         nothing.\n"
    );
}
