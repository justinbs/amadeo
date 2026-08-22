//! `moment` — writes a snapshot of the Warren **mid-run**, so a frame of it can be photographed.
//!
//! ```text
//! cargo run -p warren --bin moment
//! amadeo capture -p warren --from games/warren/snapshots/playing.snapshot --ticks 5 shot.png
//! ```
//!
//! # Why this exists, which is a finding rather than a convenience
//!
//! `docs/13` §3 defines POLISHED as **a frame from a real game**, and engine gate review 13
//! discovered while trying to take one that **nobody can**. The Warren boots into its title screen
//! and stays there: `amadeo capture --yaw/--pitch` aims the camera but cannot dismiss a menu, the
//! agent protocol has no mutating methods (item 20, deferred to M4), and no snapshot was committed.
//! Every frame any reviewer had ever taken of this game was its main menu.
//!
//! So the gate's own condition was unreachable, and had been for thirteen reviews, and the thing
//! that found it was somebody trying to satisfy it rather than anybody reading the plan.
//!
//! # Why a committed snapshot rather than a flag
//!
//! `capture --from <file>` already restores a snapshot before drawing (ADR 0028), so this needs no
//! engine work at all — which is the whole argument for it over `capture --input <replay>`, the
//! fuller answer that would.
//!
//! And a snapshot is **text** (ADR 0028), so the committed artefact is diffable and a person can
//! read where the player was standing. A binary blob would be the thing invariant I1 exists to
//! refuse.
//!
//! # Why it is regenerated rather than hand-kept
//!
//! A snapshot pins the world's shape, so it goes stale the moment a component gains a field — and
//! ADR 0069's lenient restore makes that survivable rather than invisible. Running this again is
//! how it catches up, which is `layout.rs`, `sounds.rs` and `gloom.rs`'s argument exactly: the
//! *source* is the generator and the file is its output.
//!
//! Idempotent for a given seed: the level is generated from `DEFAULT_SEED` and the simulation is
//! deterministic (I3), so two runs write byte-identical text.

use amadeo_ecs::World;
use std::path::PathBuf;

/// How far into the run the moment is taken.
///
/// **Two seconds of settling, and no more.** Long enough for the character to land, for the physics
/// broad phase to build and for `propagate_transforms` to have composed everything; short enough
/// that the player is still where the level generator put them, which is what makes the frame
/// reproducible rather than dependent on where a scripted walk happened to end.
const SETTLE: u64 = 120;

fn main() {
    let mut app = match warren::build_simulation() {
        Ok(app) => app,
        Err(error) => {
            eprintln!("the game would not build: {error}");
            std::process::exit(1);
        }
    };

    // **Past the title screen, which is the entire point.** `Screen` is this game's own hashed
    // resource and the authority; `apply_screen` projects it onto the engine's `Paused` every tick,
    // so setting it here is the same thing pressing BEGIN does.
    if let Some(screen) = app.world.resource_mut::<warren::Screen>() {
        *screen = warren::Screen::Playing;
    }

    if let Err(error) = app.run_ticks(SETTLE) {
        eprintln!("the run stopped after {SETTLE} ticks: {error}");
        std::process::exit(1);
    }

    // **With the torch in hand, because a frame of this game without it is a frame of a different
    // game.** The Warren's whole lighting design is a warm hand lamp against cold fittings
    // (`docs/11` §5a), and engine gate item 24's close condition names it. A snapshot taken at the
    // start line has the beam at `intensity 0.0` and shows only what the emergency circuit lights,
    // which is half the picture and the wrong half.
    //
    // Done through the game's own path rather than by writing the beam: `store` is what
    // `take_what_you_used` calls, `carry_the_torch` reads the bag and lights the beam, and the torch
    // loses its `Transform` on the way in (ADR 0070) so it stops being on its crate. Setting the
    // light directly would produce a state the game cannot reach.
    if let Err(error) = pick_up_the_torch(&mut app) {
        eprintln!("could not put the torch in the player's hand: {error}");
        std::process::exit(1);
    }

    let out = manifest_dir().join("snapshots");
    if let Err(error) = std::fs::create_dir_all(&out) {
        eprintln!("could not create {}: {error}", out.display());
        std::process::exit(1);
    }
    let path = out.join("playing.snapshot");

    let snapshot = app.capture_snapshot();
    let text = amadeo_snapshot::to_text(&snapshot);
    if let Err(error) = std::fs::write(&path, &text) {
        eprintln!("could not write {}: {error}", path.display());
        std::process::exit(1);
    }

    println!(
        "wrote {} — tick {}, {} entities, {} resources, state hash {:016x}",
        path.display(),
        snapshot.tick.0,
        snapshot.entities.len(),
        snapshot.resources.len(),
        snapshot.state_hash,
    );
    println!("where the player is standing: {}", standing(&app.world));
    println!(
        "\nphotograph it with:\n  amadeo capture -p warren --from games/warren/snapshots/playing.snapshot --ticks 5 shot.png"
    );
}

/// Puts the torch in the player's bag and lets the beam come up.
///
/// The extra ticks are not padding: `carry_the_torch` runs in `Simulation` and the beam's intensity
/// is read by the collection pass in `Render`, so a snapshot taken on the same tick as the pickup
/// would record a lit bag and a dark beam.
fn pick_up_the_torch(app: &mut amadeo_app::App) -> anyhow::Result<()> {
    let you = warren::player(&app.world).ok_or_else(|| anyhow::anyhow!("there is no character"))?;
    let torch = app
        .world
        .query::<(&amadeo_inventory::Item,)>()
        .find(|(_, (item,))| item.kind == warren::TORCH)
        .map(|(entity, _)| entity)
        .ok_or_else(|| anyhow::anyhow!("this level has no torch in it"))?;

    amadeo_inventory::store(&mut app.world, torch, you)
        .map_err(|error| anyhow::anyhow!("the bag would not take it: {error}"))?;
    app.run_ticks(6)?;
    Ok(())
}

/// Where the player ended up, printed because a snapshot nobody can locate is one nobody trusts.
fn standing(world: &World) -> String {
    let Some(player) = warren::player(world) else {
        return "nowhere — there is no character in this world".to_string();
    };
    world
        .get::<amadeo_transform::Transform>(player)
        .map_or_else(
            || "a character with no transform".to_string(),
            |at| {
                let [x, y, z] = at.translation;
                format!("({x:.2}, {y:.2}, {z:.2})")
            },
        )
}

/// This crate's directory, so the binary can be run from anywhere.
fn manifest_dir() -> PathBuf {
    PathBuf::from(env!("CARGO_MANIFEST_DIR"))
}
