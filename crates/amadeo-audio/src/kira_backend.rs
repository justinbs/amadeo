//! The kira backend — the one that actually makes a sound. ADR 0059.
//!
//! # Read this before changing anything in here
//!
//! **No test in this repository can tell you whether this file works.** CI has no sound card and
//! neither does a headless run, so the only verification available is a person listening. That is
//! not a gap to be closed later; it is the shape of the problem, and it is why the engine-owned half
//! was built first and why [`VoiceTracker`] exists.
//!
//! So the rule for this file is: **it contains as little decision-making as possible.** Everything
//! that could be wrong invisibly — which voices are new, which have gone, which merely moved — lives
//! in `tracker.rs`, where it is exercised headlessly. What is left here is the part that genuinely
//! needs a device, and it should stay mechanical enough to review by reading.
//!
//! **Do not move reconciliation back in here**, however tempting it looks when adding a feature.
//!
//! # Q12 turned out not to bite, and it is worth knowing why
//!
//! [`Audio`](crate::Audio) is a `Service`, `Service` requires `Send + Sync`, and open question Q12
//! predicted since the Q1 spike that `kira::AudioManager` would be the first thing unable to satisfy
//! that. **In kira 0.12 it satisfies it fine**, and so does every handle here.
//!
//! The reason generalises: kira's desktop backend does not hold the `cpal` stream itself — it hands
//! it to a stream-manager thread and keeps a controller. A library that already owns a thread has
//! usually had to become `Send + Sync` to do it. So no `LocalService`, no `Mutex`, no relaxed bound;
//! the manager goes straight into the service like any other value. Q12 stays open for a genuine
//! offender.
//!
//! # How a frame becomes sound
//!
//! kira's model is a tree of mixer tracks with sounds playing on them. This maps on as:
//!
//! - one **sub-track per [`Bus`]**, created once. Gain is already applied per voice by the
//!   collection pass, so these are for routing — but they are what makes per-bus effects and
//!   ducking possible later, and they cost four tracks.
//! - one **listener**, from [`Listener`], created on the first frame that has ears.
//! - a **spatial** voice gets its own spatial sub-track under its bus, and the sound plays on that.
//!   Distance attenuation and panning are kira's to do — ADR 0059 chose kira precisely so the engine
//!   would not be writing them, and the collection pass deliberately does not pre-attenuate.
//! - a **non-spatial** voice plays directly on its bus track, so nothing pans it.

use crate::backend::{AudioBackend, AudioError, AudioFrame, Listener, SoundData, Voice};
use crate::components::Bus;
use crate::tracker::VoiceTracker;
use amadeo_ecs::Entity;
use std::collections::BTreeMap;

use kira::sound::static_sound::{StaticSoundData, StaticSoundHandle};
use kira::track::{SpatialTrackBuilder, SpatialTrackHandle, TrackBuilder, TrackHandle};
use kira::{AudioManager, AudioManagerSettings, Decibels, DefaultBackend, Frame, Tween};

/// How long a gain or position change takes to reach its new value.
///
/// **Not zero, deliberately.** A gain applied instantly is a step change in a waveform, which is a
/// click; a position applied instantly is a sound that teleports across the stereo field. Ten
/// milliseconds is short enough to read as immediate and long enough to be smooth — the figure most
/// audio middleware uses for exactly this.
const SMOOTHING: Tween = Tween {
    start_time: kira::StartTime::Immediate,
    duration: std::time::Duration::from_millis(10),
    easing: kira::Easing::Linear,
};

/// How long a stopped voice takes to fade out.
///
/// Also not zero, and for the sharper version of the same reason: cutting a waveform mid-cycle to
/// silence is an audible click on almost any material.
const FADE_OUT: Tween = Tween {
    start_time: kira::StartTime::Immediate,
    duration: std::time::Duration::from_millis(20),
    easing: kira::Easing::Linear,
};

/// The lowest gain that is worth playing at all, as a linear amplitude.
///
/// Below this, decibels head towards negative infinity and the sound is inaudible anyway. Clamping
/// here means `gain_to_decibels` never has to return `-inf`, which some mixers propagate as NaN.
const SILENCE_THRESHOLD: f32 = 1e-4;

/// One voice as kira is holding it.
///
/// A spatial voice owns a track as well as a sound, because kira spatialises per *track*. Keeping
/// the two together is what makes stopping a voice one operation rather than two that could get out
/// of step — dropping the handles is what stops the sound.
struct LiveVoice {
    sound: StaticSoundHandle,
    /// `Some` for a spatial voice: the track it is placed on. `None` means it plays on its bus
    /// directly and nothing pans it.
    track: Option<SpatialTrackHandle>,
}

