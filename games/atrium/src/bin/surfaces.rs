//! `surfaces` — generates the Atrium's surface textures as PNGs.
//!
//! ```text
//! cargo run -p atrium --bin surfaces
//! ```
//!
//! # Why generated rather than authored
//!
//! `games/vault`'s `pix`, `games/scarp`'s `turf` and `games/atrium`'s own `tone` all make the same
//! argument and this is the fourth instance of it: invariant I1 wants everything authorable and
//! diffable, a PNG is neither, so the *source* has to be text. A sprite's source is a drawn grid of
//! characters; ground and stone are formulas, and this file is one.
//!
//! It is also `docs/12-the-bar.md` §3's requirement in practice — **Claude can author this game's
//! textures rather than asking Justin for them** — which is the half of the bar most likely to be
//! quietly dodged.
//!
//! Idempotent: running it twice writes byte-identical files.
//!
//! # Why a laid grid rather than noise alone
//!
//! These exist to carry ADR 0078's `uv_scale`, and **a grid is the only pattern whose density you can
//! read off a picture**. Noise at two different texel densities looks like noise; a floor whose slabs
//! are two metres across next to a plinth whose slabs are two metres across is either obviously right
//! or obviously wrong at a glance. That is the whole failure the field exists to prevent, so the test
//! image has to make it visible.

use std::path::{Path, PathBuf};

/// Texture size in pixels. A power of two, so mip generation halves cleanly.
const SIZE: u32 = 256;

/// How many slabs across one tile of the image.
///
/// Two, so that one repeat of the texture is a 2×2 block of slabs. Combined with a `uv_scale` chosen
/// per surface this is what sets the real-world slab size, and keeping it small keeps the joint lines
/// crisp at 256 pixels.
const SLABS: u32 = 2;

fn main() {
    let out = manifest_dir().join("assets/textures");
    if let Err(error) = std::fs::create_dir_all(&out) {
        eprintln!("could not create {}: {error}", out.display());
        std::process::exit(1);
    }

    // The floor and the plinth share one image at different `uv_scale`s, which is precisely the
    // demonstration: same slab size on a 20 m floor and a 3 m plinth.
    write(
        &out.join("stone_slab.png"),
        "stone_slab",
        &render(
            0x51A9_3C7E,
            [0.52, 0.51, 0.49],
            [0.63, 0.62, 0.59],
            [0.34, 0.33, 0.32],
        ),
    );
}

/// Renders a slab pattern: two greys blended by tiling noise, with a darker joint between slabs.
fn render(seed: u64, low: [f32; 3], high: [f32; 3], joint: [f32; 3]) -> Vec<u8> {
    let mut pixels = Vec::with_capacity((SIZE * SIZE * 4) as usize);

    for y in 0..SIZE {
        for x in 0..SIZE {
            let (u, v) = (x as f32 / SIZE as f32, y as f32 / SIZE as f32);

            // The stone itself: two octaves of tiling noise between two greys.
            let value =
                tiling_noise(seed, u, v, 8) * 0.6 + tiling_noise(seed ^ 0x2C1F, u, v, 32) * 0.4;
            let t = (value * 0.5 + 0.5).clamp(0.0, 1.0);

            // How close this pixel is to a joint, in tile-local coordinates. `min` of the distance to
            // each nearer edge, so a corner is dark from both directions at once.
            let slab = |coordinate: f32| {
                let within = (coordinate * SLABS as f32).fract();
                within.min(1.0 - within)
            };
            let edge = slab(u).min(slab(v));
            // A joint about a fortieth of a slab wide, with a short ramp so it is not aliased into a
            // hard stair at a grazing angle.
            let mortar = (1.0 - (edge / 0.025).clamp(0.0, 1.0)).clamp(0.0, 1.0);

            for channel in 0..3 {
                let stone = low[channel] + (high[channel] - low[channel]) * t;
                let colour = stone + (joint[channel] - stone) * mortar;
                // sRGB encode, because the texture is uploaded as `Rgba8UnormSrgb` and the GPU
                // decodes it back to linear when sampling. Writing linear here comes out too dark.
                pixels.push(to_srgb_byte(colour));
            }
            pixels.push(255);
        }
    }

    pixels
}

