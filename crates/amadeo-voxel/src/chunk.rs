//! Which chunks a world keeps loaded, and how that is decided.
//!
//! # The whole point of this module is that it is integer arithmetic
//!
//! Which chunks are active is **gameplay state**. ADR 0041 §2 says so: a chunk's collider is what a
//! character stands on, so if two machines disagreed about which chunks exist they would disagree
//! about where the character ends up. That makes residency part of invariant I3 rather than a
//! performance detail.
//!
//! So residency is decided by comparing integers. There is exactly one floating-point step in the
//! whole module — turning a world position into a chunk coordinate in [`ChunkKey::containing`] — and
//! it uses only division, `floor` and a saturating cast, all of which IEEE-754 defines exactly.
//! Everything after that is `i32`.
//!
//! # Concentric boxes, not an octree
//!
//! A viewer keeps a box of chunks around it at each level of detail. Zylann's `godot_voxel` — the
//! closest production analogue to what this engine is building — **migrated away from an octree to
//! exactly this**, for two reasons that apply here: loading patterns are predictable, and several
//! viewers are supported without the split/merge logic having to agree about them. Six of this
//! project's eight target games are co-op or multiplayer (ADR 0006), so more than one viewer is a
//! requirement rather than a nicety.
//!
//! # The apron, expressed as data rather than as a warning
//!
//! [`Field`](crate::Field) is sized in *samples*, not cells: meshing the last cell of a chunk reads
//! samples that belong to the next chunk along. A chunk meshed without them cracks at every seam,
//! and the symptom points at the renderer rather than at the data.
//!
//! That constraint is the reason [`Residency`] has **three** sets rather than one. The `data` set is
//! the `visual` set grown by one chunk in every direction, so the outermost chunk anyone draws
//! always has a loaded neighbour to read its apron from.
//!
//! Previously this was a sentence in `STATUS.md` that a future session had to remember. Now it is
//! [`Residency::data`], and `the_data_box_always_exceeds_the_visual_box` fails if anyone breaks it.

use std::collections::BTreeSet;

/// Which chunk, and at what level of detail.
///
/// # Why `lod` is here now, while everything is still at level 0
///
/// Q25 (level of detail) is deliberately still open, and terrain currently runs at one resolution.
/// The field is here anyway because it is part of a chunk's *identity*: two chunks covering the same
/// volume at different resolutions are different chunks, with different meshes, different jobs and
/// different collider ids. Adding that later would change the key type that chunk storage, the job
/// inbox, the collider registry and this module's residency sets are all built on.
///
/// This is the same move ADR 0038 made for `ShadowMode`: one value now is a value of a field that
/// has to exist anyway, not a shortcut to undo.
///
/// # Ordering
///
/// `Ord` is derived, and the field order is load-bearing enough to state: **`lod` first**, so keys
/// group by detail level before position. Any *total* order would satisfy ADR 0041's requirement
/// that an [`Inbox`](../../amadeo_jobs/struct.Inbox.html) drains in key order rather than completion
/// order; this particular one keeps a level's chunks contiguous, which is the order they are
/// generated and meshed in.
#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Hash)]
pub struct ChunkKey {
    /// Detail level. `0` is full resolution; higher numbers are coarser. Always `0` today.
    pub lod: u8,
    /// Chunk coordinate along x.
    pub x: i32,
    /// Chunk coordinate along y.
    pub y: i32,
    /// Chunk coordinate along z.
    pub z: i32,
}

impl ChunkKey {
    /// A chunk at full resolution.
    #[must_use]
    pub fn new(x: i32, y: i32, z: i32) -> Self {
        Self { lod: 0, x, y, z }
    }

    /// A chunk at a given detail level.
    #[must_use]
    pub fn at_lod(lod: u8, x: i32, y: i32, z: i32) -> Self {
        Self { lod, x, y, z }
    }

    /// Which chunk contains a world position.
    ///
    /// `chunk_size` is the width of a chunk in world units. A chunk owns the half-open range
    /// `[k * size, (k + 1) * size)` on each axis, so a position exactly on a boundary belongs to the
    /// chunk above it — one rule, applied on all three axes, so a point is never in two chunks or in
    /// none.
    ///
    /// # Determinism
    ///
    /// The only floating-point step in this module. Division, `floor` and the cast to `i32` are all
    /// exactly specified by IEEE-754 and by Rust (the cast saturates rather than wrapping or being
    /// undefined), so two machines agree bit for bit.
    ///
    /// A `chunk_size` that is zero or negative would put every position in the same chunk or invert
    /// the world, so it is treated as `1.0` rather than panicking — this runs inside a tick, and a
    /// panic there takes the game down.
    #[must_use]
    pub fn containing(position: [f32; 3], chunk_size: f32) -> Self {
        let size = if chunk_size > 0.0 { chunk_size } else { 1.0 };
        Self::new(
            (position[0] / size).floor() as i32,
            (position[1] / size).floor() as i32,
            (position[2] / size).floor() as i32,
        )
    }

