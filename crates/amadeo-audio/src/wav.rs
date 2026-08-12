//! Reading a `.wav` file into samples.
//!
//! # Why this is hand-written and why it is WAV
//!
//! `amadeo-image` sets the precedent exactly: a real format through a crate (PNG through `png`) and
//! a simple one written out (PPM). WAV is the simple one. It is a length-prefixed chunk format
//! wrapping raw samples, it needs no dependency, and every tool that makes audio can export it.
//!
//! **It does not live in its own crate**, unlike `amadeo-image`. That split exists because that crate
//! holds a non-`thiserror` dependency and the graph is cleaner with it isolated; this is a hundred
//! lines of byte-shuffling with no dependency at all, so a crate boundary would buy nothing.
//!
//! # What it deliberately does not do
//!
//! Compressed audio. Ogg Vorbis and MP3 are what a soundtrack ships as, and both are real decoders
//! rather than an afternoon — `symphonia` is the intended answer and `docs/02` already names it. A
//! compressed file here fails with a message that says which format it found, rather than producing
//! noise.
//!
//! Uncompressed is the right first step regardless: it is what short effects ship as, and it is what
//! makes the rest of the audio path testable without also trusting a decoder.

use crate::backend::SoundData;

/// What can go wrong reading a `.wav`.
#[derive(Debug, thiserror::Error, PartialEq, Eq)]
pub enum WavError {
    /// Not a RIFF/WAVE file at all.
    #[error("not a WAV file: expected a RIFF header, found {found:?}")]
    NotWav {
        /// The first four bytes, as text where they are text.
        found: String,
    },

    /// The file ends in the middle of something.
    #[error("the WAV file ends unexpectedly while reading {what}")]
    Truncated {
        /// What was being read.
        what: &'static str,
    },

    /// The audio is compressed, or otherwise not raw samples.
    ///
    /// Names the format code, because "unsupported" on its own tells nobody which converter setting
    /// to change.
    #[error(
        "this WAV holds format {code} ({name}); only uncompressed PCM and IEEE float are supported. \
         Re-export as 16-bit PCM, or wait for the compressed decoder"
    )]
    Unsupported {
        /// The `wFormatTag` from the `fmt ` chunk.
        code: u16,
        /// What that code means, where it is one of the common ones.
        name: &'static str,
    },

    /// A sample width this decoder does not read.
    #[error("{bits}-bit samples are not supported; use 16-bit PCM, 24-bit PCM or 32-bit float")]
    UnsupportedDepth {
        /// Bits per sample, from the `fmt ` chunk.
        bits: u16,
    },

    /// The file has no `fmt ` or no `data` chunk.
    #[error("the WAV file has no {chunk} chunk")]
    MissingChunk {
        /// Which one.
        chunk: &'static str,
    },
}

/// Uncompressed integer samples.
const FORMAT_PCM: u16 = 1;
/// Uncompressed floating-point samples.
const FORMAT_FLOAT: u16 = 3;
/// The wrapper Windows tools emit for anything over two channels or 16 bits. The real format code
/// sits inside the extension, and this decoder reads it from there.
const FORMAT_EXTENSIBLE: u16 = 0xFFFE;

/// Turns `.wav` bytes into samples.
///
/// Accepts 16-bit PCM, 24-bit PCM and 32-bit float, mono or stereo — which between them cover what
/// every audio tool exports by default.
///
/// # Errors
///
/// [`WavError`] if the bytes are not a WAV, end early, or hold audio in a format this does not read.
/// Every variant names what it found, because "could not load sound" is not a report anyone can act
/// on.
pub fn decode_wav(bytes: &[u8]) -> Result<SoundData, WavError> {
    let mut reader = Reader::new(bytes);

    // RIFF<size>WAVE. The size is the rest of the file and is not trusted — a file truncated in
    // transit has a size that says otherwise, and every read below is bounds-checked anyway.
    let riff = reader.tag("the RIFF header")?;
    if &riff != b"RIFF" {
        return Err(WavError::NotWav {
            found: String::from_utf8_lossy(&riff).to_string(),
        });
    }
    reader.u32("the RIFF size")?;
    let wave = reader.tag("the WAVE tag")?;
    if &wave != b"WAVE" {
        return Err(WavError::NotWav {
            found: String::from_utf8_lossy(&wave).to_string(),
        });
    }

    let mut format: Option<Format> = None;
    let mut samples: Option<Vec<f32>> = None;

    // Chunks in order, skipping the ones this does not care about. **Skipping rather than failing**
    // is the whole reason a chunk format has lengths: a `LIST` of authoring metadata is extremely
    // common and has nothing to do with the audio.
    while let Some((tag, size)) = reader.chunk_header() {
        match &tag {
            b"fmt " => format = Some(read_format(&mut reader, size)?),
            b"data" => {
                let Some(format) = format else {
                    // `data` before `fmt ` is legal by the spec and vanishingly rare. Reporting it as
                    // a missing chunk is honest: the format is missing *at the point it is needed*.
                    return Err(WavError::MissingChunk { chunk: "fmt " });
                };
                samples = Some(read_samples(&mut reader, size, format)?);
            }
            _ => reader.skip(size),
        }
        // Chunks are padded to an even length, and the pad byte is not counted in the size. Missing
        // this reads every subsequent chunk one byte out, which looks like a corrupt file.
        if size % 2 == 1 {
            reader.skip(1);
        }
    }

    let format = format.ok_or(WavError::MissingChunk { chunk: "fmt " })?;
    let samples = samples.ok_or(WavError::MissingChunk { chunk: "data" })?;

    Ok(SoundData {
        samples,
        channels: format.channels,
        sample_rate: format.sample_rate,
    })
}

