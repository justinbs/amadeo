//! The audio backend abstraction, and the null backend every build must have.

use crate::components::Bus;

/// What can go wrong while making sound.
#[derive(Debug, thiserror::Error, PartialEq, Eq)]
pub enum AudioError {
    /// The backend could not be created at all.
    ///
    /// A machine with no sound card, or one whose device is held exclusively by something else. **Not
    /// fatal** — a game with no audio is a game, where a game that refuses to start is not.
    #[error("could not initialise the {backend} audio backend: {reason}")]
    InitFailed {
        /// Which backend failed.
        backend: &'static str,
        /// Why.
        reason: String,
    },

    /// A voice named a sound the backend does not hold.
    ///
    /// Names the id, because the only useful version of this message is the one that says which
    /// asset — the same standard `AssetCatalogue` errors are held to.
    #[error("no sound is loaded under the id `{id}`; declare it in the scene's `assets` block")]
    UnknownSound {
        /// The id that was asked for.
        id: String,
    },

    /// The sound's samples cannot be played.
    #[error("the sound `{id}` cannot be played: {reason}")]
    BadSound {
        /// Which sound.
        id: String,
        /// What is wrong with it.
        reason: String,
    },
}

/// Decoded audio, ready to play.
///
/// **Interleaved `f32` samples**, which is what every audio API in Rust wants and what avoids a
/// conversion at the point where a conversion would be most audible. Decoding happens at load, for
/// the same reason ADR 0026 decodes images at load: the runtime should never parse a source asset.
#[derive(Debug, Clone, PartialEq)]
pub struct SoundData {
    /// Samples, interleaved by channel: `[l, r, l, r, …]` for stereo.
    pub samples: Vec<f32>,
    /// How many channels are interleaved. 1 is mono, 2 is stereo.
    ///
    /// **Mono is what a spatial sound wants**, and it is worth saying because it surprises people:
    /// a stereo sound already has its own left and right, so there is nothing left for a position to
    /// decide. A backend given a stereo sound to place in the world can only pick one of a few wrong
    /// answers.
    pub channels: u16,
    /// Samples per second per channel, as authored — 44100 or 48000 in practice.
    ///
    /// Kept rather than resampled at load, because a backend knows what its device wants and the
    /// engine does not. Resampling twice is worse than resampling once.
    pub sample_rate: u32,
}

impl SoundData {
    /// How long this sound lasts, in seconds.
    #[must_use]
    pub fn duration(&self) -> f32 {
        let frames = self.samples.len() / usize::from(self.channels.max(1));
        frames as f32 / self.sample_rate.max(1) as f32
    }
}

/// Where the ears are.
///
/// Position and orientation, in world space. A backend needs both: position decides how far away a
/// sound is, and orientation decides which side it is on.
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct Listener {
    /// World-space position.
    pub position: [f32; 3],
    /// The direction it faces, normalised — the listener's own negative Z (ADR 0018).
    pub forward: [f32; 3],
    /// Which way is up for it, normalised. Needed to tell left from right, which `forward` alone
    /// cannot: a listener lying on its side hears the world rotated.
    pub up: [f32; 3],
}

/// One sound that should be audible this frame.
#[derive(Debug, Clone, PartialEq)]
pub struct Voice {
    /// The declared asset id of the sound (ADR 0020).
    pub sound: String,
    /// Which bus it is mixed on.
    pub bus: Bus,
    /// Linear gain, **with the bus and master gain already applied**.
    ///
    /// Applied by the collection pass rather than by a backend, so two backends cannot disagree
    /// about the order the multiplications happen in.
    pub gain: f32,
    /// Playback rate. `1.0` is as recorded; `2.0` is an octave up and twice as fast.
    pub pitch: f32,
    /// Whether it restarts when it reaches the end.
    pub looping: bool,
    /// Where it is in the world, or `None` for a sound heard from everywhere.
    ///
    /// `None` is what music and narration want — see `AudioSource::spatial`.
    pub position: Option<[f32; 3]>,
}

