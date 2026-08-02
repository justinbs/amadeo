//! How many sprites the batcher can turn into draw calls, and how many calls that costs.
//!
//! # Why this file exists
//!
//! Q3's remaining third asked which shape the render pipeline should take.
//! `docs/06-open-questions.md` was explicit that it should be **decided against a real throughput
//! number rather than argued beforehand**, and `docs/04-subsystems.md` §4 named the figure to beat:
//! 20,000 sprites at 60 fps. This is that measurement, and ADR 0023 is what it decided.
//!
//! # What is asserted versus what is reported
//!
//! **Batch counts are asserted.** They are a pure function of the world — no clock involved — and
//! they are the number that actually decides GPU cost, because binding a texture is the expensive
//! state change and drawing another rectangle is not. A regression here is a real regression.
//!
//! **Times are printed, not asserted tightly.** CI runners are shared and variable, and
//! `CLAUDE.md` §6 forbids tests that depend on wall-clock. The only timing assertion is a
//! deliberately enormous ceiling that would catch an algorithmic collapse — an accidental O(n²) —
//! and nothing subtler. Run with `--nocapture` to see the table.
//!
//! ```text
//! cargo test -p amadeo-render --test sprite_throughput -- --nocapture
//! ```

use amadeo_ecs::World;
use amadeo_render::{SortOrder, Sprite, collect_sprites};
use amadeo_transform::Transform;

/// One 60 Hz frame. The whole budget, for everything — so the batcher wanting a *fraction* of it is
/// the actual bar.
const FRAME_BUDGET: std::time::Duration = std::time::Duration::from_micros(16_667);

/// Fills a world with `count` sprites spread over `textures` textures and `layers` sort orders.
///
/// Textures and layers are assigned so they are **independent** of each other — texture cycles every
/// entity, layer cycles every `textures` entities — so all `textures * layers` combinations occur.
/// An earlier version used `i % textures` and `i % layers`, which correlates them whenever one
/// divides the other: with 8 textures and 4 layers it produced only 8 distinct pairs, not 32.
///
/// This deliberately **interleaves** textures, which is the worst realistic case for batching since
/// consecutive entities never share one. A real tilemap is far more clustered, so these numbers are
/// pessimistic rather than flattering.
fn populate(count: usize, textures: usize, layers: usize) -> World {
    let mut world = World::new();
    for i in 0..count {
        let entity = world.spawn();
        world.insert(entity, Transform::at(i as f32, 0.0));
        world.insert(
            entity,
            Sprite::new(format!("texture_{}", i % textures), 1.0, 1.0),
        );
        world.insert(entity, SortOrder::new(((i / textures) % layers) as i32));
    }
    world
}

/// Runs the batcher a few times and returns the best time, plus the batch count.
///
/// Best rather than mean: this is measuring how long the work *takes*, and a scheduler interruption
/// makes a sample longer but never shorter, so the minimum is the least noisy estimator available.
fn measure(world: &World, runs: u32) -> (std::time::Duration, usize) {
    let mut best = std::time::Duration::MAX;
    let mut batches = 0;

    for _ in 0..runs {
        let start = std::time::Instant::now();
        let result = collect_sprites(world);
        let elapsed = start.elapsed();

        batches = result.len();
        best = best.min(elapsed);
        // Kept alive across the timer so the optimiser cannot delete the work being measured.
        std::hint::black_box(&result);
    }

    (best, batches)
}

#[test]
fn twenty_thousand_sprites_fit_in_a_frame() {
    // The figure `docs/04-subsystems.md` §4 named, at a realistic texture spread: a handful of
    // sheets and a handful of layers, which is what a tile-based game looks like.
    let world = populate(20_000, 8, 4);
    let (elapsed, batches) = measure(&world, 5);

    println!(
        "\n20,000 sprites / 8 textures / 4 layers: {elapsed:?}, {batches} batches \
         ({:.1}% of a 60 Hz frame)",
        elapsed.as_secs_f64() / FRAME_BUDGET.as_secs_f64() * 100.0
    );

    // The deterministic assertion. Eight textures across four layers can produce at most 32
    // distinct (order, texture) pairs, and the population hits all of them -- so a perfect batcher
    // emits exactly 32 no matter how the sprites are interleaved. Anything more means batching
    // fragmented; this is the property the whole module exists for.
    assert_eq!(
        batches, 32,
        "20,000 interleaved sprites should collapse to one batch per (order, texture) pair"
    );

    // The loose ceiling: five whole frames. Not a performance bar -- an algorithmic-collapse alarm.
    assert!(
        elapsed < FRAME_BUDGET * 5,
        "batching 20,000 sprites took {elapsed:?}, which suggests the cost stopped being linear"
    );
}

#[test]
fn a_single_tilesheet_is_always_one_draw_call() {
    // The Terraria and RimWorld case, and the reason `Sprite::region` exists. However many tiles a
    // world has, if they share a sheet and a layer they cost exactly one state change.
    let world = populate(50_000, 1, 1);
    let (elapsed, batches) = measure(&world, 3);

    println!("50,000 sprites / 1 texture / 1 layer:  {elapsed:?}, {batches} batch");

    assert_eq!(batches, 1, "one texture and one layer must never fragment");
}

#[test]
fn cost_grows_linearly_with_sprite_count() {
    // The property that matters more than any single number: doubling the sprites must roughly
    // double the work. A superlinear batcher looks fine at 1,000 and dies at 50,000, which is
    // exactly the scale Stellaris and Terraria live at.
    //
    // The bound is deliberately generous (4x for a 4x increase would be linear; 12x allows a lot of
    // measurement noise on a shared runner) because the failure being caught is quadratic, which
    // would show up as ~16x.
    let small = populate(10_000, 8, 4);
    let large = populate(40_000, 8, 4);

    let (small_time, _) = measure(&small, 5);
    let (large_time, _) = measure(&large, 5);

    let ratio = large_time.as_secs_f64() / small_time.as_secs_f64().max(f64::EPSILON);
    println!("10,000 -> 40,000 sprites (4x): time went up {ratio:.2}x");

    assert!(
        ratio < 12.0,
        "4x the sprites cost {ratio:.2}x the time, which is not linear growth"
    );
}

#[test]
fn batch_count_tracks_distinct_texture_and_layer_pairs() {
    // Pins the batching rule itself, with no clock involved: a batch is exactly one
    // (sort order, texture) pair. This is the assertion that would fail if someone "improved"
    // batching by merging across sort orders, which would silently break layering.
    for (textures, layers) in [(1, 1), (4, 1), (1, 4), (3, 5), (8, 4)] {
        let world = populate(2_000, textures, layers);
        let batches = collect_sprites(&world).len();

        assert_eq!(
            batches,
            textures * layers,
            "{textures} textures across {layers} layers should be {} batches",
            textures * layers
        );
    }
}

#[test]
fn worst_case_every_sprite_its_own_texture() {
    // The pathological case, measured so its cost is known rather than feared. A thousand sprites
    // with a thousand textures cannot batch at all -- that is not a batcher failure, it is what the
    // scene asked for. Worth having a number for, because it is what a game hits if it gives every
    // object its own texture instead of an atlas.
    let world = populate(1_000, 1_000, 1);
    let (elapsed, batches) = measure(&world, 3);

    println!("1,000 sprites / 1,000 textures (worst case): {elapsed:?}, {batches} batches");
    assert_eq!(
        batches, 1_000,
        "nothing can be batched here, by construction"
    );
}
