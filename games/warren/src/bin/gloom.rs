//! `gloom` — generates the Warren's environment map, which is what lights everything the torch does
//! not.
//!
//! ```text
//! cargo run -p warren --bin gloom
//! ```
//!
//! # It is an ambient, not a sky
//!
//! `games/scarp`'s `sky` binary makes a sky you can *see*: a sun, a horizon, ground bounce. This
//! makes something nobody ever looks at. The Warren has a ceiling in every room, so the sky pass
//! never draws a pixel of this — its entire job is the **indirect** half of lighting (ADR 0049),
//! which is the light arriving at a surface from everywhere that is not a light.
//!
//! That was the gap `STATUS.md` recorded twice. With `sky ""` there is no indirect term at all, so
//! any surface a light does not reach is exactly black, and the dim `spill` directional was standing
//! in for an ambient by shining at everything from one angle. An angle is not an ambient: it leaves
//! the underside of every crate and the inside of every doorway unlit, and it cannot be tinted by
//! what the room is made of.
//!
//! **`spill` is gone as of session 23**, which is this paragraph's argument finally cashed. Engine
//! gate review 14 asked what a directional light called *"Spill from somewhere"* is doing a hundred
//! feet underground once the bore is sealed at both ends by bulkheads, and the honest answer is
//! nothing a player could name. Measured before removing it rather than after: with it, 12.5% of
//! pixels change by up to 52 levels — real, but it was buying a slightly brighter gloom by putting a
//! sun in a shelter. This map does the same job with a cause.
//!
//! # Why it is not simply a constant
//!
//! Because a constant is what ADR 0049 replaced, and Q28 is closed. A flat number lights every
//! surface identically whichever way it faces, which is exactly what makes untextured geometry read
//! as cardboard. This has **direction**: a little more from above, where the corridor lights are,
//! and a warm bounce from below, where the screed and the duckboards are. Surfaces facing up read cool and surfaces
//! facing down read warm, for nothing at runtime.
//!
//! # It is deliberately, awkwardly dim
//!
//! An ambient bright enough to see by is an ambient that makes the torch pointless, and the torch is
//! the game. So this is set low enough that an unlit room is *nearly* black — you can tell a wall
//! from a doorway and not much else — and everything else is the torch's job.

use amadeo_image::{HdrImage, encode_hdr};
use std::path::{Path, PathBuf};

/// Width of the equirectangular map. The height is half of it.
///
/// **Small on purpose, and it costs nothing here.** The Scarp needs resolution because its sky is
/// visible and has a sun in it; this is convolved into an irradiance map and a handful of blurred
/// specular levels before anything reads it, so detail finer than the convolution is thrown away
/// immediately. 128 is already more than the result can carry.
const WIDTH: u32 = 128;

/// The colour of the air above, in linear light — cool, because the corridor lamps are.
const ABOVE: [f32; 3] = [0.030, 0.032, 0.038];

/// The colour of the bounce from below — warmer and dimmer, because it is dust over concrete.
const BELOW: [f32; 3] = [0.020, 0.016, 0.012];

/// What the whole thing is multiplied by.
///
/// **The one number to change if the Warren is too dark or not dark enough**, and the one that
/// decides whether the torch matters. Everything above is a ratio; this is the level.
///
/// # Raised 5.0 → 8.0 in session 23, and the reason is measured rather than felt
///
/// Engine gate review 16 found three separate authored objects rendering at literally `RGB(0, 0, 0)`
/// — a 20 cm skirting kerb running the full width of every frame as a hard black band, the fitting's
/// housing, and the sign's surround. It suspected a metallic material, A/B'd that (7.9% of pixels,
/// max 23 levels, every zero still zero), **withdrew**, and A/B'd the ambient instead: lifting it
/// turned the kerb from 23 flat rows of zero into a graded top face and under-edge.
///
/// So the geometry was not missing and the materials were not wrong. There was nothing for a surface
/// facing away from every light to reflect. `docs/11` §1 quotes Frictional on exactly this: pitch
/// black is *not* effective, and what works is a carried source, **a little ambient**, and fog that
/// thickens with distance. This game had the first and the third.
///
/// The mood survives it, which is the part worth checking rather than assuming: the unlit direction
/// still peaks below 140 and keeps better than a seventh of the frame under luma 16.
const LEVEL: f32 = 8.0;