/// Everything a backend needs to produce one moment of sound.
///
/// The audio equivalent of `FrameData`, and it carries the same promise: a backend is handed
/// everything and never reaches back into the world.
///
/// # It describes a *state*, not a set of changes
///
/// "These are the sounds that should be audible now" rather than "start this, stop that". A backend
/// diffs it against what it is already playing. That is what makes the collection pass declarative —
/// a sound stops because its entity stopped existing, and nobody has to have remembered to stop it.
#[derive(Debug, Clone, PartialEq, Default)]
pub struct AudioFrame {
    /// Where the ears are, or `None` when nothing in the world has any.
    pub listener: Option<Listener>,
    /// Every audible sound, in a reproducible order.
    pub voices: Vec<Voice>,
}

/// What a backend has to be able to do.
pub trait AudioBackend: std::fmt::Debug + Send + Sync {
    /// A short name, for diagnostics and error messages.
    fn name(&self) -> &'static str;

    /// Takes a decoded sound and holds it under an id.
    ///
    /// # Errors
    ///
    /// [`AudioError::BadSound`] if the samples cannot be played — a zero sample rate, or a channel
    /// count the device cannot handle.
    fn upload(&mut self, id: &str, sound: SoundData) -> Result<(), AudioError>;

    /// Whether a sound is already held under this id.
    fn has(&self, id: &str) -> bool;

    /// Makes the world sound like this.
    ///
    /// # Errors
    ///
    /// [`AudioError`] if the frame cannot be realised — most often a voice naming a sound that was
    /// never uploaded.
    fn submit(&mut self, frame: &AudioFrame) -> Result<(), AudioError>;

    /// The null backend, when this *is* one.
    ///
    /// The same downcast `RenderBackend::as_null` provides, and for the same reason: a test needs to
    /// read what would have been heard, and there is no other way to observe audio.
    fn as_null(&self) -> Option<&NullAudio> {
        None
    }
}

/// A backend that makes no sound and remembers everything it was asked to.
///
/// # Required, not a convenience
///
/// Invariant I7: every subsystem is headless-capable. CI has no sound card, and neither does a
/// machine running `amadeo replay` — so this is the backend the engine is tested through, and the
/// wgpu-shaped one is the exception rather than the rule.
///
/// **It is also the only way audio is testable at all.** Nothing can assert on a sound. Everything
/// can assert on the frame that would have produced one, which is why `last_frame` exists and why
/// the collection pass hands over a complete description rather than issuing commands.
#[derive(Debug, Default)]
pub struct NullAudio {
    /// Ids it has been handed, and how long each sound is.
    ///
    /// The duration rather than the samples, because holding a world's worth of audio to throw it
    /// away is the one thing a null backend should not do — and because "was this uploaded, and is
    /// it the right sound" is what a test actually asks.
    sounds: std::collections::BTreeMap<String, f32>,
    last_frame: Option<AudioFrame>,
    submissions: u64,
}

impl NullAudio {
    /// A fresh one, holding nothing.
    #[must_use]
    pub fn new() -> Self {
        Self::default()
    }

    /// The last frame it was given.
    #[must_use]
    pub fn last_frame(&self) -> Option<&AudioFrame> {
        self.last_frame.as_ref()
    }

    /// How many frames it has been given. For tests that want to know audio is being driven at all.
    #[must_use]
    pub fn submissions(&self) -> u64 {
        self.submissions
    }

    /// How long the sound held under an id is, in seconds.
    #[must_use]
    pub fn duration_of(&self, id: &str) -> Option<f32> {
        self.sounds.get(id).copied()
    }
}

