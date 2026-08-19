//! Turning a height field into the three maps a PBR material wants.
//!
//! # One height field, three maps, and that is the point
//!
//! Base colour, a tangent-space normal map (ADR 0047) and a packed occlusion-roughness-metallic map
//! (ADR 0048, ADR 0083). The way a material set goes wrong is by **drifting apart** — a normal map
//! whose bumps do not line up with the colour's is worse than no normal map at all, because the eye
//! is given two conflicting surfaces at once. Reading all of them off one function is what makes
//! that impossible rather than merely unlikely.
//!
//! # Colour is sRGB, everything else is linear, and nothing in a PNG says which
//!
//! A normal map read through the sRGB curve has every direction bent, and a roughness map read
//! through it is wrong everywhere. **Q31's trap**: the file itself carries no such tag, so the
//! `.ama-meta` sidecar's `color_space` is the only thing that says — and forgetting it is silent.
//! [`Space`] exists so the caller has to name it at the point of writing rather than remember later.

use amadeo_image::{PixelFormat, TextureData, encode_png};

/// Whether an image holds colour or data.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Space {
    /// Colour, written through the sRGB transfer curve because the GPU will decode it back.
    Srgb,
    /// Data — directions, roughness, occlusion — written as it is meant to be read.
    Linear,
}

impl Space {
    /// The pixel format an image in this space is uploaded as.
    #[must_use]
    pub fn format(self) -> PixelFormat {
        match self {
            Space::Srgb => PixelFormat::Rgba8UnormSrgb,
            Space::Linear => PixelFormat::Rgba8Unorm,
        }
    }

    /// What a sidecar's `color_space` has to declare for an image in this space, or `None` when the
    /// default already says it.
    ///
    /// **The asymmetry is the engine's, not this crate's**, and it is worth surfacing rather than
    /// smoothing over: an absent `color_space` means sRGB, because that is what an art file holds
    /// and what every texture was before normal maps existed. So a colour map needs no line and a
    /// data map's line is load-bearing — forget it and every direction in a normal map is bent, with
    /// nothing to say so.
    #[must_use]
    pub fn sidecar(self) -> Option<&'static str> {
        match self {
            Space::Srgb => None,
            Space::Linear => Some("linear"),
        }
    }
}

/// A square image being built, one pixel at a time.
#[derive(Debug, Clone)]
pub struct Canvas {
    size: u32,
    space: Space,
    pixels: Vec<u8>,
}

impl Canvas {
    /// An opaque black canvas of the given size.
    ///
    /// A power of two is wanted rather than required, so mip generation halves cleanly.
    #[must_use]
    pub fn new(size: u32, space: Space) -> Self {
        Self {
            size,
            space,
            pixels: vec![0; (size as usize) * (size as usize) * 4],
        }
    }

    /// How wide and tall it is.
    #[must_use]
    pub fn size(&self) -> u32 {
        self.size
    }