    /// The world position of this chunk's low corner.
    ///
    /// What a chunk's mesh and collider are translated by: [`surface_nets`](crate::surface_nets)
    /// works in grid units starting at zero, so placing a chunk is this offset and nothing else.
    #[must_use]
    pub fn origin(&self, chunk_size: f32) -> [f32; 3] {
        [
            self.x as f32 * chunk_size,
            self.y as f32 * chunk_size,
            self.z as f32 * chunk_size,
        ]
    }

    /// The same chunk offset by a whole number of chunks, saturating at the ends of `i32`.
    ///
    /// Saturating rather than wrapping: a world large enough to overflow two billion chunks does not
    /// exist, and if one did, clamping puts the player at an edge where wrapping would teleport them
    /// to the opposite corner of the world.
    #[must_use]
    pub fn offset(&self, dx: i32, dy: i32, dz: i32) -> Self {
        Self {
            lod: self.lod,
            x: self.x.saturating_add(dx),
            y: self.y.saturating_add(dy),
            z: self.z.saturating_add(dz),
        }
    }
}

/// Something the world loads terrain around — a player, a camera, a dedicated server's area of
/// interest.
///
/// # Two radii, because a chunk has two products with different rules
///
/// ADR 0041 §2: a chunk's **mesh** is drawn and nothing else, so it may arrive whenever; its
/// **collider** is gameplay, because a character stands on it. They therefore get separate radii,
/// and `collision` is normally much the smaller — collision only has to exist where something can
/// actually touch it, and every chunk that has one is a chunk the simulation may have to **block**
/// on.
///
/// This mirrors `godot_voxel`'s per-viewer `requires_visuals` / `requires_collisions` flags, which
/// is the same split arrived at independently.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct Viewer {
    /// The chunk this viewer is standing in.
    pub centre: ChunkKey,
    /// How many chunks out, in every direction, must be **drawn**.
    pub visual_radius: i32,
    /// How many chunks out must be **solid**.
    ///
    /// Should not exceed `visual_radius`: a chunk you can stand on but cannot see is a hole in the
    /// world that stops you walking. [`Residency::of`] clamps it rather than trusting the caller.
    pub collision_radius: i32,
}

impl Viewer {
    /// A viewer with the given radii, at a world position.
    #[must_use]
    pub fn at(
        position: [f32; 3],
        chunk_size: f32,
        visual_radius: i32,
        collision_radius: i32,
    ) -> Self {
        Self {
            centre: ChunkKey::containing(position, chunk_size),
            visual_radius,
            collision_radius,
        }
    }
}

/// What must be loaded this tick, in three nested sets.
///
/// The invariant, which `the_sets_are_nested` asserts: `collision ⊆ visual ⊆ data`.
///
/// - **`collision`** — must be *solid*. Gameplay blocks on these (ADR 0041 §2).
/// - **`visual`** — must be *drawn*. May arrive whenever.
/// - **`data`** — must have *samples*, because meshing a `visual` chunk reads one sample into each
///   neighbour. This is the apron, and it is why `data` is `visual` grown by one chunk.
///
/// `BTreeSet` rather than `HashSet`, and not as a style preference: `CLAUDE.md` trap 2 names
/// unordered iteration as a determinism leak, and everything downstream — which chunks to generate,
/// in what order to drain their results — iterates these.
#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub struct Residency {
    /// Chunks that need samples. A superset of `visual` by exactly one chunk of margin.
    pub data: BTreeSet<ChunkKey>,
    /// Chunks that need a mesh.
    pub visual: BTreeSet<ChunkKey>,
    /// Chunks that need a collider.
    pub collision: BTreeSet<ChunkKey>,
}

