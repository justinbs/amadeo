//! Turning an image file's bytes into the flat pixel grid a GPU can accept.
//!
//! ```
//! use amadeo_image::{PixelFormat, decode};
//!
//! // A 1x1 white pixel, as plain ASCII PPM.
//! let file = b"P3\n1 1\n255\n255 255 255\n";
//! let texture = decode(file, "white.ppm").expect("valid PPM");
//!
//! assert_eq!((texture.width, texture.height), (1, 1));
//! assert_eq!(texture.format, PixelFormat::Rgba8UnormSrgb);
//! assert_eq!(texture.pixels, vec![255, 255, 255, 255]);
//! ```
//!
//! # Where this sits, and why it is its own crate
//!
//! `amadeo-assets` deliberately reads bytes and stops there — decoding is per-format knowledge and it
//! did not want any. This crate is where that knowledge went. It depends on **no engine crate at
//! all**, so it sits at the bottom of the graph beside `amadeo-derive` and cannot participate in a
//! cycle (invariant I6).
//!
//! Keeping it separate is also what stops the `png` dependency spreading: `amadeo-scene`,
//! `amadeo-agent`, `amadeo-app` and the CLI never pull it in, because they never ask for pixels.
//!
//! # Decoding happens at load time, for now — ADR 0026
//!
//! Every mature engine compiles source art into an internal format at *import* time and never opens
//! a `.png` at runtime. That is where Amadeo is going too, and the reason is concrete rather than
//! tidy: GPU-compressed formats such as BC7 take **seconds to minutes per texture** to encode, so
//! they can only ever be produced offline.
//!
//! What makes it safe to decode at load time in the meantime is [`PixelFormat`]. Because the runtime
//! carries an explicit format tag from the first line of code, an import pipeline that emits BC7 is
//! a *new variant and a new producer*, not a redesign of everything that consumes a texture. The
//! expensive part of the decision is in the type; the pipeline is schedulable.
//!
//! # Nothing here can move a replay
//!
//! ADR 0021: gameplay holds an asset id and never observes an asset's state. Decoding is downstream
//! of that rule — it happens on the rendering side, from an `amadeo_ecs::Service` that
//! `World::state_hash` excludes by trait bound. (Named rather than linked: this crate deliberately
//! depends on no engine crate, so there is nothing here to link to.) A texture that fails to decode
//! changes what is on screen and nothing else.

mod png_format;
mod ppm;

pub use png_format::{EncodeError, decode_png, encode_png};
pub use ppm::decode_ppm;

/// How the bytes in a [`TextureData`] are laid out.
///
/// # Why this exists when there is only one variant
///
/// It is the extension point ADR 0026 turns on, and it is deliberately here before it is needed.
/// Adding GPU-compressed textures later means adding a variant and a producer; without the tag it
/// would mean changing the loader, the texture cache, the backend, and every test that asserts on
/// pixels. The tag costs nothing now and is the one part of this design that is expensive to
/// retrofit.
///
/// What will be added, and what will drive it:
///
/// - `Rgba8Unorm` — the same bytes read as *linear* rather than gamma-encoded. Wanted the first time
///   a texture carries data rather than colour (a normal map, a mask). Driven by a `color_space`
///   setting in the `.ama-meta` sidecar.
/// - `Bc7` / `Astc…` — GPU-compressed. Only ever produced by an import pipeline, never by [`decode`].
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub enum PixelFormat {
    /// Four bytes per pixel, red-green-blue-alpha, gamma-encoded (sRGB).
    ///
    /// What every source image decodes to, because art files hold gamma-encoded colour. The GPU
    /// converts to linear when sampling, which is why the surface is configured sRGB too.
    Rgba8UnormSrgb,
}

impl PixelFormat {
    /// How many bytes one pixel occupies.
    #[must_use]
    pub fn bytes_per_pixel(self) -> u32 {
        match self {
            PixelFormat::Rgba8UnormSrgb => 4,
        }
    }
}

