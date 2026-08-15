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
        None => 20_250_815,
    };
    let rooms: usize = match arguments.get(1) {
        Some(raw) => raw.parse().map_err(|_| {
            anyhow::anyhow!("`{raw}` is not a room count; it takes a whole number, as in `14`")
        })?,
        None => 14,
    };
    let path = arguments.get(2).map_or_else(
        || PathBuf::from("games/warren/scenes/generated.scene"),
        PathBuf::from,
    );

    let layout = warren::lay_out(seed, rooms);
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
