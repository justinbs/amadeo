//! PPM: the image format that is also a text file.
//!
//! # Why this engine bothers with a format nothing exports
//!
//! `assets/textures/placeholder.ppm` is four lines of numbers a human can read, edit, and check
//! against what appears on screen. That is the same trade invariant I1 makes everywhere else in the
//! project — the `.scene` format, the `.replay` format, the `.ama-meta` sidecar — and it is worth
//! roughly two hundred lines here.
//!
//! Concretely, it buys two things PNG cannot. A test can assert an exact pixel value against a file
//! whose contents are visible in the diff. And a missing-texture placeholder can be understood, and
//! changed, without a paint program.
//!
//! # What is supported
//!
//! Both Netpbm colour forms:
//!
//! - **P3** — samples written as whitespace-separated decimal text.
//! - **P6** — the same header, then raw binary samples.
//!
//! Maximum sample values from 1 to 65535, so 16-bit files decode (scaled down to 8 bits per
//! channel). PPM has no alpha channel, so every decoded pixel is fully opaque.
//!
//! The greyscale and bitmap cousins (P1, P2, P4, P5) are refused *by name*, because "this is a
//! greyscale PGM, save it as PPM or PNG" is a fixable message and "unknown format" is not.

use crate::{DecodeError, PixelFormat, TextureData};

/// What [`DecodeError`] calls this format.
const FORMAT: &str = "PPM";

/// Decodes a Netpbm colour image (P3 or P6).
///
/// # Errors
///
/// [`DecodeError::Malformed`] for a truncated or unparsable file, [`DecodeError::Unsupported`] for a
/// Netpbm variant this does not read.
pub fn decode_ppm(bytes: &[u8], file: &str) -> Result<TextureData, DecodeError> {
    let mut reader = Reader {
        bytes,
        pos: 0,
        file,
    };

    let magic = reader.next_token()?;
    let binary = match magic.as_slice() {
        b"P3" => false,
        b"P6" => true,
        b"P1" | b"P4" => {
            return Err(unsupported(
                file,
                "a black-and-white PBM, which holds one bit per pixel rather than colour. \
                 Re-export it as a colour PPM (P3 or P6) or as a PNG",
            ));
        }
        b"P2" | b"P5" => {
            return Err(unsupported(
                file,
                "a greyscale PGM, which holds one channel rather than three. \
                 Re-export it as a colour PPM (P3 or P6) or as a PNG",
            ));
        }
        other => {
            return Err(malformed(
                file,
                format!(
                    "it starts with `{}`, which is not a Netpbm magic number. \
                     A colour PPM starts with `P3` or `P6`",
                    String::from_utf8_lossy(other)
                ),
            ));
        }
    };

    let width = reader.next_number("width")?;
    let height = reader.next_number("height")?;
    let max_value = reader.next_number("maximum sample value")?;

    if max_value == 0 || max_value > 65535 {
        return Err(malformed(
            file,
            format!(
                "the maximum sample value is {max_value}, and Netpbm allows only 1 to 65535. \
                 The line after the dimensions is usually `255`"
            ),
        ));
    }
    // Two bytes per sample above 255, one byte at or below it. Only P6 cares -- P3 writes every
    // sample as text regardless.
    let wide_samples = max_value > 255;

    // `usize` for the buffer, `u64` for the count: a header claiming 100000x100000 would overflow a
    // 32-bit multiply into a small, believable number, and then the length check would pass.
    let pixel_count = u64::from(width) * u64::from(height);
    let sample_count = pixel_count.checked_mul(3).ok_or_else(|| {
        malformed(
            file,
            format!("the header claims {width}x{height}, which is more pixels than can be counted"),
        )
    })?;

    let samples: Vec<u16> = if binary {
        reader.read_binary_samples(sample_count, wide_samples)?
    } else {
        reader.read_text_samples(sample_count)?
    };

    // PPM stores red, green and blue with no alpha, so alpha is filled in as fully opaque. Samples
    // are rescaled to 0..=255 when the file used a different maximum; at the usual 255 the
    // multiply-and-divide is exact and leaves every value untouched.
    let mut pixels = Vec::with_capacity(samples.len() / 3 * 4);
    for channel in samples.chunks_exact(3) {
        for sample in channel {
            pixels.push(rescale(*sample, max_value));
        }
        pixels.push(255);
    }

    TextureData::new(
        file,
        FORMAT,
        width,
        height,
        PixelFormat::Rgba8UnormSrgb,
        pixels,
    )
}