impl Residency {
    /// What the given viewers require, as the union of their boxes.
    ///
    /// A union rather than anything cleverer: two players standing together should not load a chunk
    /// twice, and two players far apart should each get their own region. Order of the viewers does
    /// not affect the result, which matters because a set of players is not inherently ordered.
    ///
    /// A negative radius is treated as zero — the chunk the viewer stands in is always loaded, since
    /// a viewer standing in nothing falls out of the world.
    #[must_use]
    pub fn of(viewers: &[Viewer]) -> Self {
        let mut residency = Self::default();

        for viewer in viewers {
            let visual = viewer.visual_radius.max(0);
            // Clamped, not trusted: collision beyond the drawn region is invisible ground.
            let collision = viewer.collision_radius.clamp(0, visual);

            // The apron. `visual + 1` is the whole of ADR 0042's consequence 3, and the reason it is
            // computed here rather than remembered by a caller.
            insert_box(&mut residency.data, viewer.centre, visual.saturating_add(1));
            insert_box(&mut residency.visual, viewer.centre, visual);
            insert_box(&mut residency.collision, viewer.centre, collision);
        }

        residency
    }

    /// Chunks in `self` that were not in `previous` — what to start work on.
    #[must_use]
    pub fn newly_visible(&self, previous: &Residency) -> Vec<ChunkKey> {
        self.visual.difference(&previous.visual).copied().collect()
    }

    /// Chunks in `previous` that are not in `self` — what to throw away.
    #[must_use]
    pub fn no_longer_needed(&self, previous: &Residency) -> Vec<ChunkKey> {
        previous.data.difference(&self.data).copied().collect()
    }
}