/// A backend that plays sound through `kira`.
///
/// # No kira type crosses the boundary
///
/// ADR 0036 §4's rule, applied to audio by ADR 0059: nothing in this struct's public surface names a
/// kira type, so the choice of library stays reversible. Everything below is private.
pub struct KiraAudio {
    manager: AudioManager<DefaultBackend>,
    /// One track per [`Bus`], indexed by `bus as usize`.
    buses: Vec<TrackHandle>,
    /// The ears. Created on the first frame that has any — a world with no listener never gets here,
    /// because the collection pass submits no voices without one.
    listener: Option<kira::listener::ListenerHandle>,
    /// Uploaded sounds, by asset id. `StaticSoundData` holds its samples in an `Arc`, so playing one
    /// clones a handle rather than a buffer.
    sounds: BTreeMap<String, StaticSoundData>,
    /// What is currently playing, by the entity making it.
    live: BTreeMap<Entity, LiveVoice>,
    /// The shared reconciliation logic. **Not this file's business** — see the module docs.
    tracker: VoiceTracker,
}

/// Hand-written because `AudioManager` and the handles are not `Debug`, and `AudioBackend` requires
/// it. Reports what is useful in a diagnostic rather than pretending to show the manager.
impl std::fmt::Debug for KiraAudio {
    fn fmt(&self, formatter: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        formatter
            .debug_struct("KiraAudio")
            .field("sounds", &self.sounds.len())
            .field("live", &self.live.len())
            .field("has_listener", &self.listener.is_some())
            .finish()
    }
}

impl KiraAudio {
    /// Opens the default audio device and builds the bus tracks.
    ///
    /// # Errors
    ///
    /// [`AudioError::InitFailed`] if there is no usable device — no sound card, or one held
    /// exclusively by something else. **Callers should carry on**: a game with no audio is a game,
    /// where a game that refuses to start is not.
    pub fn new() -> Result<KiraAudio, AudioError> {
        let mut manager = AudioManager::<DefaultBackend>::new(AudioManagerSettings::default())
            .map_err(|error| AudioError::InitFailed {
                backend: "kira",
                reason: error.to_string(),
            })?;

        // One sub-track per bus, in `Bus` order, so `buses[bus as usize]` is the right one. The
        // `every_bus_has_a_slot_in_the_gain_array` test in `components.rs` is what keeps that
        // indexing honest when a variant is added.
        let mut buses = Vec::with_capacity(Bus::COUNT);
        for index in 0..Bus::COUNT {
            let track = manager
                .add_sub_track(TrackBuilder::new())
                .map_err(|error| AudioError::InitFailed {
                    backend: "kira",
                    reason: format!("could not create the mixer track for bus {index}: {error}"),
                })?;
            buses.push(track);
        }

        Ok(KiraAudio {
            manager,
            buses,
            listener: None,
            sounds: BTreeMap::new(),
            live: BTreeMap::new(),
            tracker: VoiceTracker::new(),
        })
    }

    /// Makes sure the ears exist and are where the frame says.
    ///
    /// Returns whether there are ears at all: a spatial track cannot be created without a listener
    /// id, so a frame with voices and no listener has to fall back to unpanned playback rather than
    /// failing. In practice the collection pass never sends one, because a world with no listener
    /// submits no voices.
    fn sync_listener(&mut self, listener: &Listener) -> bool {
        // Plain arrays rather than `mint` types. kira's setters take `impl Into<Value<..>>` and
        // `mint` converts from `[f32; 3]` and `[f32; 4]`, so this crate needs no `mint` dependency
        // of its own — which also means no version to keep in step with kira's.
        let position = listener.position;
        let orientation = orientation_from(listener);

        match self.listener.as_mut() {
            Some(handle) => {
                handle.set_position(position, SMOOTHING);
                handle.set_orientation(orientation, SMOOTHING);
                true
            }
            None => match self.manager.add_listener(position, orientation) {
                Ok(handle) => {
                    self.listener = Some(handle);
                    true
                }
                // A listener is a resource with a capacity like any other. Failing to add one means
                // no spatialisation, not no sound — so this reports `false` and playback continues
                // unpanned, which is quieter than it should be rather than silent.
                Err(_) => false,
            },
        }
    }

