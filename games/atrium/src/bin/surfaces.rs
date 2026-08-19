//! `surfaces` — generates the Atrium's surface textures as PNGs.
//!
//! ```text
//! cargo run -p atrium --bin surfaces
//! ```
//!
//! # Why generated rather than authored
//!
//! `games/vault`'s `pix`, `games/scarp`'s `turf` and `games/atrium`'s own `tone` all make the same
//! argument: invariant I1 wants everything authorable and diffable, a PNG is neither, so the *source*
//! has to be text. A sprite's source is a drawn grid of characters; ground and stone are formulas,
//! and this file is one.
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
//!
//! # Three maps per stone, from one height field
//!
//! Base colour, a tangent-space normal map (ADR 0047) and a metallic-roughness map (ADR 0048). Both
//! of those paths had been written, tested and exercised by **zero content** since session 14, which
//! is two thirds of session 20's original finding about this engine.
//!
//! All three are read off one [`height`] function. The way a material set goes wrong is by drifting
//! apart — a normal map whose bumps do not line up with the colour's is worse than no normal map at
//! all, because the eye is given two conflicting surfaces at once.

use std::path::{Path, PathBuf};

/// Texture size in pixels. A power of two, so mip generation halves cleanly.
const SIZE: u32 = 512;

/// How many slabs across one tile of the image.
///
/// **Four, not two.** Two meant a 20 m wall showed the same pair of slabs five times over, which is
/// the machine-grid read [`Stone::slab_variation`] exists to break — and it cannot break it when
/// there are only four slabs in the whole image to vary. Sixteen per tile, at twice the resolution,
/// keeps the joints crisp and doubles the texels per metre at the same `uv_scale`.
const SLABS: u32 = 4;

/// Whether an image holds colour or data, which decides its sidecar.
#[derive(Clone, Copy, PartialEq, Eq)]
enum ColourSpace {
    /// Colour, read through the sRGB curve.
    Srgb,
    /// Data — directions, roughness — read as written.
    Linear,
}

/// One stone: the numbers that make a slab pattern read as a particular material.
struct Stone {
    /// Asset id stem. `_normal` and `_surface` are appended for the other two maps.
    id: &'static str,
    seed: u64,
    /// Linear RGB at the darkest and lightest of the grain.
    low: [f32; 3],
    /// The lighter end of the same.
    high: [f32; 3],
    /// Linear RGB of the joint between slabs.
    joint: [f32; 3],
    /// How much one slab may differ in tone from its neighbour, as a fraction.
    ///
    /// **This is what stops a floor reading as a machine grid.** Real paving is cut from different
    /// blocks and no two match. Nothing else in the pattern varies at slab scale, so without it the
    /// image is one slab repeated and looks it.
    slab_variation: f32,
    /// Roughness of a slab face.
    face_roughness: f32,
    /// Roughness of a joint, which is always higher — mortar is not polished.
    joint_roughness: f32,
}

fn main() {
    let out = manifest_dir().join("assets/textures");
    if let Err(error) = std::fs::create_dir_all(&out) {
        eprintln!("could not create {}: {error}", out.display());
        std::process::exit(1);
    }

    let stones = [
        // The floor and the plinth share this at different `uv_scale`s, which is precisely the
        // demonstration: the same slab size on a 20 m floor and a 3 m plinth.
        Stone {
            id: "stone_slab",
            seed: 0x51A9_3C7E,
            low: [0.52, 0.51, 0.49],
            high: [0.63, 0.62, 0.59],
            joint: [0.34, 0.33, 0.32],
            slab_variation: 0.10,
            face_roughness: 0.72,
            joint_roughness: 0.95,
        },
        // The walls. **The largest surface in the game by a wide margin** — a capture facing a bare
        // wall is most of a frame — so it is the one that most needs to not be a flat colour.
        Stone {
            id: "slate_course",
            seed: 0x7C3E_1B45,
            low: [0.26, 0.27, 0.30],
            high: [0.34, 0.35, 0.38],
            joint: [0.16, 0.17, 0.19],
            slab_variation: 0.16,
            face_roughness: 0.78,
            joint_roughness: 0.96,
        },
    ];

    for stone in &stones {
        write(
            &out.join(format!("{}.png", stone.id)),
            stone.id,
            &render_colour(stone),
            ColourSpace::Srgb,
        );
        write(
            &out.join(format!("{}_normal.png", stone.id)),
            &format!("{}_normal", stone.id),
            &render_normal(stone),
            ColourSpace::Linear,
        );
        write(
            &out.join(format!("{}_surface.png", stone.id)),
            &format!("{}_surface", stone.id),
            &render_surface(stone),
            ColourSpace::Linear,
        );
    }
}

