//! `daylight` — generates the Atrium's environment map as a Radiance `.hdr`.
//!
//! ```text
//! cargo run -p atrium --bin daylight
//! ```
//!
//! # What this is for, and why the room needed it
//!
//! The Atrium declared `sky ""`, and the engine gate found that one empty string producing **four**
//! separate symptoms before anybody named the cause: the standing lamp could not read as a lamp, the
//! glazed screen had nothing to reflect, the wall gradients had no environment to grade, and a third
//! of the player's frame was flat unlit ceiling. A roof underside gets no direct sun by definition —
//! it is lit by the sky or it is lit by nothing.
//!
//! # It deliberately contains **no sun**
//!
//! This is the one thing to understand before editing it, and `games/scarp`'s `sky` learned it the
//! expensive way. An environment map is the **indirect** half of lighting. The Atrium's scene already
//! has a `DirectionalLight` for the sun, so a bright disc in here would be the same light counted
//! twice.
//!
//! It is worse than it sounds, because irradiance weighs a direction by the solid angle it covers: a
//! five-degree disc at 250× is half a percent of the sky and contributes more ambient than all the
//! rest of it put together. The Scarp did exactly that once and every surface blew out to near-white,
//! which reads as an exposure problem rather than as double-counting.
//!
//! So this is a gradient and nothing else. The sun in the picture is the `DirectionalLight`; the sky
//! in here is what fills the shadows.
//!
//! # A gradient rather than two tones, deliberately
//!
//! `games/warren`'s `gloom` writes a **two-tone** map, and session 20 traced the horizontal seam
//! across every wall in every Warren frame to exactly that: at grazing angles the Fresnel term makes
//! the ambient reflection dominate, and the reflected ray sweeps through the map's horizon at eye
//! level, so the tone boundary is drawn across the world at the height of the camera.
//!
//! Every transition here is smoothed over a wide band for that reason. There is no edge anywhere in
//! this image, and a surface should never be able to find one.
//!
//! Idempotent: running it twice writes a byte-identical file.

use amadeo_image::{HdrImage, encode_hdr};
use std::path::PathBuf;

/// Width of the equirectangular map. Height is half of it.
///
/// 512 is plenty for a gradient with no detail in it — the prefilter convolves it heavily anyway, and
/// the file is committed, so a bigger one would be a bigger diff for no picture.
const WIDTH: u32 = 512;

/// Linear RGB straight up. A cool, slightly desaturated blue.
const ZENITH: [f32; 3] = [0.34, 0.47, 0.78];

/// Linear RGB at the horizon: paler and warmer, which is what atmosphere does to a sky near the
/// ground and what makes a gradient read as *sky* rather than as a blue wash.
const HORIZON: [f32; 3] = [0.72, 0.76, 0.80];

/// Linear RGB straight down — light bouncing back off the ground outside.
///
/// **Not black**, and this is the value that does most of the work indoors. Every upward-facing
/// surface reads the lower hemisphere, so a black floor colour here means a room lit only from above
/// with pitch-dark undersides. Warm and dim, like pale stone in daylight.
const GROUND: [f32; 3] = [0.62, 0.56, 0.48];

/// Overall multiplier on the sky's own colour.
///
/// **1.0, and it is deliberately no longer the fill knob.** It was 0.34, because this one number was
/// both the brightness of the visible sky and the strength of the ambient light it casts -- and those
/// want different values. At 0.34 the daylight through the oculus was darker than the sunlit floor
/// beneath it, which is a sky dimmer than a surface it is lighting.
///
/// The fill now lives on the look, as `Environment::sky_ambient`, which `atrium.environment` sets to
/// 0.34 -- so the fill is exactly what it was and only the picture changed. **If the room is too
/// bright or too flat, that is the number to reach for, not this one.** This is the sky's colour.
const SCALE: f32 = 1.0;

fn main() {
    let out = manifest_dir().join("assets/skies");
    if let Err(error) = std::fs::create_dir_all(&out) {
        eprintln!("could not create {}: {error}", out.display());
        std::process::exit(1);
    }

    let path = out.join("atrium_daylight.hdr");
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

    // No `color_space` line: an `.hdr` is linear by definition, so unlike a normal map there is
    // nothing to declare and nothing to forget (Q31 does not reach this format).
    let meta = path.with_extension("hdr.ama-meta");
    if let Err(error) = std::fs::write(&meta, "id = \"atrium_daylight\"\n") {
        eprintln!("could not write {}: {error}", meta.display());
        std::process::exit(1);
    }
    println!("wrote {}", meta.display());
}

fn render() -> HdrImage {
    let height = WIDTH / 2;
    let mut pixels: Vec<[f32; 4]> = Vec::with_capacity((WIDTH * height) as usize);

    for y in 0..height {
        // Latitude, top row to bottom: zero straight up, PI straight down — the same mapping
        // `Cubemap::from_equirectangular` reads it back with.
        let latitude = (y as f32 + 0.5) / height as f32 * std::f32::consts::PI;
        let up = latitude.cos();

        // Alpha is unused by an environment map and is written as one for tidiness.
        let [r, g, b] = sky_colour(up);
        for _ in 0..WIDTH {
            pixels.push([r, g, b, 1.0]);
        }
    }

    HdrImage {
        width: WIDTH,
        height,
        pixels,
    }
}

/// The sky's colour for a direction, given only how far up it points.
///
/// **Rotationally symmetric on purpose.** A sky with a bright side would put a direction into the
/// ambient that the `DirectionalLight` already provides, and the two would have to agree about where
/// the sun is — a second place to get the same fact wrong, which is what `games/scarp`'s `sky`
/// records as its own worst bug.
fn sky_colour(up: f32) -> [f32; 3] {
    // Two blends, both over a wide band so there is no edge to find. `smooth` is the cubic
    // smoothstep: flat at both ends, so neither the horizon nor the zenith has a visible boundary.
    let smooth = |t: f32| {
        let t = t.clamp(0.0, 1.0);
        t * t * (3.0 - 2.0 * t)
    };

    let mut colour = [0.0_f32; 3];
    for channel in 0..3 {
        let sky = if up >= 0.0 {
            // Horizon to zenith over the whole upper hemisphere, so the change is gradual everywhere
            // rather than concentrated in a band a wall can reflect as a line.
            let t = smooth(up);
            HORIZON[channel] + (ZENITH[channel] - HORIZON[channel]) * t
        } else {
            // Horizon down to the ground bounce, and deliberately slower: the first few degrees below
            // the horizon are what a vertical wall reflects at eye level, and that is precisely where
            // the Warren's seam appeared.
            let t = smooth(-up * 0.75);
            HORIZON[channel] + (GROUND[channel] - HORIZON[channel]) * t
        };
        colour[channel] = sky * SCALE;
    }
    colour
}

fn manifest_dir() -> PathBuf {
    PathBuf::from(env!("CARGO_MANIFEST_DIR"))
}
