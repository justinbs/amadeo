//! `sounds` — generates the Warren's `.wav` files from the descriptions below.
//!
//! ```text
//! cargo run -p warren --bin sounds
//! ```
//!
//! # Why this exists, and why it is the second copy
//!
//! The argument `games/vault`'s `pix` makes for sprites and `games/atrium`'s `tone` makes for its
//! two drones. Invariant I1 wants a human to be able to author and diff everything a game is made
//! of, and a `.wav` is neither — it is a few hundred kilobytes of samples and a diff of one says
//! nothing. So the *source* is this file, and the `.wav` is derived from it.
//!
//! **These are placeholders and are meant to be replaced.** Drop a real `.wav` in with the same
//! asset id and nothing else changes.
//!
//! It duplicates the Atrium's renderer rather than sharing one, deliberately. `modules/` is for
//! genre-flavoured *runtime* code and this is a build tool, so sharing it would mean new structure
//! for scaffolding that exists to be thrown away. What would justify that is a third game, or a
//! generator anybody wanted to keep — and the honest note is that this is now the second copy.
//!
//! # What it has that the Atrium's does not: noise that loops
//!
//! A horror ambience is not sine tones. Breath, scrape and air are **noise**, and the obvious way to
//! make noise — random samples — cannot be looped, because the join is a step and a step is a click,
//! once per lap forever.
//!
//! So noise here is *additive*: a [`Band`] expands into many partials at whole multiples of the loop
//! frequency, with pseudo-random phases and a smooth amplitude curve across the band. Every partial
//! completes a whole number of cycles by construction, so the clip loops seamlessly and still
//! sounds like air rather than like a test tone. That is a real technique rather than a trick — it
//! is how a periodic noise table is built — and it costs about thirty lines.
//!
//! The phases come from [`amadeo_core::Rng`] rather than from the standard library, for the same
//! reason the sines come from [`amadeo_core::sin_cos_degrees`]: this has to write byte-identical
//! files every time it runs, or its output is a diff nobody can review.

use amadeo_core::{Rng, sin_cos_degrees};
use std::path::{Path, PathBuf};

/// Samples per second.
const SAMPLE_RATE: u32 = 44_100;

/// One partial of a generated sound: a frequency, a level, and where in its cycle it starts.
struct Partial {
    /// Hertz. **Must divide evenly into the clip length** for a looping clip, or it will click.
    hertz: f32,
    /// Linear amplitude, before the whole clip is scaled to `peak`.
    level: f32,
}

/// A band of noise, built additively so that it loops.
struct Band {
    /// The lowest frequency in the band, in hertz. Rounded up to a whole multiple of the loop.
    low_hertz: f32,
    /// The highest.
    high_hertz: f32,
    /// Linear amplitude of the band as a whole.
    level: f32,
    /// How many partials to spread across it. More is smoother and slower; a few dozen is plenty at
    /// these lengths, and below about eight it stops sounding like noise and starts sounding like a
    /// chord.
    partials: usize,
}

/// A sound to generate.
struct Sound {
    /// The asset id, which is also the filename.
    id: &'static str,
    /// How long one loop, or one shot, is in seconds.
    seconds: f32,
    /// The loudest sample in the finished clip, linear. Deliberately well below 1.0 for ambience: a
    /// background sound at full scale leaves nothing for anything else.
    peak: f32,
    /// Whether the clip is meant to run end-to-start forever.
    ///
    /// Decides two things at once. A looping clip is checked for whole-cycle partials, because a
    /// loop that does not join cleanly clicks once per lap; a **one-shot** is given an envelope
    /// instead, because a clip that starts and stops at full amplitude clicks at both ends.
    looping: bool,
    partials: &'static [Partial],
    bands: &'static [Band],
}

