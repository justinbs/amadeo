//! `tone` — generates the Atrium's `.wav` files from the descriptions below.
//!
//! ```text
//! cargo run -p atrium --bin tone
//! ```
//!
//! # Why this exists
//!
//! The same argument `games/vault`'s `pix` makes for sprites. Invariant I1 wants a human to be able
//! to author and diff everything a game is made of, and a `.wav` is neither: it is a few hundred
//! kilobytes of samples, and a diff of one says nothing. So the *source* is this file — a handful of
//! frequencies and levels you can read and change — and the `.wav` is derived from it.
//!
//! It is also what makes the Atrium's audio demo self-contained. Nothing has to be downloaded,
//! licensed or committed as an opaque blob to hear whether the audio system works.
//!
//! **These are placeholders and are meant to be replaced.** Two sine drones are enough to answer
//! "does a spatial sound pan and attenuate, and does a non-spatial one stay put" — which is the
//! question this milestone actually has. They are not sound design. Drop a real `.wav` in with the
//! same asset id and it will be used instead, with nothing else to change.
//!
//! # Idempotent, and it uses the engine's own trigonometry to stay that way
//!
//! Running it twice writes byte-identical files, so it can be re-run freely and a diff shows only
//! what changed in the description above.
//!
//! That is why it calls [`amadeo_core::sin_cos_degrees`] rather than `f32::sin`. ADR 0053 wrote the
//! engine's own trigonometry because Rust documents the standard library's as varying by platform,
//! by version, **and between two calls in one execution** — which for a generator means the output
//! file could differ from itself for no reason anyone could see. Nothing here reaches the state
//! hash, so this is not ADR 0044's determinism rule; it is the weaker and more mundane requirement
//! that a build step be reproducible.

use amadeo_core::sin_cos_degrees;
use std::path::{Path, PathBuf};

/// Samples per second. 44100 rather than 48000 because these are placeholder assets and the smaller
/// file is the only difference that matters here.
const SAMPLE_RATE: u32 = 44_100;

/// One partial of a generated drone: a frequency and how loud it is.
struct Partial {
    /// Hertz. **Must divide evenly into the clip length** or the loop will click — see `render`.
    hertz: f32,
    /// Linear amplitude, before the whole clip is scaled to `peak`.
    level: f32,
}

/// A sound to generate.
struct Drone {
    /// The asset id, which is also the filename.
    id: &'static str,
    /// How long one loop is, in seconds.
    seconds: f32,
    /// The loudest sample in the finished clip, linear. Deliberately well below 1.0: these are
    /// ambience, and a background sound at full scale leaves nothing for anything else.
    peak: f32,
    partials: &'static [Partial],
}

/// The Atrium's two sounds.
///
/// **One spatial and one not, on purpose.** They exercise the two different paths through the kira
/// backend — a spatial voice gets its own positioned track, a non-spatial one plays on its bus
/// directly — and those paths cannot be told apart by any test. Hearing the first move as you walk
/// and the second stay put is the check.
const DRONES: &[Drone] = &[
    // The warm lamp's electrical hum. Mains-transformer flavoured: a low fundamental with its
    // harmonics, which is what makes it read as a *thing humming* rather than as a test tone.
    Drone {
        id: "lamp_hum",
        seconds: 1.0,
        peak: 0.5,
        partials: &[
            Partial {
                hertz: 60.0,
                level: 1.0,
            },
            Partial {
                hertz: 120.0,
                level: 0.5,
            },
            Partial {
                hertz: 180.0,
                level: 0.28,
            },
            Partial {
                hertz: 300.0,
                level: 0.1,
            },
        ],
    },
    // A room tone: low, slow, and not from anywhere. Two close partials rather than one, so it
    // beats slowly instead of sitting still, which is what stops a drone sounding synthetic.
    Drone {
        id: "room_tone",
        seconds: 2.0,
        peak: 0.16,
        partials: &[
            Partial {
                hertz: 55.0,
                level: 1.0,
            },
            Partial {
                hertz: 55.5,
                level: 0.9,
            },
            Partial {
                hertz: 110.0,
                level: 0.35,
            },
            Partial {
                hertz: 164.5,
                level: 0.12,
            },
        ],
    },
];

fn main() {
    let out = manifest_dir().join("assets/sounds");
    if let Err(error) = std::fs::create_dir_all(&out) {
        eprintln!("could not create {}: {error}", out.display());
        std::process::exit(1);
    }

    for drone in DRONES {
        let path = out.join(format!("{}.wav", drone.id));
        let bytes = wav_of(&render(drone));
        if let Err(error) = write_if_changed(&path, &bytes) {
            eprintln!("could not write {}: {error}", path.display());
            std::process::exit(1);
        }
        println!(
            "{} — {:.1}s, {} bytes",
            path.display(),
            drone.seconds,
            bytes.len()
        );
    }
}