/// Adds every chunk within `radius` of `centre` on all three axes to `set`.
///
/// A cube rather than a sphere. It is one comparison per axis instead of a distance, it tiles the
/// world exactly, and the corners that a sphere would have excluded cost a few chunks rather than
/// the branch and the arithmetic on every one.
fn insert_box(set: &mut BTreeSet<ChunkKey>, centre: ChunkKey, radius: i32) {
    for dz in -radius..=radius {
        for dy in -radius..=radius {
            for dx in -radius..=radius {
                set.insert(centre.offset(dx, dy, dz));
            }
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn a_position_maps_to_the_chunk_containing_it() {
        // The half-open rule: a chunk owns [k * size, (k + 1) * size).
        assert_eq!(
            ChunkKey::containing([0.0, 0.0, 0.0], 32.0),
            ChunkKey::new(0, 0, 0)
        );
        assert_eq!(
            ChunkKey::containing([31.9, 0.0, 0.0], 32.0),
            ChunkKey::new(0, 0, 0)
        );
        assert_eq!(
            ChunkKey::containing([32.0, 0.0, 0.0], 32.0),
            ChunkKey::new(1, 0, 0)
        );
    }

    #[test]
    fn negative_positions_do_not_fold_onto_the_positive_side() {
        // Truncation towards zero would put -1.0 and +1.0 both in chunk 0, which makes a world
        // symmetric about the origin quietly wrong on one side only -- the kind of bug that looks
        // like terrain generation until someone walks west.
        assert_eq!(
            ChunkKey::containing([-0.1, 0.0, 0.0], 32.0),
            ChunkKey::new(-1, 0, 0)
        );
        assert_eq!(
            ChunkKey::containing([-32.0, 0.0, 0.0], 32.0),
            ChunkKey::new(-1, 0, 0)
        );
        assert_eq!(
            ChunkKey::containing([-32.1, 0.0, 0.0], 32.0),
            ChunkKey::new(-2, 0, 0)
        );
    }

    #[test]
    fn a_chunk_size_of_zero_is_survivable() {
        // This runs inside a tick. A panic here takes the game down, and returning something
        // arbitrary-but-defined does not.
        assert_eq!(
            ChunkKey::containing([5.0, 0.0, 0.0], 0.0),
            ChunkKey::new(5, 0, 0)
        );
        assert_eq!(
            ChunkKey::containing([5.0, 0.0, 0.0], -32.0),
            ChunkKey::new(5, 0, 0)
        );
    }

    #[test]
    fn an_origin_round_trips_back_to_its_own_chunk() {
        let key = ChunkKey::new(-3, 7, 12);
        assert_eq!(ChunkKey::containing(key.origin(32.0), 32.0), key);
    }

    #[test]
    fn the_sets_are_nested() {
        // **The invariant that matters.** collision must be solid, visual must be drawn, and data
        // must exist so that visual can be meshed at all.
        let residency = Residency::of(&[Viewer {
            centre: ChunkKey::new(0, 0, 0),
            visual_radius: 3,
            collision_radius: 1,
        }]);

        assert!(residency.collision.is_subset(&residency.visual));
        assert!(residency.visual.is_subset(&residency.data));
    }

    #[test]
    fn the_data_box_always_exceeds_the_visual_box() {
        // **The apron, as an assertion rather than as a comment.** Every drawn chunk needs all six
        // neighbours to have samples, or its outermost cells mesh against nothing and it cracks at
        // the seam -- which reads as a rendering bug.
        let residency = Residency::of(&[Viewer {
            centre: ChunkKey::new(0, 0, 0),
            visual_radius: 2,
            collision_radius: 0,
        }]);

        for chunk in &residency.visual {
            for (dx, dy, dz) in [
                (1, 0, 0),
                (-1, 0, 0),
                (0, 1, 0),
                (0, -1, 0),
                (0, 0, 1),
                (0, 0, -1),
            ] {
                let neighbour = chunk.offset(dx, dy, dz);
                assert!(
                    residency.data.contains(&neighbour),
                    "{chunk:?} is drawn but its neighbour {neighbour:?} has no samples to mesh against"
                );
            }
        }
    }

    #[test]
    fn collision_beyond_the_visible_region_is_clamped_away() {
        // Ground you can stand on but cannot see is a hole that stops you walking, and it is a
        // configuration mistake rather than something to honour.
        let residency = Residency::of(&[Viewer {
            centre: ChunkKey::new(0, 0, 0),
            visual_radius: 1,
            collision_radius: 5,
        }]);
        assert!(residency.collision.is_subset(&residency.visual));
        assert_eq!(residency.collision.len(), residency.visual.len());
    }

    #[test]
    fn the_order_viewers_are_given_in_cannot_change_the_answer() {
        // I3. A set of players is not inherently ordered, so residency must not depend on how the
        // world happened to iterate them.
        let a = Viewer {
            centre: ChunkKey::new(0, 0, 0),
            visual_radius: 2,
            collision_radius: 1,
        };
        let b = Viewer {
            centre: ChunkKey::new(9, 0, -4),
            visual_radius: 1,
            collision_radius: 1,
        };
        assert_eq!(Residency::of(&[a, b]), Residency::of(&[b, a]));
    }

    #[test]
    fn two_viewers_standing_together_load_what_one_would() {
        let solo = Residency::of(&[Viewer {
            centre: ChunkKey::new(4, 0, 4),
            visual_radius: 2,
            collision_radius: 1,
        }]);
        let together = Residency::of(&[
            Viewer {
                centre: ChunkKey::new(4, 0, 4),
                visual_radius: 2,
                collision_radius: 1,
            },
            Viewer {
                centre: ChunkKey::new(4, 0, 4),
                visual_radius: 2,
                collision_radius: 1,
            },
        ]);
        assert_eq!(solo, together);
    }

    #[test]
    fn a_viewer_always_loads_the_chunk_it_stands_in() {
        // A radius of zero, or a nonsensical negative one, must still leave the ground underfoot.
        for radius in [0, -1, -100] {
            let residency = Residency::of(&[Viewer {
                centre: ChunkKey::new(2, 3, 4),
                visual_radius: radius,
                collision_radius: radius,
            }]);
            assert!(residency.visual.contains(&ChunkKey::new(2, 3, 4)));
            assert!(residency.collision.contains(&ChunkKey::new(2, 3, 4)));
        }
    }

    #[test]
    fn walking_one_chunk_east_adds_a_face_and_drops_a_face() {
        // What streaming actually does every time the player crosses a boundary. Asserting the
        // counts rather than the contents catches an off-by-one in the box that a subset check
        // would not.
        let before = Residency::of(&[Viewer {
            centre: ChunkKey::new(0, 0, 0),
            visual_radius: 2,
            collision_radius: 0,
        }]);
        let after = Residency::of(&[Viewer {
            centre: ChunkKey::new(1, 0, 0),
            visual_radius: 2,
            collision_radius: 0,
        }]);

        // A 5x5x5 box moving one step: one 5x5 face arrives, one leaves.
        assert_eq!(after.newly_visible(&before).len(), 25);
        assert_eq!(after.no_longer_needed(&before).len(), 49); // the data box is 7x7x7
    }

    #[test]
    fn standing_still_changes_nothing() {
        let viewer = Viewer {
            centre: ChunkKey::new(1, 2, 3),
            visual_radius: 2,
            collision_radius: 1,
        };
        let residency = Residency::of(&[viewer]);
        assert!(residency.newly_visible(&residency).is_empty());
        assert!(residency.no_longer_needed(&residency).is_empty());
    }

    #[test]
    fn keys_order_by_detail_level_first() {
        // The order an Inbox drains in (ADR 0041). Any total order is correct; this one keeps a
        // level's chunks contiguous, which is the order they are generated and meshed in.
        let fine = ChunkKey::at_lod(0, 100, 100, 100);
        let coarse = ChunkKey::at_lod(1, -100, -100, -100);
        assert!(fine < coarse);
    }
}