    /// Starts one voice, replacing anything already live for that entity.
    fn start(&mut self, voice: &Voice, spatialise: bool) -> Result<(), AudioError> {
        let Some(data) = self.sounds.get(&voice.sound) else {
            return Err(AudioError::UnknownSound {
                id: voice.sound.clone(),
            });
        };

        // `StaticSoundData` is settings plus an `Arc` to the samples, so each of these builders
        // copies a handful of fields rather than the audio.
        let mut data = data
            .volume(gain_to_decibels(voice.gain))
            .playback_rate(f64::from(voice.pitch));
        if voice.looping {
            // The whole clip. A loop region within a clip is a later feature and belongs on the
            // component, not here.
            data = data.loop_region(..);
        }

        let bus = &mut self.buses[voice.bus as usize];

        let live = match (spatialise, voice.position, self.listener.as_ref()) {
            (true, Some(position), Some(listener)) => {
                let mut track = bus
                    .add_spatial_sub_track(listener.id(), position, SpatialTrackBuilder::new())
                    .map_err(|error| AudioError::BadSound {
                        id: voice.sound.clone(),
                        reason: format!("no room for another spatial voice: {error}"),
                    })?;
                let sound = track.play(data).map_err(|error| AudioError::BadSound {
                    id: voice.sound.clone(),
                    reason: error.to_string(),
                })?;
                LiveVoice {
                    sound,
                    track: Some(track),
                }
            }
            // Non-spatial, or spatial with nowhere to put it: straight onto the bus, unpanned.
            _ => {
                let sound = bus.play(data).map_err(|error| AudioError::BadSound {
                    id: voice.sound.clone(),
                    reason: error.to_string(),
                })?;
                LiveVoice { sound, track: None }
            }
        };

        self.live.insert(voice.source, live);
        Ok(())
    }

    /// Stops one voice and forgets it.
    ///
    /// Dropping the handles is what actually frees the track; the explicit `stop` is what makes it
    /// fade rather than cut.
    fn stop(&mut self, entity: Entity) {
        if let Some(mut live) = self.live.remove(&entity) {
            live.sound.stop(FADE_OUT);
        }
    }

    /// Re-applies gain, pitch and position to a voice that is already playing.
    fn update(&mut self, voice: &Voice) {
        let Some(live) = self.live.get_mut(&voice.source) else {
            return;
        };
        live.sound
            .set_volume(gain_to_decibels(voice.gain), SMOOTHING);
        live.sound
            .set_playback_rate(f64::from(voice.pitch), SMOOTHING);

        if let (Some(track), Some(position)) = (live.track.as_mut(), voice.position) {
            track.set_position(position, SMOOTHING);
        }
    }
}

impl AudioBackend for KiraAudio {
    fn name(&self) -> &'static str {
        "kira"
    }

    fn upload(&mut self, id: &str, sound: SoundData) -> Result<(), AudioError> {
        // Validated the same way `NullAudio` validates, so a caller handing over a broken sound
        // fails identically against both backends. A real backend that accepted more than the null
        // one would hide the bug until somebody turned the sound on.
        if sound.sample_rate == 0 || sound.channels == 0 {
            return Err(AudioError::BadSound {
                id: id.to_string(),
                reason: format!(
                    "{} channels at {} Hz is not playable",
                    sound.channels, sound.sample_rate
                ),
            });
        }

        self.sounds.insert(id.to_string(), to_kira(id, &sound)?);
        Ok(())
    }

    fn has(&self, id: &str) -> bool {
        self.sounds.contains_key(id)
    }

    fn submit(&mut self, frame: &AudioFrame) -> Result<(), AudioError> {
        // Ears first: a voice that starts this frame needs a listener to be placed against, and
        // creating the listener afterwards would leave the first frame of every spatial sound
        // unpanned.
        let spatialise = match frame.listener.as_ref() {
            Some(listener) => self.sync_listener(listener),
            None => false,
        };

        let changes = self.tracker.reconcile(frame);

        // In the order `VoiceChanges` declares them, which is the order that keeps the live voice
        // count from spiking when a scene swaps one sound for another.
        for entity in changes.stopped {
            self.stop(entity);
        }

        // **The first error is remembered but the rest of the frame still plays.** One voice naming
        // a sound that has not been uploaded should not silence the world — ADR 0021 makes that a
        // survivable case, since gameplay is not allowed to wait for an asset.
        let mut first_error = None;
        for voice in &changes.started {
            if let Err(error) = self.start(voice, spatialise)
                && first_error.is_none()
            {
                first_error = Some(error);
            }
        }
        for voice in &changes.updated {
            self.update(voice);
        }

        match first_error {
            Some(error) => Err(error),
            None => Ok(()),
        }
    }
}