    /// Fills every pixel from a function of the texture coordinate.
    ///
    /// The function is handed `(u, v)` at the **centre** of each texel rather than at its corner.
    /// That is not fussiness: sampling at the corner puts the first row exactly on `v = 0`, so a
    /// feature the caller places at the origin lands half a texel off, and a height field
    /// differenced for a normal map inherits the same shift — which shows as relief that does not
    /// line up with the colour it came from.
    pub fn fill(&mut self, mut shade: impl FnMut(f32, f32) -> [f32; 4]) {
        #[expect(
            clippy::cast_precision_loss,
            reason = "a texture size is at most a few thousand; f32 is exact well past that"
        )]
        let extent = self.size as f32;
        for y in 0..self.size {
            for x in 0..self.size {
                #[expect(
                    clippy::cast_precision_loss,
                    reason = "a pixel coordinate inside a texture, exactly representable"
                )]
                let (u, v) = ((x as f32 + 0.5) / extent, (y as f32 + 0.5) / extent);
                let value = shade(u, v);
                let at = ((y as usize) * (self.size as usize) + (x as usize)) * 4;
                for (channel, component) in value.iter().take(3).enumerate() {
                    self.pixels[at + channel] = match self.space {
                        Space::Srgb => to_srgb_byte(*component),
                        Space::Linear => to_unit_byte(*component),
                    };
                }
                // **Alpha is never transfer-encoded**, in either space. It is coverage rather than
                // light, so the sRGB curve does not apply to it — the same rule `amadeo-image`'s
                // mip chain follows when it averages one.
                self.pixels[at + 3] = to_unit_byte(value[3]);
            }
        }
    }

    /// The finished image, ready to upload or encode.
    #[must_use]
    pub fn finish(self) -> TextureData {
        TextureData {
            width: self.size,
            height: self.size,
            format: self.space.format(),
            pixels: self.pixels,
        }
    }

    /// The finished image as PNG bytes.
    ///
    /// # Errors
    ///
    /// If the encoder rejects the buffer, which it does only for a size or a length that cannot
    /// describe an image.
    pub fn encode(self) -> Result<Vec<u8>, amadeo_image::EncodeError> {
        encode_png(&self.finish())
    }
}

/// A tangent-space normal from a height field, by central difference.
///
/// The gradient of the height is the surface's slope, and the tangent-space normal is
/// `(-dh/du, -dh/dv, 1)` normalised. `relief` is how pronounced it comes out: **smaller is
/// stronger**, because it divides the slope.
///
/// # Sample the height function, never the colour image
///
/// Colour carries per-slab tone and a joint's darkening, and neither of those is a *shape*. A normal
/// map differenced from a colour image embosses the tone variation, so every slab reads as a
/// different height and the wall looks quilted. This takes a function for exactly that reason.
///
/// Wrapped with `rem_euclid`, so the map tiles as seamlessly as the height it came from — `%` is
/// wrong here, because a negative coordinate must wrap to the far edge rather than to itself.
#[must_use]
pub fn normal_from_height(
    height: &impl Fn(f32, f32) -> f32,
    u: f32,
    v: f32,
    step: f32,
    relief: f32,
) -> [f32; 3] {
    let at = |du: f32, dv: f32| height((u + du).rem_euclid(1.0), (v + dv).rem_euclid(1.0));
    // **Divided through by the sample spacing, so this is a real derivative** rather than a raw
    // difference. Without it the answer depends on the texture's resolution — the same height field
    // rendered at 512 and at 1024 would come out with relief half as strong at the higher size, and
    // the only clue would be a normal map that looked mysteriously flatter.
    let span = (2.0 * step).max(1e-9);
    let dhdu = (at(step, 0.0) - at(-step, 0.0)) / span;
    let dhdv = (at(0.0, step) - at(0.0, -step)) / span;
    normalise([-dhdu / relief, -dhdv / relief, 1.0])
}

