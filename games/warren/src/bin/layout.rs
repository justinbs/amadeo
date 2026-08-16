//! Writes a generated interior to a scene file (ADR 0071).
//!
//! ```text
//! cargo run -p warren --bin layout -- 12345 14 games/warren/scenes/generated.scene
//! ```
//!
//! # Why a binary in the game rather than `amadeo generate`
//!
//! Two reasons, and the second is the one that decides it.
//!
//! `games/vault`'s `pix` and `games/atrium`'s `tone` set the precedent: a game generating its own
//! content with a small binary beside it is how this project has done this twice already, and both
//! stayed there rather than growing into engine commands.
//!
//! And ADR 0071 §5 keeps the *generator* in the game until a second game wants it. A CLI command
//! would put it in the engine on its first day, which is exactly the "designed against zero users"
//! bet `modules/amadeo-interaction` lost. When a second game wants a layout, this moves to
//! `modules/` and an `amadeo generate` becomes a reasonable thing to ask for.
//!
//! # It writes and stops
//!
//! No world is built and no game is launched. That is the whole of ADR 0071 §1: the artefact is a
//! file, so producing one needs nothing but text — and `amadeo check` on the result is a far
//! stronger test of it than this program could perform on itself.

use std::path::PathBuf;

fn main() -> anyhow::Result<()> {
    let arguments: Vec<String> = std::env::args().skip(1).collect();

    // Defaults rather than a required argument list, so that running it with nothing at all does
    // something useful and shows what the arguments mean.
    let seed: u64 = match arguments.first() {
        Some(raw) => raw.parse().map_err(|_| {
            anyhow::anyhow!("`{raw}` is not a seed; it takes a whole number, as in `20250815`")
        })?,
        // The shipped level's own seed, so running this with no arguments rewrites `generated.scene`
        // as it already is rather than replacing the level the game plays with a different one.
        None => warren::GENERATED_SEED,
    };
    let rooms: usize = match arguments.get(1) {
        Some(raw) => raw.parse().map_err(|_| {
            anyhow::anyhow!("`{raw}` is not a room count; it takes a whole number, as in `14`")
        })?,
        None => warren::GENERATED_ROOMS,
    };
    let path = arguments.get(2).map_or_else(
        || PathBuf::from("games/warren/scenes/generated.scene"),
        PathBuf::from,
    );

    let layout = warren::lay_out(seed, rooms);

    // **The gate that did not exist, and the reason a bad level shipped.**
    //
    // Every earlier check asked whether a layout was *valid* — connected, looped, byte-stable. None
    // asked whether it was any good, so seed 20250815 went in with the key one door from the door it
    // opens and nothing anywhere noticed: it loaded, it validated, the whole suite was green, and the
    // capture was a room. A bad layout is indistinguishable from a good one unless something says
    // what good means.
    //
    // It **refuses** rather than warning. A warning printed by a tool that then does the thing anyway
    // is a warning nobody reads, and this one is being added precisely because nobody looked.
    let shortcomings = layout.shortcomings();
    if !shortcomings.is_empty() {
        eprintln!("seed {seed} makes a poor level and was not written:");
        for problem in &shortcomings {
            eprintln!("  - {problem}");
        }
        eprintln!("\nTry another seed. Most work; this one is a bad draw.");
        std::process::exit(1);
    }

    let scene = warren::to_scene(&layout);

    if let Some(parent) = path.parent() {
        std::fs::create_dir_all(parent)?;
    }
    std::fs::write(&path, &scene)?;

    // Reported rather than silent, and the numbers are the ones worth knowing: a layout with no
    // loop is a level of dead ends, and a room count that does not match what was asked for is the
    // loop-closing room being added (see `lay_out`).
    println!(
        "wrote {} — seed {seed}, {} rooms, {} doors, loop: {}",
        path.display(),
        layout.rooms.len(),
        layout.door_count(),
        if layout.has_loop() { "yes" } else { "NO" }
    );
    println!("check it with `amadeo check {}`", path.display());
    Ok(())
}