/// The parts of a `fmt ` chunk that decide how to read the samples.
#[derive(Debug, Clone, Copy)]
struct Format {
    code: u16,
    channels: u16,
    sample_rate: u32,
    bits: u16,
}

fn read_format(reader: &mut Reader<'_>, size: u32) -> Result<Format, WavError> {
    let start = reader.at;
    let mut code = reader.u16("the format code")?;
    let channels = reader.u16("the channel count")?;
    let sample_rate = reader.u32("the sample rate")?;
    reader.u32("the byte rate")?;
    reader.u16("the block align")?;
    let bits = reader.u16("the bit depth")?;

    // WAVE_FORMAT_EXTENSIBLE puts the real code first in its extension. Without this, every file a
    // Windows tool exports above 16 bits is rejected as an unknown format.
    if code == FORMAT_EXTENSIBLE && size >= 40 {
        reader.u16("the extension size")?;
        reader.u16("the valid bits")?;
        reader.u32("the channel mask")?;
        code = reader.u16("the sub-format")?;
    }

    // Whatever is left of the chunk, so the next header is where it should be.
    let read = reader.at - start;
    reader.skip((size as usize).saturating_sub(read) as u32);

    if code != FORMAT_PCM && code != FORMAT_FLOAT {
        return Err(WavError::Unsupported {
            code,
            name: match code {
                2 => "Microsoft ADPCM",
                6 => "A-law",
                7 => "mu-law",
                0x0055 => "MP3",
                _ => "unknown",
            },
        });
    }

    Ok(Format {
        code,
        channels: channels.max(1),
        sample_rate,
        bits,
    })
}

fn read_samples(reader: &mut Reader<'_>, size: u32, format: Format) -> Result<Vec<f32>, WavError> {
    let bytes = reader.take(size as usize, "the sample data")?;

    // **Every branch normalises to −1.0..1.0**, which is what `SoundData` promises and what every
    // mixer expects. Dividing by the type's maximum rather than by a rounded power of two is the
    // detail worth getting right: 16-bit audio runs from −32768 to 32767, so dividing by 32768 keeps
    // the full negative swing without ever exceeding 1.0.
    let samples = match (format.code, format.bits) {
        (FORMAT_PCM, 16) => bytes
            .chunks_exact(2)
            .map(|pair| {
                let raw = i16::from_le_bytes([pair[0], pair[1]]);
                f32::from(raw) / 32768.0
            })
            .collect(),
        (FORMAT_PCM, 24) => bytes
            .chunks_exact(3)
            .map(|triple| {
                // Sign-extended by putting the three bytes in the *top* of an i32 and shifting back
                // down, which is far easier to get right than a conditional subtract.
                let raw = i32::from_le_bytes([0, triple[0], triple[1], triple[2]]) >> 8;
                raw as f32 / 8_388_608.0
            })
            .collect(),
        (FORMAT_FLOAT, 32) => bytes
            .chunks_exact(4)
            .map(|quad| f32::from_le_bytes([quad[0], quad[1], quad[2], quad[3]]))
            .collect(),
        (_, bits) => return Err(WavError::UnsupportedDepth { bits }),
    };
    Ok(samples)
}

