//! Turning sprites into draw calls — the batcher.
//!
//! # What a batch is, and why the count matters more than the sprite count
//!
//! Drawing a textured sprite is cheap. **Changing which texture is bound is not.** So a 2D renderer's
//! speed is decided by how many times it has to switch state, not by how many rectangles it draws:
//! twenty thousand sprites sharing one texture is one draw call and is fast; twenty thousand sprites
//! each with their own texture is twenty thousand draw calls and is not.
//!
//! Batching is the act of collapsing a run of same-texture sprites into one call. This module does
//! it, and [`FrameData::batch_count`](crate::FrameData::batch_count) is the number to watch.
//!
//! # The decision this module encodes — ADR 0023, Q3's last third
//!
//! Draw order and batching pull against each other. Sorting purely by [`SortOrder`] preserves exactly
//! what the author asked for but interleaves textures, producing a batch every time the texture
//! changes. Sorting by texture batches perfectly but reorders sprites, which is wrong the moment two
//! of them overlap with transparency.
//!
//! **The rule here: sort by `(order, texture)`.** Within one sort order, sprites are grouped by
//! texture; across sort orders, the author's layering is exact. So:
//!
//! - **Layering is never violated.** A sprite in order 5 always draws over one in order 3.
//! - **Within a single order, draw order between *different* textures is not guaranteed** — that is
//!   the price, and it is the same trade every 2D engine makes. Two sprites that must overlap in a
//!   specific way need different sort orders, which is what `SortOrder` is *for*.
//! - **Within one order and one texture, order is stable** and follows entity order, so it is
//!   reproducible (invariant I3).
//!
//! Stating the cost plainly: if you put a character and its shadow on the same `SortOrder` with
//! different textures, which draws first is decided by texture id, not by you. Give them different
//! orders. `amadeo describe SortOrder` says what it is for.
//!
//! # None of this can touch the simulation
//!
//! Everything here reads the world and writes a [`FrameData`](crate::FrameData) into a `Service`.
//! Rendering cannot move the state hash (ADR 0009), which is what lets a headless run and a windowed
//! run agree (invariant I7).

use crate::backend::{SpriteBatch, SpriteInstance};
use crate::components::{SortOrder, Sprite};
use crate::local_matrix;
use amadeo_ecs::World;
use amadeo_transform::{GlobalTransform, Transform};

/// The label the app layer registers [`collect_sprites`] under.
pub const COLLECT_SPRITES: &str = "collect_sprites";

/// One sprite, flattened, with the two integers it will be sorted by.
///
/// A named struct rather than a tuple because a five-field tuple sorted on two of its fields is
/// exactly the kind of code that is unreadable six months later.
struct Keyed {
    /// The entity's sort order. Primary sort key.
    order: i32,
    /// Index into the sorted texture table. Secondary sort key — see [`collect_sprites`] for why
    /// this is an integer rather than the texture name.
    texture: u32,
    /// The instance itself, already built.
    instance: SpriteInstance,
}

/// Reads every sprite in the world and groups them into draw calls.
///
/// Returns batches in ascending [`SortOrder`], each holding one texture's instances.
///
/// Read-only: uses [`World::iter_pair`](amadeo_ecs::World::iter_pair) rather than a mutable query, so
/// drawing does not mark every sprite as changed each frame and make change detection worthless.
///
/// # Why the sort key is an integer and not the texture name
///
/// **This was measured, not assumed.** Sorting 20,000 sprites directly by `(order, &str)` costs
/// roughly 285,000 string comparisons, and at that scale it dominated everything else the batcher
/// did — the first working version spent about half a 60 Hz frame here.
///
/// So the distinct texture names are collected once into a sorted table, each sprite is keyed by its
/// *index* in that table, and the sort compares two integers. The table is sorted by name, so the
/// index ordering is a function of the names themselves rather than of entity iteration order — the
/// result is reproducible for reasons that do not depend on how the world happened to be built
/// (invariant I3).
///
/// Numbers, and the version that motivated this, are in `tests/sprite_throughput.rs`.
#[must_use]
pub fn collect_sprites(world: &World) -> Vec<SpriteBatch> {
    // Pass one: build every instance, and learn which textures exist.
    //
    // A `BTreeSet` because the table has to end up sorted, and inserting into one is cheaper than
    // sorting a list with duplicates afterwards. It holds borrowed names, so no allocation happens
    // per sprite — only per *distinct* texture, later.
    let mut distinct: std::collections::BTreeSet<&str> = std::collections::BTreeSet::new();
    let mut rows: Vec<(&str, i32, SpriteInstance)> = Vec::new();

    // One query, four components, two of them optional. `SortOrder` and `GlobalTransform` are
    // resolved **once per archetype** rather than looked up per entity — which is the whole of Q17
    // and, after ADR 0024, was the batcher's dominant remaining cost.
    for (_entity, (transform, sprite, order, global)) in world.query::<(
        &Transform,
        &Sprite,
        Option<&SortOrder>,
        Option<&GlobalTransform>,
    )>() {
        distinct.insert(sprite.texture.as_str());
        rows.push((
            sprite.texture.as_str(),
            order.copied().unwrap_or_default().order,
            instance_for(transform, sprite, global),
        ));
    }

    let table: Vec<&str> = distinct.into_iter().collect();

    // Pass two: swap each name for its index, so the sort is integers only.
    let mut keyed: Vec<Keyed> = rows
        .into_iter()
        .map(|(texture, order, instance)| Keyed {
            order,
            // Always found -- every name in `rows` was inserted into `distinct` above. Both arms
            // yield the index, so this needs no unwrap and no unreachable branch.
            texture: match table.binary_search(&texture) {
                Ok(index) | Err(index) => index as u32,
            },
            instance,
        })
        .collect();

    keyed.sort_unstable_by_key(|row| (row.order, row.texture));

    // Pass three: walk the sorted list, starting a batch whenever the pair changes. Because the list
    // is sorted by exactly that pair, every run is contiguous — so one comparison against the
    // previous row is enough, with no grouping map and one string allocation per batch rather than
    // per sprite.
    let mut batches: Vec<SpriteBatch> = Vec::new();
    let mut current: Option<(i32, u32)> = None;

    for row in keyed {
        if current == Some((row.order, row.texture)) {
            if let Some(last) = batches.last_mut() {
                last.instances.push(row.instance);
            }
        } else {
            current = Some((row.order, row.texture));
            batches.push(SpriteBatch {
                texture: table
                    .get(row.texture as usize)
                    .copied()
                    .unwrap_or_default()
                    .to_string(),
                order: row.order,
                instances: vec![row.instance],
            });
        }
    }

    batches
}

