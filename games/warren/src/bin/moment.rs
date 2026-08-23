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

    // **Standing back from the thing and looking AT it**, which the exit snapshot did not do.
    //
    // Every one of these used to stand at `cell + 4.2` in `z` and face north, on the argument that a
    // bore runs north–south so that always looks down the tube. For the key and the warden that is
    // right. For the exit it is exactly backwards: the door sits on the bulkhead at one END of its
    // cell, so facing north from the south side of the cell puts the way out **behind the camera**.
    //
    // The snapshot committed for a reviewer to photograph the way out therefore faced away from it,
    // and every measurement anybody has taken of that door — including engine gate review 19's "an
    // 11-level range across the entire face of the door" — was taken of the bulkhead at the far end
    // instead. `exit_side` already knows which end the door is on and is what the level generator
    // places it with, so this asks the same function rather than assuming.
    //
    // The exit also stands *closer*. A landmark snapshot is a photograph of the thing it is named
    // after, and 4.2 m back from the middle of the cell leaves the door ten metres away at the end
    // of the bay — small, fogged, and impossible to judge. Three metres is the distance a player
    // actually arrives at it from.
    // **Each landmark is photographed looking AT the thing it is named after, and off its axis.**
    //
    // Both halves were wrong. Every one of these used to stand at `cell + 4.2` in `z` facing north,
    // which put the exit door behind the camera (`exit_side` already knew which bulkhead it was on)
    // and the key **61.9° off the view axis** against a ~50° half-FOV — so the snapshot committed for
    // a reviewer to photograph the key did not contain it. Engine gate review 20 found the second one
    // after review 19 found the first, in the same function, because fixing one landmark did not
    // prompt anybody to check the other two.
    //
    // And they were bullseye compositions: symmetry of 14.8 / 17.8 / 11.1 on row 600 against the
    // authored frame's 70.9, because a camera on the axis of a symmetrical bore makes both halves of
    // the picture the same picture. `across` is the look direction turned a quarter turn.
    let side = match name {
        "exit" => warren::exit_side(&layout),
        // The key board hangs on the east lining, `PROP_OFFSET` along the bore from the middle to
        // keep it clear of a centred cross-passage. Stand back from that wall and look at it.
        "key" => warren::Side::East,
        _ => warren::Side::North,
    };
    let (fx, fz) = side.step((0, 0));
    let (fx, fz) = (fx as f32, fz as f32);
    // How far back from the cell's centre to stand, along the look direction. Negative walks away
    // from what is being photographed, which is right for the warden -- it is a figure in a space and
    // wants the space around it -- and wrong for a door or a board, which are the subject.
    let along = match name {
        "exit" => 2.6,
        "key" => warren::BORE_HALF_WIDTH - 2.6,
        _ => -4.2,
    };
    // Level with the key board along the bore, so it is in front of the camera rather than beside it.
    let (ax, az) = if name == "key" {
        (0.0, warren::KEY_ALONG)
    } else {
        (0.0, 0.0)
    };
    let across = match name {
        "warden" => 1.1,
        "key" => 0.55,
        _ => 0.75,
    };
    let you = warren::player(&app.world).ok_or_else(|| anyhow::anyhow!("there is no character"))?;
    let at = [
        cell.0 as f32 * warren::CELL + fx * along + ax + (-fz) * across,
        warren::PLAYER_STAND,
        cell.1 as f32 * warren::CELL + fz * along + az + fx * across,
    ];
    if let Some(transform) = app.world.get_mut::<amadeo_transform::Transform>(you) {
        transform.translation = at;
        transform.rotation = [0.0, warren::facing(side), 0.0];
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