/// A decoded image: dimensions, a format, and the pixels.
///
/// Flat and owned on purpose — this is handed to a GPU upload, which wants one contiguous slice and
/// no indirection.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct TextureData {
    /// Width in pixels. Never zero; a decoder rejects a zero-sized image.
    pub width: u32,
    /// Height in pixels. Never zero.
    pub height: u32,
    /// How to read [`TextureData::pixels`].
    pub format: PixelFormat,
    /// The pixels, row by row from the top, with no padding between rows.
    ///
    /// Length is always `width * height * format.bytes_per_pixel()`, which
    /// [`TextureData::new`] checks so no consumer has to.
    pub pixels: Vec<u8>,
}

impl TextureData {
    /// Builds a texture, checking that the pixel data matches the dimensions.
    ///
    /// Every decoder in this crate goes through here, so "the buffer is the right size" is
    /// established once rather than trusted at each upload site.
    ///
    /// # Errors
    ///
    /// [`DecodeError::Malformed`] if either dimension is zero or the buffer is the wrong length.
    pub fn new(
        file: &str,
        format_name: &'static str,
        width: u32,
        height: u32,
        format: PixelFormat,
        pixels: Vec<u8>,
    ) -> Result<TextureData, DecodeError> {
        if width == 0 || height == 0 {
            return Err(DecodeError::Malformed {
                file: file.to_string(),
                format: format_name,
                detail: format!(
                    "the image is {width}x{height}, and an image with no area cannot be uploaded \
                     as a texture"
                ),
            });
        }

        // `u64` throughout: a 65535x65535 image overflows `u32` at four bytes per pixel, and an
        // overflow here would turn a huge image into a plausible-looking wrong length.
        let expected = u64::from(width) * u64::from(height) * u64::from(format.bytes_per_pixel());
        if pixels.len() as u64 != expected {
            return Err(DecodeError::Malformed {
                file: file.to_string(),
                format: format_name,
                detail: format!(
                    "a {width}x{height} image needs {expected} bytes of pixel data, but the decoder \
                     produced {}",
                    pixels.len()
                ),
            });
        }

        Ok(TextureData {
            width,
            height,
            format,
            pixels,
        })
    }

    /// The four bytes of one pixel, or `None` if the coordinate is outside the image.
    ///
    /// For tests and diagnostics. The render path uploads the whole buffer and never indexes it.
    #[must_use]
    pub fn pixel(&self, x: u32, y: u32) -> Option<[u8; 4]> {
        if x >= self.width || y >= self.height {
            return None;
        }
        let stride = self.format.bytes_per_pixel();
        let start = ((y * self.width + x) * stride) as usize;
        let bytes = self.pixels.get(start..start + 4)?;
        Some([bytes[0], bytes[1], bytes[2], bytes[3]])
    }
}

/// Why an image could not be decoded.
///
/// **Never fatal to the engine.** ADR 0021 requires a broken asset to produce a visible stand-in and
/// a structured report rather than a crash, because an agent's only eyes are what the renderer says
/// it drew. Every message names the file, because "invalid PNG" with no filename is unactionable to
/// a human and to an agent alike.
///
/// The field is `file` rather than `source` because `thiserror` treats a field called `source` as
/// the underlying error — the same reason `amadeo_assets::LoadFailure` avoids the name.
#[derive(Debug, Clone, PartialEq, Eq, thiserror::Error)]
pub enum DecodeError {
    /// The leading bytes match no format this crate knows.
    #[error(
        "`{file}` is not an image this engine can read: it starts with {leading}.\n\
         Supported formats are PNG (.png) and PPM (.ppm, both ASCII P3 and binary P6)"
    )]
    UnknownFormat {
        /// The file the bytes came from.
        file: String,
        /// The first few bytes, rendered readably, so a mis-named or truncated file is obvious.
        leading: String,
    },

    /// The format was recognised but the contents are broken.
    #[error("`{file}` is not a valid {format} file: {detail}")]
    Malformed {
        /// The file the bytes came from.
        file: String,
        /// Which format it was read as.
        format: &'static str,
        /// What specifically was wrong.
        detail: String,
    },

    /// The format was recognised and valid, but uses a feature this decoder does not handle.
    ///
    /// Kept separate from [`DecodeError::Malformed`] because the fix is completely different: a
    /// malformed file is corrupt, an unsupported one needs re-exporting with different settings.
    #[error("`{file}` is a valid {format} file, but {detail}")]
    Unsupported {
        /// The file the bytes came from.
        file: String,
        /// Which format it was read as.
        format: &'static str,
        /// What is not handled, and what to do instead.
        detail: String,
    },
}