/// Flattens one entity's transform and sprite into a GPU-ready instance.
///
/// Everything it needs comes from the query rather than from further lookups — that is the point of
/// asking for all four components at once.
fn instance_for(
    transform: &Transform,
    sprite: &Sprite,
    global: Option<&GlobalTransform>,
) -> SpriteInstance {
    // `GlobalTransform` is what this entity's parents made of its transform, so this is where
    // hierarchy reaches the screen. Falls back to the local transform when propagation has not run —
    // correct for an unparented entity, and better than drawing nothing for a game that forgot the
    // system.
    let placement = match global {
        Some(global) => *global,
        None => GlobalTransform::from(local_matrix(transform)),
    };

    let matrix = placement.to_mat4();
    let translation = matrix.translation();

    // The composed matrix's first two columns *are* the sprite's world-space axes, already carrying
    // every parent's rotation and scale. Multiplying each by the sprite's own size gives the two
    // half-extent axes the backend needs — four multiplies, and no trigonometry in either direction.
    //
    // The previous version decomposed these into a width, a height, and an angle, which the shader
    // would then have had to turn back into the same two vectors. See `SpriteInstance` for the
    // measurement that removed it.
    SpriteInstance {
        center: [translation[0], translation[1]],
        axes: [
            [
                matrix.columns[0][0] * sprite.size[0],
                matrix.columns[0][1] * sprite.size[0],
            ],
            [
                matrix.columns[1][0] * sprite.size[1],
                matrix.columns[1][1] * sprite.size[1],
            ],
        ],
        color: sprite.color,
        region: sprite.region,
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use amadeo_ecs::Entity;

    fn add_sprite(world: &mut World, texture: &str, x: f32, order: i32) -> Entity {
        let entity = world.spawn();
        world.insert(entity, Transform::at(x, 0.0));
        world.insert(entity, Sprite::new(texture, 1.0, 1.0));
        world.insert(entity, SortOrder::new(order));
        entity
    }

    #[test]
    fn sprites_sharing_a_texture_collapse_into_one_batch() {
        // The entire point of the module: this is the difference between one draw call and five.
        let mut world = World::new();
        for i in 0..5 {
            add_sprite(&mut world, "tiles", i as f32, 0);
        }

        let batches = collect_sprites(&world);
        assert_eq!(batches.len(), 1);
        assert_eq!(batches[0].instances.len(), 5);
        assert_eq!(batches[0].texture, "tiles");
    }

    #[test]
    fn interleaved_textures_in_one_order_still_produce_one_batch_each() {
        // Sorting by (order, texture) is what buys this. Sorting by order alone would produce six
        // batches from these six sprites, which is the failure the ADR 0023 trade-off exists to
        // avoid.
        let mut world = World::new();
        for i in 0..3 {
            add_sprite(&mut world, "tiles", i as f32, 0);
            add_sprite(&mut world, "items", i as f32, 0);
        }

        let batches = collect_sprites(&world);
        assert_eq!(batches.len(), 2, "got {batches:#?}");
        assert_eq!(batches[0].instances.len(), 3);
        assert_eq!(batches[1].instances.len(), 3);
    }

    #[test]
    fn layering_is_never_violated_even_when_it_costs_a_batch() {
        // The half of the trade that is NOT negotiable. `background` appears in two sort orders, so
        // it must be split into two batches rather than merged into one -- merging would draw the
        // order-10 background behind the order-5 character.
        let mut world = World::new();
        add_sprite(&mut world, "background", 0.0, 0);
        add_sprite(&mut world, "character", 1.0, 5);
        add_sprite(&mut world, "background", 2.0, 10);

        let batches = collect_sprites(&world);

        assert_eq!(batches.len(), 3, "got {batches:#?}");
        let orders: Vec<i32> = batches.iter().map(|b| b.order).collect();
        assert_eq!(orders, vec![0, 5, 10], "batches must ascend by sort order");
    }

    #[test]
    fn the_result_is_reproducible() {
        // Invariant I3 reaching the renderer. Two collections of the same world must agree exactly,
        // or a frame is not a function of the state that produced it.
        let mut world = World::new();
        for i in 0..20 {
            let texture = if i % 3 == 0 { "a" } else { "b" };
            add_sprite(&mut world, texture, i as f32, i % 4);
        }

        assert_eq!(collect_sprites(&world), collect_sprites(&world));
    }

    #[test]
    fn a_sprite_with_no_sort_order_draws_at_zero() {
        let mut world = World::new();
        let entity = world.spawn();
        world.insert(entity, Transform::at(0.0, 0.0));
        world.insert(entity, Sprite::new("tiles", 1.0, 1.0));

        let batches = collect_sprites(&world);
        assert_eq!(batches.len(), 1);
        assert_eq!(batches[0].order, 0);
    }

    #[test]
    fn a_parents_transform_reaches_the_sprite() {
        use amadeo_transform::{Parent, propagate_transforms};

        let mut world = World::new();

        let mut turned = Transform::default();
        turned.rotation[2] = 90.0;
        let parent = world.spawn();
        world.insert(parent, turned);

        let child = world.spawn();
        world.insert(child, Transform::at(2.0, 0.0));
        world.insert(child, Parent(parent));
        world.insert(child, Sprite::new("tiles", 1.0, 1.0));

        propagate_transforms(&mut world);
        let batches = collect_sprites(&world);
        let drawn = batches[0].instances[0];

        // The parent's quarter turn moves the child from (2, 0) to (0, 2).
        assert!(drawn.center[0].abs() < 1e-5, "got {:?}", drawn.center);
        assert!(
            (drawn.center[1] - 2.0).abs() < 1e-5,
            "got {:?}",
            drawn.center
        );
    }

    #[test]
    fn a_region_survives_into_the_instance() {
        // What makes a tilesheet work: one texture, many cells, one batch.
        let mut world = World::new();
        let entity = world.spawn();
        world.insert(entity, Transform::at(0.0, 0.0));
        world.insert(
            entity,
            Sprite::new("tiles", 1.0, 1.0).with_region(0.25, 0.5, 0.25, 0.25),
        );

        let batches = collect_sprites(&world);
        assert_eq!(batches[0].instances[0].region, [0.25, 0.5, 0.25, 0.25]);
    }

    #[test]
    fn a_whole_tilesheet_is_one_draw_call() {
        // The Terraria/RimWorld case stated as a test: a thousand tiles, all different cells of one
        // sheet, must cost exactly one state change.
        let mut world = World::new();
        for i in 0..1000 {
            let entity = world.spawn();
            world.insert(entity, Transform::at(i as f32, 0.0));
            let cell = (i % 16) as f32 / 16.0;
            world.insert(
                entity,
                Sprite::new("tilesheet", 1.0, 1.0).with_region(cell, 0.0, 0.0625, 0.0625),
            );
        }

        let batches = collect_sprites(&world);
        assert_eq!(batches.len(), 1, "a tilesheet must not fragment");
        assert_eq!(batches[0].instances.len(), 1000);
    }

    #[test]
    fn an_empty_world_produces_no_batches() {
        assert!(collect_sprites(&World::new()).is_empty());
    }

    #[test]
    fn quads_and_sprites_coexist() {
        // Sprites did not replace `Quad`; an untextured rectangle is still the cheapest thing to
        // draw and the demo uses it. The two passes must not see each other's entities.
        use crate::components::Quad;

        let mut world = World::new();
        add_sprite(&mut world, "tiles", 0.0, 0);

        let quad = world.spawn();
        world.insert(quad, Transform::at(1.0, 0.0));
        world.insert(quad, Quad::default());

        let batches = collect_sprites(&world);
        assert_eq!(batches.len(), 1);
        assert_eq!(batches[0].instances.len(), 1, "a Quad is not a Sprite");
    }
}