/// Turns a linear gain into decibels, which is what kira's volume is in.
///
/// `20 * log10(amplitude)` — the definition. `log10` is a transcendental and ADR 0044 bans those in
/// anything deciding gameplay state; it is safe here for the reason `amadeo-image` uses `powf`
/// safely, and more strongly: audio is a `Service` and is outside the state hash entirely, so this
/// number cannot reach a replay even in principle.
fn gain_to_decibels(gain: f32) -> Decibels {
    if gain <= SILENCE_THRESHOLD {
        return Decibels::SILENCE;
    }
    Decibels(20.0 * gain.log10())
}

/// Turns a listener's forward and up into the quaternion kira wants, as `[x, y, z, w]`.
///
/// That component order is `mint`'s, which is what kira's setters convert from. Getting it wrong
/// swaps the scalar part with an axis, and what that sounds like is a listener facing a direction
/// nothing is in.
///
/// # Why the basis is rebuilt rather than trusted
///
/// `forward` and `up` arrive from a transform and are very nearly orthonormal, but "very nearly" is
/// not good enough for a rotation matrix — a slightly non-orthogonal basis makes a quaternion that
/// is not a rotation, and what that sounds like is the stereo field drifting. So `right` comes from
/// a cross product and `up` is recomputed from it, which makes the basis orthonormal by
/// construction.
fn orientation_from(listener: &Listener) -> [f32; 4] {
    let forward = normalise(listener.forward, [0.0, 0.0, -1.0]);
    let up_hint = normalise(listener.up, [0.0, 1.0, 0.0]);

    let right = normalise(cross(forward, up_hint), [1.0, 0.0, 0.0]);
    let up = cross(right, forward);

    // kira uses glam, whose `Quat::look_to`-style conventions are not exposed here, so this is the
    // standard matrix-to-quaternion conversion written out. The columns are the basis vectors of the
    // rotation, with -forward in the third: the listener faces along its own negative Z (ADR 0018),
    // exactly as a camera looks and a light aims.
    let m = [
        [right[0], right[1], right[2]],
        [up[0], up[1], up[2]],
        [-forward[0], -forward[1], -forward[2]],
    ];

    let trace = m[0][0] + m[1][1] + m[2][2];
    // Four branches rather than one, because the naive formula divides by something near zero for
    // three of the four possible dominant axes. This is Shepperd's method, and it is written out
    // rather than reached for because it is the standard one and every graphics text has it.
    let (w, x, y, z) = if trace > 0.0 {
        let s = (trace + 1.0).sqrt() * 2.0;
        (
            0.25 * s,
            (m[1][2] - m[2][1]) / s,
            (m[2][0] - m[0][2]) / s,
            (m[0][1] - m[1][0]) / s,
        )
    } else if m[0][0] > m[1][1] && m[0][0] > m[2][2] {
        let s = (1.0 + m[0][0] - m[1][1] - m[2][2]).sqrt() * 2.0;
        (
            (m[1][2] - m[2][1]) / s,
            0.25 * s,
            (m[1][0] + m[0][1]) / s,
            (m[2][0] + m[0][2]) / s,
        )
    } else if m[1][1] > m[2][2] {
        let s = (1.0 + m[1][1] - m[0][0] - m[2][2]).sqrt() * 2.0;
        (
            (m[2][0] - m[0][2]) / s,
            (m[1][0] + m[0][1]) / s,
            0.25 * s,
            (m[2][1] + m[1][2]) / s,
        )
    } else {
        let s = (1.0 + m[2][2] - m[0][0] - m[1][1]).sqrt() * 2.0;
        (
            (m[0][1] - m[1][0]) / s,
            (m[2][0] + m[0][2]) / s,
            (m[2][1] + m[1][2]) / s,
            0.25 * s,
        )
    };

    [x, y, z, w]
}

/// Unit length, or `fallback` if the vector is too short to have a direction.
fn normalise(v: [f32; 3], fallback: [f32; 3]) -> [f32; 3] {
    let length_squared = v[0] * v[0] + v[1] * v[1] + v[2] * v[2];
    if length_squared < 1e-12 {
        return fallback;
    }
    let inverse = 1.0 / length_squared.sqrt();
    [v[0] * inverse, v[1] * inverse, v[2] * inverse]
}

