//! Gate 2: the game is checked through `render.describe`, not by looking at it.
//!
//! > Verification of that game done purely through `inspect`, headless runs, and `render.describe` —
//! > with screenshots used only for final confirmation.
//! >
//! > — `docs/05-roadmap.md`, M1 exit gate 2
//!
//! `plays_itself.rs` covers the simulation. This covers everything a screenshot would otherwise have
//! been needed for: is it on screen, is it the right size, is it in front, is it where the map says.
//!
//! There is no GPU in this process and no window. Every answer comes from `describe_frame`, which is
//! the same thing the protocol's `render.describe` returns.

use amadeo_render::{DrawnKind, FrameDescription, describe_frame};
use amadeo_transform::Transform;
use vault::build_simulation;
use vault::game::{Player, ScoreDigit, Sigil, Wall, Warden};
use vault::level;

/// The game, described.
fn described() -> FrameDescription {
    let app = build_simulation().expect("the game builds");
    describe_frame(&app.world)
}

/// Every entry drawing a given texture.
fn by_texture<'a>(
    description: &'a FrameDescription,
    wanted: &str,
) -> Vec<&'a amadeo_render::DrawnEntity> {
    description
        .drawn
        .iter()
        .filter(|entry| matches!(&entry.kind, DrawnKind::Sprite { texture } if texture == wanted))
        .collect()
}

#[test]
fn everything_the_level_declares_is_actually_on_screen() {
    // The single most useful thing an agent can ask, and the one a screenshot answers slowly. If a
    // sigil were authored outside the arena this fails, naming the count.
    let description = described();

    assert_eq!(
        description.off_screen_count(),
        0,
        "{} of {} drawn entities are off screen",
        description.off_screen_count(),
        description.drawn.len()
    );
    assert!(description.visible_count() > 50, "the arena should be full");
}

#[test]
fn the_player_is_in_the_middle_of_the_view() {
    let description = described();
    let player = by_texture(&description, "player");
    assert_eq!(player.len(), 1, "exactly one player");

    let centre = [
        description.viewport[0] as f32 / 2.0,
        description.viewport[1] as f32 / 2.0,
    ];
    let at = player[0].center;
    assert!(
        (at[0] - centre[0]).abs() < 1.0,
        "the player should start horizontally centred, at {at:?} of {centre:?}"
    );
}

#[test]
fn the_player_draws_in_front_of_the_walls_and_the_floor() {
    // Sort order, checked as a fact about the screen rather than as a number in a file. If the
    // player ever ended up behind the floor it would be invisible, and this says so without anyone
    // looking.
    let description = described();
    let player = by_texture(&description, "player")[0];
    let walls = by_texture(&description, "wall");

    assert!(
        walls.iter().all(|wall| wall.order < player.order),
        "the player must draw over every wall"
    );

    let floor = description
        .drawn
        .iter()
        .find(|entry| entry.kind == DrawnKind::Quad)
        .expect("the floor is a quad");
    assert!(floor.order < player.order);
}

#[test]
fn the_six_sigils_are_all_visible_and_none_overlap() {
    // Overlap is the other question `render.describe` exists for. Two sigils on top of each other
    // would look like five and be a level-design bug that no test of the simulation could see.
    let description = described();
    let sigils = by_texture(&description, "sigil");
    assert_eq!(sigils.len(), 6);

    for (index, sigil) in sigils.iter().enumerate() {
        assert!(sigil.visible, "sigil {index} is off screen");
        for other in sigils.iter().skip(index + 1) {
            assert!(
                !sigil.overlaps(other),
                "two sigils overlap at {:?} and {:?}",
                sigil.center,
                other.center
            );
        }
    }
}

#[test]
fn the_score_readout_sits_above_the_arena_and_does_not_cover_it() {
    // Two digits, outside the walls, not overlapping each other. A UI element quietly sitting on top
    // of the play area is exactly the kind of thing that only shows up in a screenshot -- unless
    // something can be asked.
    let description = described();
    let digits = by_texture(&description, "digits");
    assert_eq!(digits.len(), 2, "a two-digit readout");

    let walls = by_texture(&description, "wall");
    for digit in &digits {
        assert!(digit.visible, "a score digit is off screen");
        assert!(
            walls.iter().all(|wall| !digit.overlaps(wall)),
            "a score digit at {:?} is sitting on the arena",
            digit.center
        );
    }
    assert!(!digits[0].overlaps(digits[1]), "the two digits overlap");
}