/// Maps a sample from `0..=max_value` onto `0..=255`.
///
/// Done in `u32` so a 16-bit sample times 255 cannot wrap.
fn rescale(sample: u16, max_value: u32) -> u8 {
    if max_value == 255 {
        // The overwhelmingly common case, and exact -- worth skipping the arithmetic so a normal
        // file cannot be perturbed by rounding.
        return sample.min(255) as u8;
    }
    let scaled = u32::from(sample).min(max_value) * 255 / max_value;
    scaled as u8
}

/// Walks a Netpbm file, skipping whitespace and `#` comments the way the spec requires.
///
/// A tiny hand-written tokeniser rather than `split_whitespace`, because comments may appear
/// *between any two tokens* — including in the middle of the dimensions line — and because P6 needs
/// to know the exact byte offset where the header stopped.
struct Reader<'a> {
    bytes: &'a [u8],
    pos: usize,
    file: &'a str,
}

impl<'a> Reader<'a> {
    /// Advances past any run of whitespace and comments.
    ///
    /// A comment runs from `#` to the end of its line. The spec allows one anywhere whitespace is
    /// allowed, which is exactly how `placeholder.ppm` carries its explanation.
    fn skip_padding(&mut self) {
        while self.pos < self.bytes.len() {
            match self.bytes[self.pos] {
                b'#' => {
                    while self.pos < self.bytes.len() && self.bytes[self.pos] != b'\n' {
                        self.pos += 1;
                    }
                }
                byte if byte.is_ascii_whitespace() => self.pos += 1,
                _ => return,
            }
        }
    }

    /// The next run of non-whitespace bytes.
    fn next_token(&mut self) -> Result<Vec<u8>, DecodeError> {
        self.skip_padding();
        let start = self.pos;
        while self.pos < self.bytes.len() && !self.bytes[self.pos].is_ascii_whitespace() {
            self.pos += 1;
        }
        if start == self.pos {
            return Err(malformed(
                self.file,
                "the file ends before its header does".to_string(),
            ));
        }
        Ok(self.bytes[start..self.pos].to_vec())
    }

    /// The next token, parsed as a decimal number.
    ///
    /// `what` names the field so the message says which number was wrong rather than just that one
    /// was.
    fn next_number(&mut self, what: &str) -> Result<u32, DecodeError> {
        let token = self.next_token()?;
        let text = String::from_utf8_lossy(&token);
        text.parse::<u32>().map_err(|_| {
            malformed(
                self.file,
                format!("the {what} should be a whole number, but the file says `{text}`"),
            )
        })
    }

    /// Reads `wanted` samples written as decimal text (P3).
    fn read_text_samples(&mut self, wanted: u64) -> Result<Vec<u16>, DecodeError> {
        let mut samples = Vec::with_capacity(wanted.min(1 << 24) as usize);
        for index in 0..wanted {
            self.skip_padding();
            if self.pos >= self.bytes.len() {
                return Err(self.truncated(index, wanted));
            }
            let value = self.next_number("colour sample")?;
            samples.push(value.min(u32::from(u16::MAX)) as u16);
        }
        Ok(samples)
    }

    /// Reads `wanted` samples as raw bytes (P6).
    ///
    /// The spec says the header is followed by **exactly one** whitespace character and then binary
    /// data. So this consumes one byte rather than skipping a run — a run would eat the first sample
    /// whenever it happened to be a byte that looks like a space or a newline.
    fn read_binary_samples(&mut self, wanted: u64, wide: bool) -> Result<Vec<u16>, DecodeError> {
        if self.pos < self.bytes.len() && self.bytes[self.pos].is_ascii_whitespace() {
            self.pos += 1;
        }

        let stride = if wide { 2 } else { 1 };
        let available = (self.bytes.len() - self.pos) as u64 / stride;
        if available < wanted {
            return Err(self.truncated(available, wanted));
        }

        let mut samples = Vec::with_capacity(wanted.min(1 << 24) as usize);
        for _ in 0..wanted {
            if wide {
                // Netpbm is big-endian for 16-bit samples.
                let high = u16::from(self.bytes[self.pos]);
                let low = u16::from(self.bytes[self.pos + 1]);
                samples.push((high << 8) | low);
                self.pos += 2;
            } else {
                samples.push(u16::from(self.bytes[self.pos]));
                self.pos += 1;
            }
        }
        Ok(samples)
    }

