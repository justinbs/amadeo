//! `turf` — generates the Scarp's ground textures as PNGs.
//!
//! ```text
//! cargo run -p scarp --bin turf
//! ```
//!
//! # Why generated rather than authored
//!
//! Same reasoning as `games/vault`'s `pix`, arriving at a different answer for a different kind of
//! image. Invariant I1 wants everything authorable and diffable, and a PNG is neither — so the
//! *source* has to be text. For the Vault the source is a hand-drawn grid of characters, because a
//! sprite is drawn. Ground is not drawn; it is **a formula**, and this file is that formula.
//!
//! Idempotent: running it twice writes byte-identical files, so it can be re-run freely and a diff
//! shows only what actually changed.
//!
//! # Why the noise here does not come from `amadeo-noise`
//!
//! A texture has to **tile**, and tiling means periodic — the value at `x` and at `x + size` must be
//! the same bits, or the seam shows as a visible grid across the whole landscape. `amadeo_noise`
//! is deliberately not periodic: it is a function over the whole plane, which is what a world wants
//! and what a tile does not.
//!
//! So this uses the same *idea* — a hash per lattice corner, Perlin's fade curve between them — with
//! the lattice coordinates taken **modulo the lattice count**, which is what makes it wrap. It is
//! about thirty lines and it lives here rather than in the engine because nothing else has wanted it
//! yet. If a second game does, that is the moment to promote it.

use std::path::{Path, PathBuf};

/// Texture size in pixels. A power of two, so it is friendly to mipmapping when that exists.
const SIZE: u32 = 256;

fn main() {
    let out = manifest_dir().join("assets/textures");
    if let Err(error) = std::fs::create_dir_all(&out) {
        eprintln!("could not create {}: {error}", out.display());
        std::process::exit(1);
    }

    // Grass: green, with enough variation to break up a flat colour at distance.
    write(
        &out.join("turf_grass.png"),
        "turf_grass",
        &render(0x6712_ABCD, [0.30, 0.40, 0.20], [0.46, 0.56, 0.30]),
    );
    // Bare rock, for the slopes a future triplanar blend will want.
    write(
        &out.join("turf_rock.png"),
        "turf_rock",
        &render(0x1D3A_7F02, [0.30, 0.29, 0.27], [0.52, 0.50, 0.47]),
    );
}

/// Renders one texture: periodic noise at two scales, blended between two colours.
fn render(seed: u64, low: [f32; 3], high: [f32; 3]) -> Vec<u8> {
    let mut pixels = Vec::with_capacity((SIZE * SIZE * 4) as usize);
    for y in 0..SIZE {
        for x in 0..SIZE {
            let (u, v) = (x as f32 / SIZE as f32, y as f32 / SIZE as f32);
            // Two octaves, both periodic. The coarse one gives patches, the fine one gives grain.
            let value =
                tiling_noise(seed, u, v, 4) * 0.65 + tiling_noise(seed ^ 0x5BF0, u, v, 16) * 0.35;
            // Noise is -1..1; the colour blend wants 0..1.
            let t = (value * 0.5 + 0.5).clamp(0.0, 1.0);

            for channel in 0..3 {
                let colour = low[channel] + (high[channel] - low[channel]) * t;
                // sRGB encode, because the texture is uploaded as `Rgba8UnormSrgb` and the GPU will
                // decode it back to linear when sampling. Writing linear values here would make
                // everything come out visibly too dark.
                pixels.push(to_srgb_byte(colour));
            }
            pixels.push(255);
        }
    }
    pixels
}

/// Periodic gradient noise over the unit square, wrapping after `lattice` cells.
///
/// `u` and `v` run 0..1 across the texture. Corner coordinates are taken modulo `lattice`, so the
/// left edge and the right edge read the same corners and the texture tiles exactly.
fn tiling_noise(seed: u64, u: f32, v: f32, lattice: i32) -> f32 {
    let (x, y) = (u * lattice as f32, v * lattice as f32);
    let (x0, y0) = (x.floor() as i32, y.floor() as i32);
    let (fx, fy) = (x - x0 as f32, y - y0 as f32);

    // Perlin's fade: smooth in the first and second derivative, so the lattice does not show.
    let fade = |t: f32| t * t * t * (t * (t * 6.0 - 15.0) + 10.0);
    let (ux, uy) = (fade(fx), fade(fy));

    let corner = |ix: i32, iy: i32| {
        // The wrap. `rem_euclid` rather than `%` so a negative coordinate lands in range.
        let gradient = gradient_at(seed, ix.rem_euclid(lattice), iy.rem_euclid(lattice));
        gradient[0] * (fx - (ix - x0) as f32) + gradient[1] * (fy - (iy - y0) as f32)
    };

    let lerp = |a: f32, b: f32, t: f32| a + t * (b - a);
    let bottom = lerp(corner(x0, y0), corner(x0 + 1, y0), ux);
    let top = lerp(corner(x0, y0 + 1), corner(x0 + 1, y0 + 1), ux);
    // Scaled by √2 so the result fills -1..1, exactly as `amadeo_noise` does.
    lerp(bottom, top, uy) * std::f32::consts::SQRT_2
}

/// One of eight fixed gradients, chosen by hashing a lattice corner.
fn gradient_at(seed: u64, x: i32, y: i32) -> [f32; 2] {
    const GRADIENTS: [[f32; 2]; 8] = [
        [1.0, 0.0],
        [-1.0, 0.0],
        [0.0, 1.0],
        [0.0, -1.0],
        [1.0, 1.0],
        [-1.0, 1.0],
        [1.0, -1.0],
        [-1.0, -1.0],
    ];

    let mut value = seed;
    value = value.wrapping_add((i64::from(x) as u64).wrapping_mul(0x9E37_79B9_7F4A_7C15));
    value = value.wrapping_add((i64::from(y) as u64).wrapping_mul(0xBF58_476D_1CE4_E5B9));
    value ^= value >> 33;
    value = value.wrapping_mul(0xFF51_AFD7_ED55_8CCD);
    value ^= value >> 33;
    GRADIENTS[(value % 8) as usize]
}

/// Linear 0..1 to an sRGB-encoded byte.
fn to_srgb_byte(linear: f32) -> u8 {
    let clamped = linear.clamp(0.0, 1.0);
    // The standard sRGB transfer curve. `powf` is fine *here* and forbidden in a `TerrainSource`
    // (ADR 0044): this runs once, offline, and its output is a committed PNG that every machine then
    // reads identically. Nothing about a running simulation depends on it.
    let encoded = if clamped <= 0.003_130_8 {
        clamped * 12.92
    } else {
        1.055 * clamped.powf(1.0 / 2.4) - 0.055
    };
    (encoded * 255.0).round().clamp(0.0, 255.0) as u8
}

/// Writes the PNG and its sidecar.
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