/// The Warren's six sounds.
///
/// **Two of them are the reason the audio is here at all.** A horror slice lives on knowing where
/// something is without seeing it, so the warden gets a *spatial* loop, and the run's two endings
/// get a sting each. The rest — a room tone, a footstep, a chime for picking something up — is what
/// makes the silence between them read as a place rather than as a missing feature.
const SOUNDS: &[Sound] = &[
    // The Warren itself: lower and emptier than the Atrium's room tone, with a breath of air over
    // it. Two partials a fraction apart so it beats slowly rather than sitting still, which is what
    // stops a drone sounding synthetic.
    Sound {
        id: "warren_tone",
        seconds: 4.0,
        peak: 0.14,
        looping: true,
        partials: &[
            Partial {
                hertz: 42.0,
                level: 1.0,
            },
            Partial {
                hertz: 42.25,
                level: 0.85,
            },
            Partial {
                hertz: 84.0,
                level: 0.22,
            },
        ],
        // Very low air, well under the drone, so the room sounds large rather than filtered.
        bands: &[Band {
            low_hertz: 120.0,
            high_hertz: 900.0,
            level: 0.10,
            partials: 40,
        }],
    },
    // The warden, and the whole point of spatialising anything. A slow, low pulse with breath across
    // it: near enough to a thing that is alive, and low enough that direction still reads at a
    // distance. **Four seconds** so the pulse is slow — a fast loop sounds like machinery.
    Sound {
        id: "warden_breath",
        seconds: 4.0,
        peak: 0.55,
        looping: true,
        partials: &[
            Partial {
                hertz: 0.5,
                level: 0.0,
            },
            Partial {
                hertz: 58.0,
                level: 1.0,
            },
            Partial {
                hertz: 58.5,
                level: 0.7,
            },
            Partial {
                hertz: 116.0,
                level: 0.3,
            },
        ],
        bands: &[
            Band {
                low_hertz: 200.0,
                high_hertz: 1400.0,
                level: 0.45,
                partials: 48,
            },
            Band {
                low_hertz: 1400.0,
                high_hertz: 3200.0,
                level: 0.14,
                partials: 24,
            },
        ],
    },
    // **Three footsteps, not one** — `docs/13` §1b's F6 clause (b). The Warren has three floor
    // surfaces and had one clip, so walking onto the duckboards a whole review cycle went into making
    // legible sounded exactly like walking on concrete, and wading sounded like it too.
    //
    // They differ in the two things that actually separate a footfall by ear: **where the body of the
    // thump sits**, and **how much broadband noise rides on it**. Timber is hollow — a higher, more
    // resonant thump over a box of air. Screed is dead — low, short, almost no ring. Water is nearly
    // all noise, high and wide, with barely any pitch at all.
    Sound {
        id: "step_screed",
        seconds: 0.2,
        peak: 0.4,
        looping: false,
        partials: &[
            Partial {
                hertz: 72.0,
                level: 1.0,
            },
            Partial {
                hertz: 128.0,
                level: 0.4,
            },
        ],
        bands: &[Band {
            low_hertz: 300.0,
            high_hertz: 2600.0,
            level: 0.35,
            partials: 32,
        }],
    },
    // Hollow: the box of air under a duckboard gives it a ring the deck has not got.
    Sound {
        id: "step_timber",
        seconds: 0.26,
        peak: 0.42,
        looping: false,
        partials: &[
            Partial {
                hertz: 118.0,
                level: 1.0,
            },
            Partial {
                hertz: 232.0,
                level: 0.55,
            },
            Partial {
                hertz: 447.0,
                level: 0.22,
            },
        ],
        bands: &[Band {
            low_hertz: 600.0,
            high_hertz: 3400.0,
            level: 0.3,
            partials: 32,
        }],
    },
    // Nearly all noise: a splash has very little pitch, and what it has is high.
    Sound {
        id: "step_water",
        seconds: 0.34,
        peak: 0.38,
        looping: false,
        partials: &[Partial {
            hertz: 196.0,
            level: 0.28,
        }],
        bands: &[
            Band {
                low_hertz: 900.0,
                high_hertz: 7200.0,
                level: 1.0,
                partials: 48,
            },
            Band {
                low_hertz: 300.0,
                high_hertz: 900.0,
                level: 0.4,
                partials: 16,
            },
        ],
    },
    // **The warden's constant, ranged channel — and it is a tread, not a breath.**
    //
    // Design direction 1, decision 10 (`docs/15` §5). `warden_breath` used to loop in every state,
    // forever, and two things were wrong with that. A thing that breathes continuously reads as an
    // *animal*, and `docs/11` §3 is emphatic that the warden is not one — it is an institution still
    // performing its function, and the function is counting. And `docs/11` §9 wants near-silence as
    // the default *"so that a single sound is an event"*, which a permanent noise from the antagonist
    // makes impossible: §3a's most important tell is an **absence** of sound, and you cannot hear an
    // absence inside a continuous tone.
    //
    // So the thing you track it by at range is its footfall: slow, heavy, regular, and unmistakably
    // not yours. Breath belongs to pursuit and to nothing else.
    Sound {
        id: "warden_tread",
        // **2.0 s rather than 1.9** — the generator refuses a partial that is not a whole number of
        // cycles in the loop, because a fraction of a cycle at the seam is a click once per loop. At
        // 2.0 s the base is 0.5 Hz and both partials are exact.
        seconds: 2.0,
        peak: 0.3,
        looping: true,
        partials: &[
            Partial {
                hertz: 58.0,
                level: 1.0,
            },
            Partial {
                hertz: 87.0,
                level: 0.3,
            },
        ],
        bands: &[Band {
            low_hertz: 240.0,
            high_hertz: 1900.0,
            level: 0.22,
            partials: 24,
        }],
    },
    // Picking something up. Brass on wood: two partials a fifth apart and a short scrape.
    Sound {
        id: "taken",
        seconds: 0.26,
        peak: 0.45,
        looping: false,
        partials: &[
            Partial {
                hertz: 880.0,
                level: 1.0,
            },
            Partial {
                hertz: 1320.0,
                level: 0.5,
            },
            Partial {
                hertz: 2640.0,
                level: 0.18,
            },
        ],
        bands: &[Band {
            low_hertz: 1800.0,
            high_hertz: 6000.0,
            level: 0.2,
            partials: 24,
        }],
    },
    // Getting out. Rising and open, and the only sound here built on a major interval — everything
    // else in the Warren is deliberately unresolved.
    Sound {
        id: "escaped",
        seconds: 1.4,
        peak: 0.5,
        looping: false,
        partials: &[
            Partial {
                hertz: 196.0,
                level: 1.0,
            },
            Partial {
                hertz: 294.0,
                level: 0.7,
            },
            Partial {
                hertz: 392.0,
                level: 0.5,
            },
            Partial {
                hertz: 587.0,
                level: 0.28,
            },
        ],
        bands: &[],
    },
    // Being caught. A tritone, low and loud, with a wash of noise under it — the one sound in the
    // game that is allowed to be unpleasant.
    Sound {
        id: "caught",
        seconds: 1.1,
        peak: 0.75,
        looping: false,
        partials: &[
            Partial {
                hertz: 73.0,
                level: 1.0,
            },
            Partial {
                hertz: 103.0,
                level: 0.85,
            },
            Partial {
                hertz: 146.0,
                level: 0.4,
            },
        ],
        bands: &[Band {
            low_hertz: 200.0,
            high_hertz: 2400.0,
            level: 0.5,
            partials: 40,
        }],
    },
];