/// A stable per-slab number in −1..1.
///
/// Hashed from the slab's integer coordinates rather than taken from noise, so it is **flat across a
/// slab and discontinuous at the joint** — which is what "these are separate blocks of stone" looks
/// like. Noise cannot produce that: it is continuous by construction.
fn slab_tone(seed: u64, u: f32, v: f32) -> f32 {
    let sx = (u * SLABS as f32).floor() as i64;
    let sy = (v * SLABS as f32).floor() as i64;

    // **This function used to deliver a hundredth of what it was asked for**, and the way it failed
    // is worth keeping because nothing but a measurement could have found it. It fed
    // `sx.to_le_bytes()` and `sy.to_le_bytes()` through FNV-1a — and with two slabs per axis those
    // coordinates are only ever 0 or 1, so **six of the eight bytes were constant zero** and the two
    // that moved differed in one bit. FNV-1a avalanches well over many bytes and barely at all over
    // one low bit, so the four slabs came out within **0.2 of a byte on 190**: an authored 10%
    // arriving as 0.1%, below the threshold of perception, in code that ran, was tested, and changed
    // no pixel.
    //
    // splitmix64's finalizer instead. It is a bijection built to avalanche a *counter* — the exact
    // case here, where the inputs are small consecutive integers — and every output bit depends on
    // every input bit after the first shift-multiply.
    let mut hash = (sx as u64).wrapping_mul(0x9E37_79B9_7F4A_7C15)
        ^ (sy as u64).wrapping_mul(0xBF58_476D_1CE4_E5B9)
        ^ seed;
    hash ^= hash >> 30;
    hash = hash.wrapping_mul(0xBF58_476D_1CE4_E5B9);
    hash ^= hash >> 27;
    hash = hash.wrapping_mul(0x94D0_49BB_1331_11EB);
    hash ^= hash >> 31;

    ((hash >> 40) & 0xFFFF) as f32 / 65535.0 * 2.0 - 1.0
}

/// How deep in a joint a point is: 0 on a slab face, 1 in the middle of a joint.
fn mortar(u: f32, v: f32) -> f32 {
    let edge_of = |coordinate: f32| {
        let within = (coordinate * SLABS as f32).fract();
        within.min(1.0 - within)
    };
    let edge = edge_of(u).min(edge_of(v));
    // A joint about a fortieth of a slab wide, with a short ramp so it is not aliased into a hard
    // stair at a grazing angle.
    (1.0 - (edge / 0.025).clamp(0.0, 1.0)).clamp(0.0, 1.0)
}

/// The stone's fine grain, roughly −1..1.
///
/// **Three octaves at co-prime lattices, not two at 8 and 32.** Two octaves an exact factor of four
/// apart put their features on the same grid, and with only eight gradient directions — four of them
/// diagonal — the result reads as a regular 45° cross-hatch. That is a woven or anti-slip pattern
/// rather than stone, and it was most of why this material read as bathroom tile. Co-prime lattices
/// do not line up, so the pattern has no period shorter than the tile.
fn grain(seed: u64, u: f32, v: f32) -> f32 {
    tiling_noise(seed, u, v, 7) * 0.5
        + tiling_noise(seed ^ 0x2C1F, u, v, 17) * 0.32
        + tiling_noise(seed ^ 0x91B3, u, v, 43) * 0.18
}

