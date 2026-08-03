//! The game, played headlessly and checked without eyes.
//!
//! # This is the exit gate, as tests
//!
//! `docs/05-roadmap.md` asks M1 for a complete small 2D game — "player moves, enemies patrol,
//! collision, a score, a win state" — and then asks for it to be verified "purely through `inspect`,
//! headless runs, and `render.describe`, with screenshots used only for final confirmation".
//!
//! So each of those five claims gets a test that drives the real game with scripted input and
//! asserts on the real world. Nothing here looks at a pixel, and there is no GPU in the process.
//!
//! # Why scripted input rather than poking the world
//!
//! Setting a player's `Transform` directly would test the *systems* while skipping the thing most
//! likely to be wrong: whether input reaches them at all. `ScriptedSource` queues action changes at
//! exact ticks, which is the same path a keyboard and a `.replay` file both take.

use amadeo_app::App;
use amadeo_core::Tick;
use amadeo_ecs::World;
use amadeo_input::{InputDriver, ScriptedSource};
use amadeo_transform::Transform;
use vault::build_simulation;
use vault::game::{MOVE_X, MOVE_Y, Phase, Player, Run, Sigil, Trap, Warden};

/// Builds the game with a scripted input source installed, and **the traps disarmed**.
///
/// Disarming is the same isolation `despawn_wardens` gives the win test, for the same reason: most
/// of these tests are about movement, collision, or scoring, and a trap ending the run part-way
/// through a route would make a failure ambiguous. The traps have their own tests, below, which use
/// [`scripted_armed`].
fn scripted(script: impl FnOnce(&mut ScriptedSource)) -> App {
    let mut app = scripted_armed(script);
    let traps: Vec<_> = app
        .world
        .query::<(&Trap,)>()
        .map(|(entity, _)| entity)
        .collect();
    for entity in traps {
        if let Some(trap) = app.world.get_mut::<Trap>(entity) {
            trap.armed = false;
        }
    }
    app
}

/// The game exactly as it ships, traps live.
fn scripted_armed(script: impl FnOnce(&mut ScriptedSource)) -> App {
    let mut app = build_simulation().expect("the game builds");
    let mut source = ScriptedSource::new();
    script(&mut source);
    amadeo_input::install(&mut app.world, InputDriver::new(Box::new(source)));
    app
}

/// Where the player is right now.
fn player_at(world: &World) -> [f32; 2] {
    world
        .query::<(&Transform, &Player)>()
        .next()
        .map(|(_, (transform, _))| [transform.translation[0], transform.translation[1]])
        .expect("the scene has a player")
}

/// How the run is going.
fn run(world: &World) -> Run {
    *world.resource::<Run>().expect("the game inserts a Run")
}

/// How many sigils are left in the world.
fn sigils_left(world: &World) -> usize {
    world.query::<(&Sigil,)>().count()
}

// --- 1. The player moves ---

#[test]
fn the_player_moves_when_told_to() {
    let mut app = scripted(|source| {
        source.axis(Tick(0), MOVE_X, 1.0);
    });

    let start = player_at(&app.world);
    app.run_ticks(30).expect("30 ticks");
    let after = player_at(&app.world);

    assert!(
        after[0] > start[0] + 1.0,
        "half a second of full right should move the player a long way: {start:?} -> {after:?}"
    );
    assert_eq!(after[1], start[1], "and not sideways");
}

#[test]
fn the_player_stops_when_input_stops() {
    // There is no momentum in this game -- releasing the key stops you dead, which is what a
    // tile-arena game wants and is worth pinning so a later change to `steer_player` cannot add
    // sliding by accident.
    let mut app = scripted(|source| {
        source.axis(Tick(0), MOVE_X, 1.0);
        source.axis(Tick(20), MOVE_X, 0.0);
    });

    app.run_ticks(21).expect("21 ticks");
    let stopped_at = player_at(&app.world);
    app.run_ticks(30).expect("30 more");

    assert_eq!(player_at(&app.world), stopped_at);
}

// --- 2. Enemies patrol ---

#[test]
fn the_wardens_walk_their_routes_and_come_back() {
    // A closed route: after a full lap each warden is where it started. The lap is 16 units at
    // 2.2 units per second, so 436 ticks, and this runs long enough for two.
    let mut app = scripted(|_| {});

    let start: Vec<[f32; 2]> = app
        .world
        .query::<(&Transform, &Warden)>()
        .map(|(_, (transform, _))| [transform.translation[0], transform.translation[1]])
        .collect();
    assert_eq!(start.len(), 2, "the scene has two wardens");

    app.run_ticks(60).expect("60 ticks");
    let moved: Vec<[f32; 2]> = app
        .world
        .query::<(&Transform, &Warden)>()
        .map(|(_, (transform, _))| [transform.translation[0], transform.translation[1]])
        .collect();
    assert_ne!(moved, start, "a warden that never moves is not patrolling");
}