    /// The "this file is shorter than its header promised" error, with both counts.
    ///
    /// Reported in *pixels* as well as samples, because a header that says 4x4 and a body that holds
    /// twelve pixels' worth of numbers is a miscount a human can find, and "36 of 48 samples" is not
    /// how anyone thinks about it.
    fn truncated(&self, got: u64, wanted: u64) -> DecodeError {
        malformed(
            self.file,
            format!(
                "its header promises {wanted} colour samples ({} pixels) but the file holds only \
                 {got}. Either the dimensions are wrong or the pixel data is incomplete",
                wanted / 3
            ),
        )
    }
}

/// A [`DecodeError::Malformed`] for this format.
fn malformed(file: &str, detail: String) -> DecodeError {
    DecodeError::Malformed {
        file: file.to_string(),
        format: FORMAT,
        detail,
    }
}

/// A [`DecodeError::Unsupported`] for this format.
fn unsupported(file: &str, detail: &str) -> DecodeError {
    DecodeError::Unsupported {
        file: file.to_string(),
        format: FORMAT,
        detail: detail.to_string(),
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn a_two_by_two_ascii_ppm_decodes_to_the_pixels_it_names() {
        let file = b"P3\n2 2\n255\n1 2 3  4 5 6\n7 8 9  10 11 12\n";
        let texture = decode_ppm(file, "swatch.ppm").expect("valid");

        assert_eq!((texture.width, texture.height), (2, 2));
        assert_eq!(texture.pixel(0, 0), Some([1, 2, 3, 255]));
        assert_eq!(texture.pixel(1, 0), Some([4, 5, 6, 255]));
        assert_eq!(texture.pixel(0, 1), Some([7, 8, 9, 255]));
        assert_eq!(texture.pixel(1, 1), Some([10, 11, 12, 255]));
    }

    #[test]
    fn comments_may_appear_anywhere_whitespace_may() {
        // This is what lets `placeholder.ppm` explain itself in the file. The awkward case is a
        // comment *between* the two dimensions, which the spec allows and a naive line-based parser
        // gets wrong.
        let file = b"P3\n# leading\n2 # between the dimensions\n1\n# before the max\n255\n\
                     1 2 3 # trailing\n4 5 6\n";
        let texture = decode_ppm(file, "commented.ppm").expect("valid");

        assert_eq!((texture.width, texture.height), (2, 1));
        assert_eq!(texture.pixel(1, 0), Some([4, 5, 6, 255]));
    }

    #[test]
    fn the_committed_placeholder_decodes_to_its_check_pattern() {
        // The real asset, read from disk. If this ever fails, either the parser broke or someone
        // edited a file whose whole purpose is being unmistakable on screen.
        let path = concat!(
            env!("CARGO_MANIFEST_DIR"),
            "/../../assets/textures/placeholder.ppm"
        );
        let bytes = std::fs::read(path).expect("the placeholder is committed");
        let texture = decode_ppm(&bytes, "placeholder").expect("valid");

        assert_eq!((texture.width, texture.height), (4, 4));
        // Magenta and near-black alternating, as the file's own comment describes.
        assert_eq!(texture.pixel(0, 0), Some([230, 0, 230, 255]));
        assert_eq!(texture.pixel(1, 0), Some([26, 26, 31, 255]));
        assert_eq!(texture.pixel(0, 1), Some([26, 26, 31, 255]));
    }

    #[test]
    fn the_committed_wall_texture_decodes_too() {
        let path = concat!(
            env!("CARGO_MANIFEST_DIR"),
            "/../../assets/textures/wall_concrete.ppm"
        );
        let bytes = std::fs::read(path).expect("committed");
        let texture = decode_ppm(&bytes, "wall_concrete").expect("valid");

        assert_eq!((texture.width, texture.height), (2, 2));
        assert_eq!(texture.pixel(0, 0), Some([92, 90, 86, 255]));
    }

    #[test]
    fn a_binary_p6_decodes_to_the_same_pixels_as_its_ascii_twin() {
        // The two forms are the same image written two ways, so they must agree exactly. This is
        // also what pins the "exactly one whitespace byte after the header" rule: skipping a run
        // instead would swallow the first sample here, since it is 0x0A.
        let ascii = decode_ppm(b"P3 2 1 255 10 20 30 40 50 60", "a.ppm").expect("valid");

        let mut binary = b"P6 2 1 255\n".to_vec();
        binary.extend_from_slice(&[10, 20, 30, 40, 50, 60]);
        let decoded = decode_ppm(&binary, "b.ppm").expect("valid");

        assert_eq!(ascii, decoded);
    }

    #[test]
    fn a_binary_sample_that_looks_like_whitespace_is_not_eaten() {
        // 0x20 is a space, and is also a perfectly ordinary dark grey. A parser that skipped a run
        // of whitespace after the header would lose it and shift every later pixel.
        let mut file = b"P6 1 1 255\n".to_vec();
        file.extend_from_slice(&[0x20, 0x09, 0x0A]);
        let texture = decode_ppm(&file, "spacey.ppm").expect("valid");

        assert_eq!(texture.pixel(0, 0), Some([0x20, 0x09, 0x0A, 255]));
    }

    #[test]
    fn a_sixteen_bit_file_is_scaled_down_to_eight() {
        // maxval 65535, so the samples are two bytes each and full-scale must land on 255 rather
        // than wrapping to 0.
        let file = b"P3 1 1 65535 65535 32767 0";
        let texture = decode_ppm(file, "deep.ppm").expect("valid");

        let pixel = texture.pixel(0, 0).expect("one pixel");
        assert_eq!(pixel[0], 255);
        assert_eq!(pixel[2], 0);
        assert!((126..=128).contains(&pixel[1]), "got {pixel:?}");
    }

    #[test]
    fn an_unusual_maximum_is_rescaled() {
        let texture = decode_ppm(b"P3 1 1 100 100 50 0", "scaled.ppm").expect("valid");
        let pixel = texture.pixel(0, 0).expect("one pixel");
        assert_eq!(pixel[0], 255);
        assert_eq!(pixel[2], 0);
        assert!((126..=128).contains(&pixel[1]), "got {pixel:?}");
    }

    #[test]
    fn a_greyscale_pgm_is_refused_by_name() {
        // The message has to say what the file *is* and what to do, because "unknown format" for a
        // file that is plainly an image is the least useful answer available.
        let error = decode_ppm(b"P2 1 1 255 128", "grey.pgm").expect_err("greyscale");
        let message = error.to_string();

        assert!(message.contains("greyscale"), "{message}");
        assert!(message.contains("Re-export"), "{message}");
        assert!(message.contains("grey.pgm"), "{message}");
    }

    #[test]
    fn a_black_and_white_pbm_is_refused_by_name() {
        let error = decode_ppm(b"P1 1 1 1", "mask.pbm").expect_err("bitmap");
        assert!(error.to_string().contains("one bit per pixel"), "{error}");
    }

    #[test]
    fn a_truncated_file_says_how_many_pixels_are_missing() {
        // Two pixels promised, one delivered.
        let error = decode_ppm(b"P3 2 1 255 1 2 3", "short.ppm").expect_err("truncated");
        let message = error.to_string();

        assert!(message.contains("6 colour samples"), "{message}");
        assert!(message.contains("2 pixels"), "{message}");
    }

    #[test]
    fn a_truncated_binary_file_is_caught_before_indexing_past_the_end() {
        let mut file = b"P6 4 4 255\n".to_vec();
        file.extend_from_slice(&[1, 2, 3]);
        let error = decode_ppm(&file, "short.ppm").expect_err("truncated");
        assert!(error.to_string().contains("48 colour samples"), "{error}");
    }

    #[test]
    fn a_header_that_stops_early_is_a_message_not_a_panic() {
        let error = decode_ppm(b"P3\n2", "stub.ppm").expect_err("no height");
        assert!(error.to_string().contains("ends before"), "{error}");
    }

    #[test]
    fn a_non_numeric_dimension_names_which_one_was_wrong() {
        let error = decode_ppm(b"P3\nwide 2\n255\n", "odd.ppm").expect_err("not a number");
        let message = error.to_string();
        assert!(message.contains("width"), "{message}");
        assert!(message.contains("`wide`"), "{message}");
    }

    #[test]
    fn a_zero_maximum_is_refused_rather_than_dividing_by_it() {
        let error = decode_ppm(b"P3 1 1 0 0 0 0", "flat.ppm").expect_err("zero max");
        assert!(error.to_string().contains("1 to 65535"), "{error}");
    }

    #[test]
    fn a_zero_sized_image_is_refused() {
        let error = decode_ppm(b"P3 0 0 255", "nothing.ppm").expect_err("no area");
        assert!(error.to_string().contains("no area"), "{error}");
    }

    #[test]
    fn decoding_is_reproducible() {
        // Invariant I3 does not reach here -- ADR 0021 keeps assets out of the state hash -- but a
        // decoder that gave two answers would still be a defect, and the check is free.
        let file = b"P3 2 2 255 1 2 3 4 5 6 7 8 9 10 11 12";
        assert_eq!(
            decode_ppm(file, "x.ppm").expect("valid"),
            decode_ppm(file, "x.ppm").expect("valid")
        );
    }
}
