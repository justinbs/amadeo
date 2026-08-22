//! ADR 0071's room graph: bounded, connected, reproducible, and looped on purpose.
//!
//! # What is being proved, and what is not
//!
//! The *layout*, not the scene file. This is the graph half of ADR 0071 — which cells hold rooms and
//! which sides have doors — and it is separated from emitting text precisely because it is the half
//! with properties worth asserting. A writer either produces a scene `amadeo check` accepts or it
//! does not, and that is a better test of the writer than anything written here would be.

use warren::{Side, lay_out};

#[test]
fn one_seed_gives_one_layout() {
    // The property everything else rests on. A seeded generator whose *sequence* is not reproducible
    // silently lays out different levels on different machines, which is I3 gone for every
    // generated level — and it would show up as two players describing different rooms rather than
    // as anything a test failure would name.
    let first = lay_out(20_250_815, 12);
    let second = lay_out(20_250_815, 12);
    assert_eq!(first, second);
}

#[test]
fn different_seeds_give_different_layouts() {
    // The control for the test above: one that always returned the same thing would pass it.
    let a = lay_out(1, 12);
    let b = lay_out(2, 12);
    assert_ne!(a.rooms, b.rooms);
}

#[test]
fn it_places_the_rooms_it_was_asked_for_or_one_more() {
    // Bounded, which is the word the exit gate uses. A walk that could not back up would stop early
    // and quietly hand back a smaller level than anybody asked for, so the lower bound is the real
    // assertion.
    //
    // **The upper bound is `count + 1`, and that is a real consequence rather than slack.** When a
    // walk never doubles back there is no pair of touching rooms to join, so the generator adds one
    // room that touches two others to close the cycle. Asking for a hard count and getting a tree
    // would be the worse trade — a level of dead ends is a gameplay failure, and one extra room is
    // not.
    for seed in 0..32u64 {
        let layout = lay_out(seed, 14);
        assert!(
            (14..=15).contains(&layout.rooms.len()),
            "seed {seed} produced {} rooms",
            layout.rooms.len()
        );
    }
}

#[test]
fn every_room_is_reachable() {
    // An unreachable room is a level with a key nobody can get to, and a player experiences that as
    // the game being broken rather than as the game being hard. Checked across many seeds because a
    // connectivity bug is exactly the kind that one lucky seed hides.
    for seed in 0..64u64 {
        let layout = lay_out(seed, 12);
        assert!(
            layout.is_connected(),
            "seed {seed} produced an island: {:?}",
            layout.rooms
        );
    }
}

#[test]
fn there_is_always_a_loop() {
    // **ADR 0071 §3 by name.** A tree of rooms forces backtracking, and being chased back down a
    // corridor you have already cleared is the failure a horror slice cannot afford. So a cycle is
    // requested rather than hoped for, and this is what says the request was honoured.
    for seed in 0..64u64 {
        let layout = lay_out(seed, 10);
        assert!(
            layout.has_loop(),
            "seed {seed} produced a tree, which is a level of dead ends: {} rooms, {} doors",
            layout.rooms.len(),
            layout.door_count()
        );
    }
}

#[test]
fn doors_agree_from_both_sides() {
    // A door is one thing seen from two rooms. If a room lists a door its neighbour does not, the
    // level has a one-way wall — which reads to a player as a door that does not open, and to a
    // generator as nothing at all.
    for seed in 0..32u64 {
        let layout = lay_out(seed, 12);
        for room in &layout.rooms {
            for &side in &room.doors {
                let neighbour = layout
                    .at(side.step(room.cell))
                    .unwrap_or_else(|| panic!("seed {seed}: a door onto an empty cell"));
                assert!(
                    neighbour.doors.contains(&side.opposite()),
                    "seed {seed}: {:?} opens {side:?} but {:?} does not open back",
                    room.cell,
                    neighbour.cell
                );
            }
        }
    }
}