/// Turns a description into mono samples in `-1.0 ..= 1.0`.
///
/// # Why a loop does not click
///
/// A looping clip runs off its last sample straight onto its first. If a partial is halfway through
/// a cycle at the end, that join is a step in the waveform, and a step is a click — once per loop,
/// forever, which is the single most obvious way for looping ambience to sound broken.
///
/// The fix is that **every partial completes a whole number of cycles** in the clip. That is a
/// property of the numbers in `DRONES` rather than of this code, so it is checked here: a partial
/// that would not loop cleanly stops the build with the frequency that has to change.
fn render(drone: &Drone) -> Vec<f32> {
    let frames = (drone.seconds * SAMPLE_RATE as f32).round() as usize;

    let mut samples = vec![0.0f32; frames];
    let mut total_level = 0.0;

    for partial in drone.partials {
        let cycles = partial.hertz * drone.seconds;
        if (cycles - cycles.round()).abs() > 1e-4 {
            eprintln!(
                "{}: {} Hz over {} s is {cycles} cycles, which does not loop cleanly and will \
                 click once per loop. Choose a frequency that is a whole multiple of {} Hz.",
                drone.id,
                partial.hertz,
                drone.seconds,
                1.0 / drone.seconds
            );
            std::process::exit(1);
        }

        total_level += partial.level;
        for (index, sample) in samples.iter_mut().enumerate() {
            // Degrees rather than radians, because that is what `sin_cos_degrees` reduces in and
            // where its quarter turns are exact (ADR 0053).
            let degrees = 360.0 * partial.hertz * (index as f32 / SAMPLE_RATE as f32);
            let (sine, _cosine) = sin_cos_degrees(degrees);
            *sample += sine * partial.level;
        }
    }

    // Scaled by the sum of the levels rather than by the measured peak. The measured peak depends
    // on where the partials happen to line up, so an edit to one frequency would silently change
    // the loudness of the whole clip; dividing by a number that is visible in the table above does
    // not.
    let scale = drone.peak / total_level.max(1e-6);
    for sample in &mut samples {
        *sample *= scale;
    }
    samples
}

/// Wraps mono samples in a 16-bit PCM WAV container.
///
/// Hand-written, matching `amadeo_audio::decode_wav` on the other side — which is the same
/// arrangement `amadeo-image` has for PPM, and it means this file has no dependencies to keep in
/// step with.
fn wav_of(samples: &[f32]) -> Vec<u8> {
    let channels: u16 = 1;
    let bits: u16 = 16;
    let block_align = channels * bits / 8;
    let byte_rate = SAMPLE_RATE * u32::from(block_align);
    let data_len = (samples.len() * usize::from(block_align)) as u32;

    let mut out = Vec::with_capacity(44 + data_len as usize);
    out.extend_from_slice(b"RIFF");
    out.extend_from_slice(&(36 + data_len).to_le_bytes());
    out.extend_from_slice(b"WAVE");

    out.extend_from_slice(b"fmt ");
    out.extend_from_slice(&16u32.to_le_bytes());
    out.extend_from_slice(&1u16.to_le_bytes()); // 1 is uncompressed PCM
    out.extend_from_slice(&channels.to_le_bytes());
    out.extend_from_slice(&SAMPLE_RATE.to_le_bytes());
    out.extend_from_slice(&byte_rate.to_le_bytes());
    out.extend_from_slice(&block_align.to_le_bytes());
    out.extend_from_slice(&bits.to_le_bytes());

    out.extend_from_slice(b"data");
    out.extend_from_slice(&data_len.to_le_bytes());
    for &sample in samples {
        // Clamped before scaling, so a description that adds up to more than full scale distorts
        // rather than wrapping around — wrapping turns a slightly-too-loud sound into white noise.
        let clamped = sample.clamp(-1.0, 1.0);
        // 32767 rather than 32768: the positive side of a signed 16-bit range is one short, and
        // using the larger number makes exactly-1.0 wrap to the most negative sample there is.
        out.extend_from_slice(&((clamped * 32767.0).round() as i16).to_le_bytes());
    }
    out
}

/// Writes only when the contents differ, so re-running leaves timestamps alone.
fn write_if_changed(path: &Path, bytes: &[u8]) -> std::io::Result<()> {
    if let Ok(existing) = std::fs::read(path)
        && existing == bytes
    {
        return Ok(());
    }
    std::fs::write(path, bytes)
}

/// This game's own directory, so the tool works from anywhere.
fn manifest_dir() -> PathBuf {
    PathBuf::from(env!("CARGO_MANIFEST_DIR"))
}
