//! `amadeo image` — read a capture at the pixel level.
//!
//! # Why this is in the engine rather than in a reviewer's scratch directory
//!
//! This project judges nearly everything by capture, and `docs/14-the-critic.md` §3 requires a
//! pixel probe behind every quantitative claim about a picture: *"the lamp does not light the
//! floor"* is arguable, and *"row y=600 reads 198/200/202/211 past the lamp base, with no local
//! peak"* is not. Review 12 wrote one of these from scratch to say that, and would have written it
//! again next time.
//!
//! It is standalone, like `fmt` — it reads a PNG and prints numbers, and never needs to ask a game
//! anything. So it works on a capture taken months ago, or on one from a project whose game no
//! longer compiles.
//!
//! **`crop` magnifies by an integer factor with no filtering**, which is the whole point of it: a
//! smoothly resampled crop shows an interpolation of the texels rather than the texels, and the
//! defects this exists to find — a shadow edge stepping along a texel grid, a normal map aliasing
//! into a comb of dashes — are *made of* the texel grid.

use amadeo_image::{PixelFormat, TextureData, decode_png, encode_png};
use anyhow::{Context, Result, bail};
use std::path::Path;

/// One operation on one capture.
#[derive(Debug, PartialEq, Eq)]
pub(crate) enum Op {
    /// Print the colour at each named pixel.
    Probe { points: Vec<(u32, u32)> },
    /// Print one horizontal scanline as `x r g b`.
    Row { y: u32, from: u32, to: u32 },
    /// Print one vertical scanline as `y r g b`.
    Column { x: u32, from: u32, to: u32 },
    /// Magnify a rectangle by an integer factor and write it out.
    Crop {
        x: u32,
        y: u32,
        width: u32,
        height: u32,
        scale: u32,
        out: std::path::PathBuf,
    },
    /// A luminance histogram, over the whole image or one rectangle of it.
    Stats {
        region: Option<(u32, u32, u32, u32)>,
    },
    /// Compare two captures: where they differ, by how much, and over what area.
    ///
    /// **The most-used operation in this job, and it was hand-rolled six times before it existed.**
    /// `docs/14` §4 instructs a reviewer to split a variable and move one half of it, which makes
    /// "how do these two frames differ" the question that follows almost every experiment. Review 13
    /// asked for it by name and said the bounding box would have replaced twelve probes with one
    /// command.
    Diff { other: std::path::PathBuf },
}

