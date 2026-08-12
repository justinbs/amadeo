//! Audio: turning simulation state into sound — ADR 0059.
//!
//! # The one rule everything here is shaped by
//!
//! **Simulation must never wait on audio, and audio must never change simulation.** Mixing happens
//! on its own thread at its own rate, which has nothing to do with the fixed tick — so if gameplay
//! ever read back "has this finished playing?", the answer would depend on how fast the machine
//! managed to fill a buffer, and invariant I3 would be gone.
//!
//! That is not a rule anyone has to remember, because it is structural: [`Audio`] is a
//! [`Service`], and ADR 0009 puts services outside the state hash entirely.
//! Nothing audio does can reach a replay, and nothing in a replay depends on audio having happened.
//!
//! # The same shape as rendering and physics
//!
//! An [`AudioBackend`] trait, a [`NullAudio`] that must always exist, and a collection pass that
//! reads the world and hands a backend everything it needs. A backend never reaches back into the
//! world. This is `RenderBackend` and `PhysicsBackend` again, for the third time, because the shape
//! is what makes invariant I7 hold — the whole engine runs with no window, no GPU **and no sound
//! card**, which is how CI runs it.
//!
//! ```
//! use amadeo_audio::{Audio, AudioListener, AudioSource, Bus, NullAudio, collect_audio};
//! use amadeo_ecs::World;
//! use amadeo_transform::Transform;
//!
//! let mut world = World::new();
//! world.insert_service(Audio::new(Box::new(NullAudio::new())));
//!
//! // Nothing is audible without ears. A world with no listener submits an empty frame rather
//! // than guessing where to hear from.
//! let ears = world.spawn();
//! world.insert(ears, Transform::at(0.0, 0.0));
//! world.insert(ears, AudioListener);
//!
//! let hum = world.spawn();
//! world.insert(hum, Transform::at(3.0, 0.0));
//! world.insert(hum, AudioSource::looping("generator_hum"));
//!
//! collect_audio(&mut world);
//!
//! // The null backend records what would have been heard — assertable with no sound card.
//! let audio = world.service::<Audio>().expect("installed");
//! let frame = audio.null_backend().expect("null").last_frame().expect("a frame");
//! assert_eq!(frame.voices.len(), 1);
//! assert_eq!(frame.voices[0].sound, "generator_hum");
//! assert_eq!(frame.voices[0].bus, Bus::Effects);
//! ```

mod backend;
mod components;
mod tracker;
mod wav;

pub use backend::{AudioBackend, AudioError, AudioFrame, Listener, NullAudio, SoundData, Voice};
pub use components::{AudioListener, AudioSource, Bus};
pub use tracker::{VoiceChanges, VoiceTracker};
pub use wav::{WavError, decode_wav};

use amadeo_ecs::{Service, World};
use amadeo_transform::{GlobalTransform, Transform};

/// The label the app layer registers [`collect_audio`] under.
pub const COLLECT_AUDIO: &str = "collect_audio";

/// Holds the active audio backend and the last thing it was told.
///
/// A [`Service`]: machinery, never simulation state (ADR 0009). That is what makes the rule at the
/// top of this module structural rather than a convention.
#[derive(Debug)]
pub struct Audio {
    backend: Box<dyn AudioBackend>,
    /// Master gain, applied to every bus. `0.0` is silence, `1.0` is unchanged.
    ///
    /// Here rather than on a component because it is a *setting* — what the player moved a slider
    /// to — and not a property of the world. A scene that authored the master volume would be a
    /// scene that overrode the player's preferences on load.
    pub master: f32,
    /// Per-bus gain, in [`Bus`] order.
    pub buses: [f32; Bus::COUNT],
    /// Set when the last submission failed. Cleared on the next success.
    last_error: Option<AudioError>,
}

impl Service for Audio {}

impl Audio {
    /// Wraps a backend.
    #[must_use]
    pub fn new(backend: Box<dyn AudioBackend>) -> Self {
        Self {
            backend,
            master: 1.0,
            buses: [1.0; Bus::COUNT],
            last_error: None,
        }
    }

    /// An audio system that makes no sound. The default for headless runs.
    #[must_use]
    pub fn headless() -> Self {
        Self::new(Box::new(NullAudio::new()))
    }

