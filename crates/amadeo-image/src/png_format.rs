//! PNG, via the `png` crate — and the normalisation that makes every PNG look the same downstream.
//!
//! # What this module is actually for
//!
//! `png` does the decoding. This module exists because a PNG file can be any of a dozen shapes —
//! 1-, 2-, 4-, 8- or 16-bit samples; greyscale, greyscale-with-alpha, RGB, RGBA, or a palette; with
//! or without a transparency chunk — and a GPU upload wants exactly one. Every one of those becomes
//! 8-bit RGBA here, so nothing above this line has to know PNG has variants at all.
//!
//! Most of the flattening is `png`'s own transformation flags. The rest is the two cases it
//! deliberately does not do (`GRAY_TO_RGB` is listed as unimplemented in its source), which are
//! handled below by hand.
//!
//! # Why a dependency here and nowhere else — ADR 0026
//!
//! PNG image data is zlib/DEFLATE-compressed, so a hand-written decoder means a hand-written
//! inflate. That is a different class of thing from the PCG32 and FNV-1a this project *did* hand-roll:
//! those are ~100 lines each with published test vectors, and a mistake in one shows up immediately
//! as a wrong known answer. A mistake in inflate shows up as *slightly corrupt pixels*, which is
//! exactly the failure a Rust-learning maintainer should never have to chase.

use crate::{DecodeError, PixelFormat, TextureData};

/// What [`DecodeError`] calls this format.
const FORMAT: &str = "PNG";

/// Decodes a PNG into 8-bit RGBA.
///
/// For an animated PNG (APNG) this reads the first frame, which is also what a still-image viewer
/// shows. Animation is a `amadeo-anim` concern and is not smuggled in through the texture loader.
///
/// # Errors
///
/// [`DecodeError::Malformed`] for a corrupt or truncated file, [`DecodeError::Unsupported`] for one
/// whose decoded shape this cannot flatten.
pub fn decode_png(bytes: &[u8], file: &str) -> Result<TextureData, DecodeError> {
    // `Cursor` because `png::Decoder` wants a seekable reader and a plain byte slice is not one.
    // Nothing is copied — the cursor just carries a position alongside the borrowed slice.
    let mut decoder = png::Decoder::new(std::io::Cursor::new(bytes));

    // Ask the decoder to do as much of the flattening as it can:
    //   EXPAND    -- palette entries become real colours, and sub-8-bit greyscale becomes 8-bit.
    //                Also turns a tRNS transparency chunk into a real alpha channel.
    //   ALPHA     -- add an alpha channel where the image has none.
    //   STRIP_16  -- 16-bit samples down to 8, since the GPU format is 8-bit.
    decoder.set_transformations(
        png::Transformations::EXPAND | png::Transformations::ALPHA | png::Transformations::STRIP_16,
    );

    let mut reader = decoder
        .read_info()
        .map_err(|error| from_png_error(file, &error))?;

    let buffer_size = reader
        .output_buffer_size()
        .ok_or_else(|| DecodeError::Unsupported {
            file: file.to_string(),
            format: FORMAT,
            detail:
                "its decoded size does not fit in memory on this platform. Re-export it smaller"
                    .to_string(),
        })?;

    let mut buffer = vec![0; buffer_size];
    let info = reader
        .next_frame(&mut buffer)
        .map_err(|error| from_png_error(file, &error))?;

    // After STRIP_16 every sample should be a byte. Checked rather than assumed, because silently
    // reading 16-bit data as 8-bit produces a plausible-looking wrong image.
    if info.bit_depth != png::BitDepth::Eight {
        return Err(DecodeError::Unsupported {
            file: file.to_string(),
            format: FORMAT,
            detail: format!(
                "it decoded to {:?} samples rather than 8-bit ones, which this engine cannot upload",
                info.bit_depth
            ),
        });
    }

    // `next_frame` may leave the tail of the buffer untouched when the frame is smaller than the
    // buffer, so use the length it reports rather than the buffer's own.
    let decoded = &buffer[..info.buffer_size().min(buffer.len())];
    let pixels = to_rgba8(decoded, info.color_type, file)?;

    TextureData::new(
        file,
        FORMAT,
        info.width,
        info.height,
        PixelFormat::Rgba8UnormSrgb,
        pixels,
    )
}