fn cross(a: [f32; 3], b: [f32; 3]) -> [f32; 3] {
    [
        a[1] * b[2] - a[2] * b[1],
        a[2] * b[0] - a[0] * b[2],
        a[0] * b[1] - a[1] * b[0],
    ]
}

/// Turns the engine's interleaved samples into kira's stereo frames.
///
/// # Errors
///
/// [`AudioError::BadSound`] for more than two channels, which kira has no stereo pair to fold into.
/// Named rather than silently mixed down, because a surround file in a game's asset folder is
/// almost always a mistake and a quiet wrong answer would hide it.
fn to_kira(id: &str, sound: &SoundData) -> Result<StaticSoundData, AudioError> {
    let frames: Vec<Frame> = match sound.channels {
        // A mono sample goes to both sides. The *spatialisation* is what will pan it — see the
        // note on `SoundData::channels` about why a placed sound wants to be mono in the first
        // place.
        1 => sound
            .samples
            .iter()
            .map(|&sample| Frame::new(sample, sample))
            .collect(),
        2 => sound
            .samples
            .chunks_exact(2)
            .map(|pair| Frame::new(pair[0], pair[1]))
            .collect(),
        channels => {
            return Err(AudioError::BadSound {
                id: id.to_string(),
                reason: format!(
                    "{channels} channels is more than a stereo pair; re-export this sound as mono \
                     (for a placed sound) or stereo (for music)"
                ),
            });
        }
    };

    Ok(StaticSoundData {
        sample_rate: sound.sample_rate,
        frames: frames.into(),
        settings: kira::sound::static_sound::StaticSoundSettings::default(),
        slice: None,
    })
}

#[cfg(test)]
mod tests {
    use super::*;

    // **Everything below runs without a sound card.** Nothing here constructs an `AudioManager`,
    // because CI has no device — these test the pure functions, which are the only part of this
    // file that can be checked at all. The rest is verified by a person listening, which is stated
    // plainly rather than papered over.

    #[test]
    fn the_backend_fits_in_a_service_without_a_mutex_or_a_local_store() {
        // **Q12's answer, pinned.** Open question Q12 predicted since the Q1 spike that
        // `kira::AudioManager` would be the first thing unable to satisfy `Service: Send + Sync`,
        // and proposed a `LocalService` store or a `Mutex` to cope. In kira 0.12 it satisfies the
        // bound fine, so neither was needed.
        //
        // This is here so a future kira release that regresses it turns *this* red, with a name
        // that says what the problem is — rather than producing a confusing error at the one line
        // in a game that boxes the backend.
        fn is_send_and_sync<T: Send + Sync>() {}
        is_send_and_sync::<KiraAudio>();

        // And the whole chain that actually matters, type-checked without opening a device:
        // `KiraAudio` -> `Box<dyn AudioBackend>` -> the `Audio` service. Never called, because
        // constructing one needs a sound card; naming it is what compiles it.
        fn into_service(backend: KiraAudio) -> crate::Audio {
            crate::Audio::new(Box::new(backend))
        }
        let _ = into_service;
    }

    #[test]
    fn a_gain_of_one_is_zero_decibels() {
        // The identity, and the one that would be noticed immediately if it were wrong -- every
        // sound in the game would be at the wrong level.
        assert!((gain_to_decibels(1.0).0 - 0.0).abs() < 1e-5);
    }

    #[test]
    fn half_the_amplitude_is_about_six_decibels_down() {
        // The number every audio engineer knows, which is what makes it a good check on the formula
        // being the right way up: a gain *below* one must be a *negative* number of decibels.
        let half = gain_to_decibels(0.5).0;
        assert!((half + 6.0206).abs() < 1e-3, "got {half}");
        assert!(half < 0.0, "quieter than unity must be negative decibels");
    }

    #[test]
    fn a_gain_of_zero_is_silence_rather_than_negative_infinity() {
        // `log10(0)` is -inf, and a mixer handed one usually turns it into NaN a few operations
        // later -- at which point the *whole mix* goes silent, not just this voice.
        let silent = gain_to_decibels(0.0);
        assert!(silent.0.is_finite(), "got {}", silent.0);
        assert_eq!(silent, Decibels::SILENCE);
    }

    #[test]
    fn mono_samples_reach_both_sides() {
        let sound = SoundData {
            samples: vec![0.25, -0.5],
            channels: 1,
            sample_rate: 48_000,
        };
        let data = to_kira("hum", &sound).expect("mono is playable");
        assert_eq!(data.frames.len(), 2);
        assert_eq!(data.frames[0], Frame::new(0.25, 0.25));
        assert_eq!(data.frames[1], Frame::new(-0.5, -0.5));
        assert_eq!(data.sample_rate, 48_000);
    }

