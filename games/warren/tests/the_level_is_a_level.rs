//! The generated interior is a *level*, not a shape (M3 exit gate item 2).
//!
//! # What this file is for, and why it is not `it_lays_out_an_interior.rs`
//!
//! That file proves the room **graph**: bounded, connected, reproducible, looped. This one starts
//! where that one stops. A perfect graph with nowhere to start, nothing to fetch and no way out is
//! still not something anybody can play, and until this file existed the game booted into a
//! handcrafted room instead — which is exactly the state `docs/05` described as "demonstrated"
//! rather than done.
//!
//! # It ends by playing the thing
//!
//! The last test builds the shipped level, walks the loop, and escapes through the generated door.
//! That is the assertion the rest of the file exists to support: everything above it could pass
//! against a level that loads and cannot be won.

use amadeo_app::App;
use amadeo_core::Tick;
use amadeo_ecs::Entity;
use amadeo_input::{InputDriver, ScriptedSource};
use amadeo_interaction::USE;
use amadeo_inventory::Item;
use amadeo_transform::{GlobalTransform, Mat4, Transform};
use warren::{
    CELL, GENERATED_ROOMS, GENERATED_SCENE, GENERATED_SEED, Layout, Outcome, Side, Warden, WayOut,
    exit_side, facing, lay_out, outcome, player,
};

/// The layout the shipped level came from.
fn shipped() -> Layout {
    lay_out(GENERATED_SEED, GENERATED_ROOMS)
}

// --- The file on disk is the file the generator makes -------------------------------------------

#[test]
fn the_shipped_level_is_what_the_generator_makes() {
    // **The one test that stops the committed level drifting away from the code that writes it.**
    // `generated.scene` is compiled into the binary, so nothing else would ever notice if somebody
    // hand-edited it, or changed a placement rule and forgot to re-run `--bin layout`. Both are
    // silent: the game would keep working and would stop being reproducible from its own seed,
    // which is the property ADR 0071 §4 asks for by name.
    //
    // Hand-editing a generated level is *allowed* and is half the point of emitting a file — but it
    // has to be a deliberate act that turns this red, not something that slips through.
    assert_eq!(
        GENERATED_SCENE,
        warren::to_scene(&shipped()),
        "games/warren/scenes/generated.scene is not what `cargo run -p warren --bin layout` \
         produces from seed {GENERATED_SEED}. Re-run it, or change GENERATED_SEED to match."
    );
}

#[test]
fn what_the_generator_writes_is_already_canonical() {
    // **ADR 0071's consequences, cashed**: "`amadeo fmt --check` on generator output is a free
    // regression test". Free only if something runs it, and until this existed nothing did — the
    // generator had been quoting prefab ids, listing its `assets` block in the wrong order, and
    // leaving a spare blank line at the end, all since the day it was written.
    //
    // It matters beyond tidiness. A generated level that is not canonical means the first person to
    // run `amadeo fmt` on it produces a diff that is entirely noise, and the next regeneration
    // produces the reverse — so the two tools fight and neither result is reviewable (I2).
    let scene = warren::to_scene(&shipped());
    let document = amadeo_scene::parse(&scene).expect("parses");
    assert_eq!(
        scene,
        amadeo_scene::to_text(&document),
        "the generator's output is not what `amadeo fmt` would write"
    );
}