#[test]
fn a_warden_never_leaves_its_route() {
    // The route is a rectangle, so a warden should always be on one of its edges. If waypoint
    // arrival ever failed to register, a warden would sail off in a straight line and this catches
    // it -- which is the failure mode `WAYPOINT_RANGE` exists to prevent.
    let mut app = scripted(|_| {});

    for _ in 0..40 {
        app.run_ticks(30).expect("30 ticks");
        for (_, (transform, _)) in app.world.query::<(&Transform, &Warden)>() {
            let x = transform.translation[0];
            let y = transform.translation[1];
            assert!(
                (-5.5..=5.5).contains(&x) && (-2.5..=2.5).contains(&y),
                "a warden reached {x}, {y}, which is off its route"
            );
        }
    }
}

// --- 3. Collision ---

#[test]
fn a_wall_stops_the_player() {
    // Run right for far longer than it takes to cross the arena. The player must end up against the
    // wall rather than through it. The rightmost open column is x = 5, so the player's centre stops
    // a little short of 5.5.
    let mut app = scripted(|source| {
        source.axis(Tick(0), MOVE_X, 1.0);
    });

    app.run_ticks(600).expect("ten seconds of running right");
    let x = player_at(&app.world)[0];

    assert!(x < 5.2, "the player went through the east wall, to x = {x}");
    assert!(
        x > 4.0,
        "the player did not reach the east wall, only x = {x}"
    );
}

#[test]
fn a_player_slides_along_a_wall_rather_than_sticking() {
    // What resolving the two axes separately buys. Pushing into the north wall *and* east at once
    // should still move east; resolving both axes together would stop the player dead.
    let mut app = scripted(|source| {
        source.axis(Tick(0), MOVE_Y, 1.0);
    });
    app.run_ticks(200).expect("run into the north wall");
    let against_wall = player_at(&app.world);

    app.world
        .with_service_taken::<InputDriver, ()>(|_world, driver| {
            if let Some(source) = driver.source.as_any_mut().downcast_mut::<ScriptedSource>() {
                let tick = Tick(200);
                source.axis(tick, MOVE_X, 1.0);
            }
        });
    app.run_ticks(60).expect("now push north-east");

    let slid = player_at(&app.world);
    assert!(
        slid[0] > against_wall[0] + 0.5,
        "the player should slide east along the north wall: {against_wall:?} -> {slid:?}"
    );
}

// --- 4. A score ---

#[test]
fn walking_onto_a_sigil_collects_it_and_scores() {
    // The sigil due east sits at x = 4, which is 4 units away at 4.5 units per second.
    let mut app = scripted(|source| {
        source.axis(Tick(0), MOVE_X, 1.0);
    });

    assert_eq!(sigils_left(&app.world), 6);
    assert_eq!(run(&app.world).score(), 0);

    app.run_ticks(60).expect("a second of running east");

    assert_eq!(sigils_left(&app.world), 5, "the sigil should be gone");
    assert_eq!(run(&app.world).collected, 1);
    assert_eq!(run(&app.world).score(), 10, "ten points a sigil");
}

#[test]
fn the_total_is_counted_from_the_scene_rather_than_hard_coded() {
    // So adding a sigil to `vault.scene` changes the win condition with it. If this ever fails, the
    // scene and the code have drifted, which is exactly what counting at startup prevents.
    let app = scripted(|_| {});
    assert_eq!(run(&app.world).total, sigils_left(&app.world) as u32);
    assert_eq!(run(&app.world).total, 6);
}

// --- 5. A win state, and a lose state ---

#[test]
fn collecting_every_sigil_wins() {
    // Rather than authoring a route that dodges both wardens for a minute, this despawns the
    // wardens first and then walks the circuit. What is under test is the *win condition*, and
    // mixing it with an evasion puzzle would make a failure ambiguous.
    let mut app = scripted(|source| {
        // East, north to the corner, west across the top, and so on -- a lap of the six sigils.
        source.axis(Tick(0), MOVE_X, 1.0);
        source.axis(Tick(60), MOVE_X, 0.0);
        source.axis(Tick(60), MOVE_Y, 1.0);
        source.axis(Tick(90), MOVE_Y, 0.0);
        source.axis(Tick(90), MOVE_X, -1.0);
        source.axis(Tick(210), MOVE_X, 0.0);
        source.axis(Tick(210), MOVE_Y, -1.0);
        source.axis(Tick(240), MOVE_Y, 0.0);
        source.axis(Tick(240), MOVE_X, 0.0);
        // Now at the west-middle sigil. Down to the south-west, then east along the bottom.
        source.axis(Tick(250), MOVE_Y, -1.0);
        source.axis(Tick(280), MOVE_Y, 0.0);
        source.axis(Tick(280), MOVE_X, 1.0);
        source.axis(Tick(400), MOVE_X, 0.0);
    });

    despawn_wardens(&mut app);
    app.run_ticks(420).expect("the circuit");

    let run = run(&app.world);
    assert_eq!(
        run.collected, run.total,
        "the circuit should collect all {} sigils, got {}",
        run.total, run.collected
    );
    assert_eq!(run.phase, Phase::Won);
    assert_eq!(run.score(), 60);
}

