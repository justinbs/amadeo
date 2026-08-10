//! Radiance `.hdr` decoding: images whose pixels can be brighter than white.
//!
//! # Why an ordinary image will not do
//!
//! A `.png` stores each channel in a byte, so its brightest possible pixel is white. That is fine for
//! a texture describing *what colour a surface is* and useless for one describing *the light falling
//! on a scene* — because the sun is thousands of times brighter than the sky beside it, and a format
//! that clips them both to white cannot tell them apart.
//!
//! Image-based lighting (ADR 0049) needs the difference. A chrome ball under a clipped sky reflects a
//! uniform grey ceiling; under a real one it reflects a small blazing sun and a soft gradient, which
//! is the entire visual difference between "lit" and "lit convincingly".
//!
//! # The format, and why it is this one
//!
//! Radiance RGBE stores three mantissa bytes and **one shared exponent byte**, so four bytes hold a
//! range of about `10^38` instead of `0..1`. It is the oldest and simplest high-dynamic-range format,
//! it is what almost every free environment map is distributed as, and its decoder is about two
//! hundred lines with no dependencies — which is the whole reason it was chosen over OpenEXR, whose
//! specification is enormous and whose reference implementation is a large C++ library.
//!
//! The cost is honest: RGBE has one exponent for all three channels, so a pixel that is bright red
//! and very slightly blue loses precision in the blue. For lighting information, which is what this
//! is for, that has never mattered enough to notice.

use crate::DecodeError;

/// An image whose pixels are light rather than colour.
///
/// Deliberately **not** a [`TextureData`](crate::TextureData). That type is bytes plus a format tag,
/// built for the sprite and material path — one image, one upload, four bytes a pixel. An
/// environment map has a different life entirely: it is decoded, projected onto a cube, convolved
/// twice at several resolutions, and only then uploaded, and it is floating point throughout.
///
/// Forcing both through one type would mean every consumer of `TextureData` growing a branch for a
/// case it never sees. The same argument that gave `insert_static_mesh` its own door in
/// `PhysicsBackend` rather than widening an existing one.
#[derive(Debug, Clone, PartialEq)]
pub struct HdrImage {
    /// Width in pixels. Never zero.
    pub width: u32,
    /// Height in pixels. Never zero.
    pub height: u32,
    /// Linear RGB and alpha, row by row from the top. Values may exceed 1.0 — that is the point.
    ///
    /// Alpha is always 1.0: RGBE carries no transparency, and an environment has nothing to be
    /// transparent against. It is here so the layout matches everything else the GPU is handed.
    pub pixels: Vec<[f32; 4]>,
}

impl HdrImage {
    /// A solid image of one colour, for tests and for a generated sky.
    #[must_use]
    pub fn solid(width: u32, height: u32, colour: [f32; 4]) -> HdrImage {
        HdrImage {
            width,
            height,
            pixels: vec![colour; (width as usize) * (height as usize)],
        }
    }

    /// The pixel at a coordinate, or `None` outside the image.
    #[must_use]
    pub fn pixel(&self, x: u32, y: u32) -> Option<[f32; 4]> {
        if x >= self.width || y >= self.height {
            return None;
        }
        self.pixels.get((y * self.width + x) as usize).copied()
    }
}

/// `2^n`, built exactly.
///
/// Written by placing `n` in a float's exponent field rather than calling `exp2` or `powi`, because
/// powers of two are exactly representable and this way says so. Neither of those is *guaranteed*
/// correctly rounded, and while that would not matter for pixels (ADR 0044's ban is on anything
/// deciding gameplay state), getting an exact answer here costs one line.
fn exp2i(n: i32) -> f32 {
    if n < -126 {
        return 0.0;
    }
    if n > 127 {
        return f32::INFINITY;
    }
    // A float's exponent field is biased by 127, and a zero mantissa means exactly 1.0 times the
    // exponent — so this is 2^n and nothing else.
    f32::from_bits(((n + 127) as u32) << 23)
}