/// **The single height field all three maps are read off.**
///
/// Units are arbitrary; only differences matter, since the normal map is its gradient.
fn height(stone: &Stone, u: f32, v: f32) -> f32 {
    // The joint is cut *into* the stone and is by far the largest feature — a couple of millimetres
    // against a fraction of one for the grain.
    grain(stone.seed, u, v) * 0.035 - mortar(u, v)
}

/// The base colour map, in sRGB.
fn render_colour(stone: &Stone) -> Vec<u8> {
    let mut pixels = Vec::with_capacity((SIZE * SIZE * 4) as usize);

    for y in 0..SIZE {
        for x in 0..SIZE {
            let (u, v) = (x as f32 / SIZE as f32, y as f32 / SIZE as f32);

            let t = (grain(stone.seed, u, v) * 0.5 + 0.5).clamp(0.0, 1.0);
            let tone = slab_tone(stone.seed, u, v) * stone.slab_variation;
            let joint = mortar(u, v);

            for channel in 0..3 {
                let base = stone.low[channel] + (stone.high[channel] - stone.low[channel]) * t;
                // Per-slab tone lifts or drops the whole block. Applied *before* the joint, so a
                // joint stays a joint rather than picking up its slab's tint.
                let varied = (base * (1.0 + tone)).clamp(0.0, 1.0);
                let colour = varied + (stone.joint[channel] - varied) * joint;
                // sRGB encode, because the texture is uploaded as `Rgba8UnormSrgb` and the GPU
                // decodes it back to linear when sampling. Writing linear here comes out too dark.
                pixels.push(to_srgb_byte(colour));
            }
            pixels.push(255);
        }
    }

    pixels
}

/// The tangent-space normal map, in **linear** bytes.
///
/// A central difference of [`height`] in each direction gives the surface gradient, and the
/// tangent-space normal is `(-dh/du, -dh/dv, 1)` normalised.
///
/// **Sampling the height function rather than differencing the colour image matters.** Colour carries
/// per-slab tone and the joint's darkening, neither of which is a shape — a normal map derived from
/// it would emboss the tone variation, so every slab would read as a different height.
fn render_normal(stone: &Stone) -> Vec<u8> {
    let mut pixels = Vec::with_capacity((SIZE * SIZE * 4) as usize);
    let step = 1.0 / SIZE as f32;
    // How pronounced the relief is. Tuned so the joints read as cut rather than as canyons.
    const RELIEF: f32 = 0.156;

    for y in 0..SIZE {
        for x in 0..SIZE {
            let (u, v) = (x as f32 / SIZE as f32, y as f32 / SIZE as f32);

            // Wrapped, so the map tiles as seamlessly as the height it is taken from. `rem_euclid`
            // rather than `%` because a negative coordinate must wrap to the far edge, not to itself.
            let at = |du: f32, dv: f32| {
                height(stone, (u + du).rem_euclid(1.0), (v + dv).rem_euclid(1.0))
            };
            let dhdu = (at(step, 0.0) - at(-step, 0.0)) * 0.5;
            let dhdv = (at(0.0, step) - at(0.0, -step)) * 0.5;

            let normal = normalise([-dhdu / RELIEF, -dhdv / RELIEF, 1.0]);
            for component in normal {
                // Straight to a byte with **no sRGB curve**: these are directions. Q31's trap is that
                // nothing inside a PNG says which it is — the sidecar's `color_space = "linear"` is
                // what does, and forgetting it bends every normal in the map with no visible error.
                pixels.push(to_unit_byte(component * 0.5 + 0.5));
            }
            pixels.push(255);
        }
    }

    pixels
}

