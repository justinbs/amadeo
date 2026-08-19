//! Gradient noise that **tiles**.
//!
//! # Why this is not `amadeo-noise`
//!
//! `amadeo-noise` is a function over the whole plane, which is exactly what a world wants and
//! exactly what a texture cannot use: a texture is a square that has to meet its own opposite edge
//! without a seam. So the lattice corners here are taken modulo the lattice size, which makes the
//! field periodic by construction rather than by tapering it at the edges.
//!
//! ADR 0044's rule still applies and is why this is written rather than taken from a crate: no
//! `sin`, no `cos`, no `powf`. Everything below is `+ - * /`, `floor` and integer hashing, all of
//! which IEEE 754 specifies exactly — so the same seed writes the same PNG on every machine, which
//! is what lets a generated texture be committed as text rather than as an image.
//!
//! This is the third home for this routine. It was written in `games/scarp`'s `turf`, copied into
//! `games/atrium`'s `surfaces`, and the copy's own comment said a third user was when it should move
//! into the engine.

/// Periodic gradient noise over the unit square, wrapping after `lattice` cells.
///
/// Returns roughly `-1..1`. Values slightly outside are possible and are not clamped, because
/// clamping here would flatten the peaks a caller may want to scale.
#[must_use]
pub fn tiling(seed: u64, u: f32, v: f32, lattice: i32) -> f32 {
    let lattice = lattice.max(1);
    #[expect(
        clippy::cast_precision_loss,
        reason = "a lattice is a small count; the f32 is exact well past any usable size"
    )]
    let scale = lattice as f32;
    let (x, y) = (u * scale, v * scale);
    #[expect(
        clippy::cast_possible_truncation,
        reason = "floor of a coordinate over a small lattice; the range is far inside i32"
    )]
    let (x0, y0) = (x.floor() as i32, y.floor() as i32);
    #[expect(
        clippy::cast_precision_loss,
        reason = "the same small integers back to f32, exactly representable"
    )]
    let (fx, fy) = (x - x0 as f32, y - y0 as f32);

    // Perlin's fade: smooth in the first and second derivative, so the lattice does not show as a
    // grid of creases.
    let fade = |t: f32| t * t * t * (t * (t * 6.0 - 15.0) + 10.0);
    let (ux, uy) = (fade(fx), fade(fy));

    let corner = |ix: i32, iy: i32| {
        let gradient = gradient_at(seed, ix.rem_euclid(lattice), iy.rem_euclid(lattice));
        #[expect(
            clippy::cast_precision_loss,
            reason = "a difference of two adjacent lattice indices: 0 or 1"
        )]
        let (dx, dy) = ((ix - x0) as f32, (iy - y0) as f32);
        gradient[0] * (fx - dx) + gradient[1] * (fy - dy)
    };

    let lerp = |a: f32, b: f32, t: f32| a + t * (b - a);
    let bottom = lerp(corner(x0, y0), corner(x0 + 1, y0), ux);
    let top = lerp(corner(x0, y0 + 1), corner(x0 + 1, y0 + 1), ux);
    // The `sqrt(2)` brings the theoretical maximum of 2D gradient noise up to about one.
    lerp(bottom, top, uy) * std::f32::consts::SQRT_2
}

/// Several octaves of [`tiling`] summed, at the lattices and weights given.
///
/// # Use co-prime lattices, and the reason is a real defect this repository shipped
///
/// Two octaves an exact factor of four apart — 8 and 32 — put their features on the same grid, and
/// the result reads as a regular 45° cross-hatch rather than as a material. That is a woven or
/// anti-slip pattern, and it was most of why the first generated stone in this engine looked like
/// bathroom tile. Lattices with no common factor do not line up, so the sum has no period shorter
/// than the tile itself.
///
/// `7, 17, 43` is a good default set. `8, 16, 32` is the trap.
#[must_use]
pub fn octaves(seed: u64, u: f32, v: f32, layers: &[(i32, f32)]) -> f32 {
    let mut total = 0.0;
    for (index, (lattice, weight)) in layers.iter().enumerate() {
        // Each octave gets its own seed, so two layers at the same lattice are not the same field
        // scaled — which would double the amplitude and add no detail.
        let salt = seed ^ ((index as u64).wrapping_mul(0x9E37_79B9_7F4A_7C15));
        total += tiling(salt, u, v, *lattice) * weight;
    }
    total
}

/// A stable pseudo-random number in `0..1` from two integers.
///
/// splitmix64's finalizer, which is a bijection built to avalanche a **counter** — the exact case
/// here, where the inputs are small consecutive integers like a course number or a slab index.
///
/// **FNV-1a is the wrong hash for this and it cost a real defect.** Feeding two small integers'
/// bytes through FNV-1a leaves six of eight bytes constant zero and the two that move differing in
/// one low bit; FNV avalanches well over many bytes and barely at all over one. An authored 10%
/// per-slab tone variation arrived as 0.1% — below the threshold of perception, in code that ran,
/// was tested, and changed no pixel anybody could see.
#[must_use]
pub fn hash01(seed: u64, a: i64, b: i64) -> f32 {
    let mut hash = (a as u64).wrapping_mul(0x9E37_79B9_7F4A_7C15)
        ^ (b as u64).wrapping_mul(0xBF58_476D_1CE4_E5B9)
        ^ seed;
    hash ^= hash >> 30;
    hash = hash.wrapping_mul(0xBF58_476D_1CE4_E5B9);
    hash ^= hash >> 27;
    hash = hash.wrapping_mul(0x94D0_49BB_1331_11EB);
    hash ^= hash >> 31;
    // The top sixteen bits of a well-avalanched word, which is plenty of resolution for a tone or a
    // width and costs one shift.
    #[expect(
        clippy::cast_precision_loss,
        reason = "a 16-bit integer to f32 is exact"
    )]
    let scaled = ((hash >> 40) & 0xFFFF) as f32;
    scaled / 65535.0
}