    /// The backend's name, for diagnostics.
    #[must_use]
    pub fn backend_name(&self) -> &'static str {
        self.backend.name()
    }

    /// The error from the last failed submission, if it failed.
    ///
    /// Surfaced rather than logged-and-forgotten, for the reason `Renderer::last_error` gives: a game
    /// that is silently making no sound should be diagnosable by asking rather than by guessing.
    #[must_use]
    pub fn last_error(&self) -> Option<&AudioError> {
        self.last_error.as_ref()
    }

    /// The null backend, when that is what is installed.
    ///
    /// **What makes audio testable at all.** There is no way to assert on a sound; there is every way
    /// to assert on the frame that would have produced it.
    #[must_use]
    pub fn null_backend(&self) -> Option<&NullAudio> {
        self.backend.as_null()
    }

    /// Hands the backend a decoded sound to hold under an id.
    ///
    /// # Errors
    ///
    /// [`AudioError`] if the backend cannot hold it.
    pub fn upload(&mut self, id: &str, sound: SoundData) -> Result<(), AudioError> {
        self.backend.upload(id, sound)
    }

    /// Whether the backend already holds a sound under this id.
    #[must_use]
    pub fn has(&self, id: &str) -> bool {
        self.backend.has(id)
    }

    /// The gain a voice on `bus` ends up with, master included.
    #[must_use]
    pub fn gain_for(&self, bus: Bus) -> f32 {
        (self.master * self.buses[bus as usize]).clamp(0.0, 8.0)
    }
}

/// Reads the world and tells the audio backend what should be heard.
///
/// Registered in the app layer's `Render` stage, beside the renderer's collection pass and outside
/// the deterministic zone — for the same reason and with the same consequence: nothing it does can
/// move the state hash.
///
/// # Why this is a collection pass rather than a `play()` call
///
/// A sound that exists because an entity exists is **declarative**: a generator hums because there is
/// a generator, and it stops when the generator is destroyed. Nothing has to remember to stop it, a
/// scene file can author it, `describe` can see it, and a snapshot restores it correctly — because
/// the sound is a function of the world rather than of a call somebody made once.
///
/// This is the same argument ADR 0031 made for the camera being an entity, and it has the same
/// limitation: **a one-shot has no home here yet.** A footstep is not a property of the world, it is
/// an event, and events are what `amadeo-events` is for — see ADR 0059's consequences.
pub fn collect_audio(world: &mut World) {
    let Some(listener) = collect_listener(world) else {
        // No listener means nothing can hear anything. Submitting a frame of voices with no ears to
        // put them in would make a backend guess at a position, and guessing is what produces a
        // sound that comes from the wrong side.
        submit(world, AudioFrame::default());
        return;
    };

    let mut voices: Vec<(amadeo_ecs::Entity, Voice)> = world
        .query::<(&AudioSource, &Transform, Option<&GlobalTransform>)>()
        .filter(|(_, (source, _, _))| source.playing && source.gain > 0.0)
        .map(|(entity, (source, transform, global))| {
            let position = match global {
                Some(global) => global.translation(),
                None => transform.translation,
            };
            (
                entity,
                Voice {
                    source: entity,
                    sound: source.sound.clone(),
                    bus: source.bus,
                    gain: source.gain,
                    pitch: source.pitch,
                    looping: source.looping,
                    // A source with no spatial extent is heard everywhere at full strength, which is
                    // what music and narration want. Distance is the backend's to apply, from this
                    // position and the listener's — the engine does not pre-attenuate, because a
                    // backend that does its own spatialisation would then do it twice.
                    position: if source.spatial { Some(position) } else { None },
                },
            )
        })
        .collect();

    // By entity, so two sources at the same place always reach a backend in the same order. Audio is
    // outside the state hash, so this is about a reproducible *recording* rather than a reproducible
    // simulation — but a test that asserts on a frame is worthless if the order wobbles.
    voices.sort_by_key(|(entity, _)| (entity.index(), entity.generation()));

    let frame = AudioFrame {
        listener: Some(listener),
        voices: voices.into_iter().map(|(_, voice)| voice).collect(),
    };
    submit(world, frame);
}