/// Widens whatever channel layout came out of the decoder into four bytes per pixel.
///
/// `EXPAND | ALPHA` normally lands on `Rgba` directly. The one that does not is **greyscale**: the
/// `png` crate has no grey-to-RGB transformation, so a greyscale PNG arrives as one or two channels
/// and is spread across R, G and B here.
fn to_rgba8(
    decoded: &[u8],
    color_type: png::ColorType,
    file: &str,
) -> Result<Vec<u8>, DecodeError> {
    let widened = match color_type {
        // Already the target layout.
        png::ColorType::Rgba => decoded.to_vec(),

        // Three channels, no alpha. Reached only if `ALPHA` had nothing to do.
        png::ColorType::Rgb => {
            let mut out = Vec::with_capacity(decoded.len() / 3 * 4);
            for pixel in decoded.chunks_exact(3) {
                out.extend_from_slice(pixel);
                out.push(255);
            }
            out
        }

        // Grey plus alpha. `png` does not widen grey to RGB, so this does.
        png::ColorType::GrayscaleAlpha => {
            let mut out = Vec::with_capacity(decoded.len() / 2 * 4);
            for pixel in decoded.chunks_exact(2) {
                let (grey, alpha) = (pixel[0], pixel[1]);
                out.extend_from_slice(&[grey, grey, grey, alpha]);
            }
            out
        }

        // Grey alone.
        png::ColorType::Grayscale => {
            let mut out = Vec::with_capacity(decoded.len() * 4);
            for grey in decoded {
                out.extend_from_slice(&[*grey, *grey, *grey, 255]);
            }
            out
        }

        // `EXPAND` turns a palette into real colours, so seeing one here means the transformation
        // did not apply — a real bug rather than a bad file, and worth saying so.
        png::ColorType::Indexed => {
            return Err(DecodeError::Unsupported {
                file: file.to_string(),
                format: FORMAT,
                detail: "it decoded as a palette even though palette expansion was requested. \
                         This is an engine bug rather than a problem with the file; please report \
                         it with the file attached"
                    .to_string(),
            });
        }
    };

    Ok(widened)
}