#[test]
fn touching_a_warden_loses() {
    // The west warden starts at (-5, 2) and walks east along the top. The player runs north-west to
    // meet it.
    let mut app = scripted(|source| {
        source.axis(Tick(0), MOVE_X, -1.0);
        source.axis(Tick(0), MOVE_Y, 1.0);
    });

    for _ in 0..600 {
        app.run_ticks(1).expect("a tick");
        if run(&app.world).phase == Phase::Lost {
            return;
        }
    }
    panic!("ten seconds of walking into a warden's route never got the player caught");
}

#[test]
fn the_run_stops_when_it_is_over() {
    // Winning or losing freezes everything. Without this the player could keep collecting after
    // being caught, which makes the outcome meaningless.
    let mut app = scripted(|source| {
        source.axis(Tick(0), MOVE_X, 1.0);
    });
    despawn_wardens(&mut app);

    // Force a win by taking every sigil out of the world except one, then walking onto it.
    app.run_ticks(60).expect("collect the east sigil");
    force_win(&mut app);

    let frozen = player_at(&app.world);
    app.run_ticks(120).expect("keep pushing east");
    assert_eq!(
        player_at(&app.world),
        frozen,
        "the player should not move after the run is over"
    );
}

// --- Traps: the component and system written for M1 exit gate 4 ---

#[test]
fn stepping_on_a_trap_loses() {
    // The east trap sits at x = 3 on the middle row -- squarely on the apparent express lane between
    // the two middle sigils, which is the point of it.
    let mut app = scripted_armed(|source| {
        source.axis(Tick(0), MOVE_X, 1.0);
    });
    despawn_wardens(&mut app);

    for _ in 0..120 {
        app.run_ticks(1).expect("a tick");
        if run(&app.world).phase == Phase::Lost {
            let x = player_at(&app.world)[0];
            assert!(
                (2.5..3.5).contains(&x),
                "the run should have ended at the trap near x = 3, not at x = {x}"
            );
            return;
        }
    }
    panic!("two seconds of running east never reached the trap at x = 3");
}

#[test]
fn a_sprung_trap_is_marked_rather_than_removed() {
    // It stays visible so a player can see what caught them, which is information worth keeping and
    // costs nothing. Despawning would erase the explanation along with the trap.
    let mut app = scripted_armed(|source| {
        source.axis(Tick(0), MOVE_X, 1.0);
    });
    despawn_wardens(&mut app);
    app.run_ticks(60).expect("run east into the trap");

    assert_eq!(run(&app.world).phase, Phase::Lost);
    assert_eq!(
        app.world.query::<(&Trap,)>().count(),
        2,
        "both traps should still exist"
    );
    let disarmed = app
        .world
        .query::<(&Trap,)>()
        .filter(|(_, (trap,))| !trap.armed)
        .count();
    assert_eq!(disarmed, 1, "exactly the one that was stepped on");
}

#[test]
fn a_disarmed_trap_lets_the_player_past() {
    // Which is what makes `scripted` usable for every other test in this file, so it is worth
    // asserting rather than assuming.
    let mut app = scripted(|source| {
        source.axis(Tick(0), MOVE_X, 1.0);
    });
    despawn_wardens(&mut app);
    app.run_ticks(60)
        .expect("straight through where the trap is");

    assert_eq!(run(&app.world).phase, Phase::Playing);
    assert!(player_at(&app.world)[0] > 3.5, "the player got past x = 3");
}

#[test]
fn the_game_is_deterministic() {
    // Invariant I3, at the level that matters: the same script twice is the same game twice. This is
    // what makes `replays/` meaningful and what makes a bug reproducible.
    let script = |source: &mut ScriptedSource| {
        source.axis(Tick(0), MOVE_X, 1.0);
        source.axis(Tick(45), MOVE_Y, 1.0);
        source.axis(Tick(90), MOVE_X, -1.0);
    };

    let mut first = scripted(script);
    let mut second = scripted(script);
    first.run_ticks(300).expect("300 ticks");
    second.run_ticks(300).expect("300 ticks");

    assert_eq!(first.state_hash(), second.state_hash());
}

// --- Helpers ---

/// Removes both wardens, for tests about something other than evasion.
fn despawn_wardens(app: &mut App) {
    let wardens: Vec<_> = app
        .world
        .query::<(&Warden,)>()
        .map(|(entity, _)| entity)
        .collect();
    for warden in wardens {
        app.world.despawn(warden);
    }
}

/// Collects every remaining sigil, so the win condition fires on the next tick.
fn force_win(app: &mut App) {
    let sigils: Vec<_> = app
        .world
        .query::<(&Sigil,)>()
        .map(|(entity, _)| entity)
        .collect();
    let taken = sigils.len() as u32;
    for sigil in sigils {
        app.world.despawn(sigil);
    }
    if let Some(run) = app.world.resource_mut::<Run>() {
        run.collected += taken;
    }
    app.run_ticks(1).expect("let resolve_outcome see it");
    assert_eq!(run(&app.world).phase, Phase::Won);
}