/// Where the ears are, or `None` if nothing in the world has any.
///
/// Takes the **first** listener by entity when a world has several, which is ADR 0031's "which camera
/// when there are several" rule applied to hearing. Two listeners is a split-screen question and it
/// is not answered here.
fn collect_listener(world: &World) -> Option<Listener> {
    let mut found: Vec<(amadeo_ecs::Entity, Listener)> = world
        .query::<(&AudioListener, &Transform, Option<&GlobalTransform>)>()
        .map(|(entity, (_, transform, global))| {
            let matrix = match global {
                Some(global) => global.to_mat4(),
                None => amadeo_transform::Mat4::from_transform(
                    transform.translation,
                    transform.rotation,
                    transform.scale,
                ),
            };
            // A listener faces the way a camera looks and a light aims: along its own negative Z
            // (ADR 0018). One convention for every directional thing in the engine.
            let forward = [
                -matrix.columns[2][0],
                -matrix.columns[2][1],
                -matrix.columns[2][2],
            ];
            let up = [
                matrix.columns[1][0],
                matrix.columns[1][1],
                matrix.columns[1][2],
            ];
            (
                entity,
                Listener {
                    position: [
                        matrix.columns[3][0],
                        matrix.columns[3][1],
                        matrix.columns[3][2],
                    ],
                    forward,
                    up,
                },
            )
        })
        .collect();

    found.sort_by_key(|(entity, _)| (entity.index(), entity.generation()));
    found.into_iter().next().map(|(_, listener)| listener)
}

/// Applies the bus gains and hands the frame over, recording any failure.
fn submit(world: &mut World, mut frame: AudioFrame) {
    let Some(audio) = world.service::<Audio>() else {
        return;
    };

    // **Bus and master gain are applied here, not in the backend.** A backend that applied them
    // would have to be told them, and two backends could disagree about the order — where a voice
    // arriving with its final gain already in it cannot be misread.
    for voice in &mut frame.voices {
        voice.gain *= audio.gain_for(voice.bus);
    }

    let Some(audio) = world.service_mut::<Audio>() else {
        return;
    };
    audio.last_error = audio.backend.submit(&frame).err();
}

#[cfg(test)]
mod tests {
    use super::*;

    fn world_with_ears() -> World {
        let mut world = World::new();
        world.insert_service(Audio::new(Box::new(NullAudio::new())));
        let ears = world.spawn();
        world.insert(ears, Transform::at(0.0, 0.0));
        world.insert(ears, AudioListener);
        world
    }

    fn last(world: &World) -> AudioFrame {
        world
            .service::<Audio>()
            .expect("installed")
            .null_backend()
            .expect("null")
            .last_frame()
            .expect("a frame was submitted")
            .clone()
    }

    #[test]
    fn a_playing_source_becomes_a_voice() {
        let mut world = world_with_ears();
        let hum = world.spawn();
        world.insert(hum, Transform::at(3.0, 0.0));
        world.insert(hum, AudioSource::looping("hum"));

        collect_audio(&mut world);

        let frame = last(&world);
        assert_eq!(frame.voices.len(), 1);
        assert_eq!(frame.voices[0].sound, "hum");
        assert!(frame.voices[0].looping);
        assert_eq!(frame.voices[0].position, Some([3.0, 0.0, 0.0]));
    }

    #[test]
    fn two_entities_playing_one_sound_are_distinguishable() {
        // **What a backend needs to diff one frame against the last.** An `AudioFrame` is a state
        // rather than a set of commands, so a backend has to work out for itself which voices are
        // new, which are continuing and which have gone.
        //
        // Without an identity on the voice, two entities playing the same sound are the same voice
        // twice, and the only behaviour available is to restart everything every frame — a stutter
        // at sixty hertz rather than a hum. This is the property that makes the diff possible, and
        // it is the one thing rendering does not need: a triangle drawn this frame has nothing to do
        // with one drawn last frame, where a sound is the same sound continuing.
        let mut world = world_with_ears();
        let first = world.spawn();
        world.insert(first, Transform::at(-3.0, 0.0));
        world.insert(first, AudioSource::looping("hum"));

        let second = world.spawn();
        world.insert(second, Transform::at(3.0, 0.0));
        world.insert(second, AudioSource::looping("hum"));

        collect_audio(&mut world);

        let frame = last(&world);
        assert_eq!(frame.voices.len(), 2);
        assert_ne!(
            frame.voices[0].source, frame.voices[1].source,
            "two sources playing one sound must still be two voices a backend can tell apart"
        );
        // And the identities are the entities themselves, so a backend keyed on them stays correct
        // when one is despawned.
        let sources: Vec<_> = frame.voices.iter().map(|voice| voice.source).collect();
        assert!(sources.contains(&first) && sources.contains(&second));
    }

