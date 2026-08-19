//! `surfaces` — generates the Atrium's stone textures as PNGs.
//!
//! ```text
//! cargo run -p atrium --bin surfaces
//! ```
//!
//! # Why generated rather than authored
//!
//! `games/vault`'s `pix`, `games/scarp`'s `turf` and `games/atrium`'s own `tone` all make the same
//! argument: invariant I1 wants everything authorable and diffable, a PNG is neither, so the *source*
//! has to be text. A sprite's source is a drawn grid of characters; stone is a formula, and this file
//! is one.
//!
//! It is also `docs/12-the-bar.md` §3's requirement in practice — **Claude can author this game's
//! textures rather than asking Justin for them** — which is the half of the bar most likely to be
//! quietly dodged.
//!
//! Idempotent: running it twice writes byte-identical files.
//!
//! # What is here and what is in the engine
//!
//! Almost none of the machinery is here any more. `amadeo-texture` holds the tiling noise, the
//! masonry lattice, the height-to-normal difference, the cavity estimate and the sRGB encoder, all
//! with their own tests — engine gate item 13, which required the generator to be *engine code*
//! rather than a binary in one game.
//!
//! What stays here is the part that is genre knowledge (invariant I4): **which numbers make a
//! particular stone**. Two of them, a pale paving and a dark slate, and the difference between them
//! is a palette and a course size rather than any code.
//!
//! # Three maps per stone, from one height field
//!
//! Base colour, a tangent-space normal map (ADR 0047) and a packed occlusion-roughness-metallic map
//! (ADR 0048, ADR 0083). All three are read off one [`height`] function, because the way a material
//! set goes wrong is by drifting apart — a normal map whose bumps do not line up with the colour's
//! gives the eye two conflicting surfaces at once.

use amadeo_texture::{Bond, Canvas, Courses, Space, Wall, maps, noise};
use std::path::{Path, PathBuf};

/// Texture size in pixels. A power of two, so mip generation halves cleanly.
const SIZE: u32 = 512;

/// One stone: the numbers that make a course of masonry read as a particular material.
struct Stone {
    /// Asset id stem. `_normal` and `_surface` are appended for the other two maps.
    id: &'static str,
    /// How the wall is cut.
    courses: Courses,
    /// Linear RGB at the darkest and lightest of the grain.
    low: [f32; 3],
    /// The lighter end of the same.
    high: [f32; 3],
    /// Linear RGB of the mortar between stones.
    joint: [f32; 3],
    /// How much one stone may differ in tone from its neighbour, as a fraction.
    ///
    /// **This is what stops a wall reading as one block repeated.** Real masonry is cut from
    /// different stone and no two pieces match.
    tone_variation: f32,
    /// Roughness of a stone's face.
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
        // demonstration ADR 0078 exists for: the same stone size on a 20 m floor and a 3 m plinth.
        Stone {
            id: "stone_slab",
            courses: Courses {
                seed: 0x51A9_3C7E,
                rows: 5,
                across: 4,
                variation: 0.34,
                joint: 0.007,
                bond: Bond::Broken,
            },
            low: [0.52, 0.51, 0.49],
            high: [0.63, 0.62, 0.59],
            joint: [0.34, 0.33, 0.32],
            tone_variation: 0.11,
            face_roughness: 0.72,
            joint_roughness: 0.95,
        },
        // The walls. **The largest surface in the game by a wide margin** — a capture facing a bare
        // wall is most of a frame — so it is the one that most needs to not read as a grid. Taller,
        // narrower courses than the paving, which is what separates a wall from a floor at a glance.
        Stone {
            id: "slate_course",
            courses: Courses {
                seed: 0x7C3E_1B45,
                rows: 7,
                across: 3,
                variation: 0.40,
                joint: 0.006,
                bond: Bond::Broken,
            },
            low: [0.26, 0.27, 0.30],
            high: [0.34, 0.35, 0.38],
            joint: [0.16, 0.17, 0.19],
            tone_variation: 0.17,
            face_roughness: 0.78,
            joint_roughness: 0.96,
        },
    ];

    for stone in &stones {
        let wall = stone.courses.lay();
        write(
            &out.join(format!("{}.png", stone.id)),
            stone.id,
            render_colour(stone, &wall),
            Space::Srgb,
        );
        write(
            &out.join(format!("{}_normal.png", stone.id)),
            &format!("{}_normal", stone.id),
            render_normal(stone, &wall),
            Space::Linear,
        );
        write(
            &out.join(format!("{}_surface.png", stone.id)),
            &format!("{}_surface", stone.id),
            render_surface(stone, &wall),
            Space::Linear,
        );
    }
}