fn main() {
    let out = manifest_dir().join("assets/sounds");
    if let Err(error) = std::fs::create_dir_all(&out) {
        eprintln!("could not create {}: {error}", out.display());
        std::process::exit(1);
    }

    for sound in SOUNDS {
        let path = out.join(format!("{}.wav", sound.id));
        let bytes = wav_of(&render(sound));
        if let Err(error) = write_if_changed(&path, &bytes) {
            eprintln!("could not write {}: {error}", path.display());
            std::process::exit(1);
        }
        println!(
            "{} — {:.2}s, {} bytes",
            path.display(),
            sound.seconds,
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
/// forever, which is the most obvious way for looping ambience to sound broken.
///
/// The fix is that **every partial completes a whole number of cycles** in the clip. For the
/// hand-written partials that is a property of the numbers above, so it is checked here and a bad
/// one stops the build. For a [`Band`] it is guaranteed by construction: the expansion below only
/// ever emits whole multiples of the loop frequency.
fn render(sound: &Sound) -> Vec<f32> {
    let frames = (sound.seconds * SAMPLE_RATE as f32).round() as usize;
    let mut samples = vec![0.0f32; frames];
    let mut total_level = 0.0;

    // Every partial as (hertz, level, phase in degrees), hand-written ones first so that adding a
    // band cannot change what the existing tones sound like.
    let mut voices: Vec<(f32, f32, f32)> = Vec::new();
    for partial in sound.partials {
        let cycles = partial.hertz * sound.seconds;
        // Only a *looping* clip has a seam to get wrong. A one-shot ends in silence because of the
        // envelope below, so demanding whole cycles of it would be an arbitrary constraint on which
        // frequencies a footstep may be built from.
        if sound.looping && (cycles - cycles.round()).abs() > 1e-4 {
            eprintln!(
                "{}: {} Hz over {} s is {cycles} cycles, which does not loop cleanly and will \
                 click once per loop. Choose a frequency that is a whole multiple of {} Hz.",
                sound.id,
                partial.hertz,
                sound.seconds,
                1.0 / sound.seconds
            );
            std::process::exit(1);
        }
        voices.push((partial.hertz, partial.level, 0.0));
    }
    voices.extend(expand(sound));

    for (hertz, level, phase) in &voices {
        total_level += level;
        for (index, sample) in samples.iter_mut().enumerate() {
            // Degrees rather than radians, because that is what `sin_cos_degrees` reduces in — and
            // reducing in degrees is what makes the quarter turns exact (ADR 0053).
            let degrees = phase + 360.0 * hertz * (index as f32) / SAMPLE_RATE as f32;
            let (sine, _) = sin_cos_degrees(degrees);
            *sample += sine * level;
        }
    }

    // Normalised to the requested peak by the *sum of levels* rather than by the loudest sample
    // actually produced. Deliberate: peak-normalising would make the output depend on how the
    // partials happened to line up, so adding one could quietly change the level of all the others.
    if total_level > 0.0 {
        let scale = sound.peak / total_level;
        for sample in &mut samples {
            *sample *= scale;
        }
    }

    if !sound.looping {
        envelope(&mut samples);
    }
    samples
}

/// Expands every [`Band`] into partials that all complete whole cycles.
///
/// # The two things that make this noise rather than a chord
///
/// **Pseudo-random phases.** Partials that all start at zero sum into one enormous spike at the
/// beginning of the clip and cancel elsewhere; scattered phases spread the energy evenly, which is
/// what noise is. The `Rng` is seeded from the sound's own id so that two sounds do not share a
/// pattern and one sound is identical on every run.
///
/// **A curve across the band**, rather than a flat level. A band with hard edges reads as a filtered
/// tone; rolling the level off towards both ends reads as air.
fn expand(sound: &Sound) -> Vec<(f32, f32, f32)> {
    let mut voices = Vec::new();
    // The lowest frequency that completes one cycle in the clip. Every partial is a whole multiple
    // of it, which is exactly the condition a seamless loop needs.
    let step = 1.0 / sound.seconds;

    for (index, band) in sound.bands.iter().enumerate() {
        // Seeded from the id and the band's position, so each band is its own pattern and both are
        // the same on every machine and every run.
        let mut rng = Rng::new(seed_of(sound.id).wrapping_add(index as u64 * 0x9E37_79B9));
        let low = (band.low_hertz / step).ceil().max(1.0);
        let high = (band.high_hertz / step).floor().max(low);
        let count = band.partials.max(1);

        for slot in 0..count {
            // Spread towards the **low** end rather than evenly, because hearing is logarithmic: an
            // even spread puts most of the partials in the top octave and the band sounds thin.
            //
            // Squared rather than geometric, and that is not a compromise for its own sake.
            // Geometric would want `powf`, which Rust documents as varying by platform and between
            // two calls in one execution — and this file's whole claim is that it writes
            // byte-identical output every time it runs. `fraction * fraction` is exactly specified
            // by IEEE 754 and bends the spread the same way. ADR 0044's argument, one layer out
            // from gameplay.
            let fraction = slot as f32 / count as f32;
            let multiple = (low + (high - low) * fraction * fraction).round().max(1.0);

            // A raised-cosine curve across the band, so it fades in and out rather than switching.
            let (_, cosine) = sin_cos_degrees(fraction * 360.0);
            let shape = 0.5 - 0.5 * cosine;

            let phase = rng.range_f32(0.0, 360.0);
            voices.push((multiple * step, band.level * shape / count as f32, phase));
        }
    }
    voices
}

/// A stable seed from a name, so a sound's noise is its own and is the same every run.
fn seed_of(id: &str) -> u64 {
    // FNV-1a, written out rather than depended on: it is four lines and this file already refuses
    // to depend on the standard library's hasher, whose output is not specified across versions.
    let mut hash: u64 = 0xcbf2_9ce4_8422_2325;
    for byte in id.as_bytes() {
        hash ^= u64::from(*byte);
        hash = hash.wrapping_mul(0x0000_0100_0000_01b3);
    }
    hash
}

/// Fades a one-shot in and out, so it does not click at either end.
///
/// A short attack and a long decay, which is what nearly every percussive sound is: the attack is
/// the contact and the decay is the room. Two milliseconds in is fast enough to still read as a hit.
fn envelope(samples: &mut [f32]) {
    let frames = samples.len();
    if frames == 0 {
        return;
    }
    let attack = ((SAMPLE_RATE as f32 * 0.002) as usize).clamp(1, frames);

    for (index, sample) in samples.iter_mut().enumerate() {
        let gain = if index < attack {
            index as f32 / attack as f32
        } else {
            // Squared, so the tail falls away quickly at first and then lingers. A linear decay
            // sounds like a fade rather than like something stopping.
            let remaining = (frames - index) as f32 / (frames - attack) as f32;
            remaining * remaining
        };
        *sample *= gain;
    }
}

/// Wraps samples in a 16-bit mono PCM `.wav`.
///
/// Sixteen bits rather than float, because it is the format every tool reads and these are
/// placeholders. `amadeo_audio::decode_wav` reads all three.
fn wav_of(samples: &[f32]) -> Vec<u8> {
    let data: Vec<u8> = samples
        .iter()
        .flat_map(|sample| {
            // Clamped before scaling: a sum of partials can exceed one, and wrapping instead of
            // clipping turns a loud moment into a burst of white noise.
            let clamped = sample.clamp(-1.0, 1.0);
            let value = (clamped * f32::from(i16::MAX)) as i16;
            value.to_le_bytes()
        })
        .collect();

    let mut out = Vec::with_capacity(44 + data.len());
    out.extend_from_slice(b"RIFF");
    out.extend_from_slice(&((36 + data.len()) as u32).to_le_bytes());
    out.extend_from_slice(b"WAVE");

    out.extend_from_slice(b"fmt ");
    out.extend_from_slice(&16u32.to_le_bytes()); // chunk size
    out.extend_from_slice(&1u16.to_le_bytes()); // PCM
    out.extend_from_slice(&1u16.to_le_bytes()); // mono
    out.extend_from_slice(&SAMPLE_RATE.to_le_bytes());
    out.extend_from_slice(&(SAMPLE_RATE * 2).to_le_bytes()); // bytes per second
    out.extend_from_slice(&2u16.to_le_bytes()); // bytes per frame
    out.extend_from_slice(&16u16.to_le_bytes()); // bits per sample

    out.extend_from_slice(b"data");
    out.extend_from_slice(&(data.len() as u32).to_le_bytes());
    out.extend_from_slice(&data);
    out
}

/// Writes only when the bytes differ, so re-running leaves timestamps alone.
fn write_if_changed(path: &Path, bytes: &[u8]) -> std::io::Result<()> {
    if let Ok(existing) = std::fs::read(path)
        && existing == bytes
    {
        return Ok(());
    }
    std::fs::write(path, bytes)
}

/// This crate's directory, so the tool works from anywhere.
fn manifest_dir() -> PathBuf {
    PathBuf::from(env!("CARGO_MANIFEST_DIR"))
}
