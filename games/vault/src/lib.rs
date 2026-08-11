//! **The Vault** — M1's exit gate, as a library so it can be tested.
//!
//! The game is a binary; this is the same game with the windowing left out, which is what lets
//! `tests/` build a real world and interrogate it. `src/main.rs` is a thin shell over
//! [`build_simulation`] plus a winit event loop.
//!
//! # What this game is for
//!
//! `docs/05-roadmap.md` sets the bar: a complete small 2D game — "player moves, enemies patrol,
//! collision, a score, a win state" — **built with zero editor use**, authored entirely through text
//! files and RPC, and verified through `inspect`, headless runs, and `render.describe` rather than by
//! looking at it.
//!
//! So the point is not the game. It is that every claim about the game is checkable without eyes,
//! which is what `tests/plays_itself.rs` and `tests/verified_without_eyes.rs` do.
//!
//! # Where the level lives
//!
//! `scenes/vault.scene` holds the player, both wardens with their patrol routes, the six sigils, the
//! floor, and the score readout — everything designed. The wall grid is the one exception, and
//! [`level`] explains why.

pub mod game;
pub mod level;

use amadeo_app::{App, Stage, system};
use amadeo_input::{InputDriver, NullSource, SAMPLE_INPUT, sample_input};
use amadeo_render::{BoxMesh, Camera, Environment, Quad, SortOrder, Sprite, TextureCache};
use amadeo_transform::{GlobalTransform, PROPAGATE_TRANSFORMS, Transform, propagate_transforms};
use game::{Floor, Patrol, Player, Run, ScoreDigit, Sigil, Trap, Wall, Warden, labels};

/// The seed this game runs at unless told otherwise.
///
/// Nothing in the Vault is random, so the seed changes nothing today. It is read anyway because
/// `amadeo replay` passes one, and a game that ignores it gets a seed-mismatch error rather than a
/// working replay — see `amadeo_app::requested_seed`.
pub const DEFAULT_SEED: u64 = 0;

/// Where this game keeps its assets, relative to the project root.
pub const ASSET_DIRECTORY: &str = "games/vault/assets";

/// The level, as text. Invariant I1: this file is the source of truth for the arena's contents.
pub const SCENE: &str = include_str!("../scenes/vault.scene");

/// How many world units tall the view is.
///
/// Eight, so the seven-row arena fits with a row of margin above for the score readout. Width
/// follows the window's aspect ratio, which is why the arena is wider than it is tall.
///
/// **The camera in `vault.scene` is the authority**; this is here for the tests that reason about
/// what fits on screen. They must agree, and `the_scene_camera_matches_the_declared_view_height`
/// is what makes a disagreement a failing test rather than a mystery.
pub const VIEW_HEIGHT: f32 = 8.0;

/// Builds the world: the level from its scene file, plus the walls around it.
///
/// Shared by the windowed path, the headless path, and every test — so an answer the agent gives
/// about this world is an answer about the game that actually runs (invariant I7).
///
/// # Errors
///
/// If the scene file will not parse or will not instantiate against the registered components, or
/// if the asset directory cannot be scanned.
pub fn build_simulation() -> anyhow::Result<App> {
    // Asked for before the app exists, because `with_seed` fixes the seed at construction.
    let mut app = App::with_seed(amadeo_app::requested_seed().unwrap_or(DEFAULT_SEED));

    // Registration is what puts a type into `amadeo describe` and lets the scene file name it
    // (ADR 0016, invariant I8). Engine components and this game's own, together — the schema
    // describes *this game*.
    app.register_component::<Transform>()?;
    app.register_component::<GlobalTransform>()?;
    app.register_component::<Sprite>()?;
    app.register_component::<Quad>()?;
    app.register_component::<SortOrder>()?;
    app.register_component::<Player>()?;
    app.register_component::<Warden>()?;
    app.register_component::<Patrol>()?;
    app.register_component::<Sigil>()?;
    app.register_component::<Wall>()?;
    app.register_component::<ScoreDigit>()?;
    app.register_component::<Floor>()?;
    app.register_component::<Trap>()?;

    app.scan_assets(ASSET_DIRECTORY)?;
    app.insert_service(TextureCache::new());
    // The camera is an entity now (ADR 0031), and it is authored in `vault.scene` rather than here.
    // Its position is the nudge that keeps the score readout above the arena without overlapping the
    // top wall -- found by `render.describe` rather than by looking.
    app.register_component::<Camera>()?;
    // Registered because the Vault *ships* `assets/looks/corridor_dark.environment`, even though its
    // camera deliberately does not use it. Without this, loading the file still works — the loader
    // reads the type directly — but `amadeo check` refuses it, saying no component named
    // `Environment` is registered. A game whose own asset fails the validator it ships with is worse
    // than one that has no validator, so registration is part of shipping the asset.
    app.register_component::<Environment>()?;
    app.register_component::<BoxMesh>()?;
    app.insert_resource(Run::default());

    // Compiled in rather than read at runtime, so the binary carries its own level and a replay
    // cannot silently be run against an edited one. `amadeo check games/vault/scenes/vault.scene`
    // validates the same text against the real schema.
    let document = amadeo_scene::parse(SCENE)
        .map_err(|error| anyhow::anyhow!("games/vault/scenes/vault.scene: {error}"))?;
    app.load_scene(&document)?;

    level::spawn_walls(&mut app);

    // Counted from the world rather than hard-coded, so adding a sigil to the scene file changes the
    // win condition with it and nothing has to be kept in step by hand.
    let total = app.world.query::<(&Sigil,)>().count() as u32;
    if let Some(run) = app.world.resource_mut::<Run>() {
        run.total = total;
    }

    app.add_system(Stage::PreSimulation, system(SAMPLE_INPUT, sample_input));
    app.add_system(
        Stage::Simulation,
        system(labels::STEER_PLAYER, game::steer_player),
    );
    app.add_system(
        Stage::Simulation,
        system(labels::PATROL_WARDENS, game::patrol_wardens),
    );
    app.add_system(
        Stage::PostSimulation,
        system(labels::COLLECT_SIGILS, game::collect_sigils),
    );
    app.add_system(
        Stage::PostSimulation,
        system(labels::SPRING_TRAPS, game::spring_traps).after(labels::COLLECT_SIGILS),
    );
    // After the trap, so stepping on one and taking the last sigil on the same tick is a loss.
    app.add_system(
        Stage::PostSimulation,
        system(labels::RESOLVE_OUTCOME, game::resolve_outcome).after(labels::SPRING_TRAPS),
    );
    app.add_system(
        Stage::PostSimulation,
        system(labels::SHOW_SCORE, game::show_score).after(labels::COLLECT_SIGILS),
    );
    // Last, so the composed transforms reflect where everything finally ended up this tick.
    app.add_system(
        Stage::PostSimulation,
        system(PROPAGATE_TRANSFORMS, propagate_transforms).after(labels::RESOLVE_OUTCOME),
    );

    Ok(app)
}

/// The same simulation with no window, no GPU, and no keyboard — what the agent inspects.
///
/// # Errors
///
/// Whatever [`build_simulation`] returns.
pub fn build_headless() -> anyhow::Result<App> {
    let mut app = build_simulation()?;
    amadeo_input::install(&mut app.world, InputDriver::new(Box::new(NullSource)));
    Ok(app)
}