/// The stone's fine grain, roughly −1..1.
///
/// **Three octaves at co-prime lattices, not two at 8 and 32.** Two octaves an exact factor of four
/// apart put their features on the same grid, and the result reads as a regular 45° cross-hatch —
/// a woven or anti-slip pattern rather than stone. `amadeo-texture`'s own documentation records why.
fn grain(seed: u64, u: f32, v: f32) -> f32 {
    noise::octaves(seed, u, v, &[(7, 0.5), (17, 0.32), (43, 0.18)])
}

/// A slow drift across the whole tile, at a period that has nothing to do with the courses.
///
/// **The second half of not looking machine-made**, and the half a masonry lattice cannot supply.
/// Even with every stone a different size, a tile whose average brightness is uniform still repeats
/// visibly across a large wall — the eye finds the period. A low-frequency tint at a lattice that
/// does not divide the course grid puts light and dark regions *across* stones, which is what damp,
/// weathering and age actually do to a wall.
fn macro_tint(seed: u64, u: f32, v: f32) -> f32 {
    noise::tiling(seed ^ 0x4D_AC_C0, u, v, 3) * 0.5
        + noise::tiling(seed ^ 0x4D_AC_C1, u, v, 5) * 0.5
}

/// **The single height field all three maps are read off.**
///
/// Units are arbitrary; only differences matter, since the normal map is its gradient.
fn height(stone: &Stone, wall: &Wall, u: f32, v: f32) -> f32 {
    // The joint is cut *into* the stone and is by far the largest feature — a couple of millimetres
    // against a fraction of one for the grain.
    grain(stone.courses.seed, u, v) * 0.035 - wall.at(u, v).joint
}

/// The base colour map, in sRGB.
fn render_colour(stone: &Stone, wall: &Wall) -> Canvas {
    let mut canvas = Canvas::new(SIZE, Space::Srgb);
    canvas.fill(|u, v| {
        let piece = wall.at(u, v);
        let t = (grain(stone.courses.seed, u, v) * 0.5 + 0.5).clamp(0.0, 1.0);
        let tone = (wall.tone(piece, 0) - 0.5) * 2.0 * stone.tone_variation;
        let drift = macro_tint(stone.courses.seed, u, v) * 0.13;

        // **Grime collects in the joints and just above them**, which is where water runs and stops.
        // Concentrated near a joint rather than uniform, so it reads as accumulated dirt rather than
        // as a filter over the whole image.
        let grime = piece.joint * 0.35
            + (noise::tiling(stone.courses.seed ^ 0x6D_11, u, v, 11) * 0.5 + 0.5)
                * piece.joint
                * 0.25;

        let mut colour = [0.0; 4];
        for (channel, out) in colour.iter_mut().take(3).enumerate() {
            let base = stone.low[channel] + (stone.high[channel] - stone.low[channel]) * t;
            // Per-stone tone and the macro drift both scale the whole block. Applied *before* the
            // joint, so a joint stays a joint rather than picking up its stone's tint.
            let varied = (base * (1.0 + tone + drift)).clamp(0.0, 1.0);
            let jointed = varied + (stone.joint[channel] - varied) * piece.joint;
            *out = (jointed * (1.0 - grime)).clamp(0.0, 1.0);
        }
        colour[3] = 1.0;
        colour
    });
    canvas
}