    #[test]
    fn stereo_samples_keep_their_sides() {
        // Interleaved `[l, r, l, r]`. Getting this backwards swaps left and right for every stereo
        // sound in the game, which is both completely inaudible in a test and obvious in headphones.
        let sound = SoundData {
            samples: vec![1.0, -1.0, 0.5, -0.5],
            channels: 2,
            sample_rate: 44_100,
        };
        let data = to_kira("theme", &sound).expect("stereo is playable");
        assert_eq!(data.frames.len(), 2);
        assert_eq!(data.frames[0], Frame::new(1.0, -1.0));
        assert_eq!(data.frames[1], Frame::new(0.5, -0.5));
    }

    #[test]
    fn more_than_two_channels_is_refused_by_name() {
        let sound = SoundData {
            samples: vec![0.0; 30],
            channels: 6,
            sample_rate: 48_000,
        };
        let error = to_kira("surround", &sound).expect_err("5.1 has no stereo pair");
        let text = format!("{error}");
        assert!(text.contains("surround"), "{text}");
        // And it says what to do about it, because "unsupported" is not an actionable report.
        assert!(text.contains("mono"), "{text}");
    }

    #[test]
    fn a_default_listener_faces_along_negative_z_with_no_rotation() {
        // Forward `-Z` and up `+Y` is the identity orientation (ADR 0018). If this came out as
        // anything else, every sound in a world with an unrotated listener would be on the wrong
        // side -- the failure mode this whole function exists to avoid.
        let [x, y, z, w] = orientation_from(&Listener {
            position: [0.0; 3],
            forward: [0.0, 0.0, -1.0],
            up: [0.0, 1.0, 0.0],
        });
        assert!((w - 1.0).abs() < 1e-5, "{w}");
        assert!(x.abs() < 1e-5 && y.abs() < 1e-5 && z.abs() < 1e-5);
    }

    #[test]
    fn a_quarter_turn_is_a_unit_quaternion_of_the_right_angle() {
        // A listener turned to face `-X` -- a quarter turn left about Y. The angle is recoverable
        // from `w = cos(half angle)`, which is `cos(45°)` for 90°.
        let orientation = orientation_from(&Listener {
            position: [0.0; 3],
            forward: [-1.0, 0.0, 0.0],
            up: [0.0, 1.0, 0.0],
        });
        assert!(
            (length_of(orientation) - 1.0).abs() < 1e-5,
            "not a rotation: {orientation:?}"
        );

        // 90 degrees about +Y: `w` is cos(45°) and the whole vector part is on Y.
        let [x, y, z, w] = orientation;
        let half = std::f32::consts::FRAC_1_SQRT_2;
        assert!((w.abs() - half).abs() < 1e-4, "{orientation:?}");
        assert!((y.abs() - half).abs() < 1e-4, "{orientation:?}");
        assert!(x.abs() < 1e-4 && z.abs() < 1e-4, "{orientation:?}");
    }

    /// The length of a quaternion. A rotation has length 1; anything else is not one.
    fn length_of([x, y, z, w]: [f32; 4]) -> f32 {
        (x * x + y * y + z * z + w * w).sqrt()
    }

    #[test]
    fn a_skewed_basis_still_produces_a_rotation() {
        // **The reason the basis is rebuilt rather than trusted.** A `forward` and `up` that are not
        // quite perpendicular arrive routinely from a transform; a quaternion built from them
        // directly is not a rotation, and what that sounds like is the stereo field drifting.
        let orientation = orientation_from(&Listener {
            position: [0.0; 3],
            forward: [0.0, 0.3, -1.0],
            up: [0.0, 1.0, 0.2],
        });
        assert!(
            (length_of(orientation) - 1.0).abs() < 1e-5,
            "not a rotation: {orientation:?}"
        );
    }

    #[test]
    fn a_degenerate_listener_falls_back_rather_than_producing_nans() {
        // Zero vectors reach here from an entity whose transform has not been filled in. A NaN
        // orientation propagates into the mixer and takes the whole output with it.
        let orientation = orientation_from(&Listener {
            position: [0.0; 3],
            forward: [0.0; 3],
            up: [0.0; 3],
        });
        assert!(
            orientation.iter().all(|part| part.is_finite()),
            "{orientation:?}"
        );
        assert!(
            (length_of(orientation) - 1.0).abs() < 1e-5,
            "{orientation:?}"
        );
    }
}