#[test]
fn no_two_rooms_share_a_cell() {
    for seed in 0..32u64 {
        let layout = lay_out(seed, 16);
        let mut cells: Vec<(i32, i32)> = layout.rooms.iter().map(|room| room.cell).collect();
        cells.sort_unstable();
        let before = cells.len();
        cells.dedup();
        assert_eq!(
            before,
            cells.len(),
            "seed {seed} stacked two rooms in a cell"
        );
    }
}

#[test]
fn the_rooms_come_back_sorted() {
    // Byte-stability (I2) starts here: the writer emits them in this order, so an unsorted layout
    // would make two identical levels produce two different files.
    let layout = lay_out(99, 12);
    let mut sorted = layout.rooms.clone();
    sorted.sort_by_key(|room| room.cell);
    assert_eq!(layout.rooms, sorted);
}

#[test]
fn a_side_and_its_opposite_step_back_to_where_they_started() {
    // The arithmetic the whole stitch rests on. Cheap, and it is the sort of thing that is wrong by
    // a sign for a long time before anybody notices.
    for side in Side::ALL {
        let there = side.step((3, -2));
        assert_eq!(side.opposite().step(there), (3, -2));
    }
}

#[test]
fn one_room_is_a_layout_with_no_doors() {
    // The degenerate case, which a generator asked for a loop could easily divide by zero on.
    let layout = lay_out(5, 1);
    assert_eq!(layout.rooms.len(), 1);
    assert_eq!(layout.door_count(), 0);
    assert!(layout.is_connected());
    assert!(!layout.has_loop(), "one room cannot loop back to itself");
}

// --- The scene it writes (ADR 0071 §1) ----------------------------------------------------------

#[test]
fn the_generated_scene_parses() {
    // **The point of ADR 0071 in one assertion.** A generated level is a text file, so the test of
    // the writer is that the engine's own parser accepts it — not that its bytes match something
    // this test also wrote.
    let scene = warren::to_scene(&lay_out(3, 10));
    amadeo_scene::parse(&scene).expect("a generated level has to be a scene the engine can read");
}

/// How many entities in a document instance a given piece.
fn instances(document: &amadeo_scene::SceneDocument, piece: &str) -> usize {
    document
        .entities
        .iter()
        .filter(|entity| entity.prefab.as_deref() == Some(piece))
        .count()
}

/// How many of a layout's doors run east-west, and how many north-south.
///
/// The split is what the architecture is made of: every bore runs north-south, so a north or south
/// door is the tube carrying on and an east or west door is a cross-passage cut through the ground
/// between two tubes. The two are counted differently everywhere below.
fn doors_by_axis(layout: &warren::Layout) -> (usize, usize) {
    let mut across = 0usize;
    let mut along = 0usize;
    for room in &layout.rooms {
        for &side in &room.doors {
            match side {
                warren::Side::East | warren::Side::West => across += 1,
                warren::Side::North | warren::Side::South => along += 1,
            }
        }
    }
    // Each door is seen from both of its rooms.
    (across / 2, along / 2)
}

#[test]
fn it_writes_a_bore_per_cell_and_two_side_walls_with_it() {
    let layout = lay_out(11, 12);
    let scene = warren::to_scene(&layout);
    let document = amadeo_scene::parse(&scene).expect("parses");

    assert_eq!(instances(&document, warren::ROOM_PIECE), layout.rooms.len());

    // **Two side walls per cell, always.** A bore has an east side and a west side whatever its
    // doors are; what the doors change is *which* piece goes there, not whether one does. That is
    // the difference from the old shell, where a side with no wall was a room open to the void.
    let solid = instances(&document, warren::WALL_PIECE);
    let open = instances(&document, warren::DOORWAY_PIECE);
    assert_eq!(solid + open, layout.rooms.len() * 2);

    // **Twice per east-west door, not once.** A cross-passage is written from both ends — each cell
    // puts the 3.6 m from its own wall to the boundary — so both cells need an opening in their
    // wall. The old rule emitted a shared side once, and it does not apply to a passage.
    let (across, _) = doors_by_axis(&layout);
    assert_eq!(open, across * 2);
    assert_eq!(instances(&document, warren::PASSAGE_PIECE), across * 2);
}