/// The eight bytes every PNG file begins with.
const PNG_SIGNATURE: [u8; 8] = [0x89, b'P', b'N', b'G', 0x0D, 0x0A, 0x1A, 0x0A];

/// Decodes an image, choosing the format from its leading bytes.
///
/// `file` is used only for error messages, and should be whatever the reader would recognise — an
/// asset id or a path.
///
/// # Why the format comes from the bytes and not the extension
///
/// A file's name is a claim; its contents are a fact. Sniffing means a `.png` that is secretly a
/// JPEG produces "this is not a format I can read" rather than "invalid PNG", and it means the
/// decoder does not need the filename to be correct in order to work. Both matter more here than in
/// most engines, because assets are addressed by **id** (ADR 0020) and the path is bookkeeping that
/// an author is explicitly allowed to change.
///
/// # Errors
///
/// [`DecodeError`], always naming `file`.
/// Builds the full chain of progressively halved copies of an image — its **mip levels**.
///
/// Level 0 is the original; each level after it is half the width and half the height, rounded down
/// and never below one pixel, ending at 1×1. A 256×256 image gives nine levels.
///
/// # What this is for
///
/// A texture drawn smaller than its pixel count **shimmers**: as the camera moves, each screen pixel
/// lands on a different, unrelated texel, and the surface crawls with noise. Mip levels are the
/// standard fix — the GPU picks a level roughly matching how small the surface is on screen and
/// samples that instead.
///
/// Until this existed the backend uploaded one level, so `games/scarp`'s terrain tile had to be a
/// coarse eight metres to keep the noise tolerable. ADR 0045 puts this first on M3's renderer list
/// for that reason: it is what caps how fine any texture is allowed to be.
///
/// # Averaging happens in linear light, and that is the whole subtlety
///
/// [`PixelFormat::Rgba8UnormSrgb`] means the stored bytes are **sRGB-encoded** — a perceptual curve,
/// not a measurement of light. Averaging those bytes directly is averaging the wrong quantity, and
/// the result is visibly too dark: it is the classic mipmap bug, and it shows as a texture that
/// darkens as it recedes.
///
/// So each level decodes to linear, averages four pixels, and re-encodes. Alpha is **not** encoded
/// that way — it is already linear coverage — so it is averaged directly.
///
/// # `powf` is used here and is forbidden in a `TerrainSource`
///
/// ADR 0044 bans transcendentals from anything deciding gameplay state, because their precision
/// varies by platform. The sRGB curve needs one, and it is safe *here* for the same reason the
/// `turf` generator's is: this runs at load, its output is pixels, and nothing about a simulation
/// depends on it. A mip level differing in its last bit between two machines changes a shade of
/// green, not where the ground is.
#[must_use]
pub fn mip_chain(texture: &TextureData) -> Vec<TextureData> {
    let mut levels = vec![texture.clone()];

    while let Some(previous) = levels.last() {
        if previous.width == 1 && previous.height == 1 {
            break;
        }
        levels.push(halve(previous));
    }
    levels
}