/// Turns one RGBE quadruple into linear floating-point RGB.
///
/// The exponent is shared by all three channels and biased by 128. A zero exponent is the format's
/// spelling of black, and is a special case rather than a very small number.
fn rgbe_to_linear(rgbe: [u8; 4]) -> [f32; 4] {
    if rgbe[3] == 0 {
        return [0.0, 0.0, 0.0, 1.0];
    }
    // 128 for the exponent bias, plus 8 because the mantissa bytes are 0..255 rather than 0..1.
    let scale = exp2i(i32::from(rgbe[3]) - (128 + 8));
    // The half is Radiance's own rounding: a mantissa byte represents the middle of its bucket
    // rather than the bottom, which removes a systematic darkening across the whole image.
    [
        (f32::from(rgbe[0]) + 0.5) * scale,
        (f32::from(rgbe[1]) + 0.5) * scale,
        (f32::from(rgbe[2]) + 0.5) * scale,
        1.0,
    ]
}

/// Decodes a Radiance `.hdr` file.
///
/// # Errors
///
/// [`DecodeError::Malformed`] if the header, the resolution line or the pixel data is broken, and
/// [`DecodeError::Unsupported`] for a valid file this decoder does not handle — a non-RGBE `FORMAT`,
/// or a scanline order other than the usual top-to-bottom, left-to-right.
pub fn decode_hdr(bytes: &[u8], file: &str) -> Result<HdrImage, DecodeError> {
    let malformed = |detail: String| DecodeError::Malformed {
        file: file.to_string(),
        format: "Radiance HDR",
        detail,
    };
    let unsupported = |detail: String| DecodeError::Unsupported {
        file: file.to_string(),
        format: "Radiance HDR",
        detail,
    };

    let mut at = 0usize;
    let mut line = || -> Option<String> {
        let start = at;
        while at < bytes.len() && bytes[at] != b'\n' {
            at += 1;
        }
        if at >= bytes.len() {
            return None;
        }
        let text = String::from_utf8_lossy(&bytes[start..at]).into_owned();
        at += 1;
        Some(text)
    };

    let Some(signature) = line() else {
        return Err(malformed("the file has no header at all".to_string()));
    };
    if !signature.starts_with("#?") {
        return Err(malformed(format!(
            "expected a `#?RADIANCE` signature on the first line, found `{signature}`"
        )));
    }

    // Header lines until a blank one. The only setting that matters is FORMAT; the rest (EXPOSURE,
    // SOFTWARE, comments) are metadata this decoder has no use for.
    let mut format_line: Option<String> = None;
    loop {
        let Some(text) = line() else {
            return Err(malformed(
                "the header never ended -- there is no blank line before the resolution"
                    .to_string(),
            ));
        };
        if text.trim().is_empty() {
            break;
        }
        if let Some(value) = text.strip_prefix("FORMAT=") {
            format_line = Some(value.trim().to_string());
        }
    }

    // `32-bit_rle_rgbe` is the ordinary one. `32-bit_rle_xyze` is the same encoding holding CIE XYZ
    // instead of RGB, which would need a colour-space conversion nothing here does -- refused by
    // name rather than silently decoded as though it were RGB, which would come out wrongly tinted.
    match format_line.as_deref() {
        Some("32-bit_rle_rgbe") => {}
        Some(other) => {
            return Err(unsupported(format!(
                "its FORMAT is `{other}`; this decoder reads `32-bit_rle_rgbe` only"
            )));
        }
        None => {
            return Err(malformed(
                "the header declares no FORMAT, so there is no way to know how the pixels are \
                 encoded"
                    .to_string(),
            ));
        }
    }

    let Some(resolution) = line() else {
        return Err(malformed(
            "the file ends before its resolution line".to_string(),
        ));
    };
    // `-Y height +X width` is the standard orientation: rows top to bottom, columns left to right.
    // The other seven combinations are legal and essentially unused, and supporting them without a
    // file to test against would be writing code that has never been run.
    let parts: Vec<&str> = resolution.split_whitespace().collect();
    let (height, width) = match parts.as_slice() {
        ["-Y", height, "+X", width] => (
            height.parse::<u32>().map_err(|_| {
                malformed(format!("`{height}` in the resolution line is not a number"))
            })?,
            width.parse::<u32>().map_err(|_| {
                malformed(format!("`{width}` in the resolution line is not a number"))
            })?,
        ),
        _ => {
            return Err(unsupported(format!(
                "its resolution line is `{resolution}`; this decoder reads `-Y <height> +X <width>` \
                 only, which is what every ordinary .hdr file uses"
            )));
        }
    };

    if width == 0 || height == 0 {
        return Err(malformed(format!(
            "the image is {width}x{height}, and an image with no area cannot be used"
        )));
    }

    let mut pixels = Vec::with_capacity((width as usize) * (height as usize));
    let data = &bytes[at..];
    let mut cursor = 0usize;

    for row in 0..height {
        let mut scanline = vec![[0u8; 4]; width as usize];
        read_scanline(data, &mut cursor, &mut scanline, width, row, &malformed)?;
        pixels.extend(scanline.into_iter().map(rgbe_to_linear));
    }

    Ok(HdrImage {
        width,
        height,
        pixels,
    })
}