/// The occlusion-metallic-roughness map, in **linear** bytes.
///
/// glTF 2.0's packing, which the engine follows rather than invents: **red is occlusion**, green is
/// roughness, blue is metallic.
///
/// Stone is a dielectric, so metallic is zero everywhere. The other two both come off the same
/// height field as the normal map: joints are rougher than faces, and joints are also *deeper*, so
/// they see less of the sky.
///
/// # Red was written as zero and read by nothing, which is worse than either
///
/// It was documented here as unused, and the shader did discard it — but nothing in that arrangement
/// was safe. `mesh.wgsl` reading `packed.r` as occlusion against a map full of zeroes would have made
/// every stone surface in the game pitch black in ambient light, and the only thing standing between
/// those two facts was that neither file had changed yet. ADR 0083 read the channel; this fills it,
/// in the same commit, because either alone is a defect.
fn render_surface(stone: &Stone) -> Vec<u8> {
    let mut pixels = Vec::with_capacity((SIZE * SIZE * 4) as usize);

    for y in 0..SIZE {
        for x in 0..SIZE {
            let (u, v) = (x as f32 / SIZE as f32, y as f32 / SIZE as f32);

            let joint = mortar(u, v);
            let jitter = grain(stone.seed ^ 0x5B7D, u, v) * 0.06;
            let roughness = stone.face_roughness
                + (stone.joint_roughness - stone.face_roughness) * joint
                + jitter;

            pixels.push(to_unit_byte(cavity(stone, u, v)));
            pixels.push(to_unit_byte(roughness));
            // Metallic. Zero, and it is a statement rather than a placeholder: stone is a dielectric,
            // and a map saying otherwise would make the floor reflect like a mirror and lose its
            // colour entirely, which is what `metallic` means (ADR 0048).
            pixels.push(0);
            pixels.push(255);
        }
    }

    pixels
}

/// How much of the sky a point can see, 0 fully enclosed and 1 fully open — the occlusion channel.
///
/// # Why this is not just `1 - mortar(u, v)`
///
/// Occlusion is not depth. A point at the *bottom* of a wide shallow scoop is barely occluded, and a
/// point in a narrow crack is heavily occluded at the same depth — what matters is how much higher
/// the surroundings are, not how low this point is. So this compares the height here against the
/// average height of a ring around it, which is the cheap standard approximation and the one every
/// bake tool reduces to when the ray budget runs out.
///
/// Sampling [`height`] rather than differencing an image keeps all three maps read off one function,
/// which is the property this file exists to protect: colour, relief and occlusion that disagree give
/// the eye three conflicting surfaces at once.
fn cavity(stone: &Stone, u: f32, v: f32) -> f32 {
    // A ring wide enough to span a joint and narrow enough to stay inside one slab. A radius smaller
    // than the joint reads only the joint floor and returns "open" in the middle of it; one larger
    // than a slab drags a whole neighbouring block into the average and darkens the faces.
    const RADIUS: f32 = 0.012;
    // How dark a point gets when the ring around it averages one full joint-depth above it.
    //
    // **Measured rather than reasoned, and the reasoning was wrong.** I estimated a joint floor
    // would average 0.37 of a joint-depth below its ring, on the grounds that the ring straddles the
    // joint; it averages **0.75**, because the joint is only about 0.0125 wide in uv and a 0.012
    // radius puts most of the ring out on the faces at full height rather than on the joint's own
    // ramp. At the first value that took the joint to 25/255 -- a near-black line rather than a
    // shadow -- so this is the value that puts it at about 140, which is what a joint in raking light
    // actually looks like. Probe it with `amadeo image row <surface>.png 64 118 140`.
    const STRENGTH: f32 = 0.6;

    let here = height(stone, u, v);

    // Eight neighbours on a ring: four square, four diagonal at the same radius rather than at
    // sqrt(2) times it, so no direction is weighted more heavily than another.
    const DIAGONAL: f32 = std::f32::consts::FRAC_1_SQRT_2;
    let offsets: [(f32, f32); 8] = [
        (1.0, 0.0),
        (-1.0, 0.0),
        (0.0, 1.0),
        (0.0, -1.0),
        (DIAGONAL, DIAGONAL),
        (DIAGONAL, -DIAGONAL),
        (-DIAGONAL, DIAGONAL),
        (-DIAGONAL, -DIAGONAL),
    ];

    let mut higher = 0.0;
    for (du, dv) in offsets {
        let around = height(
            stone,
            (u + du * RADIUS).rem_euclid(1.0),
            (v + dv * RADIUS).rem_euclid(1.0),
        );
        // Only neighbours **above** this point block anything. One below is a drop, and a drop
        // occludes nothing -- without this clamp a slab face beside a joint would be darkened by the
        // joint next to it, which is backwards.
        higher += (around - here).max(0.0);
    }

    (1.0 - (higher / offsets.len() as f32) * STRENGTH).clamp(0.0, 1.0)
}