/// The tangent-space normal map, in **linear** bytes.
///
/// **Sampling the height function rather than differencing the colour image matters.** Colour
/// carries per-stone tone, the macro drift and the grime, none of which is a shape — a normal map
/// derived from it would emboss all three, so every stone would read as a different height.
fn render_normal(stone: &Stone, wall: &Wall) -> Canvas {
    let mut canvas = Canvas::new(SIZE, Space::Linear);
    let step = 1.0 / f32::from(u16::try_from(SIZE).unwrap_or(512));
    // How pronounced the relief is, as height units per uv unit for a 45° lean. Larger is flatter.
    const RELIEF: f32 = 80.0;

    canvas.fill(|u, v| {
        let field = |x: f32, y: f32| height(stone, wall, x, y);
        maps::encode_normal(maps::normal_from_height(&field, u, v, step, RELIEF))
    });
    canvas
}

/// The occlusion-metallic-roughness map, in **linear** bytes.
///
/// glTF 2.0's packing, which the engine follows rather than invents: **red is occlusion**, green is
/// roughness, blue is metallic.
///
/// Stone is a dielectric, so metallic is zero everywhere. The other two come off the same height
/// field as the normal map: joints are rougher than faces, and joints are also *deeper*, so they see
/// less of the sky.
fn render_surface(stone: &Stone, wall: &Wall) -> Canvas {
    let mut canvas = Canvas::new(SIZE, Space::Linear);
    // A ring wide enough to span a joint and narrow enough to stay inside one stone. Smaller than
    // the joint reads only the joint floor and returns "open" in the middle of it; larger than a
    // stone drags a whole neighbouring block into the average and darkens the faces.
    const RADIUS: f32 = 0.012;
    // How dark a point gets when the ring around it averages one full joint-depth above it.
    //
    // **Measured rather than reasoned, and the reasoning was wrong.** The first estimate for a joint
    // floor was 0.37 of a joint-depth below its ring; it is about 0.75, because the joint is narrow
    // and most of the ring lands out on the faces at full height. At the first value the joint came
    // out at 25 of 255 — a near-black line rather than a shadow. Probe it with
    // `amadeo image row <surface>.png 64 118 140`.
    const STRENGTH: f32 = 0.6;

    canvas.fill(|u, v| {
        let piece = wall.at(u, v);
        let field = |x: f32, y: f32| height(stone, wall, x, y);
        let occlusion = maps::cavity_from_height(&field, u, v, RADIUS, STRENGTH);

        let jitter = grain(stone.courses.seed ^ 0x5B7D, u, v) * 0.06;
        let roughness = stone.face_roughness
            + (stone.joint_roughness - stone.face_roughness) * piece.joint
            + jitter;

        // Metallic zero, and it is a statement rather than a placeholder: stone is a dielectric, and
        // a map saying otherwise would make the floor reflect like a mirror and lose its colour
        // entirely, which is what `metallic` means (ADR 0048).
        maps::pack_orm(occlusion, roughness, 0.0)
    });
    canvas
}

/// Writes one image and the sidecar that says what colour space it is in.
fn write(path: &Path, id: &str, canvas: Canvas, space: Space) {
    let bytes = match canvas.encode() {
        Ok(bytes) => bytes,
        Err(error) => {
            eprintln!("could not encode {}: {error}", path.display());
            std::process::exit(1);
        }
    };
    if let Err(error) = std::fs::write(path, &bytes) {
        eprintln!("could not write {}: {error}", path.display());
        std::process::exit(1);
    }

    // **Q31's trap**: nothing inside a PNG says whether its bytes are colour or data, so the sidecar
    // is the only thing that does — and a normal map read through the sRGB curve has every direction
    // bent, silently. `Space::sidecar` is what keeps the two statements together.
    let sidecar = path.with_extension("png.ama-meta");
    let contents = match space.sidecar() {
        Some(declared) => format!("id = \"{id}\"\ncolor_space = \"{declared}\"\n"),
        None => format!("id = \"{id}\"\n"),
    };
    if let Err(error) = std::fs::write(&sidecar, contents) {
        eprintln!("could not write {}: {error}", sidecar.display());
        std::process::exit(1);
    }

    println!("wrote {} ({} bytes)", path.display(), bytes.len());
}

/// This crate's directory, so the binary can be run from anywhere.
fn manifest_dir() -> PathBuf {
    PathBuf::from(env!("CARGO_MANIFEST_DIR"))
}