impl AudioBackend for NullAudio {
    fn name(&self) -> &'static str {
        "null"
    }

    fn upload(&mut self, id: &str, sound: SoundData) -> Result<(), AudioError> {
        // Validated here as well as in a real backend, so a caller handing over a broken sound fails
        // the same way against both. A null backend that accepted more than the real one would hide
        // the bug until someone turned the sound on.
        if sound.sample_rate == 0 || sound.channels == 0 {
            return Err(AudioError::BadSound {
                id: id.to_string(),
                reason: format!(
                    "{} channels at {} Hz is not playable",
                    sound.channels, sound.sample_rate
                ),
            });
        }
        self.sounds.insert(id.to_string(), sound.duration());
        Ok(())
    }

    fn has(&self, id: &str) -> bool {
        self.sounds.contains_key(id)
    }

    /// Records the frame and makes no sound.
    ///
    /// # This being useless is the point
    ///
    /// The same posture `NullBackend` and `NullPhysics` take. **It does not check that a voice names
    /// a loaded sound**, deliberately: an unknown sound is survivable — ADR 0021's rule that gameplay
    /// may not ask whether an asset has loaded means a voice can legitimately reference something
    /// still on its way — and a null backend that failed where a real one would keep playing would
    /// make headless runs stricter than the game.
    fn submit(&mut self, frame: &AudioFrame) -> Result<(), AudioError> {
        self.last_frame = Some(frame.clone());
        self.submissions += 1;
        Ok(())
    }

    fn as_null(&self) -> Option<&NullAudio> {
        Some(self)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn a_second_of_silence() -> SoundData {
        SoundData {
            samples: vec![0.0; 48_000],
            channels: 1,
            sample_rate: 48_000,
        }
    }

    #[test]
    fn a_sounds_duration_comes_from_its_samples_and_rate() {
        assert!((a_second_of_silence().duration() - 1.0).abs() < 1e-6);

        // Stereo interleaves, so the same number of samples is half as long.
        let stereo = SoundData {
            channels: 2,
            ..a_second_of_silence()
        };
        assert!((stereo.duration() - 0.5).abs() < 1e-6);
    }

    #[test]
    fn an_unplayable_sound_is_refused_with_its_id_in_the_message() {
        let mut backend = NullAudio::new();
        let error = backend
            .upload(
                "broken",
                SoundData {
                    sample_rate: 0,
                    ..a_second_of_silence()
                },
            )
            .expect_err("a zero sample rate is not playable");

        // The id is in the message because "a sound is broken" is not an actionable report, and both
        // a human and an agent read these.
        assert!(format!("{error}").contains("broken"));
        assert!(!backend.has("broken"));
    }

    #[test]
    fn a_null_backend_remembers_the_last_frame_and_counts_them() {
        let mut backend = NullAudio::new();
        assert!(backend.last_frame().is_none());

        backend.submit(&AudioFrame::default()).expect("accepted");
        backend
            .submit(&AudioFrame {
                listener: Some(Listener {
                    position: [1.0, 2.0, 3.0],
                    forward: [0.0, 0.0, -1.0],
                    up: [0.0, 1.0, 0.0],
                }),
                voices: Vec::new(),
            })
            .expect("accepted");

        assert_eq!(backend.submissions(), 2);
        assert_eq!(
            backend.last_frame().expect("a frame").listener,
            Some(Listener {
                position: [1.0, 2.0, 3.0],
                forward: [0.0, 0.0, -1.0],
                up: [0.0, 1.0, 0.0],
            })
        );
    }

    #[test]
    fn a_voice_naming_an_unloaded_sound_is_accepted_rather_than_refused() {
        // ADR 0021: gameplay may not ask whether an asset has loaded, so a voice can legitimately
        // name something still on its way. Failing here would make a headless run stricter than the
        // game and would turn a survivable case into an error.
        let mut backend = NullAudio::new();
        let frame = AudioFrame {
            listener: None,
            voices: vec![Voice {
                sound: "never_loaded".to_string(),
                bus: Bus::Effects,
                gain: 1.0,
                pitch: 1.0,
                looping: false,
                position: None,
            }],
        };
        assert!(backend.submit(&frame).is_ok());
    }
}