/// Reads one scanline, in whichever of the two encodings it uses.
fn read_scanline(
    data: &[u8],
    cursor: &mut usize,
    scanline: &mut [[u8; 4]],
    width: u32,
    row: u32,
    malformed: &impl Fn(String) -> DecodeError,
) -> Result<(), DecodeError> {
    let short = || malformed(format!("the pixel data ends part way through row {row}"));

    if *cursor + 4 > data.len() {
        return Err(short());
    }
    let header = [
        data[*cursor],
        data[*cursor + 1],
        data[*cursor + 2],
        data[*cursor + 3],
    ];

    // The "new" RLE encoding announces itself with 2, 2 and a big-endian width that matches the
    // image. Anything else is the old flat encoding -- including, legitimately, an image narrower
    // than 8 or wider than 32767, which the new encoding cannot express.
    let declared = (u32::from(header[2]) << 8) | u32::from(header[3]);
    let is_rle =
        header[0] == 2 && header[1] == 2 && declared == width && (8..32768).contains(&width);

    if !is_rle {
        return read_flat_scanline(data, cursor, scanline, malformed);
    }
    *cursor += 4;

    // Each of the four channels is run-length encoded across the *whole* scanline, one after
    // another -- so red for every pixel, then green, and so on. That is why this cannot simply
    // decode pixel by pixel.
    for channel in 0..4 {
        let mut x = 0usize;
        while x < scanline.len() {
            if *cursor >= data.len() {
                return Err(short());
            }
            let count = data[*cursor];
            *cursor += 1;

            if count > 128 {
                // A run: repeat the next byte this many times.
                let run = usize::from(count) - 128;
                if *cursor >= data.len() {
                    return Err(short());
                }
                let value = data[*cursor];
                *cursor += 1;
                if x + run > scanline.len() {
                    return Err(malformed(format!(
                        "a run in row {row} claims {run} pixels, which overruns the scanline"
                    )));
                }
                for pixel in &mut scanline[x..x + run] {
                    pixel[channel] = value;
                }
                x += run;
            } else {
                // A literal stretch: copy this many bytes straight across. A count of zero would
                // make no progress and loop forever, so it is refused rather than skipped.
                let run = usize::from(count);
                if run == 0 {
                    return Err(malformed(format!(
                        "row {row} contains a zero-length run, which cannot be decoded"
                    )));
                }
                if *cursor + run > data.len() {
                    return Err(short());
                }
                if x + run > scanline.len() {
                    return Err(malformed(format!(
                        "a literal in row {row} claims {run} pixels, which overruns the scanline"
                    )));
                }
                for offset in 0..run {
                    scanline[x + offset][channel] = data[*cursor + offset];
                }
                *cursor += run;
                x += run;
            }
        }
    }
    Ok(())
}

/// The old encoding: RGBE quadruples straight through, with one rare run marker.
fn read_flat_scanline(
    data: &[u8],
    cursor: &mut usize,
    scanline: &mut [[u8; 4]],
    malformed: &impl Fn(String) -> DecodeError,
) -> Result<(), DecodeError> {
    let mut x = 0usize;
    // How many times the previous pixel has been repeated in a row, which the old format's run
    // marker shifts left by for each consecutive marker.
    let mut shift = 0u32;

    while x < scanline.len() {
        if *cursor + 4 > data.len() {
            return Err(malformed(
                "the pixel data ends part way through a scanline".to_string(),
            ));
        }
        let quad = [
            data[*cursor],
            data[*cursor + 1],
            data[*cursor + 2],
            data[*cursor + 3],
        ];
        *cursor += 4;

        // 1,1,1,n means "repeat the previous pixel n times". Consecutive markers extend the count
        // by another eight bits each, which is how the old format expressed long runs.
        if quad[0] == 1 && quad[1] == 1 && quad[2] == 1 {
            if x == 0 {
                return Err(malformed(
                    "a scanline begins with a repeat marker, but there is no previous pixel to \
                     repeat"
                        .to_string(),
                ));
            }
            let run = (usize::from(quad[3])) << shift;
            let previous = scanline[x - 1];
            let end = (x + run).min(scanline.len());
            for pixel in &mut scanline[x..end] {
                *pixel = previous;
            }
            x = end;
            shift += 8;
        } else {
            scanline[x] = quad;
            x += 1;
            shift = 0;
        }
    }
    Ok(())
}

