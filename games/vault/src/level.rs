//! The arena's walls, and the map they come from.
//!
//! # Why the walls are here and everything else is in the scene file
//!
//! Invariant I1 wants the level to be a text file, and `scenes/vault.scene` is one — it holds the
//! player, the wardens with their routes, the sigils, the floor, and the score readout. Everything
//! **designed** is in there, and editing it changes the game with no recompile.
//!
//! The walls are not, and the reason is a real finding rather than a shortcut.
//!
//! A wall tile in the scene format costs ten lines: an entity line, a `Wall` marker, four `Sprite`
//! fields, three `Transform` fields. This arena has **forty-four** of them, which is four hundred
//! lines of near-identical text to author and to re-read on every diff. That is what **prefabs**
//! exist to fix — `entity w1 "Wall" from wall_tile` would be one line — and prefab instancing is
//! **blocked on Q7**, where ADR 0014 and ADR 0020 disagree about whether `from` holds a path or an
//! asset id.
//!
//! So the honest position: the scene format is fine for designed content and impractical for
//! repeated content, and the thing that closes the gap is already identified and already blocked.
//! Until then the tile grid comes from [`MAP`] below, which is at least still a picture of the
//! level that a human can edit.
//!
//! Written up in `STATUS.md` as one of the things building a real game found.

use crate::game::Wall;
use amadeo_app::App;
use amadeo_render::{SortOrder, Sprite};
use amadeo_transform::Transform;

/// The arena, one character per tile.
///
/// `#` is a wall and everything else is open. The scene file places the contents; this places the
/// structure they sit in, and the two have to agree — `every_authored_entity_stands_on_open_floor`
/// is the test that says so.
pub const MAP: &[&str] = &[
    "#############",
    "#...........#",
    "#..##...##..#",
    "#...........#",
    "#..##...##..#",
    "#...........#",
    "#############",
];

/// How wide a tile is, in world units.
pub const TILE: f32 = 1.0;

/// The world-space centre of a tile.
///
/// The grid is centred on the origin, so the camera needs no offset and a position in the scene file
/// reads as a position on the map. Row 0 is the **top**, which is why y is subtracted: the map reads
/// like the screen.
#[must_use]
pub fn tile_centre(column: usize, row: usize) -> [f32; 2] {
    let width = MAP[0].len() as f32;
    let height = MAP.len() as f32;
    [
        (column as f32 - (width - 1.0) / 2.0) * TILE,
        ((height - 1.0) / 2.0 - row as f32) * TILE,
    ]
}

/// Whether a tile is solid.
#[must_use]
pub fn is_wall(column: usize, row: usize) -> bool {
    MAP.get(row)
        .and_then(|line| line.as_bytes().get(column))
        .is_some_and(|tile| *tile == b'#')
}

/// Whether a world position falls on a wall tile.
///
/// Used by the tests that check the authored content agrees with the map.
#[must_use]
pub fn wall_at_world(position: [f32; 2]) -> bool {
    walls().any(|(column, row)| {
        let centre = tile_centre(column, row);
        (position[0] - centre[0]).abs() < TILE / 2.0 && (position[1] - centre[1]).abs() < TILE / 2.0
    })
}

/// Every wall tile's grid coordinates, in reading order.
///
/// One place that walks the map, so the two callers cannot disagree about what counts as a wall or
/// about the order they visit them in — and order matters, because spawn order decides entity
/// indices and therefore the state hash (invariant I3).
fn walls() -> impl Iterator<Item = (usize, usize)> {
    MAP.iter().enumerate().flat_map(|(row, line)| {
        line.bytes()
            .enumerate()
            .filter(|(_, tile)| *tile == b'#')
            .map(move |(column, _)| (column, row))
    })
}

/// Spawns a sprite for every wall tile in [`MAP`].
///
/// Every tile shares one texture, so the whole arena is **one draw call** (ADR 0023) no matter how
/// many tiles there are.
pub fn spawn_walls(app: &mut App) -> usize {
    let mut count = 0;
    for (column, row) in walls().collect::<Vec<_>>() {
        let centre = tile_centre(column, row);

        let entity = app.world.spawn();
        app.world
            .insert(entity, Transform::at(centre[0], centre[1]));
        app.world.insert(entity, Sprite::new("wall", TILE, TILE));
        // Above the floor, below everything standing on it.
        app.world.insert(entity, SortOrder::new(-50));
        app.world.insert(entity, Wall);
        count += 1;
    }
    count
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn the_map_is_rectangular() {
        // A ragged map would put tiles in places the coordinate maths does not expect, and the
        // failure would look like a level design mistake rather than a typo.
        let width = MAP[0].len();
        for (index, row) in MAP.iter().enumerate() {
            assert_eq!(row.len(), width, "row {index} is a different width");
        }
    }

    #[test]
    fn the_arena_is_walled_all_the_way_round() {
        // Nothing keeps the player in but these tiles, so a gap is an escape.
        let last_row = MAP.len() - 1;
        let last_column = MAP[0].len() - 1;

        for column in 0..MAP[0].len() {
            assert!(is_wall(column, 0), "gap in the top wall at {column}");
            assert!(
                is_wall(column, last_row),
                "gap in the bottom wall at {column}"
            );
        }
        for row in 0..MAP.len() {
            assert!(is_wall(0, row), "gap in the left wall at {row}");
            assert!(is_wall(last_column, row), "gap in the right wall at {row}");
        }
    }

    #[test]
    fn the_grid_is_centred_on_the_origin() {
        // So a translation in the scene file reads as a position on the map above, with no offset
        // to remember.
        let width = MAP[0].len();
        let height = MAP.len();
        let middle = tile_centre(width / 2, height / 2);
        assert_eq!(middle, [0.0, 0.0]);
    }

    #[test]
    fn row_zero_is_the_top_of_the_screen() {
        // The map reads like the screen, which is only true if y decreases as the row index rises.
        assert!(tile_centre(1, 1)[1] > tile_centre(1, 5)[1]);
    }
}