/// A 0..1 value as a byte, with no transfer curve. For data maps.
fn to_unit_byte(value: f32) -> u8 {
    (value.clamp(0.0, 1.0) * 255.0).round() as u8
}

fn normalise(v: [f32; 3]) -> [f32; 3] {
    let length = (v[0] * v[0] + v[1] * v[1] + v[2] * v[2]).sqrt();
    if length <= f32::EPSILON {
        return [0.0, 0.0, 1.0];
    }
    [v[0] / length, v[1] / length, v[2] / length]
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
    // The 45 degree ones, named so clippy does not read them as a fumbled `FRAC_1_SQRT_2` -- which is
    // exactly what they are.
    const DIAGONAL: f32 = std::f32::consts::FRAC_1_SQRT_2;

    // **Sixteen directions, not eight.** Eight means every gradient is axis-aligned or exactly
    // diagonal, and a field built from those has visible structure along those four lines — which,
    // combined with two octaves an exact factor apart, is what made the first grain read as a woven
    // cross-hatch. The odd multiples of 22.5° break that up for the cost of eight more constants.
    const GRADIENTS: [[f32; 2]; 16] = [
        [1.0, 0.0],
        [-1.0, 0.0],
        [0.0, 1.0],
        [0.0, -1.0],
        [DIAGONAL, DIAGONAL],
        [-DIAGONAL, DIAGONAL],
        [DIAGONAL, -DIAGONAL],
        [-DIAGONAL, -DIAGONAL],
        [0.923_879_5, 0.382_683_4],
        [-0.923_879_5, 0.382_683_4],
        [0.923_879_5, -0.382_683_4],
        [-0.923_879_5, -0.382_683_4],
        [0.382_683_4, 0.923_879_5],
        [-0.382_683_4, 0.923_879_5],
        [0.382_683_4, -0.923_879_5],
        [-0.382_683_4, -0.923_879_5],
    ];

    // FNV-1a over the two coordinates and the seed. Integer arithmetic throughout, so it is identical
    // on every machine — the same rule `amadeo-noise` follows and for the same reason.
    let mut hash = 0xcbf2_9ce4_8422_2325_u64 ^ seed;
    for byte in x.to_le_bytes().iter().chain(y.to_le_bytes().iter()) {
        hash ^= u64::from(*byte);
        hash = hash.wrapping_mul(0x100_0000_01b3);
    }
    GRADIENTS[(hash % 16) as usize]
}

fn write(path: &Path, id: &str, pixels: &[u8], space: ColourSpace) {
    let image = amadeo_image::TextureData {
        width: SIZE,
        height: SIZE,
        // The *file* is written the same way either way — a PNG has no colour-space field this
        // engine reads. What decides how the bytes are interpreted is the sidecar below, which is
        // exactly why Q31 calls this silent.
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
    let mut settings = format!("id = \"{id}\"\n");
    if space == ColourSpace::Linear {
        settings.push_str("color_space = \"linear\"\n");
    }
    let sidecar = path.with_extension("png.ama-meta");
    if let Err(error) = std::fs::write(&sidecar, settings) {
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
