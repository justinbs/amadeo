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

/// The places a moment can be taken, as `(argument, landmark, file)`.
///
/// # Why more than one, which is a review finding rather than a convenience
///
/// Engine gate review 16 pointed out that item 31 had been closed with a route reaching **exactly
/// one location**: every frame any reviewer could take of this game was the player's start bore, one
/// of fourteen. The key, the way out, the warden and two of the three section conditions were
/// unphotographable, while item 24's close condition is *"a capture of it shows…"*. Review 17
/// repeated it and called the gate *"decided on one fourteenth of the game"*.
///
/// So the landmarks `Landmarks` already chooses are the places worth standing, and each writes its
/// own snapshot. The player is **placed** at the landmark rather than walked there: where a scripted
/// walk ends depends on the level, and a snapshot whose position depends on pathing is one that
/// moves every time the generator does.
const MOMENTS: [(&str, &str, &str); 4] = [
    ("start", "where you wake up", "playing.snapshot"),
    ("key", "the room the key is in", "at_key.snapshot"),
    ("exit", "the way out", "at_exit.snapshot"),
    ("warden", "where the warden begins", "at_warden.snapshot"),
];

fn main() {
    let wanted: Vec<String> = std::env::args().skip(1).collect();
    if wanted.iter().any(|argument| argument == "--help") {
        println!("usage: moment [start|key|exit|warden]...   (default: all of them)");
        for (name, what, file) in MOMENTS {
            println!("  {name:<7} {what:<26} -> snapshots/{file}");
        }
        return;
    }

    let chosen: Vec<&(&str, &str, &str)> = if wanted.is_empty() {
        MOMENTS.iter().collect()
    } else {
        MOMENTS
            .iter()
            .filter(|(name, _, _)| wanted.iter().any(|argument| argument == name))
            .collect()
    };
    if chosen.is_empty() {
        eprintln!("no such moment: {}", wanted.join(" "));
        eprintln!("try one of: start key exit warden");
        std::process::exit(1);
    }

    for (name, what, file) in chosen {
        write_moment(name, what, file);
    }
}

/// Builds the level, stands the player at one landmark, and writes that snapshot.
fn write_moment(name: &str, what: &str, file: &str) {
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

    // **Standing where the moment asks for.** Placed rather than walked, and *after* the settle: the
    // character controller rewrites its own `Transform` from `CharacterMotion` every tick, so a write
    // before it has found the floor is undone (Q30).
    if name != "start"
        && let Err(error) = stand_at_landmark(&mut app, name)
    {
        eprintln!("could not stand at the {name}: {error}");
        std::process::exit(1);
    }

    let out = manifest_dir().join("snapshots");
    if let Err(error) = std::fs::create_dir_all(&out) {
        eprintln!("could not create {}: {error}", out.display());
        std::process::exit(1);
    }
    let path = out.join(file);

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
    println!("  {name} — {what}, standing at {}", standing(&app.world));
    println!(
        "  photograph it with: amadeo capture -p warren --from games/warren/snapshots/{file} --ticks 5 shot.png\n"
    );
}

/// Puts the player at one of [`Landmarks`]' cells, on the floor at the bore's centreline.
///
/// Offset along the bore from dead centre, for the reason the generator offsets its props: standing
/// exactly on a landmark can mean standing inside whatever the landmark put there.
///
/// [`Landmarks`]: warren::Landmarks
fn stand_at_landmark(app: &mut amadeo_app::App, name: &str) -> anyhow::Result<()> {
    let layout = warren::lay_out(warren::GENERATED_SEED, warren::GENERATED_ROOMS);
    let marks = layout.landmarks;
    let cell = match name {
        "key" => marks.key,
        "exit" => marks.exit,
        "warden" => marks.warden,
        other => anyhow::bail!("no landmark called `{other}`"),
    };

    let you = warren::player(&app.world).ok_or_else(|| anyhow::anyhow!("there is no character"))?;
    let at = [
        cell.0 as f32 * warren::CELL,
        warren::PLAYER_STAND,
        cell.1 as f32 * warren::CELL + 4.2,
    ];
    if let Some(transform) = app.world.get_mut::<amadeo_transform::Transform>(you) {
        transform.translation = at;
        // **Facing along the bore, not at whatever the start happened to face.** A landmark is a
        // cell, and the player's rotation is carried over from where they woke up — which put the
        // first of these snapshots a metre from a wall with the hand lamp blowing it out. Every bore
        // runs north-south, so `facing(North)` looks down the tube from the south end of the cell.
        transform.rotation = [0.0, warren::facing(warren::Side::North), 0.0];
    }
    app.run_ticks(8)?;
    Ok(())
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