/// A bounds-checked cursor over the file.
///
/// Every read returns a typed error rather than panicking, because a `.wav` is an *asset* — content
/// somebody exported — and a malformed one must name the problem rather than take the game down.
struct Reader<'a> {
    bytes: &'a [u8],
    at: usize,
}

impl<'a> Reader<'a> {
    fn new(bytes: &'a [u8]) -> Self {
        Self { bytes, at: 0 }
    }

    fn take(&mut self, count: usize, what: &'static str) -> Result<&'a [u8], WavError> {
        let end = self
            .at
            .checked_add(count)
            .ok_or(WavError::Truncated { what })?;
        let slice = self
            .bytes
            .get(self.at..end)
            .ok_or(WavError::Truncated { what })?;
        self.at = end;
        Ok(slice)
    }

    fn tag(&mut self, what: &'static str) -> Result<[u8; 4], WavError> {
        let slice = self.take(4, what)?;
        Ok([slice[0], slice[1], slice[2], slice[3]])
    }

    fn u16(&mut self, what: &'static str) -> Result<u16, WavError> {
        let slice = self.take(2, what)?;
        Ok(u16::from_le_bytes([slice[0], slice[1]]))
    }

    fn u32(&mut self, what: &'static str) -> Result<u32, WavError> {
        let slice = self.take(4, what)?;
        Ok(u32::from_le_bytes([slice[0], slice[1], slice[2], slice[3]]))
    }

    /// Saturating, so a chunk claiming a size past the end of the file leaves the cursor at the end
    /// and the loop terminates, rather than wrapping round to the start and reading forever.
    fn skip(&mut self, count: u32) {
        self.at = self.at.saturating_add(count as usize).min(self.bytes.len());
    }

    /// The next chunk's tag and size, or `None` at the end of the file.
    fn chunk_header(&mut self) -> Option<([u8; 4], u32)> {
        let tag = self.tag("a chunk header").ok()?;
        let size = self.u32("a chunk size").ok()?;
        Some((tag, size))
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    /// Builds a WAV in memory, so the tests need no fixture file and say what they are testing.
    fn wav(code: u16, bits: u16, channels: u16, rate: u32, data: &[u8]) -> Vec<u8> {
        let mut out = Vec::new();
        out.extend_from_slice(b"RIFF");
        out.extend_from_slice(&(36 + data.len() as u32).to_le_bytes());
        out.extend_from_slice(b"WAVE");

        out.extend_from_slice(b"fmt ");
        out.extend_from_slice(&16u32.to_le_bytes());
        out.extend_from_slice(&code.to_le_bytes());
        out.extend_from_slice(&channels.to_le_bytes());
        out.extend_from_slice(&rate.to_le_bytes());
        let block = u32::from(channels) * u32::from(bits / 8);
        out.extend_from_slice(&(rate * block).to_le_bytes());
        out.extend_from_slice(&(block as u16).to_le_bytes());
        out.extend_from_slice(&bits.to_le_bytes());

        out.extend_from_slice(b"data");
        out.extend_from_slice(&(data.len() as u32).to_le_bytes());
        out.extend_from_slice(data);
        out
    }

    #[test]
    fn sixteen_bit_pcm_decodes_to_normalised_samples() {
        // The full negative swing and the full positive one. Dividing by 32768 rather than 32767 is
        // what keeps -32768 at exactly -1.0 without letting 32767 exceed 1.0 — the other choice
        // clips the loudest positive sample of every file.
        let data: Vec<u8> = [i16::MIN, 0, i16::MAX]
            .iter()
            .flat_map(|value| value.to_le_bytes())
            .collect();
        let sound = decode_wav(&wav(FORMAT_PCM, 16, 1, 48_000, &data)).expect("decodes");

        assert_eq!(sound.channels, 1);
        assert_eq!(sound.sample_rate, 48_000);
        assert_eq!(sound.samples.len(), 3);
        assert!((sound.samples[0] + 1.0).abs() < 1e-6);
        assert!(sound.samples[1].abs() < 1e-6);
        assert!(sound.samples[2] < 1.0 && sound.samples[2] > 0.999);
    }

    #[test]
    fn thirty_two_bit_float_passes_straight_through() {
        let data: Vec<u8> = [-1.0f32, 0.25, 1.0]
            .iter()
            .flat_map(|value| value.to_le_bytes())
            .collect();
        let sound = decode_wav(&wav(FORMAT_FLOAT, 32, 1, 44_100, &data)).expect("decodes");
        assert_eq!(sound.samples, vec![-1.0, 0.25, 1.0]);
    }

    #[test]
    fn twenty_four_bit_pcm_is_sign_extended() {
        // The one arithmetic trap in the file. 24-bit samples have no Rust type, so the sign has to
        // be extended by hand — and getting it wrong turns every negative sample into a very large
        // positive one, which is heard as loud noise rather than as a quiet mistake.
        //
        // 0xFF_FF_FF little-endian is -1, the quietest possible negative sample.
        let data = [0xFF, 0xFF, 0xFF, 0x00, 0x00, 0x00];
        let sound = decode_wav(&wav(FORMAT_PCM, 24, 1, 48_000, &data)).expect("decodes");

        assert_eq!(sound.samples.len(), 2);
        assert!(
            sound.samples[0] < 0.0 && sound.samples[0] > -0.001,
            "0xFFFFFF is -1 of 8388608, a whisper below zero — got {}",
            sound.samples[0]
        );
        assert!(sound.samples[1].abs() < 1e-9);
    }

    #[test]
    fn stereo_stays_interleaved_and_halves_the_duration() {
        let data: Vec<u8> = [0i16, 0, 0, 0]
            .iter()
            .flat_map(|value| value.to_le_bytes())
            .collect();
        let sound = decode_wav(&wav(FORMAT_PCM, 16, 2, 4, &data)).expect("decodes");

        assert_eq!(sound.channels, 2);
        assert_eq!(sound.samples.len(), 4);
        // Four samples across two channels at four hertz is half a second.
        assert!((sound.duration() - 0.5).abs() < 1e-6);
    }

    #[test]
    fn an_unknown_chunk_is_skipped_rather_than_fatal() {
        // A `LIST` of authoring metadata is in almost every file a real tool exports, and it has
        // nothing to do with the audio. **Skipping is the reason a chunk format carries lengths.**
        let mut bytes = Vec::new();
        bytes.extend_from_slice(b"RIFF");
        bytes.extend_from_slice(&0u32.to_le_bytes());
        bytes.extend_from_slice(b"WAVE");
        // An odd-sized chunk, so this also covers the pad byte: without it every later chunk is read
        // one byte out and the file looks corrupt.
        bytes.extend_from_slice(b"LIST");
        bytes.extend_from_slice(&3u32.to_le_bytes());
        bytes.extend_from_slice(b"abc\0");

        let tail = wav(FORMAT_PCM, 16, 1, 8_000, &[0, 0]);
        bytes.extend_from_slice(&tail[12..]);

        let sound = decode_wav(&bytes).expect("the unknown chunk is skipped");
        assert_eq!(sound.samples.len(), 1);
        assert_eq!(sound.sample_rate, 8_000);
    }

    #[test]
    fn a_compressed_file_says_which_format_it_found() {
        // "Unsupported format" tells nobody which converter setting to change. Naming it does.
        let error = decode_wav(&wav(2, 4, 1, 8_000, &[0, 0])).expect_err("ADPCM is not PCM");
        let message = format!("{error}");
        assert!(message.contains("ADPCM"), "got {message}");
        assert!(
            message.contains("16-bit PCM"),
            "and says what to do: {message}"
        );
    }

    #[test]
    fn something_that_is_not_a_wav_is_refused_by_its_header() {
        let error = decode_wav(b"\x89PNG\r\n\x1a\n and then some").expect_err("a PNG is not a WAV");
        assert!(format!("{error}").contains("PNG"), "names what it found");
    }

    #[test]
    fn a_truncated_file_reports_what_it_was_reading() {
        let error = decode_wav(b"RIFF").expect_err("ends after the tag");
        assert!(format!("{error}").contains("ends unexpectedly"));
    }

    #[test]
    fn a_chunk_claiming_more_than_the_file_holds_terminates() {
        // A saturating skip rather than a wrapping one. The wrapping version puts the cursor back
        // near the start and the chunk loop never ends, which is a hang rather than an error — and a
        // hang on a malformed asset is the worst of the available failures.
        let mut bytes = Vec::new();
        bytes.extend_from_slice(b"RIFF");
        bytes.extend_from_slice(&0u32.to_le_bytes());
        bytes.extend_from_slice(b"WAVE");
        bytes.extend_from_slice(b"junk");
        bytes.extend_from_slice(&u32::MAX.to_le_bytes());

        let error = decode_wav(&bytes).expect_err("there is no fmt chunk");
        assert!(format!("{error}").contains("fmt"));
    }
}
