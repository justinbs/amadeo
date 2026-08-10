//! `sky` — generates the Scarp's sky as a Radiance `.hdr`.
//!
//! ```text
//! cargo run -p scarp --bin sky
//! ```
//!
//! # Why generated rather than downloaded
//!
//! The same argument `turf` makes, one step stronger. Invariant I1 wants everything authorable and
//! diffable, and an `.hdr` is neither — so the *source* has to be text, and this file is it. A sky is
//! not drawn; it is a formula about where the sun is and how air scatters light.
//!
//! It also keeps the repository free of a downloaded environment map whose licence would have to be
//! tracked, and it means the sky can be re-derived for any sun direction rather than being one fixed
//! afternoon.
//!
//! # Why it has to be HDR, and what that buys
//!
//! A `.png` clips at white, so the sun and the sky beside it would be the same colour — and the sun
//! is the part that matters. Here it is **hundreds of times** brighter than the sky around it, which
//! is what puts a bright highlight on a metal and a soft blue fill on everything facing up.
//!
//! # The sun in here and the sun in the scene must agree
//!
//! [`SUN_DIRECTION`] is the same direction `scenes/scarp.scene` gives its `DirectionalLight`. If they
//! drift apart, shadows fall one way and the bright part of the sky sits another, which reads as
//! nothing being wrong and everything looking slightly off. There is no mechanism holding them
//! together — see ADR 0049's consequences.
//!
//! Idempotent: running it twice writes a byte-identical file.

use amadeo_image::{HdrImage, encode_hdr};
use std::path::{Path, PathBuf};

/// Width in pixels. Equirectangular, so height is half.
///
/// Modest on purpose: the prefiltering that consumes this immediately blurs it into a 16-pixel
/// irradiance cube and a 128-pixel specular chain, so detail beyond this is thrown away at load.
const WIDTH: u32 = 512;

/// The direction light travels, matching the `Transform` on the Scarp's sun.
///
/// A `DirectionalLight` points along its entity's own negative Z, and the scene rotates that by
/// `-42°` about X and `28°` about Y. Written out as the resulting vector rather than recomputed
/// here, so this file needs no rotation maths — and stated to three decimals so a reader can check
/// it rather than trust it.
const SUN_DIRECTION: [f32; 3] = [-0.349, 0.669, -0.656];

/// How wide the sun's disc is, as the cosine of its angular radius.
///
/// The real sun is about half a degree across. This is deliberately wider — a disc that small lands
/// between the samples of a 512-pixel map and disappears entirely, which would take the highlight
/// with it.
const SUN_COS_RADIUS: f32 = 0.995;

/// How bright the sun's disc is relative to the sky. The whole reason for a high-dynamic-range file.
const SUN_INTENSITY: f32 = 250.0;

fn main() {
    let out = manifest_dir().join("assets/skies");
    if let Err(error) = std::fs::create_dir_all(&out) {
        eprintln!("could not create {}: {error}", out.display());
        std::process::exit(1);
    }

    let path = out.join("scarp_sky.hdr");
    let image = render();
    if let Err(error) = std::fs::write(&path, encode_hdr(&image)) {
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
    // to forget (Q31 does not reach this format).
    let meta = path.with_extension("hdr.ama-meta");
    if let Err(error) = std::fs::write(&meta, "id = \"scarp_sky\"\n") {
        eprintln!("could not write {}: {error}", meta.display());
        std::process::exit(1);
    }
    println!("wrote {}", meta.display());
}

/// Builds the sky.
fn render() -> HdrImage {
    let height = WIDTH / 2;
    let mut pixels = Vec::with_capacity((WIDTH * height) as usize);

    let sun = normalise(SUN_DIRECTION);

    for y in 0..height {
        // Latitude, top row to bottom. Zero at straight up, PI at straight down — the same mapping
        // `Cubemap::from_equirectangular` reads it back with.
        let latitude = (y as f32 + 0.5) / height as f32 * std::f32::consts::PI;
        for x in 0..WIDTH {
            let longitude = ((x as f32 + 0.5) / WIDTH as f32 - 0.5) * std::f32::consts::TAU;

            let direction = [
                latitude.sin() * longitude.cos(),
                latitude.cos(),
                latitude.sin() * longitude.sin(),
            ];
            pixels.push(sky_colour(direction, sun));
        }
    }

    HdrImage {
        width: WIDTH,
        height,
        pixels,
    }
}

/// The colour of the sky in one direction.
fn sky_colour(direction: [f32; 3], sun: [f32; 3]) -> [f32; 4] {
    let up = direction[1];

    // Below the horizon is ground rather than sky: a dim, desaturated green picking up the turf, so
    // that surfaces facing down get a plausible bounce instead of black. This is the half of an
    // environment people forget, and its absence reads as objects floating.
    if up < 0.0 {
        let depth = (-up).min(1.0);
        let shade = 0.10 - 0.04 * depth;
        return [shade * 0.85, shade, shade * 0.65, 1.0];
    }

    // Sky: deeper blue overhead, paler and warmer towards the horizon, which is roughly what
    // atmospheric scattering does and is most of what makes a sky read as a sky.
    let horizon = [0.52, 0.60, 0.72];
    let zenith = [0.12, 0.26, 0.58];
    let blend = up.powf(0.55);
    let mut colour = [
        horizon[0] + (zenith[0] - horizon[0]) * blend,
        horizon[1] + (zenith[1] - horizon[1]) * blend,
        horizon[2] + (zenith[2] - horizon[2]) * blend,
    ];

    // A glow around the sun, widening the bright region so the light does not arrive from a single
    // point. Real sky has this and it is what softens a highlight's edge.
    //
    // `towards_sun` because SUN_DIRECTION is the direction light *travels*, so the sun itself is the
    // other way — the same negation `mesh.wgsl` does with `light_direction`.
    let towards_sun = [-sun[0], -sun[1], -sun[2]];
    let alignment = dot(direction, towards_sun).max(0.0);
    let glow = alignment.powf(64.0) * 3.0 + alignment.powf(8.0) * 0.35;
    for (channel, warm) in colour.iter_mut().zip([1.0, 0.93, 0.80]) {
        *channel += glow * warm;
    }

    // And the disc itself, hundreds of times brighter than anything around it.
    if alignment > SUN_COS_RADIUS {
        for (channel, warm) in colour.iter_mut().zip([1.0, 0.96, 0.88]) {
            *channel += SUN_INTENSITY * warm;
        }
    }

    [colour[0], colour[1], colour[2], 1.0]
}

fn dot(a: [f32; 3], b: [f32; 3]) -> f32 {
    a[0] * b[0] + a[1] * b[1] + a[2] * b[2]
}

fn normalise(a: [f32; 3]) -> [f32; 3] {
    let length = dot(a, a).sqrt();
    [a[0] / length, a[1] / length, a[2] / length]
}

/// This crate's directory, so the generator writes next to the game rather than next to the shell.
fn manifest_dir() -> PathBuf {
    Path::new(env!("CARGO_MANIFEST_DIR")).to_path_buf()
}
