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
//! [`SUN_ROTATION`] holds the same three angles `scenes/scarp.scene` gives its `DirectionalLight`,
//! and the direction is derived from them with the engine's own transform code — so the two cannot
//! disagree about what the angles *mean*, only about the angles themselves.
//!
//! That is a narrower gap than it was. The first version wrote the direction vector down by hand and
//! **got the sign of Y wrong**, putting the sun below the horizon: light travelled upward, the sky
//! had no sun in it, and nothing failed. Deriving it removed that whole class of mistake, and what
//! is left is three numbers a reader can compare at a glance.
//!
//! # An environment map must not contain a light the scene already has
//!
//! The Scarp's sun is a `DirectionalLight` *and* a bright disc in this sky. An environment is the
//! **indirect** half of lighting, so anything in it that is also a direct light gets counted twice.
//!
//! Irradiance weighs a direction by the solid angle it covers, which makes this easy to get very
//! wrong: a five-degree disc at 250× is half a percent of the sky and contributes more ambient light
//! than all the rest of it. That is what the first version did, and every surface in the demo blew
//! out to near-white — reading as an exposure problem rather than as double-counting. See
//! [`SUN_INTENSITY`].
//!
//! Idempotent: running it twice writes a byte-identical file.

use amadeo_image::{HdrImage, encode_hdr};
use std::path::{Path, PathBuf};

/// Width in pixels. Equirectangular, so height is half.
///
/// Modest on purpose: the prefiltering that consumes this immediately blurs it into a 16-pixel
/// irradiance cube and a 128-pixel specular chain, so detail beyond this is thrown away at load.
const WIDTH: u32 = 512;

/// The sun's rotation, copied from the `Transform` on `scenes/scarp.scene`'s sun.
///
/// **Three numbers that read identically in both files**, rather than a direction vector worked out
/// by hand — which is exactly how the first version of this got it wrong. It had the Y component
/// positive, meaning light travelling *upward*, which put the sun below the horizon and left the sky
/// with no sun in it at all. Nothing failed; the picture just quietly had no sun.
///
/// The direction itself is derived below by [`sun_direction`], using the engine's own transform
/// code, so the sky and the scene cannot disagree about what these angles *mean* — only about the
/// angles themselves, which are three numbers a reader can compare at a glance.
const SUN_ROTATION: [f32; 3] = [-42.0, 28.0, 0.0];

/// How wide the sun's disc is, as the cosine of its angular radius.
///
/// The real sun is about half a degree across. This is a little wider — a disc that small falls
/// between the samples of a 512-pixel map and disappears entirely, taking the visible sun with it.
const SUN_COS_RADIUS: f32 = 0.9995;

/// How bright the sun's disc is relative to the sky.
///
/// # This is the number that must not be large, and the reason is not obvious
///
/// **The scene already has a `DirectionalLight` for this same sun.** An environment map is the
/// *indirect* half of lighting — everything arriving from everywhere else — so anything in it that
/// is also modelled as a direct light gets counted twice, and the surface receives double the
/// sunlight.
///
/// It is easy to miss how strongly, because irradiance weighs a direction by the solid angle it
/// covers. The first version of this file used a five-degree disc at `250.0`: half a percent of the
/// sky, contributing *more ambient light than the entire rest of it*. Every surface in the Scarp
/// blew out to near-white and the cause read as an exposure problem rather than as double-counting.
///
/// So the disc is now small and modest — bright enough to read as a sun when you look at it, far too
/// small to matter to the convolution. The energy of the sunlight lives in the `DirectionalLight`,
/// where it can also cast a shadow.
const SUN_INTENSITY: f32 = 40.0;

/// Overall scale on the sky's colour, which is what it contributes as ambient light.
///
/// The sky is a genuinely strong light source — a real overcast sky lights a scene substantially —
/// but the Scarp's `DirectionalLight` intensity was tuned against the `0.12` constant that used to
/// stand in for ambient. Turning the sky up to physical brightness without retuning the sun would be
/// changing two things at once.
///
/// So this scales the sky to land near where the old constant did *on average*, while keeping what
/// the constant could never have: direction and colour. Surfaces facing up read cool, surfaces
/// facing the ground read warm, and shadows are filled by the sky rather than by a flat number.
const SKY_SCALE: f32 = 0.42;

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

/// The direction the sun's light travels, from [`SUN_ROTATION`].
///
/// Uses `Mat4::from_transform` — the same function the renderer uses to place the light — and takes
/// the negative Z axis out of it, because a `DirectionalLight` shines along its entity's own
/// negative Z (ADR 0018). Deriving it rather than writing the vector down is what stops this file
/// and the scene meaning different things by the same angles.
fn sun_direction() -> [f32; 3] {
    let matrix =
        amadeo_transform::Mat4::from_transform([0.0, 0.0, 0.0], SUN_ROTATION, [1.0, 1.0, 1.0]);
    // Column two is the entity's local +Z in world space; the light travels along its negative.
    normalise([
        -matrix.columns[2][0],
        -matrix.columns[2][1],
        -matrix.columns[2][2],
    ])
}

/// Builds the sky.
fn render() -> HdrImage {
    let height = WIDTH / 2;
    let mut pixels = Vec::with_capacity((WIDTH * height) as usize);

    let sun = sun_direction();

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
    //
    // **Faded into the horizon over the first stretch rather than starting at full strength.** A
    // hard edge there is correct for a sky *box* and wrong for a sky: the terrain's own horizon sits
    // below the environment's, so the gap between them showed as a flat dark band across the whole
    // picture, reading as a wall rather than as distance. The fade is haze, which is what really
    // fills that gap outdoors.
    if up < 0.0 {
        let depth = (-up).min(1.0);
        let ground = [0.085, 0.10, 0.065];
        let haze = [0.52, 0.60, 0.72];
        // Reaches the ground colour by about 25° below the horizon, and is nearly all haze at it.
        //
        // **Scaled here too**, which the first version forgot: this branch returned before the
        // `SKY_SCALE` applied to the sky below, so the whole lower hemisphere came out about two and
        // a half times too bright. It showed as a near-white band along the horizon and as a general
        // wash over everything facing downward — read as the *terrain* being too pale rather than as
        // the sky being wrong, which is why it survived a look.
        let blend = (depth * 4.0).min(1.0);
        return [
            (haze[0] + (ground[0] - haze[0]) * blend) * SKY_SCALE,
            (haze[1] + (ground[1] - haze[1]) * blend) * SKY_SCALE,
            (haze[2] + (ground[2] - haze[2]) * blend) * SKY_SCALE,
            1.0,
        ];
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
    // Kept small for the same reason the disc is: this is spread over a wide area, so it contributes
    // to the convolution far more than its brightness suggests.
    let glow = alignment.powf(96.0) * 0.9 + alignment.powf(12.0) * 0.12;
    for (channel, warm) in colour.iter_mut().zip([1.0, 0.93, 0.80]) {
        *channel += glow * warm;
    }

    // Scaled before the disc is added, so `SKY_SCALE` tunes the *ambient* light without touching how
    // bright the sun reads when you look straight at it.
    for channel in &mut colour {
        *channel *= SKY_SCALE;
    }

    // And the disc itself. Small and modest — see `SUN_INTENSITY` for why it must not be large.
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