/// How much of the sky a point can see, `0.0` fully enclosed and `1.0` fully open.
///
/// This is glTF's occlusion channel, which ADR 0083 made the red lane of the packed map.
///
/// # Why this is not just "how low is it"
///
/// Occlusion is not depth. A point at the bottom of a wide shallow scoop is barely occluded, and a
/// point in a narrow crack is heavily occluded at the same depth — what matters is how much *higher*
/// the surroundings are, not how low this point is. So the height here is compared against a ring
/// around it, which is the cheap standard approximation and what every bake tool reduces to when the
/// ray budget runs out.
///
/// Only neighbours **above** the point count. One below is a drop, and a drop occludes nothing —
/// without that clamp a face beside a joint would be darkened by the joint next to it, which is
/// backwards.
#[must_use]
pub fn cavity_from_height(
    height: &impl Fn(f32, f32) -> f32,
    u: f32,
    v: f32,
    radius: f32,
    strength: f32,
) -> f32 {
    // Eight neighbours on a ring: four square, four diagonal at the same radius rather than at
    // sqrt(2) times it, so no direction is weighted more heavily than another.
    const DIAGONAL: f32 = std::f32::consts::FRAC_1_SQRT_2;
    const RING: [(f32, f32); 8] = [
        (1.0, 0.0),
        (-1.0, 0.0),
        (0.0, 1.0),
        (0.0, -1.0),
        (DIAGONAL, DIAGONAL),
        (DIAGONAL, -DIAGONAL),
        (-DIAGONAL, DIAGONAL),
        (-DIAGONAL, -DIAGONAL),
    ];

    let here = height(u, v);
    let mut higher = 0.0;
    for (du, dv) in RING {
        let around = height(
            (u + du * radius).rem_euclid(1.0),
            (v + dv * radius).rem_euclid(1.0),
        );
        higher += (around - here).max(0.0);
    }

    #[expect(
        clippy::cast_precision_loss,
        reason = "the ring has eight entries; the literal is exact"
    )]
    let mean = higher / RING.len() as f32;
    (1.0 - mean * strength).clamp(0.0, 1.0)
}

/// The glTF channel layout for the packed map: **red is occlusion, green is roughness, blue is
/// metallic**, alpha opaque.
///
/// A function rather than a comment, because the packing is the thing most easily got wrong by one
/// channel and the failure is silent — swapping green and blue turns stone into a mirror, and
/// swapping red and green turns roughness into occlusion, both of which render perfectly happily.
#[must_use]
pub fn pack_orm(occlusion: f32, roughness: f32, metallic: f32) -> [f32; 4] {
    [occlusion, roughness, metallic, 1.0]
}

/// Normalises a vector, falling back to straight up rather than dividing by zero.
#[must_use]
pub fn normalise(v: [f32; 3]) -> [f32; 3] {
    let length = (v[0] * v[0] + v[1] * v[1] + v[2] * v[2]).sqrt();
    if length <= f32::EPSILON {
        return [0.0, 0.0, 1.0];
    }
    [v[0] / length, v[1] / length, v[2] / length]
}

/// A `0..1` value as a byte through the sRGB transfer curve.
///
/// The texture is uploaded as `Rgba8UnormSrgb` and the GPU decodes it back to linear when sampling,
/// so writing linear values here comes out visibly too dark.
#[must_use]
pub fn to_srgb_byte(linear: f32) -> u8 {
    let linear = linear.clamp(0.0, 1.0);
    // The IEC 61966-2-1 curve. `powf` is used here and is safe: ADR 0044 bans it in anything that
    // decides where the ground is, and this decides what colour a pixel is at *generation* time —
    // its output is a committed PNG, checked by its own hash, not gameplay state.
    let encoded = if linear <= 0.003_130_8 {
        linear * 12.92
    } else {
        1.055 * linear.powf(1.0 / 2.4) - 0.055
    };
    to_unit_byte(encoded)
}

/// A `0..1` value as a byte with no transfer curve. For data maps.
#[must_use]
pub fn to_unit_byte(value: f32) -> u8 {
    #[expect(
        clippy::cast_possible_truncation,
        clippy::cast_sign_loss,
        reason = "clamped to 0..1 and scaled to 0..255 before rounding"
    )]
    let byte = (value.clamp(0.0, 1.0) * 255.0).round() as u8;
    byte
}