fn main() {
    let out = manifest_dir().join("assets/skies");
    if let Err(error) = std::fs::create_dir_all(&out) {
        eprintln!("could not create {}: {error}", out.display());
        std::process::exit(1);
    }

    let path = out.join("warren_gloom.hdr");
    let image = render();
    if let Err(error) = write_if_changed(&path, &encode_hdr(&image)) {
        eprintln!("could not write {}: {error}", path.display());
        std::process::exit(1);
    }
    println!(
        "wrote {} ({}x{})",
        path.display(),
        image.width,
        image.height
    );

    // The sidecar, so the asset system can find it by id (ADR 0020). No `color_space` line: an
    // `.hdr` is linear by definition, so unlike a normal map there is nothing to declare and nothing
    // to forget — Q31 does not reach this format.
    let meta = path.with_extension("hdr.ama-meta");
    if let Err(error) = write_if_changed(&meta, b"id = \"warren_gloom\"\n") {
        eprintln!("could not write {}: {error}", meta.display());
        std::process::exit(1);
    }
    println!("wrote {}", meta.display());
}

/// Builds the map.
///
/// # No trigonometry, which is not the usual way to do this
///
/// The obvious spelling turns each pixel into a direction with `sin` and `cos` and then reads the
/// vertical component back out — and for a gradient that depends *only* on height, every one of
/// those cancels: the vertical component of the direction at row `y` is simply `cos(latitude)`, and
/// `cos` of an evenly spaced latitude is an evenly spaced value from 1 to -1. So the whole map is a
/// linear ramp down the rows, and the horizontal coordinate does not enter into it at all.
///
/// That is worth doing rather than being clever about, because it means this file writes
/// byte-identical output on every platform without depending on anything IEEE 754 does not pin —
/// the same requirement `sounds.rs` has, and the same one ADR 0044 imposes a layer down.
fn render() -> HdrImage {
    let height = WIDTH / 2;
    let mut pixels = Vec::with_capacity((WIDTH * height) as usize);

    for y in 0..height {
        // 1.0 at the top row, -1.0 at the bottom: the vertical component of the direction this row
        // looks along. `+ 0.5` samples each row's centre rather than its edge.
        let up = 1.0 - 2.0 * (y as f32 + 0.5) / height as f32;
        // Remapped to 0..1 and squared towards the ends, so the two colours meet in a soft band
        // around the horizon rather than at a line. Squaring rather than a smoothstep because it is
        // one multiply and the difference is invisible after convolution.
        let blend = (up + 1.0) * 0.5;
        let shaped = blend * blend * (3.0 - 2.0 * blend);

        let colour = [
            mix(BELOW[0], ABOVE[0], shaped) * LEVEL,
            mix(BELOW[1], ABOVE[1], shaped) * LEVEL,
            mix(BELOW[2], ABOVE[2], shaped) * LEVEL,
            1.0,
        ];
        for _ in 0..WIDTH {
            pixels.push(colour);
        }
    }

    HdrImage {
        width: WIDTH,
        height,
        pixels,
    }
}

/// Linear interpolation, written out because this file has no other maths in it.
fn mix(a: f32, b: f32, t: f32) -> f32 {
    a + (b - a) * t
}

/// Writes only when the bytes differ, so re-running leaves timestamps alone.
fn write_if_changed(path: &Path, bytes: &[u8]) -> std::io::Result<()> {
    if let Ok(existing) = std::fs::read(path)
        && existing == bytes
    {
        return Ok(());
    }
    std::fs::write(path, bytes)
}

/// This crate's directory, so the tool works from anywhere.
fn manifest_dir() -> PathBuf {
    PathBuf::from(env!("CARGO_MANIFEST_DIR"))
}