/// Periodic gradient noise over the unit square, wrapping after `lattice` cells.
///
/// The same routine `games/scarp`'s `turf` uses and for the same reason: a texture has to **tile**,
/// so the noise has to be periodic, and `amadeo_noise` is deliberately not — it is a function over
/// the whole plane, which is what a world wants and what a tile does not. Corner coordinates are
/// taken modulo `lattice`, so the left and right edges read the same corners.
///
/// Two copies of thirty lines in two games is the moment before promotion rather than after: a third
/// user is when this should move into the engine.
fn tiling_noise(seed: u64, u: f32, v: f32, lattice: i32) -> f32 {
    let (x, y) = (u * lattice as f32, v * lattice as f32);
    let (x0, y0) = (x.floor() as i32, y.floor() as i32);
    let (fx, fy) = (x - x0 as f32, y - y0 as f32);

    // Perlin's fade: smooth in the first and second derivative, so the lattice does not show.
    let fade = |t: f32| t * t * t * (t * (t * 6.0 - 15.0) + 10.0);
    let (ux, uy) = (fade(fx), fade(fy));

    let corner = |ix: i32, iy: i32| {
        let gradient = gradient_at(seed, ix.rem_euclid(lattice), iy.rem_euclid(lattice));
        gradient[0] * (fx - (ix - x0) as f32) + gradient[1] * (fy - (iy - y0) as f32)
    };

    let lerp = |a: f32, b: f32, t: f32| a + t * (b - a);
    let bottom = lerp(corner(x0, y0), corner(x0 + 1, y0), ux);
    let top = lerp(corner(x0, y0 + 1), corner(x0 + 1, y0 + 1), ux);
    lerp(bottom, top, uy) * std::f32::consts::SQRT_2
}

/// One of eight fixed gradients, chosen by hashing a lattice corner.
fn gradient_at(seed: u64, x: i32, y: i32) -> [f32; 2] {
    const GRADIENTS: [[f32; 2]; 8] = [
        [1.0, 0.0],
        [-1.0, 0.0],
        [0.0, 1.0],
        [0.0, -1.0],
        [0.707, 0.707],
        [-0.707, 0.707],
        [0.707, -0.707],
        [-0.707, -0.707],
    ];

    // FNV-1a over the two coordinates and the seed. Integer arithmetic throughout, so it is identical
    // on every machine — the same rule `amadeo-noise` follows and for the same reason.
    let mut hash = 0xcbf2_9ce4_8422_2325_u64 ^ seed;
    for byte in x.to_le_bytes().iter().chain(y.to_le_bytes().iter()) {
        hash ^= u64::from(*byte);
        hash = hash.wrapping_mul(0x100_0000_01b3);
    }
    GRADIENTS[(hash % 8) as usize]
}

fn write(path: &Path, id: &str, pixels: &[u8]) {
    let image = amadeo_image::TextureData {
        width: SIZE,
        height: SIZE,
        format: amadeo_image::PixelFormat::Rgba8UnormSrgb,
        pixels: pixels.to_vec(),
    };
    let encoded = match amadeo_image::encode_png(&image) {
        Ok(bytes) => bytes,
        Err(error) => {
            eprintln!("could not encode {}: {error}", path.display());
            std::process::exit(1);
        }
    };
    if let Err(error) = std::fs::write(path, &encoded) {
        eprintln!("could not write {}: {error}", path.display());
        std::process::exit(1);
    }
    let sidecar = path.with_extension("png.ama-meta");
    if let Err(error) = std::fs::write(&sidecar, format!("id = \"{id}\"\n")) {
        eprintln!("could not write {}: {error}", sidecar.display());
        std::process::exit(1);
    }
    println!("wrote {} ({} bytes)", path.display(), encoded.len());
}

fn manifest_dir() -> PathBuf {
    PathBuf::from(env!("CARGO_MANIFEST_DIR"))
}

fn to_srgb_byte(linear: f32) -> u8 {
    let clamped = linear.clamp(0.0, 1.0);
    // `powf` is fine *here* and forbidden in a `TerrainSource` (ADR 0044): this runs once, offline,
    // and its output is a committed PNG every machine then reads identically.
    let encoded = if clamped <= 0.003_130_8 {
        clamped * 12.92
    } else {
        1.055 * clamped.powf(1.0 / 2.4) - 0.055
    };
    (encoded * 255.0).round().clamp(0.0, 255.0) as u8
}