/// One of sixteen fixed gradients, chosen by hashing a lattice corner.
fn gradient_at(seed: u64, x: i32, y: i32) -> [f32; 2] {
    // The 45 degree ones, named so nobody reads them as a fumbled `FRAC_1_SQRT_2` — which is
    // exactly what they are.
    const DIAGONAL: f32 = std::f32::consts::FRAC_1_SQRT_2;
    // Cosine and sine of 22.5°, written as literals because ADR 0044 forbids computing them.
    const NEAR: f32 = 0.923_879_5;
    const FAR: f32 = 0.382_683_43;

    // **Sixteen directions, not eight.** Eight means every gradient is axis-aligned or exactly
    // diagonal, and a field built from those has visible structure along those four lines. The odd
    // multiples of 22.5° break it up for the cost of eight more constants.
    const GRADIENTS: [[f32; 2]; 16] = [
        [1.0, 0.0],
        [-1.0, 0.0],
        [0.0, 1.0],
        [0.0, -1.0],
        [DIAGONAL, DIAGONAL],
        [-DIAGONAL, DIAGONAL],
        [DIAGONAL, -DIAGONAL],
        [-DIAGONAL, -DIAGONAL],
        [NEAR, FAR],
        [-NEAR, FAR],
        [NEAR, -FAR],
        [-NEAR, -FAR],
        [FAR, NEAR],
        [-FAR, NEAR],
        [FAR, -NEAR],
        [-FAR, -NEAR],
    ];

    let mut hash = (x as u64).wrapping_mul(0x9E37_79B9_7F4A_7C15)
        ^ (y as u64).wrapping_mul(0xBF58_476D_1CE4_E5B9)
        ^ seed;
    hash ^= hash >> 30;
    hash = hash.wrapping_mul(0xBF58_476D_1CE4_E5B9);
    hash ^= hash >> 27;
    hash = hash.wrapping_mul(0x94D0_49BB_1331_11EB);
    hash ^= hash >> 31;
    GRADIENTS[(hash >> 60) as usize & 15]
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn it_tiles_seamlessly() {
        // The one property this exists for, and the one `amadeo-noise` deliberately does not have.
        // Sampling either side of the wrap must give the same value, or every generated texture
        // shows a hard line down the middle of every wall it covers.
        for step in 0..64 {
            let v = step as f32 / 64.0;
            let left = tiling(0x51A9, 0.0, v, 8);
            let right = tiling(0x51A9, 1.0, v, 8);
            assert!(
                (left - right).abs() < 1e-5,
                "the field must wrap at u=0/1: {left} against {right} at v={v}"
            );
            let bottom = tiling(0x51A9, v, 0.0, 8);
            let top = tiling(0x51A9, v, 1.0, 8);
            assert!(
                (bottom - top).abs() < 1e-5,
                "and at v=0/1: {bottom} against {top} at u={v}"
            );
        }
    }

    #[test]
    fn it_actually_varies() {
        // The companion assertion, and it is not padding: a function returning a constant tiles
        // perfectly, so the test above passes for the worst possible implementation.
        let mut lowest = f32::MAX;
        let mut highest = f32::MIN;
        for step in 0..256 {
            let value = tiling(0x51A9, step as f32 / 256.0, 0.37, 8);
            lowest = lowest.min(value);
            highest = highest.max(value);
        }
        assert!(
            highest - lowest > 0.5,
            "one scanline of noise spanned only {} — a near-constant field is a bug that the \
             seam test cannot see",
            highest - lowest
        );
    }

    #[test]
    fn a_grid_of_samples_hashes_to_a_known_number() {
        // `amadeo-noise`'s pinned-literal test, applied here for the same reason: this feeds files
        // that are committed, so a one-bit change in a constant has to turn CI red on both platforms
        // rather than quietly rewriting every texture in the repository.
        let mut hash: u64 = 0xCBF2_9CE4_8422_2325;
        for y in 0..16 {
            for x in 0..16 {
                let value = tiling(0x7C3E, x as f32 / 16.0, y as f32 / 16.0, 7);
                for byte in value.to_le_bytes() {
                    hash ^= u64::from(byte);
                    hash = hash.wrapping_mul(0x1000_0000_01B3);
                }
            }
        }
        assert_eq!(hash, 7_396_742_714_973_454_890, "the field moved");
    }

    #[test]
    fn hash01_avalanches_over_small_integers() {
        // **The exact case FNV-1a failed at**, kept as a test because the failure was invisible: a
        // per-slab tone authored at 10% arrived at 0.1% and nothing but a measurement could tell.
        // Four consecutive coordinates must spread across the range rather than clustering.
        let values: Vec<f32> = (0..4)
            .flat_map(|y| (0..4).map(move |x| hash01(0x1234, x, y)))
            .collect();
        let lowest = values.iter().copied().fold(f32::MAX, f32::min);
        let highest = values.iter().copied().fold(f32::MIN, f32::max);
        assert!(
            highest - lowest > 0.7,
            "sixteen small consecutive coordinates spanned only {} of 0..1",
            highest - lowest
        );
    }
}