/// A tangent-space normal packed into a byte triple, where `(128, 128, 255)` is flat.
#[must_use]
pub fn encode_normal(normal: [f32; 3]) -> [f32; 4] {
    [
        normal[0] * 0.5 + 0.5,
        normal[1] * 0.5 + 0.5,
        normal[2] * 0.5 + 0.5,
        1.0,
    ]
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn srgb_encoding_is_the_curve_and_not_a_multiply() {
        assert_eq!(to_srgb_byte(0.0), 0);
        assert_eq!(to_srgb_byte(1.0), 255);
        // **Half the light is not half the byte**, and this number is the whole reason the curve
        // exists. Linear 0.5 encodes to about 188, not 128 — which is also why averaging sRGB bytes
        // directly makes a mip chain too dark, the classic mipmap bug `amadeo-image` documents.
        let half = to_srgb_byte(0.5);
        assert!(
            (186..=190).contains(&half),
            "linear 0.5 should encode near 188, got {half}"
        );
        // And the other direction: sRGB 0x80 is 0.216 of the light, not 0.5.
        let quarter = to_srgb_byte(0.216);
        assert!(
            (126..=130).contains(&quarter),
            "linear 0.216 should encode near 128, got {quarter}"
        );
    }

    #[test]
    fn a_flat_height_field_gives_a_flat_normal() {
        let flat = |_u: f32, _v: f32| 0.7;
        let normal = normal_from_height(&flat, 0.3, 0.4, 1.0 / 512.0, 0.15);
        assert_eq!(encode_normal(normal)[0], 0.5);
        assert_eq!(encode_normal(normal)[1], 0.5);
        assert!(normal[2] > 0.99);
    }

    #[test]
    fn a_slope_leans_the_normal_downhill() {
        // The sign is the thing worth pinning. A normal map with its x inverted lights every surface
        // from the wrong side, which reads as plausible relief carved the wrong way round — the
        // hardest kind of wrong to notice, because it looks like a decision.
        // Relief 1.0 means "a slope of one height unit per uv unit is a 45 degree lean", so this
        // ramp -- which rises exactly one over the tile -- must come out at exactly 45 degrees.
        let ramp = |u: f32, _v: f32| u;
        let normal = normal_from_height(&ramp, 0.5, 0.5, 1.0 / 512.0, 1.0);
        assert!(
            (normal[0] + std::f32::consts::FRAC_1_SQRT_2).abs() < 0.01,
            "a surface rising with u must lean its normal towards -u, got {normal:?}"
        );
    }

    #[test]
    fn a_pit_is_occluded_and_a_peak_is_not() {
        // A cone-shaped pit at the centre of the tile, and its inverse.
        let pit = |u: f32, v: f32| {
            let (du, dv) = (u - 0.5, v - 0.5);
            (du * du + dv * dv).sqrt().min(0.2)
        };
        let peak = |u: f32, v: f32| -pit(u, v);

        let in_pit = cavity_from_height(&pit, 0.5, 0.5, 0.02, 1.0);
        let on_peak = cavity_from_height(&peak, 0.5, 0.5, 0.02, 1.0);
        let out_flat = cavity_from_height(&pit, 0.5, 0.5 + 0.35, 0.02, 1.0);

        assert!(
            in_pit < 0.99,
            "the bottom of a pit sees less sky than an open surface, got {in_pit}"
        );
        assert!(
            (on_peak - 1.0).abs() < 1e-6,
            "the top of a peak is occluded by nothing at all, got {on_peak} — a value below one \
             means neighbours *below* the point are being counted, which is backwards"
        );
        assert!(
            (out_flat - 1.0).abs() < 1e-3,
            "flat ground away from the feature should be open, got {out_flat}"
        );
    }

    #[test]
    fn a_canvas_samples_at_texel_centres() {
        // Half a texel, and it matters because a height field differenced for a normal map inherits
        // the same offset — so relief that does not line up with its own colour is what a corner
        // sample would produce.
        let mut canvas = Canvas::new(4, Space::Linear);
        let mut first = None;
        canvas.fill(|u, _v| {
            first.get_or_insert(u);
            [u, 0.0, 0.0, 1.0]
        });
        assert!(
            (first.expect("filled") - 0.125).abs() < 1e-6,
            "the first texel of a four-wide canvas centres on 0.125, not 0.0"
        );
    }
}