/// Turns a `png` error into one of ours, keeping its text and adding the filename.
///
/// The distinction that matters to a reader is corrupt-versus-unreadable, so a limits error becomes
/// [`DecodeError::Unsupported`] and everything else [`DecodeError::Malformed`].
fn from_png_error(file: &str, error: &png::DecodingError) -> DecodeError {
    match error {
        png::DecodingError::LimitsExceeded => DecodeError::Unsupported {
            file: file.to_string(),
            format: FORMAT,
            detail: "it is larger than the decoder's memory limits allow. Re-export it smaller"
                .to_string(),
        },
        other => DecodeError::Malformed {
            file: file.to_string(),
            format: FORMAT,
            detail: other.to_string(),
        },
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    /// Encodes an image with the `png` crate, so tests need no committed binary fixtures.
    ///
    /// The point of these tests is not that `png` decodes PNG — it does — but that **every colour
    /// type arrives above this module as the same four bytes per pixel**. So each case encodes a
    /// known picture in a different shape and asserts the same RGBA comes back.
    fn encode(
        width: u32,
        height: u32,
        color: png::ColorType,
        depth: png::BitDepth,
        data: &[u8],
        palette: Option<Vec<u8>>,
    ) -> Vec<u8> {
        let mut out = Vec::new();
        {
            let mut encoder = png::Encoder::new(&mut out, width, height);
            encoder.set_color(color);
            encoder.set_depth(depth);
            if let Some(palette) = palette {
                encoder.set_palette(palette);
            }
            let mut writer = encoder.write_header().expect("header");
            writer.write_image_data(data).expect("data");
        }
        out
    }

    #[test]
    fn an_rgba_png_round_trips_its_pixels() {
        let file = encode(
            2,
            1,
            png::ColorType::Rgba,
            png::BitDepth::Eight,
            &[10, 20, 30, 40, 50, 60, 70, 80],
            None,
        );
        let texture = decode_png(&file, "pair.png").expect("valid");

        assert_eq!((texture.width, texture.height), (2, 1));
        assert_eq!(texture.pixel(0, 0), Some([10, 20, 30, 40]));
        assert_eq!(texture.pixel(1, 0), Some([50, 60, 70, 80]));
    }

    #[test]
    fn an_rgb_png_gains_an_opaque_alpha_channel() {
        let file = encode(
            1,
            1,
            png::ColorType::Rgb,
            png::BitDepth::Eight,
            &[7, 8, 9],
            None,
        );
        let texture = decode_png(&file, "opaque.png").expect("valid");

        assert_eq!(texture.pixel(0, 0), Some([7, 8, 9, 255]));
    }

    #[test]
    fn a_greyscale_png_is_spread_across_the_colour_channels() {
        // The case `png` explicitly does not handle for us. Without the widening in `to_rgba8` this
        // would upload one channel of data as if it were four, and the image would be garbage.
        let file = encode(
            2,
            1,
            png::ColorType::Grayscale,
            png::BitDepth::Eight,
            &[0, 200],
            None,
        );
        let texture = decode_png(&file, "grey.png").expect("valid");

        assert_eq!(texture.pixel(0, 0), Some([0, 0, 0, 255]));
        assert_eq!(texture.pixel(1, 0), Some([200, 200, 200, 255]));
    }

    #[test]
    fn a_greyscale_png_with_alpha_keeps_its_alpha() {
        let file = encode(
            1,
            1,
            png::ColorType::GrayscaleAlpha,
            png::BitDepth::Eight,
            &[128, 64],
            None,
        );
        let texture = decode_png(&file, "greya.png").expect("valid");

        assert_eq!(texture.pixel(0, 0), Some([128, 128, 128, 64]));
    }

    #[test]
    fn a_paletted_png_is_expanded_to_real_colours() {
        // Indexed colour is what a pixel-art tool exports by default, so this is the common case
        // for the 2D target games rather than an exotic one.
        let palette = vec![255, 0, 0, 0, 0, 255];
        let file = encode(
            2,
            1,
            png::ColorType::Indexed,
            png::BitDepth::Eight,
            &[0, 1],
            Some(palette),
        );
        let texture = decode_png(&file, "sprite.png").expect("valid");

        assert_eq!(texture.pixel(0, 0), Some([255, 0, 0, 255]));
        assert_eq!(texture.pixel(1, 0), Some([0, 0, 255, 255]));
    }

    #[test]
    fn a_sixteen_bit_png_is_stripped_to_eight() {
        // Two bytes per sample in, one out. Full scale must land on 255.
        let file = encode(
            1,
            1,
            png::ColorType::Rgb,
            png::BitDepth::Sixteen,
            &[255, 255, 0, 0, 128, 0],
            None,
        );
        let texture = decode_png(&file, "deep.png").expect("valid");

        let pixel = texture.pixel(0, 0).expect("one pixel");
        assert_eq!(pixel[0], 255);
        assert_eq!(pixel[1], 0);
        assert_eq!(pixel[3], 255);
    }

    #[test]
    fn a_sub_byte_greyscale_png_is_expanded() {
        // One bit per pixel, which is what a mask or a font atlas is often exported as.
        let file = encode(
            8,
            1,
            png::ColorType::Grayscale,
            png::BitDepth::One,
            &[0b1010_1010],
            None,
        );
        let texture = decode_png(&file, "mask.png").expect("valid");

        assert_eq!(texture.width, 8);
        assert_eq!(texture.pixel(0, 0), Some([255, 255, 255, 255]));
        assert_eq!(texture.pixel(1, 0), Some([0, 0, 0, 255]));
    }

    #[test]
    fn every_colour_type_produces_four_bytes_per_pixel() {
        // The single property this module exists to provide, stated once over all of them: whatever
        // shape went in, what comes out is uploadable without asking any further questions.
        let cases: Vec<Vec<u8>> = vec![
            encode(
                1,
                1,
                png::ColorType::Rgba,
                png::BitDepth::Eight,
                &[1, 2, 3, 4],
                None,
            ),
            encode(
                1,
                1,
                png::ColorType::Rgb,
                png::BitDepth::Eight,
                &[1, 2, 3],
                None,
            ),
            encode(
                1,
                1,
                png::ColorType::Grayscale,
                png::BitDepth::Eight,
                &[1],
                None,
            ),
            encode(
                1,
                1,
                png::ColorType::GrayscaleAlpha,
                png::BitDepth::Eight,
                &[1, 2],
                None,
            ),
            encode(
                1,
                1,
                png::ColorType::Indexed,
                png::BitDepth::Eight,
                &[0],
                Some(vec![9, 9, 9]),
            ),
        ];

        for file in cases {
            let texture = decode_png(&file, "any.png").expect("valid");
            assert_eq!(texture.format, PixelFormat::Rgba8UnormSrgb);
            assert_eq!(texture.pixels.len(), 4, "one pixel must be four bytes");
        }
    }

    #[test]
    fn a_truncated_png_is_a_message_naming_the_file() {
        let mut file = encode(
            4,
            4,
            png::ColorType::Rgba,
            png::BitDepth::Eight,
            &[128; 64],
            None,
        );
        file.truncate(30);

        let error = decode_png(&file, "half-written.png").expect_err("truncated");
        assert!(error.to_string().contains("half-written.png"), "{error}");
    }

    #[test]
    fn a_file_with_a_png_signature_and_nothing_else_does_not_panic() {
        let error = decode_png(
            &[0x89, b'P', b'N', b'G', 0x0D, 0x0A, 0x1A, 0x0A],
            "stub.png",
        )
        .expect_err("no header chunk");
        assert!(error.to_string().contains("stub.png"), "{error}");
    }

    #[test]
    fn decoding_is_reproducible() {
        let file = encode(
            2,
            2,
            png::ColorType::Rgba,
            png::BitDepth::Eight,
            &[3; 16],
            None,
        );
        assert_eq!(
            decode_png(&file, "x.png").expect("valid"),
            decode_png(&file, "x.png").expect("valid")
        );
    }
}

/// Encodes 8-bit RGBA pixels as a PNG.
///
/// The counterpart to [`decode_png`], and the engine's only image *writer*. Added in session 8 for
/// `render.capture`: a captured frame has to leave the process somehow, and pixels in a JSON reply
/// would be megabytes of base64 for a screenshot.
///
/// # Why PNG rather than the PPM this crate already handles
///
/// PPM would have been a smaller change — the writer is twenty lines and needs no dependency at all.
/// But the whole point of a capture is that **a human opens it**, and nothing opens a PPM: not a file
/// browser, not a chat client, not a pull request. A capture nobody can look at is a capture that
/// does not do its job. The `png` crate is already here for decoding, so encoding costs no new
/// dependency — the reasoning in this module's header about not hand-rolling DEFLATE applies in
/// exactly the same way to writing it.
///
/// Lossless, so what comes back is what the GPU produced, bit for bit. That matters for a capture
/// used as evidence.
///
/// # Errors
///
/// [`EncodeError`] if the pixel buffer is the wrong length for the stated dimensions, or if the
/// encoder itself fails.
pub fn encode_png(image: &TextureData) -> Result<Vec<u8>, EncodeError> {
    let expected = (image.width as usize)
        .saturating_mul(image.height as usize)
        .saturating_mul(image.format.bytes_per_pixel() as usize);
    if image.pixels.len() != expected {
        return Err(EncodeError::WrongLength {
            width: image.width,
            height: image.height,
            expected,
            found: image.pixels.len(),
        });
    }

    let mut out = Vec::new();
    // Scoped so the encoder's writer is dropped — and the stream finished — before `out` is returned.
    {
        let mut encoder = png::Encoder::new(&mut out, image.width, image.height);
        encoder.set_color(png::ColorType::Rgba);
        encoder.set_depth(png::BitDepth::Eight);
        // The pixels are sRGB (`PixelFormat::Rgba8UnormSrgb`), and saying so is what makes a viewer
        // show the same colours the GPU produced rather than a slightly washed-out version.
        encoder.set_source_srgb(png::SrgbRenderingIntent::Perceptual);

        let mut writer = encoder
            .write_header()
            .map_err(|error| EncodeError::Failed(error.to_string()))?;
        writer
            .write_image_data(&image.pixels)
            .map_err(|error| EncodeError::Failed(error.to_string()))?;
        writer
            .finish()
            .map_err(|error| EncodeError::Failed(error.to_string()))?;
    }
    Ok(out)
}

/// What can go wrong writing an image.
#[derive(Debug, Clone, PartialEq, Eq, thiserror::Error)]
pub enum EncodeError {
    /// The pixel buffer does not match the stated dimensions.
    ///
    /// Its own variant rather than a generic failure because it means the *caller* built the
    /// `TextureData` wrongly, and the numbers say by how much — which is usually enough to spot a
    /// forgotten row-padding strip.
    #[error(
        "cannot encode a {width}x{height} image: that needs {expected} bytes of pixels, found {found}"
    )]
    WrongLength {
        /// Stated width.
        width: u32,
        /// Stated height.
        height: u32,
        /// How many bytes those dimensions require.
        expected: usize,
        /// How many were supplied.
        found: usize,
    },

    /// The encoder itself refused.
    #[error("the PNG encoder failed: {0}")]
    Failed(String),
}