#[test]
fn it_places_exactly_one_of_each_thing_a_run_needs() {
    // Counted from the parsed document rather than by searching the text, so a piece that is named
    // in a comment or an entity label cannot be mistaken for one that is instanced.
    let document = amadeo_scene::parse(GENERATED_SCENE).expect("the shipped level parses");
    let count = |piece: &str| {
        document
            .entities
            .iter()
            .filter(|entity| entity.prefab.as_deref() == Some(piece))
            .count()
    };

    // Two is as bad as none for every one of these: two players is undefined, two ways out makes
    // the level half as long, and two keys makes the detour optional.
    assert_eq!(count(warren::PLAYER_PIECE), 1, "somewhere to wake up");
    assert_eq!(count(warren::EXIT_PIECE), 1, "a way out");
    assert_eq!(count(warren::KEY_PIECE), 1, "a key to open it with");
    assert_eq!(count(warren::TORCH_PIECE), 1, "something to see by");
    assert_eq!(count(warren::WARDEN_PIECE), 1, "something to run from");
    assert_eq!(count(warren::HUD_PIECE), 1, "words on the screen");

    // And at least one working lamp, because the start room's is not optional.
    assert!(count(warren::LAMP_PIECE) >= 1, "a room that is lit");
}

#[test]
fn every_piece_it_instances_is_declared() {
    // ADR 0021, and the same failure the HUD hit in session 18: an undeclared asset does not error,
    // it simply is not there. Widened from the three geometry pieces to all nine, because adding a
    // tenth and forgetting the `assets` block would have no symptom at all.
    let document = amadeo_scene::parse(GENERATED_SCENE).expect("parses");
    for piece in warren::PIECES {
        assert!(
            document.assets.iter().any(|id| id == piece),
            "`{piece}` is instanced but never declared"
        );
    }
}

// --- Where the landmarks go ---------------------------------------------------------------------