#[test]
fn every_room_is_a_prefab_instance_rather_than_loose_geometry() {
    // ADR 0071 §2: a piece is a prefab. A generator that emitted walls directly would work and would
    // put level geometry in generated text rather than in a piece somebody can open and edit.
    let document = amadeo_scene::parse(&warren::to_scene(&lay_out(4, 8))).expect("parses");
    assert!(
        document
            .entities
            .iter()
            .all(|entity| entity.prefab.is_some()),
        "every entity a layout emits should be an instance of a piece"
    );
}

#[test]
fn the_same_layout_writes_the_same_bytes() {
    // Byte-stability (I2), which is what makes a generated level diffable and what lets
    // `amadeo fmt --check` be a regression test for this writer.
    let layout = lay_out(77, 12);
    assert_eq!(warren::to_scene(&layout), warren::to_scene(&layout));
}

#[test]
fn the_seed_survives_into_the_file() {
    // ADR 0071 §4: a file says how to make it again. In the scene's *name*, because the format has
    // no comments and inventing one to carry a number would be trap 4.
    let scene = warren::to_scene(&lay_out(4242, 6));
    assert!(
        scene.starts_with("scene warren_generated_4242\n"),
        "{scene}"
    );
}

#[test]
fn the_pieces_it_needs_are_declared() {
    // ADR 0021: a scene declares its requirements. Forgetting this is the exact shape of the bug
    // that left the Warren's HUD with no words on it — a missing declaration has no symptom.
    let document = amadeo_scene::parse(&warren::to_scene(&lay_out(9, 8))).expect("parses");
    for piece in [
        warren::ROOM_PIECE,
        warren::DOORWAY_PIECE,
        warren::WALL_PIECE,
    ] {
        assert!(
            document.assets.iter().any(|id| id == piece),
            "`{piece}` is instanced but never declared"
        );
    }
}

#[test]
fn every_end_of_every_bore_is_capped_unless_the_next_one_carries_on() {
    // **The property additive geometry forces, in its new shape.** A bore's north and south ends are
    // open by construction — `ArchMesh` has no end caps — so an end that is not a door has to be
    // closed by a piece, and an end nothing closes is a level open to the void along its own axis.
    // That is exactly the defect this suite missed for three sessions in the old shells, where the
    // symptom was a band of sky map above every wall.
    //
    // **Both cells cap a shared end**, unlike a shared wall: two bores that do not join need a
    // bulkhead each, which is why `HEAD_INSET` exists to keep the two plates off one plane.
    let layout = lay_out(21, 12);
    let document = amadeo_scene::parse(&warren::to_scene(&layout)).expect("parses");

    let mut wanted = 0usize;
    for room in &layout.rooms {
        for side in [warren::Side::North, warren::Side::South] {
            if !room.doors.contains(&side) {
                wanted += 1;
            }
        }
    }

    assert_eq!(
        instances(&document, warren::HEAD_PIECE),
        wanted,
        "every north or south side without a door needs a bulkhead across it"
    );

    // And the other half of the same property: a north-south door means the next bore carries on,
    // so it must NOT be capped. Two ends per cell, minus the ones that are doors.
    let (_, along) = doors_by_axis(&layout);
    assert_eq!(wanted, layout.rooms.len() * 2 - along * 2);
}

#[test]
fn a_passage_goes_where_there_is_a_cross_door_and_a_blank_wall_where_there_is_not() {
    let layout = lay_out(31, 10);
    let document = amadeo_scene::parse(&warren::to_scene(&layout)).expect("parses");

    let (across, _) = doors_by_axis(&layout);
    let open = instances(&document, warren::DOORWAY_PIECE);

    assert_eq!(open, across * 2, "one opening at each end of each passage");
    assert_eq!(
        instances(&document, warren::WALL_PIECE),
        layout.rooms.len() * 2 - open,
        "every side that is not an opening is a blank wall"
    );
}