#[cfg(test)]
mod encode_tests {
    use super::*;

    /// A small image with a distinct value in every channel of every pixel.
    fn sample() -> TextureData {
        let mut pixels = Vec::new();
        for y in 0..3u8 {
            for x in 0..4u8 {
                pixels.extend_from_slice(&[x * 60, y * 80, 255 - x * 60, 255 - y * 40]);
            }
        }
        TextureData::new(
            "sample.png",
            FORMAT,
            4,
            3,
            PixelFormat::Rgba8UnormSrgb,
            pixels,
        )
        .expect("well-formed")
    }

    #[test]
    fn an_encoded_png_decodes_back_to_the_same_pixels() {
        // The property that makes a capture usable as evidence: lossless, so what a test asserts on
        // is what the GPU produced. Decoded with this crate's *own* decoder, so the two halves are
        // checked against each other rather than against an assumption.
        let original = sample();
        let bytes = encode_png(&original).expect("encodes");
        let round_tripped = decode_png(&bytes, "round-trip.png").expect("decodes");

        assert_eq!(round_tripped.width, original.width);
        assert_eq!(round_tripped.height, original.height);
        assert_eq!(round_tripped.pixels, original.pixels);
    }

    #[test]
    fn the_output_is_a_real_png() {
        // The eight-byte signature every PNG starts with. Worth pinning: the whole reason for
        // choosing PNG over PPM is that other tools open it, which requires being one.
        let bytes = encode_png(&sample()).expect("encodes");
        assert_eq!(&bytes[..8], b"\x89PNG\r\n\x1a\n");
    }

    #[test]
    fn a_mismatched_pixel_buffer_says_by_how_much() {
        // The likely caller mistake is forgetting to strip a readback's row padding, which leaves
        // the buffer too *long* by a predictable amount — so the message gives both numbers.
        let mut broken = sample();
        broken.pixels.truncate(10);

        let error = encode_png(&broken).expect_err("too few pixels");
        let message = error.to_string();
        assert!(message.contains("4x3"), "{message}");
        assert!(message.contains("48"), "{message}");
        assert!(message.contains("10"), "{message}");
    }
}