    #[test]
    fn a_stopped_source_is_not_submitted_at_all() {
        // Rather than submitted with zero gain. A backend should never be handed a voice it is meant
        // to ignore -- that is a voice allocated, mixed and multiplied by nothing, every frame, for
        // as long as the entity exists.
        let mut world = world_with_ears();
        let hum = world.spawn();
        world.insert(hum, Transform::at(3.0, 0.0));
        world.insert(
            hum,
            AudioSource {
                playing: false,
                ..AudioSource::looping("hum")
            },
        );

        collect_audio(&mut world);
        assert!(last(&world).voices.is_empty());
    }

    #[test]
    fn a_world_with_no_listener_submits_no_voices() {
        // **Not "submits nothing"** -- an empty frame still goes to the backend, because a backend
        // holding voices from last frame has to be told they are gone. Skipping the submission
        // entirely would leave a sound playing after whatever made it stopped existing.
        let mut world = World::new();
        world.insert_service(Audio::new(Box::new(NullAudio::new())));
        let hum = world.spawn();
        world.insert(hum, Transform::at(3.0, 0.0));
        world.insert(hum, AudioSource::looping("hum"));

        collect_audio(&mut world);

        let frame = last(&world);
        assert!(frame.listener.is_none());
        assert!(frame.voices.is_empty());
    }

    #[test]
    fn bus_and_master_gain_reach_the_voice() {
        let mut world = world_with_ears();
        let hum = world.spawn();
        world.insert(hum, Transform::at(0.0, 0.0));
        world.insert(
            hum,
            AudioSource {
                gain: 0.5,
                bus: Bus::Music,
                ..AudioSource::looping("theme")
            },
        );

        {
            let audio = world.service_mut::<Audio>().expect("installed");
            audio.master = 0.5;
            audio.buses[Bus::Music as usize] = 0.5;
        }
        collect_audio(&mut world);

        // 0.5 authored x 0.5 master x 0.5 bus.
        let frame = last(&world);
        assert!((frame.voices[0].gain - 0.125).abs() < 1e-6);
    }

    #[test]
    fn a_non_spatial_source_carries_no_position() {
        // Music and narration are heard from everywhere. A backend given a position for them would
        // pan the soundtrack as the player turned around, which is the single most obvious way for
        // game audio to sound broken.
        let mut world = world_with_ears();
        let theme = world.spawn();
        world.insert(theme, Transform::at(50.0, 0.0));
        world.insert(
            theme,
            AudioSource {
                spatial: false,
                bus: Bus::Music,
                ..AudioSource::looping("theme")
            },
        );

        collect_audio(&mut world);
        assert_eq!(last(&world).voices[0].position, None);
    }

    #[test]
    fn the_listener_faces_along_its_own_negative_z() {
        // The same convention a camera looks along and a light aims along (ADR 0018). Getting it
        // backwards puts every sound on the wrong side, which is both obvious and impossible to
        // notice in a test that only checks a sound played.
        let mut world = World::new();
        world.insert_service(Audio::new(Box::new(NullAudio::new())));
        let ears = world.spawn();
        world.insert(ears, Transform::at(0.0, 0.0));
        world.insert(ears, AudioListener);

        collect_audio(&mut world);

        let listener = last(&world).listener.expect("one listener");
        assert_eq!(listener.forward, [0.0, 0.0, -1.0]);
        assert_eq!(listener.up, [0.0, 1.0, 0.0]);
    }

    #[test]
    fn audio_cannot_move_the_state_hash() {
        // **The claim the whole module is shaped by**, checked rather than asserted in prose. A
        // service is outside the hash by construction (ADR 0009), so this is really a test that
        // `collect_audio` writes nothing anywhere else -- which it could easily do by accident, for
        // instance by clearing a `playing` flag.
        let mut world = world_with_ears();
        let hum = world.spawn();
        world.insert(hum, Transform::at(3.0, 0.0));
        world.insert(hum, AudioSource::looping("hum"));

        let before = world.state_hash();
        collect_audio(&mut world);
        assert_eq!(before, world.state_hash());
    }
}