#[test]
fn nothing_authored_is_standing_inside_a_wall() {
    // The scene file and `level::MAP` are two separate texts that have to agree, and nothing makes
    // them. This is the check that they do -- a sigil authored at (-5, 2) would be inside the west
    // wall and unreachable, and the game would simply be unwinnable with no error anywhere.
    let app = build_simulation().expect("builds");

    let mut checked = 0;
    for (name, position) in app
        .world
        .query::<(&Transform, &Sigil)>()
        .map(|(_, (transform, _))| ("sigil", transform.translation))
        .chain(
            app.world
                .query::<(&Transform, &Player)>()
                .map(|(_, (transform, _))| ("player", transform.translation)),
        )
        .chain(
            app.world
                .query::<(&Transform, &Warden)>()
                .map(|(_, (transform, _))| ("warden", transform.translation)),
        )
    {
        assert!(
            !level::wall_at_world([position[0], position[1]]),
            "a {name} is authored inside a wall at {position:?}"
        );
        checked += 1;
    }
    assert_eq!(checked, 9, "one player, two wardens, six sigils");
}

#[test]
fn a_patrol_route_stays_clear_of_walls() {
    // Wardens do not collide, deliberately, so a route that crosses a pillar would have one walking
    // through it. Catching that here rather than at runtime is the point: it is an authoring
    // mistake, and the right time to find one is before the game runs.
    use vault::game::Patrol;

    let app = build_simulation().expect("builds");
    let mut routes = 0;

    for (_, (patrol, _)) in app.world.query::<(&Patrol, &Warden)>() {
        for window in 0..patrol.points.len() {
            let from = patrol.points[window];
            let to = patrol.points[(window + 1) % patrol.points.len()];

            // Sampled rather than solved: the routes are axis-aligned, so twenty points along each
            // leg is far finer than a one-unit tile and much simpler than a line-box intersection.
            for step in 0..=20 {
                let t = step as f32 / 20.0;
                let point = [
                    from[0] + (to[0] - from[0]) * t,
                    from[1] + (to[1] - from[1]) * t,
                ];
                assert!(
                    !level::wall_at_world(point),
                    "a patrol leg from {from:?} to {to:?} passes through a wall at {point:?}"
                );
            }
        }
        routes += 1;
    }
    assert_eq!(routes, 2);
}

#[test]
fn every_wall_tile_reached_the_screen() {
    // The map has forty-four wall tiles and the screen should show forty-four wall sprites. A
    // mismatch means `spawn_walls` and `MAP` disagree.
    let app = build_simulation().expect("builds");
    let spawned = app.world.query::<(&Wall,)>().count();

    let description = describe_frame(&app.world);
    assert_eq!(by_texture(&description, "wall").len(), spawned);
    assert_eq!(spawned, 44, "the arena's wall count");
}

#[test]
fn the_score_digits_start_showing_zero() {
    // Region `[0, 0, 1, 0.1]` is the first cell of the ten-cell sheet, which is the glyph `0`.
    use amadeo_render::Sprite;

    let mut app = build_simulation().expect("builds");
    app.run_ticks(1).expect("one tick, so show_score has run");

    for (_, (sprite, digit)) in app.world.query::<(&Sprite, &ScoreDigit)>() {
        assert_eq!(
            sprite.region[1], 0.0,
            "digit at place {} should be showing zero",
            digit.place
        );
        assert!((sprite.region[3] - 0.1).abs() < 1e-6, "one cell of ten");
    }
}

#[test]
fn describing_the_game_twice_gives_the_same_answer() {
    // So a diff between two ticks shows what moved and nothing else, which is what makes this
    // usable as a verification channel at all.
    let app = build_simulation().expect("builds");
    assert_eq!(describe_frame(&app.world), describe_frame(&app.world));
}

#[test]
fn the_scene_camera_matches_the_declared_view_height() {
    // `VIEW_HEIGHT` is what the tests in this file reason about; the camera in `vault.scene` is what
    // actually draws. They are two places holding one number since ADR 0031 moved the camera into
    // the scene file, so this is what makes a disagreement a failing test rather than a layout
    // mystery found by looking at a window.
    let app = build_simulation().expect("the game builds");
    let (camera, eye) = amadeo_render::primary_camera(&app.world).expect("the scene authors one");

    // `height()` returns `None` for a perspective camera rather than a fallback, so this asserts
    // the projection *and* the number in one go (ADR 0032).
    assert_eq!(
        camera.projection.height(),
        Some(vault::VIEW_HEIGHT),
        "a 2D game wants a parallel projection at the declared height"
    );
    // The nudge upward that keeps the score readout clear of the top wall — found by
    // `render.describe` rather than by looking, and now authored in the level rather than in code.
    assert!(eye[1] > 0.0, "the view is nudged up, got {eye:?}");
}

#[test]
fn the_camera_is_authored_in_the_scene_file_not_in_code() {
    // Invariant I1 reaching the renderer: the view is part of the level. If someone moves it back
    // into `build_simulation`, editing `vault.scene` would stop changing what you see, and this
    // fails rather than the change going unnoticed.
    assert!(
        vault::SCENE.contains("Camera"),
        "vault.scene must author the camera"
    );
}