/// One level of [`mip_chain`]: half the size, averaged in linear light.
fn halve(source: &TextureData) -> TextureData {
    let width = (source.width / 2).max(1);
    let height = (source.height / 2).max(1);
    let srgb = source.format == PixelFormat::Rgba8UnormSrgb;
    let mut pixels = Vec::with_capacity((width * height * 4) as usize);

    for y in 0..height {
        for x in 0..width {
            // The four source pixels this one covers. `min` rather than an assumption, because an
            // odd-sized level has a last row and column with no partner — sampling the same pixel
            // twice is a better answer than reading past the edge.
            let x0 = x * 2;
            let y0 = y * 2;
            let x1 = (x0 + 1).min(source.width - 1);
            let y1 = (y0 + 1).min(source.height - 1);

            let corners = [
                source.pixel(x0, y0),
                source.pixel(x1, y0),
                source.pixel(x0, y1),
                source.pixel(x1, y1),
            ];

            let mut total = [0.0_f32; 4];
            let mut counted = 0.0_f32;
            for corner in corners.into_iter().flatten() {
                for channel in 0..3 {
                    let value = f32::from(corner[channel]) / 255.0;
                    total[channel] += if srgb { srgb_to_linear(value) } else { value };
                }
                // Alpha is coverage, never gamma-encoded, so it averages as it is.
                total[3] += f32::from(corner[3]) / 255.0;
                counted += 1.0;
            }

            let scale = if counted == 0.0 { 1.0 } else { counted };
            for colour in &total[..3] {
                let average = colour / scale;
                let encoded = if srgb {
                    linear_to_srgb(average)
                } else {
                    average
                };
                pixels.push(to_byte(encoded));
            }
            pixels.push(to_byte(total[3] / scale));
        }
    }

    TextureData {
        width,
        height,
        format: source.format,
        pixels,
    }
}

/// The sRGB transfer curve, decoding a stored value to linear light.
fn srgb_to_linear(value: f32) -> f32 {
    if value <= 0.040_45 {
        value / 12.92
    } else {
        ((value + 0.055) / 1.055).powf(2.4)
    }
}

/// The same curve in the other direction.
fn linear_to_srgb(value: f32) -> f32 {
    if value <= 0.003_130_8 {
        value * 12.92
    } else {
        1.055 * value.powf(1.0 / 2.4) - 0.055
    }
}

/// Rounds a 0..1 value to a byte, clamped so a rounding overshoot cannot wrap to zero.
fn to_byte(value: f32) -> u8 {
    (value.clamp(0.0, 1.0) * 255.0).round() as u8
}

/// Decodes an image, choosing the format from its leading bytes.
///
/// `file` is used only for error messages, and should be whatever the reader would recognise — an
/// asset id or a path.
///
/// # Why the format comes from the bytes and not the extension
///
/// A file's name is a claim; its contents are a fact. Sniffing means a `.png` that is secretly a
/// JPEG produces "this is not a format I can read" rather than "invalid PNG", and it means the
/// decoder does not need the filename to be correct in order to work. Both matter more here than in
/// most engines, because assets are addressed by **id** (ADR 0020) and the path is bookkeeping that
/// an author is explicitly allowed to change.
///
/// # Errors
///
/// [`DecodeError`], always naming `file`.
pub fn decode(bytes: &[u8], file: &str) -> Result<TextureData, DecodeError> {
    if bytes.starts_with(&PNG_SIGNATURE) {
        return decode_png(bytes, file);
    }
    // PPM's magic is two ASCII characters: P3 is the whitespace-separated text form, P6 the binary
    // one. The other Netpbm magics (P1/P2/P4/P5) are bitmaps and greyscale, which are handled inside
    // the PPM module so the error can say what they are rather than "unknown".
    if bytes.starts_with(b"P") {
        return decode_ppm(bytes, file);
    }

    Err(DecodeError::UnknownFormat {
        file: file.to_string(),
        leading: describe_leading_bytes(bytes),
    })
}

/// Renders the first few bytes of a file readably, for the "this is not an image" message.
///
/// Printable ASCII is shown as itself and everything else as hex, so both a text file that arrived
/// by mistake and a binary one are recognisable at a glance.
fn describe_leading_bytes(bytes: &[u8]) -> String {
    if bytes.is_empty() {
        return "nothing at all (the file is empty)".to_string();
    }

    let mut rendered = String::from("`");
    for byte in bytes.iter().take(8) {
        if byte.is_ascii_graphic() || *byte == b' ' {
            rendered.push(*byte as char);
        } else {
            rendered.push_str(&format!("\\x{byte:02x}"));
        }
    }
    rendered.push('`');
    rendered
}