/// Runs one operation and prints its answer.
pub(crate) fn run(path: &Path, op: &Op) -> Result<()> {
    let bytes = std::fs::read(path)
        .with_context(|| format!("could not read {} — is the path right?", path.display()))?;
    let image = decode_png(&bytes, &path.display().to_string())
        .with_context(|| format!("{} is not a PNG this can read", path.display()))?;

    let width = image.width;
    let height = image.height;
    // Four bytes per pixel in every format `decode_png` produces, sRGB or linear alike -- the tag
    // says how to *interpret* them, and this command deliberately reports the stored bytes.
    let channels = 4usize;
    let at = |x: u32, y: u32| -> (u8, u8, u8, u8) {
        let index = (y as usize * width as usize + x as usize) * channels;
        (
            image.pixels[index],
            image.pixels[index + 1],
            image.pixels[index + 2],
            image.pixels[index + 3],
        )
    };
    let inside = |x: u32, y: u32| -> Result<()> {
        if x >= width || y >= height {
            bail!("({x},{y}) is outside a {width} x {height} image");
        }
        Ok(())
    };

    match op {
        Op::Probe { points } => {
            for &(x, y) in points {
                inside(x, y)?;
                let (r, g, b, a) = at(x, y);
                println!("({x},{y}) = {r} {g} {b} {a}  luma {}", luminance(r, g, b));
            }
        }
        Op::Row { y, from, to } => {
            inside(*from, *y)?;
            for x in *from..(*to).min(width) {
                let (r, g, b, _) = at(x, *y);
                println!("{x} {r} {g} {b} {}", luminance(r, g, b));
            }
        }
        Op::Column { x, from, to } => {
            inside(*x, *from)?;
            for y in *from..(*to).min(height) {
                let (r, g, b, _) = at(*x, y);
                println!("{y} {r} {g} {b} {}", luminance(r, g, b));
            }
        }
        Op::Crop {
            x,
            y,
            width: crop_width,
            height: crop_height,
            scale,
            out,
        } => {
            if *scale == 0 {
                bail!("--scale must be at least 1");
            }
            inside(*x, *y)?;
            let out_width = crop_width * scale;
            let out_height = crop_height * scale;
            let mut pixels = vec![0u8; out_width as usize * out_height as usize * channels];
            for row in 0..out_height {
                for column in 0..out_width {
                    // Integer division is the nearest-neighbour magnification: every output pixel
                    // takes one source texel whole, so a texel-grid artefact stays a texel grid.
                    let source_x = (x + column / scale).min(width - 1);
                    let source_y = (y + row / scale).min(height - 1);
                    let (r, g, b, a) = at(source_x, source_y);
                    let index = (row as usize * out_width as usize + column as usize) * channels;
                    pixels[index] = r;
                    pixels[index + 1] = g;
                    pixels[index + 2] = b;
                    pixels[index + 3] = a;
                }
            }
            let magnified = TextureData {
                width: out_width,
                height: out_height,
                format: PixelFormat::Rgba8UnormSrgb,
                pixels,
            };
            let encoded = encode_png(&magnified)
                .map_err(|error| anyhow::anyhow!("could not encode the crop: {error}"))?;
            std::fs::write(out, encoded)
                .with_context(|| format!("could not write {}", out.display()))?;
            println!(
                "wrote {} — {} x {} at {}x from ({},{})",
                out.display(),
                out_width,
                out_height,
                scale,
                x,
                y
            );
        }
        Op::Stats { region } => {
            let (rx, ry, rw, rh) = region.unwrap_or((0, 0, width, height));
            inside(rx, ry)?;
            let rw = rw.min(width - rx);
            let rh = rh.min(height - ry);

            let buckets = 16usize;
            let mut histogram = vec![0u64; buckets];
            let mut lowest = 255u8;
            let mut highest = 0u8;
            let mut total = 0u64;
            // **How many pixels are at the top of the range**, which the histogram's 240-255 bucket
            // hides: a frame can be perfectly exposed and still have a blown highlight, and the
            // difference between 250 and 255 is the difference between a bright surface and one
            // with no detail left in it. Review 13 found a light erasing a stone pillar over a
            // 200-row run and said this figure would have surfaced it in the first command.
            let mut clipped = 0u64;
            for y in ry..ry + rh {
                for x in rx..rx + rw {
                    let (r, g, b, _) = at(x, y);
                    let luma = luminance(r, g, b);
                    histogram[(luma as usize * buckets) / 256] += 1;
                    lowest = lowest.min(luma);
                    highest = highest.max(luma);
                    total += u64::from(luma);
                    if r >= 254 || g >= 254 || b >= 254 {
                        clipped += 1;
                    }
                }
            }
            let count = (u64::from(rw) * u64::from(rh)).max(1);
            #[expect(
                clippy::cast_precision_loss,
                reason = "a mean over at most a few million pixels, printed to one decimal"
            )]
            let mean = total as f64 / count as f64;
            #[expect(
                clippy::cast_precision_loss,
                reason = "a percentage, printed to two decimals"
            )]
            let blown = clipped as f64 * 100.0 / count as f64;
            if region.is_some() {
                println!("{rw} x {rh} at ({rx},{ry}) of a {width} x {height} image");
            } else {
                println!("{width} x {height}");
            }
            println!("min {lowest}, max {highest}, mean {mean:.1}");
            // Any channel, not luma: a clipped red on an orange surface is lost detail even where
            // the luminance is nowhere near the top.
            println!("clipped (any channel >= 254)  {clipped} px  {blown:.2}%");
            for (bucket, pixels) in histogram.iter().enumerate() {
                #[expect(
                    clippy::cast_precision_loss,
                    reason = "a percentage, printed to one decimal"
                )]
                let share = *pixels as f64 * 100.0 / count as f64;
                let bar = "#".repeat(usize::try_from(pixels * 40 / count).unwrap_or(0));
                println!(
                    "{:>3}-{:>3} {:>8} {share:>5.1}% {bar}",
                    bucket * 256 / buckets,
                    (bucket + 1) * 256 / buckets - 1,
                    pixels
                );
            }
        }
        Op::Diff { other } => {
            let other_bytes = std::fs::read(other)
                .with_context(|| format!("could not read {}", other.display()))?;
            let second = decode_png(&other_bytes, &other.display().to_string())
                .with_context(|| format!("{} is not a PNG this can read", other.display()))?;
            if second.width != width || second.height != height {
                bail!(
                    "the two images are different sizes: {width} x {height} against {} x {}",
                    second.width,
                    second.height
                );
            }

            let mut worst = 0u8;
            let mut worst_at = (0u32, 0u32);
            let mut changed = 0u64;
            // The bounding box of everything that moved, which is the figure that turns "something
            // changed" into "*this* changed" — and the one review 13 said would have replaced
            // twelve probes with one command.
            let (mut left, mut top) = (u32::MAX, u32::MAX);
            let (mut right, mut bottom) = (0u32, 0u32);

            for y in 0..height {
                for x in 0..width {
                    let (r, g, b, _) = at(x, y);
                    let index = (y as usize * width as usize + x as usize) * channels;
                    let difference = r
                        .abs_diff(second.pixels[index])
                        .max(g.abs_diff(second.pixels[index + 1]))
                        .max(b.abs_diff(second.pixels[index + 2]));
                    if difference == 0 {
                        continue;
                    }
                    changed += 1;
                    if difference > worst {
                        worst = difference;
                        worst_at = (x, y);
                    }
                    left = left.min(x);
                    top = top.min(y);
                    right = right.max(x);
                    bottom = bottom.max(y);
                }
            }

            let count = (u64::from(width) * u64::from(height)).max(1);
            #[expect(
                clippy::cast_precision_loss,
                reason = "a percentage, printed to two decimals"
            )]
            let share = changed as f64 * 100.0 / count as f64;
            if changed == 0 {
                println!("identical — every one of the {count} pixels matches");
                return Ok(());
            }
            println!("changed        {changed} px of {count}  ({share:.2}%)");
            println!(
                "largest        {worst} levels at ({},{})",
                worst_at.0, worst_at.1
            );
            println!(
                "bounding box   x {left}..{right}, y {top}..{bottom}   ({} x {})",
                right - left + 1,
                bottom - top + 1
            );
        }
    }
    Ok(())
}

/// Rec. 601 luma, in integers.
///
/// Integer weights rather than floats because this is a *reporting* number that two reviews must
/// be able to compare: `30/59/11` is exact and reproducible, where a float mean drifts in its last
/// digit between builds and invites an argument about whether a value moved.
fn luminance(r: u8, g: u8, b: u8) -> u8 {
    let weighted = u32::from(r) * 30 + u32::from(g) * 59 + u32::from(b) * 11;
    u8::try_from(weighted / 100).unwrap_or(255)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn luminance_weights_green_hardest() {
        assert_eq!(luminance(0, 0, 0), 0);
        assert_eq!(luminance(255, 255, 255), 255);
        // Green is 59% of the answer, which is what makes this a perceptual number rather than a
        // channel average -- pure green reads far brighter than pure blue at the same value.
        assert!(luminance(0, 255, 0) > luminance(0, 0, 255) * 4);
    }
}