/// Encodes an [`HdrImage`] as a Radiance `.hdr` file.
///
/// Uncompressed — every scanline uses the old flat encoding, which is legal, universally readable,
/// and about fifteen lines. Compression would save space on a file a generator writes once and
/// nothing distributes.
///
/// Exists because the demos generate their skies rather than shipping a downloaded environment map:
/// `games/scarp`'s grass texture is already produced this way (`bin/turf`), and a sky that is
/// committed as the code that makes it stays readable and diffable (invariant I1).
#[must_use]
pub fn encode_hdr(image: &HdrImage) -> Vec<u8> {
    let mut out = Vec::new();
    out.extend_from_slice(b"#?RADIANCE\n");
    out.extend_from_slice(b"FORMAT=32-bit_rle_rgbe\n\n");
    out.extend_from_slice(format!("-Y {} +X {}\n", image.height, image.width).as_bytes());

    for pixel in &image.pixels {
        out.extend_from_slice(&linear_to_rgbe(*pixel));
    }
    out
}

/// The inverse of [`rgbe_to_linear`]: three mantissas and a shared exponent.
fn linear_to_rgbe(colour: [f32; 4]) -> [u8; 4] {
    let peak = colour[0].max(colour[1]).max(colour[2]);
    // Too dim to express, or not a number at all. Both become black, and NaN is checked explicitly
    // rather than left to fall through a comparison: the exponent search below is two `while` loops
    // driven by comparisons, and a value that compares false against everything would leave them
    // with an exponent of zero and produce a plausible-looking wrong pixel instead of an obvious
    // black one. Encoders are exactly where a NaN from upstream arithmetic first becomes visible.
    if peak.is_nan() || peak < 1e-32 {
        return [0, 0, 0, 0];
    }

    // The exponent is chosen so the brightest channel lands just under 256 -- the most precision the
    // three mantissa bytes can carry for this pixel.
    let mut exponent = 0i32;
    let mut scaled = peak;
    while scaled >= 1.0 {
        scaled /= 2.0;
        exponent += 1;
    }
    while scaled < 0.5 {
        scaled *= 2.0;
        exponent -= 1;
    }

    let scale = 256.0 / exp2i(exponent);
    [
        (colour[0] * scale).clamp(0.0, 255.0) as u8,
        (colour[1] * scale).clamp(0.0, 255.0) as u8,
        (colour[2] * scale).clamp(0.0, 255.0) as u8,
        (exponent + 128).clamp(0, 255) as u8,
    ]
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn powers_of_two_are_exact() {
        assert_eq!(exp2i(0), 1.0);
        assert_eq!(exp2i(1), 2.0);
        assert_eq!(exp2i(-1), 0.5);
        assert_eq!(exp2i(10), 1024.0);
        assert_eq!(exp2i(-10), 1.0 / 1024.0);
    }

    #[test]
    fn a_value_brighter_than_white_survives_a_round_trip() {
        // **The whole reason this format exists.** A `.png` would clip every one of these to 1.0,
        // and with them the difference between a sky and the sun in it.
        let bright = HdrImage {
            width: 3,
            height: 1,
            pixels: vec![
                [0.5, 0.5, 0.5, 1.0],
                [12.0, 8.0, 4.0, 1.0],
                [400.0, 380.0, 350.0, 1.0],
            ],
        };

        let encoded = encode_hdr(&bright);
        let back = decode_hdr(&encoded, "bright.hdr").expect("round trips");

        assert_eq!((back.width, back.height), (3, 1));
        for (index, (before, after)) in bright.pixels.iter().zip(&back.pixels).enumerate() {
            // **The tolerance is relative to the pixel's brightest channel, not to each channel**,
            // and that is what RGBE actually guarantees rather than a convenience. One exponent is
            // shared by all three, so it is chosen for the brightest — and a dim channel beside a
            // bright one is quantised in steps sized for the bright one. In `[12, 8, 4]` the blue
            // comes back as 4.03125: 0.8% of itself, but 0.3% of the twelve that set the scale.
            //
            // This is the format's documented trade-off, not slack in the implementation. Anything
            // needing better would need OpenEXR, whose cost the module header weighs.
            let peak = before[0].max(before[1]).max(before[2]);
            for channel in 0..3 {
                let error = (before[channel] - after[channel]).abs();
                assert!(
                    error <= peak / 256.0 + 1e-4,
                    "pixel {index} channel {channel}: {} came back as {}, which is further off \
                     than one step of a {peak}-scaled exponent",
                    before[channel],
                    after[channel]
                );
            }
        }
    }

    #[test]
    fn black_round_trips_exactly() {
        // Zero is the format's one special case -- a zero exponent rather than a very small number.
        let black = HdrImage::solid(2, 2, [0.0, 0.0, 0.0, 1.0]);
        let back = decode_hdr(&encode_hdr(&black), "black.hdr").expect("round trips");
        assert_eq!(back.pixels, black.pixels);
    }

    #[test]
    fn the_run_length_encoding_decodes_to_the_same_pixels_as_the_flat_one() {
        // This engine's encoder writes flat scanlines, so without this the RLE path -- which is what
        // every real .hdr file downloaded from anywhere actually uses -- would have no coverage at
        // all. Built by hand here, in the format's own layout.
        let width = 16usize;
        let mut file = Vec::new();
        file.extend_from_slice(b"#?RADIANCE\nFORMAT=32-bit_rle_rgbe\n\n-Y 1 +X 16\n");
        // The new RLE scanline header: 2, 2, then the width big-endian.
        file.extend_from_slice(&[2, 2, 0, 16]);
        // Each channel across the whole scanline: a run of 16 identical bytes.
        for value in [200u8, 100, 50, 128] {
            file.push(128 + 16); // a run of sixteen
            file.push(value);
        }

        let decoded = decode_hdr(&file, "rle.hdr").expect("valid RLE");
        assert_eq!(decoded.pixels.len(), width);

        let expected = rgbe_to_linear([200, 100, 50, 128]);
        for pixel in &decoded.pixels {
            assert_eq!(*pixel, expected);
        }
    }

    #[test]
    fn a_zero_length_run_is_refused_rather_than_looping_forever() {
        // A malformed file must not hang the loader. A count of zero makes no progress, so without
        // this check the decoder spins on it until something else kills the process.
        let mut file = Vec::new();
        file.extend_from_slice(b"#?RADIANCE\nFORMAT=32-bit_rle_rgbe\n\n-Y 1 +X 16\n");
        file.extend_from_slice(&[2, 2, 0, 16]);
        file.push(0);

        let error = decode_hdr(&file, "bad.hdr").expect_err("a zero run is not decodable");
        assert!(
            error.to_string().contains("zero-length run"),
            "the message should name the problem, got: {error}"
        );
    }

    #[test]
    fn a_different_colour_space_is_refused_by_name() {
        // `xyze` is a real and legal Radiance variant holding CIE XYZ. Decoding it as though it were
        // RGB would produce a picture that is wrong in a way nobody would attribute to the decoder.
        let file = b"#?RADIANCE\nFORMAT=32-bit_rle_xyze\n\n-Y 1 +X 1\n";
        let error = decode_hdr(file, "xyz.hdr").expect_err("xyze is not supported");
        assert!(
            error.to_string().contains("32-bit_rle_xyze"),
            "the message should name the format it found, got: {error}"
        );
    }

    #[test]
    fn something_that_is_not_an_hdr_says_so() {
        // Two shapes of wrong, because they fail at different points and a reader needs to be told
        // which. A file with a first line that is not the signature is *probably an image in some
        // other format*; a file with no line ending at all is probably truncated or binary junk.
        let wrong_signature =
            decode_hdr(b"not an image at all\nmore text\n", "wrong.hdr").expect_err("not an HDR");
        assert!(
            wrong_signature.to_string().contains("#?RADIANCE"),
            "the message should name the signature it wanted, got: {wrong_signature}"
        );

        let no_lines = decode_hdr(b"\x00\x01\x02", "junk.hdr").expect_err("not an HDR");
        assert!(
            no_lines.to_string().contains("no header"),
            "the message should say the header is missing, got: {no_lines}"
        );
    }
}