#[cfg(test)]
mod tests {
    use super::*;

    /// A 1x1 PNG holding one RGBA pixel, built in memory so no fixture has to be committed.
    ///
    /// Encoding with `png` to test a decoder built on `png` would be circular if this crate owned
    /// both ends. It does not: what is under test here is Amadeo's wrapper — signature sniffing,
    /// colour-type normalisation, and error shape — not `png`'s own correctness.
    fn one_pixel_png(rgba: [u8; 4]) -> Vec<u8> {
        let mut out = Vec::new();
        {
            let mut encoder = png::Encoder::new(&mut out, 1, 1);
            encoder.set_color(png::ColorType::Rgba);
            encoder.set_depth(png::BitDepth::Eight);
            let mut writer = encoder.write_header().expect("header");
            writer.write_image_data(&rgba).expect("data");
        }
        out
    }

    #[test]
    fn a_png_is_recognised_by_its_signature_not_its_name() {
        // Built in memory rather than read from disk, so the test does not depend on a fixture.
        let encoded = one_pixel_png([12, 34, 56, 255]);
        let texture = decode(&encoded, "not-called-png-at-all").expect("decodes");

        assert_eq!((texture.width, texture.height), (1, 1));
        assert_eq!(texture.pixel(0, 0), Some([12, 34, 56, 255]));
    }

    #[test]
    fn a_ppm_is_recognised_by_its_magic() {
        let texture = decode(b"P3 1 1 255 1 2 3", "swatch").expect("decodes");
        assert_eq!(texture.pixel(0, 0), Some([1, 2, 3, 255]));
    }

    #[test]
    fn something_that_is_not_an_image_says_so_readably() {
        let error = decode(b"{\"not\": \"an image\"}", "config.json").expect_err("not an image");
        let message = error.to_string();

        assert!(message.contains("config.json"), "{message}");
        // The leading bytes are shown, so a file that arrived by mistake is identifiable.
        assert!(message.contains("{\"not\""), "{message}");
        // And the message says what *would* work.
        assert!(message.contains("PNG"), "{message}");
        assert!(message.contains("PPM"), "{message}");
    }

    #[test]
    fn an_empty_file_is_a_clear_message_rather_than_an_index_panic() {
        let error = decode(b"", "empty.png").expect_err("empty");
        assert!(error.to_string().contains("empty"), "{error}");
    }

    #[test]
    fn binary_leading_bytes_are_rendered_as_hex() {
        // A JPEG, which starts 0xFF 0xD8 0xFF. Not supported, and the message should show why
        // rather than printing control characters into a terminal.
        let error = decode(&[0xFF, 0xD8, 0xFF, 0xE0], "photo.jpg").expect_err("unsupported");
        assert!(error.to_string().contains("\\xff"), "{error}");
    }

    #[test]
    fn a_texture_must_match_its_dimensions() {
        // The check that means no upload site has to repeat it.
        let error = TextureData::new("x", "PPM", 2, 2, PixelFormat::Rgba8UnormSrgb, vec![0; 4])
            .expect_err("wrong length");
        let message = error.to_string();
        assert!(message.contains("needs 16 bytes"), "{message}");
        assert!(message.contains("produced 4"), "{message}");
    }

    #[test]
    fn a_zero_sized_image_is_refused() {
        let error = TextureData::new("x", "PPM", 0, 4, PixelFormat::Rgba8UnormSrgb, Vec::new())
            .expect_err("no area");
        assert!(error.to_string().contains("0x4"), "{error}");
    }

    #[test]
    fn pixel_lookups_outside_the_image_return_none() {
        let texture = decode(b"P3 1 1 255 9 9 9", "one").expect("decodes");
        assert!(texture.pixel(0, 0).is_some());
        assert_eq!(texture.pixel(1, 0), None);
        assert_eq!(texture.pixel(0, 1), None);
    }

    #[test]
    fn a_format_knows_its_own_stride() {
        assert_eq!(PixelFormat::Rgba8UnormSrgb.bytes_per_pixel(), 4);
    }