/// Every landmark, for one layout, as `(what it is, where it is)`.
fn landmarks_of(layout: &Layout) -> [(&'static str, (i32, i32)); 5] {
    let marks = layout.landmarks;
    [
        ("start", marks.start),
        ("exit", marks.exit),
        ("key", marks.key),
        ("torch", marks.torch),
        ("warden", marks.warden),
    ]
}

#[test]
fn every_landmark_is_in_a_room_that_exists() {
    // The failure this catches is a landmark placed in an empty cell, which puts the key — or the
    // player — inside the void beyond the level with no floor under it. Checked across many seeds
    // because an off-by-one in a graph walk is exactly what one lucky seed hides.
    for seed in 0..48u64 {
        let layout = lay_out(seed, 12);
        for (what, cell) in landmarks_of(&layout) {
            assert!(
                layout.at(cell).is_some(),
                "seed {seed}: the {what} is at {cell:?}, where there is no room"
            );
        }
    }
}

#[test]
fn the_way_out_is_as_far_from_the_start_as_the_level_gets() {
    // The property that makes a generated interior a *journey* rather than a room with a door in
    // it. Stated as "no room is further" rather than by recomputing the maximum, so the test does
    // not simply repeat the implementation back to itself.
    for seed in 0..48u64 {
        let layout = lay_out(seed, 12);
        let table = layout.distances_from(layout.landmarks.start);
        let to_exit = table
            .iter()
            .find(|(cell, _)| *cell == layout.landmarks.exit)
            .map(|(_, steps)| *steps)
            .expect("the exit is reachable");

        for (cell, steps) in &table {
            assert!(
                *steps <= to_exit,
                "seed {seed}: {cell:?} is {steps} doors away and the exit is only {to_exit}"
            );
        }
    }
}

#[test]
fn the_key_is_never_where_you_start_or_where_you_finish() {
    // A key in the start room is a level with no middle; a key in the exit room is a lock you are
    // already standing next to. Both are levels that *work*, which is what makes this worth pinning
    // rather than leaving to be noticed.
    for seed in 0..48u64 {
        let layout = lay_out(seed, 12);
        let marks = layout.landmarks;
        assert_ne!(marks.key, marks.start, "seed {seed}: the key is underfoot");
        assert_ne!(marks.key, marks.exit, "seed {seed}: the key is at the door");
    }
}

#[test]
fn the_key_is_off_the_shortest_route_to_the_door() {
    // The rule's actual intent, and the reason it is a *detour* score rather than "furthest from
    // the start". Everywhere on a shortest route scores exactly the distance between the two ends,
    // so a key that scores higher is provably off it — and a player therefore has to leave the
    // route to fetch it.
    //
    // Not asserted for every seed: a layout that happens to be a straight corridor has nowhere off
    // the route to put anything, and failing on that would be asserting the generator produces
    // branches rather than that the rule is right. Asserting on the majority is the honest claim.
    let mut detours = 0;
    let seeds = 48u64;
    for seed in 0..seeds {
        let layout = lay_out(seed, 12);
        let marks = layout.landmarks;
        let from_start = layout.distances_from(marks.start);
        let from_exit = layout.distances_from(marks.exit);
        let steps = |table: &[((i32, i32), u32)], cell: (i32, i32)| {
            table
                .iter()
                .find(|(seen, _)| *seen == cell)
                .map(|(_, steps)| *steps)
                .unwrap_or(0)
        };

        let direct = steps(&from_start, marks.exit);
        let through_key = steps(&from_start, marks.key) + steps(&from_exit, marks.key);
        assert!(
            through_key >= direct,
            "seed {seed}: fetching the key is somehow shorter than not"
        );
        if through_key > direct {
            detours += 1;
        }
    }
    assert!(
        detours * 2 > seeds,
        "only {detours} of {seeds} seeds put the key off the route, which is not a detour rule"
    );
}

#[test]
fn the_torch_is_one_door_from_where_you_wake_up() {
    // Close enough that a player finds it before the dark becomes frustrating, and far enough that
    // finding it is something they did. One door is the whole design.
    for seed in 0..48u64 {
        let layout = lay_out(seed, 12);
        let marks = layout.landmarks;
        let start = layout.at(marks.start).expect("a start room");
        assert!(
            start
                .doors
                .iter()
                .any(|side| side.step(marks.start) == marks.torch),
            "seed {seed}: the torch is at {:?}, which is not through a door from {:?}",
            marks.torch,
            marks.start
        );
    }
}

#[test]
fn the_warden_never_starts_in_the_room_you_do() {
    // Being caught before you have moved is not a lose state, it is a crash with a caption.
    for seed in 0..48u64 {
        let layout = lay_out(seed, 12);
        assert_ne!(
            layout.landmarks.warden, layout.landmarks.start,
            "seed {seed}: the warden is standing on the player"
        );
    }
}

#[test]
fn the_room_you_wake_up_in_always_has_a_working_lamp() {
    // The one room whose lighting is not left to chance. Waking in the pitch dark before finding
    // the torch is indistinguishable from the game having failed to start.
    for seed in 0..64u64 {
        let layout = lay_out(seed, 12);
        assert!(
            layout
                .at(layout.landmarks.start)
                .is_some_and(|room| room.lit),
            "seed {seed}: you wake up in the dark"
        );
    }
}

#[test]
fn some_rooms_are_dark_and_some_are_not() {
    // The control for the test above. A generator that lit every room would pass it, and would make
    // the torch pointless; one that lit none would fail it. Over sixty-four seeds both must occur.
    let mut lit = 0;
    let mut dark = 0;
    for seed in 0..64u64 {
        for room in &lay_out(seed, 12).rooms {
            if room.lit {
                lit += 1;
            } else {
                dark += 1;
            }
        }
    }
    assert!(lit > 0 && dark > 0, "{lit} lit and {dark} dark");
}

#[test]
fn the_section_letters_ascend_along_the_spine_and_none_appears_twice() {
    // **`docs/11` §5.4's wayfinding system, and the reason it is not decoration.** A player who sees
    // a letter learns nothing unless the letters are *ordered along the route*.
    //
    // **This test asserted the opposite rule until session 26, and the rule was wrong.** It required
    // two cells sharing a door to carry different letters, which forced a letter to change every
    // twelve metres — and the only scheme that does that is a **distance ring**, `manhattan % 5`,
    // which rises whichever way you walk away from the start and repeats. Engine gate reviews 19, 25
    // and 30 all filed the consequence: the letters convey no direction.
    //
    // A section is a lettered **stretch**, several bores long, the way Clapham South's sixteen
    // sub-shelters were A to P. So two cells sharing a letter is correct — they are the same
    // section. The two properties that actually matter are these:
    //
    // 1. **Each letter occupies one contiguous band of the ranking**, so the same letter never
    //    appears in two unrelated places.
    // 2. **The letters ascend with distance into the level**, so a letter tells a player which way
    //    is further in.
    //
    // Across many seeds, because "by construction" is a claim about an argument and this is a claim
    // about the output.
    for seed in 0..48u64 {
        let layout = lay_out(seed, 14);

        // (2): a cell further from the start never carries an earlier letter than a nearer one.
        for a in &layout.rooms {
            for b in &layout.rooms {
                let reach = |cell: (i32, i32)| {
                    (cell.0 - layout.landmarks.start.0).unsigned_abs()
                        + (cell.1 - layout.landmarks.start.1).unsigned_abs()
                };
                if reach(a.cell) < reach(b.cell) {
                    assert!(
                        warren::section_index(&layout, a.cell)
                            <= warren::section_index(&layout, b.cell),
                        "seed {seed}: {:?} is nearer the start than {:?} and carries a later letter",
                        a.cell,
                        b.cell
                    );
                }
            }
        }

        // (1): every cell with a given letter sits in one unbroken run of the distance ranking.
        let mut ranked: Vec<(i32, i32)> = layout.rooms.iter().map(|room| room.cell).collect();
        let start = layout.landmarks.start;
        ranked.sort_by_key(|at| {
            (
                (at.0 - start.0).unsigned_abs() + (at.1 - start.1).unsigned_abs(),
                *at,
            )
        });
        let letters: Vec<usize> = ranked
            .iter()
            .map(|at| warren::section_index(&layout, *at))
            .collect();
        let mut seen: Vec<usize> = Vec::new();
        for (index, letter) in letters.iter().enumerate() {
            if index > 0 && letters[index - 1] == *letter {
                continue;
            }
            assert!(
                !seen.contains(letter),
                "seed {seed}: letter {} appears in two separate stretches",
                warren::SECTION_LETTERS[*letter]
            );
            seen.push(*letter);
        }
    }
}

#[test]
fn the_shipped_level_shows_more_than_one_letter() {
    // The control. A generator that returned the same index everywhere would satisfy nothing above
    // if the level were a straight line, and this is the assertion review 16 actually asked for:
    // **two different sections show two different letters.**
    let layout = shipped();
    let mut seen: Vec<usize> = layout
        .rooms
        .iter()
        .map(|room| warren::section_index(&layout, room.cell))
        .collect();
    seen.sort_unstable();
    seen.dedup();
    assert!(
        seen.len() >= 3,
        "the shipped level uses only {} of {} section letters",
        seen.len(),
        warren::SECTION_LETTERS.len()
    );
}

#[test]
fn every_condition_is_used_somewhere() {
    // The control for the test above, which a generator that gave every room a *different* condition
    // by cycling blindly would also pass — and which one that only ever used two would pass as well,
    // producing a level with a third of its dressing missing and no failure anywhere.
    let layout = shipped();
    for wanted in [
        warren::Condition::SleptIn,
        warren::Condition::Stripped,
        warren::Condition::Stores,
    ] {
        assert!(
            layout.rooms.iter().any(|room| room.condition == wanted),
            "the shipped level has no {wanted:?} section"
        );
    }
}

// --- The door is in a wall ----------------------------------------------------------------------

#[test]
fn the_way_out_is_set_into_an_outside_wall_when_there_is_one() {
    // A door leading into the next room is a door that means nothing. The generator prefers an
    // outer side and this says the preference is honoured whenever one exists — which, for a
    // bounded layout drawn on an open grid, is every time.
    for seed in 0..48u64 {
        let layout = lay_out(seed, 12);
        let cell = layout.landmarks.exit;
        let outer = Side::ALL
            .into_iter()
            .any(|side| layout.at(side.step(cell)).is_none());
        if !outer {
            continue;
        }
        let chosen = exit_side(&layout);
        assert!(
            layout.at(chosen.step(cell)).is_none(),
            "seed {seed}: the way out was put on {chosen:?}, which opens into another room"
        );
    }
}

#[test]
fn facing_agrees_with_the_engines_own_rotation() {
    // **The sign convention, pinned against the real matrix rather than against a comment.** A yaw
    // that is ninety degrees out is entirely plausible and entirely wrong, and the symptom is a
    // player who wakes up looking at plaster — which reads as a level with no doorway.
    //
    // Forward is -Z (ADR 0018), so a rotation's forward is its matrix's third column negated.
    for side in Side::ALL {
        let (dx, dz) = side.step((0, 0));
        let matrix = Mat4::from_euler_degrees([0.0, facing(side), 0.0]);
        let forward = [-matrix.columns[2][0], -matrix.columns[2][2]];

        assert!(
            (forward[0] - dx as f32).abs() < 1e-5 && (forward[1] - dz as f32).abs() < 1e-5,
            "facing({side:?}) = {} points at {forward:?}, not at {:?}",
            facing(side),
            (dx, dz)
        );
    }
}

// --- And it plays -------------------------------------------------------------------------------

/// The shipped level, with input a test can script.
fn level() -> App {
    let mut app = warren::build_simulation().expect("the generated level builds");
    amadeo_input::install(
        &mut app.world,
        InputDriver::new(Box::new(ScriptedSource::new())),
    );
    // Past the title screen; see `the_run_can_end.rs` for why every builder here does this.
    if let Some(screen) = app.world.resource_mut::<warren::Screen>() {
        *screen = warren::Screen::Playing;
    }
    app
}

#[test]
fn nothing_in_the_level_is_buried_and_every_wall_reaches_the_crown() {
    // **The test the sunk-wall defect needed, written the way it would have been caught.**
    //
    // Session 23 found that every wall in the shipped level stood from -1.5 m to +1.5 m instead of
    // 0 to 3: `wall.scene` authored its height on the prefab **root**, and ADR 0029's override
    // *replaces* the root's `Transform`, so the generator's `place(x, 0.0, z)` threw the 1.5 away.
    // The top 1.5 m of every bay was open to the sky map. Nothing noticed for three sessions --
    // `amadeo check` passed, `amadeo fmt` was clean, and the whole suite was green, because the
    // collider sank with the mesh and a 1.5 m wall still stops a 1.9 m capsule.
    //
    // **Asserted about the level rather than about the file format**, deliberately. The obvious
    // structural test -- "no placeable piece has a non-zero root translation" -- fails on day one:
    // `player_start` is authored at 1.0 and `warden_post` at 0.93, both on purpose, and the
    // generator writes `PLAYER_STAND` and `WARDEN_STAND` over them. A test that has to exempt two
    // pieces by name is a test that goes stale. This one knows nothing about the mechanism and
    // would catch the next thing that discards a height, whatever that turns out to be.
    let app = level();

    // How deep anything is allowed to sit. The deck slab is 0.24 m thick and hangs below the
    // walking surface, so that plus a little is the floor of what is legitimate.
    const DEEPEST: f32 = -0.4;

    let mut walls_reaching = 0usize;
    for (entity, (collider, at)) in app
        .world
        .query::<(&amadeo_physics::Collider, &GlobalTransform)>()
    {
        let amadeo_physics::Shape::Cuboid { size } = collider.shape else {
            continue;
        };
        // A `GlobalTransform` is a column-major 4x4, so the translation is elements 12..15.
        let centre = at.matrix[13];
        let (bottom, top) = (centre - size[1] / 2.0, centre + size[1] / 2.0);

        assert!(
            bottom >= DEEPEST,
            "{entity:?} has its bottom at {bottom:.2} m, which is below the deck -- something \
             placed it without the height its piece authored"
        );

        // The side walls are the one shape tall enough to reach the springing and the crown above
        // it, and they are what the defect sank. Counted rather than merely checked, so a level
        // that lost its walls entirely cannot pass by having none to check.
        if (size[1] - 3.4).abs() < 0.01 {
            assert!(
                top >= 3.35,
                "a side wall reaches only {top:.2} m; the crown springs at 2.0 and closes at 3.2"
            );
            walls_reaching += 1;
        }
    }

    // At least two per bore. Not exactly two: a wall with a cross-passage through it is two jamb
    // colliders rather than one, so the count is a floor rather than an equality -- and it is here
    // at all so that a level which lost its walls entirely cannot pass by having none to check.
    let rooms = shipped().rooms.len();
    assert!(
        walls_reaching >= rooms * 2,
        "{rooms} bores need at least {} walls reaching the crown, and {walls_reaching} do",
        rooms * 2
    );
}

/// The one entity marked as the way out.
fn the_door(app: &App) -> Entity {
    app.world
        .query::<(&WayOut,)>()
        .map(|(entity, _)| entity)
        .next()
        .expect("the generated level has a way out")
}

/// Where something ended up in the world, which for a piece's child is not its own transform.
fn world_place(app: &App, entity: Entity) -> [f32; 3] {
    app.world
        .get::<GlobalTransform>(entity)
        .map(|global| global.to_mat4().translation())
        .expect("composed at load")
}

/// Stands the player somewhere, facing a given way.
///
/// Both halves matter. An `Interactor` sweeps along its own forward, so a player standing in front
/// of the door facing the wrong way is a player who cannot open it — and the failure would read as
/// "the door does not work" rather than as "the test faces north".
fn stand_at(app: &mut App, at: [f32; 3], yaw: f32) {
    let player = player(&app.world).expect("a character");
    if let Some(transform) = app.world.get_mut::<Transform>(player) {
        transform.translation = at;
        transform.rotation = [0.0, yaw, 0.0];
    }
    app.run_ticks(1).expect("a tick runs");
}

/// Presses "use" for one tick, edge-triggered through the input source.
fn tap_use(app: &mut App) {
    let now = app.tick();
    let release = Tick(now.0 + 1);
    app.world
        .with_service_taken::<InputDriver, ()>(|_world, driver| {
            if let Some(scripted) = driver.source.as_any_mut().downcast_mut::<ScriptedSource>() {
                scripted.press(now, USE, true);
                scripted.press(release, USE, false);
            }
        });
    app.run_ticks(2).expect("ticks run");
}

/// Puts the warden out of reach, so a test about the door is not also a test about being caught.
fn banish_the_warden(app: &mut App) {
    let warden = app
        .world
        .query::<(&Warden,)>()
        .map(|(entity, _)| entity)
        .next()
        .expect("a warden");
    if let Some(transform) = app.world.get_mut::<Transform>(warden) {
        transform.translation = [0.0, 0.9, 1000.0];
    }
}

#[test]
fn the_generated_level_loads_and_stands_you_on_its_floor() {
    // **The cheapest possible proof that a generated level is real geometry.** `amadeo check`
    // validates a scene against the schema and says nothing about whether the floor is under the
    // player — session 18 shipped a generated scene that passed `check` and then refused to load at
    // all. Standing on it is the check that cannot be faked.
    let mut app = level();
    app.run_ticks(30).expect("half a second runs");
    assert!(
        warren::grounded(&app.world),
        "the player fell through a generated floor"
    );
}

#[test]
fn you_can_walk_out_of_the_generated_level() {
    // **M3 exit gate item 2, cashed.** Everything above this proves properties of a file; this
    // proves the file is a game. The key is fetched and the door is opened through the same events
    // a player's keypress would raise.
    let mut app = level();
    banish_the_warden(&mut app);
    let you = player(&app.world).expect("a character");

    let key = app
        .world
        .query::<(&Item,)>()
        .filter(|(_, (item,))| item.kind == warren::KEY)
        .map(|(entity, _)| entity)
        .next()
        .expect("the generated level has a key");
    amadeo_inventory::store(&mut app.world, key, you).expect("the bag has room");

    // In front of the door and looking at it. The side the generator chose gives both without any
    // trigonometry: the room is on the far side of the door from the wall, and facing the door
    // means facing the way that side points.
    let layout = shipped();
    let side = exit_side(&layout);
    let (ix, iz) = side.opposite().step((0, 0));
    let door = world_place(&app, the_door(&app));
    let reach = 1.3;
    stand_at(
        &mut app,
        [
            door[0] + ix as f32 * reach,
            1.0,
            door[2] + iz as f32 * reach,
        ],
        facing(side),
    );

    assert_eq!(
        warren::prompt(&app.world).as_deref(),
        Some("WAY OUT"),
        "standing at the generated door with the key should offer to open it"
    );

    tap_use(&mut app);
    assert_eq!(outcome(&app.world), Outcome::Escaped);
}

#[test]
fn the_generated_door_is_where_the_layout_says_it_is() {
    // Ties the world back to the graph. A door that loads at the origin instead of in the exit room
    // would still pass every text-level test in this file, and it is exactly what happened while
    // this was being built — a collider on a prefab *child* was being written back in world space
    // and then composed with its parent a second time.
    let app = level();
    let layout = shipped();
    let (cx, cz) = (
        layout.landmarks.exit.0 as f32 * CELL,
        layout.landmarks.exit.1 as f32 * CELL,
    );
    let door = world_place(&app, the_door(&app));

    let half = CELL / 2.0;
    assert!(
        (door[0] - cx).abs() <= half && (door[2] - cz).abs() <= half,
        "the door is at {door:?}, and the exit room spans {half} either side of ({cx}, {cz})"
    );
}

// --- The level is a good one, not merely a valid one --------------------------------------------

#[test]
fn the_shipped_level_has_no_shortcomings() {
    // **The test whose absence let a bad level ship.** Everything else in this file checks that the
    // generator produces something *valid* — connected, looped, byte-stable, one of each piece.
    // Seed 20250815 passed every one of those and put the key one door from the door it opens, so a
    // player walked ninety-six metres in a straight line and used the key next door to the exit.
    //
    // A bad layout is indistinguishable from a good one from the outside: it loads, it validates,
    // the suite is green and the capture is a room. The only thing that can tell them apart is a
    // rule written down.
    assert_eq!(
        shipped().shortcomings(),
        Vec::<String>::new(),
        "the seed this game ships with makes a level that is not worth playing"
    );
}

#[test]
fn the_generator_finds_a_playable_layout_for_most_seeds() {
    // The control. A gate that almost every seed fails is not a quality bar, it is a broken
    // generator — and one almost every seed passes is not a bar at all. Both failure modes are worth
    // catching, so this asserts a band rather than a floor.
    let good = (0..120u64)
        .filter(|seed| lay_out(*seed, GENERATED_ROOMS).shortcomings().is_empty())
        .count();
    assert!(
        (30..=110).contains(&good),
        "{good} of 120 seeds make a playable level, which is either too few to be usable or too \
         many for the check to mean anything"
    );
}

#[test]
fn the_key_is_a_journey_from_the_door_it_opens() {
    // The specific regression. Stated as a distance rather than through `shortcomings`, so that
    // relaxing the gate later cannot quietly relax this.
    let layout = shipped();
    let from_exit = layout.distances_from(layout.landmarks.exit);
    let steps = from_exit
        .iter()
        .find(|(cell, _)| *cell == layout.landmarks.key)
        .map(|(_, steps)| *steps)
        .expect("the key is reachable from the exit");
    assert!(
        steps >= 3,
        "the key is {steps} door(s) from the exit, which is a lock with its key taped to it"
    );
}