    /// A solid image of one colour, at a given size.
    fn solid(width: u32, height: u32, colour: [u8; 4]) -> TextureData {
        TextureData {
            width,
            height,
            format: PixelFormat::Rgba8UnormSrgb,
            pixels: colour.repeat((width * height) as usize),
        }
    }

    #[test]
    fn a_chain_halves_all_the_way_down_to_one_pixel() {
        let levels = mip_chain(&solid(256, 256, [10, 20, 30, 255]));

        // 256, 128, 64, 32, 16, 8, 4, 2, 1.
        assert_eq!(levels.len(), 9);
        assert_eq!((levels[0].width, levels[0].height), (256, 256));
        assert_eq!((levels[8].width, levels[8].height), (1, 1));
        for level in &levels {
            assert_eq!(
                level.pixels.len(),
                (level.width * level.height * 4) as usize,
                "level {}x{} has the wrong number of bytes",
                level.width,
                level.height
            );
        }
    }

    #[test]
    fn a_non_square_image_still_reaches_one_by_one() {
        // Each axis halves independently and stops at 1, so a wide image keeps halving its width
        // after its height has bottomed out. Getting this wrong produces a chain that never
        // terminates, which is an infinite loop rather than a wrong picture.
        let levels = mip_chain(&solid(8, 2, [255, 255, 255, 255]));
        let sizes: Vec<(u32, u32)> = levels.iter().map(|l| (l.width, l.height)).collect();
        assert_eq!(sizes, vec![(8, 2), (4, 1), (2, 1), (1, 1)]);
    }

    #[test]
    fn a_solid_colour_survives_being_halved() {
        // The sharpest check that the sRGB round trip is not drifting: averaging four identical
        // pixels must give that pixel back. A decode/encode pair that disagreed would show here as
        // a colour that slides a little darker at every level.
        let levels = mip_chain(&solid(64, 64, [200, 100, 50, 255]));
        for level in &levels {
            assert_eq!(
                level.pixel(0, 0),
                Some([200, 100, 50, 255]),
                "a {}x{} level drifted off the original colour",
                level.width,
                level.height
            );
        }
    }

    #[test]
    fn black_and_white_average_to_the_perceptual_middle_not_the_byte_middle() {
        // **The whole reason this is not four lines.** Half black and half white is, in *light*,
        // 0.5 linear — which sRGB encodes as about 188, not 128. Averaging the stored bytes gives
        // 128, a colour noticeably darker than the surface it came from, and that is the classic
        // mipmap bug: textures that dim as they recede.
        let mut checker = solid(2, 1, [0, 0, 0, 255]);
        checker.pixels[4..8].copy_from_slice(&[255, 255, 255, 255]);

        let levels = mip_chain(&checker);
        let averaged = levels[1].pixel(0, 0).expect("a 1x1 level");

        assert!(
            (186..=190).contains(&averaged[0]),
            "expected about 188 (linear 0.5 re-encoded), got {}; \
             averaging sRGB bytes directly would give 128",
            averaged[0]
        );
        assert_eq!(
            averaged[3], 255,
            "alpha is linear already and must not shift"
        );
    }

    #[test]
    fn alpha_averages_without_the_curve() {
        // Alpha is coverage, not light, and is never gamma-encoded. Half opaque and half clear is
        // exactly half — 128, the byte middle — which is the one channel where that *is* right.
        let mut half = solid(2, 1, [255, 255, 255, 0]);
        half.pixels[4..8].copy_from_slice(&[255, 255, 255, 255]);

        let levels = mip_chain(&half);
        let averaged = levels[1].pixel(0, 0).expect("a 1x1 level");
        assert!(
            (126..=130).contains(&averaged[3]),
            "alpha should average to about 128, got {}",
            averaged[3]
        );
    }

    #[test]
    fn a_one_pixel_image_is_its_own_only_level() {
        let levels = mip_chain(&solid(1, 1, [1, 2, 3, 4]));
        assert_eq!(levels.len(), 1);
    }
}
