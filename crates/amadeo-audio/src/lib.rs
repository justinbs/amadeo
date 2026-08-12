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
mod sounds;
mod tracker;
mod wav;

#[cfg(feature = "kira")]
mod kira_backend;

pub use backend::{
    AudioBackend, AudioError, AudioFrame, Listener, NullAudio, OneShot, SoundData, Voice,
};
pub use components::{AudioListener, AudioSource, Bus, SoundPlayed};
pub use sounds::{SoundCache, SoundFailure};

/// The label `audio.describe` is served under, so a host and the CLI cannot disagree about it.
pub const AUDIO_DESCRIBE: &str = "audio.describe";
pub use tracker::{VoiceChanges, VoiceTracker};
pub use wav::{WavError, decode_wav};

#[cfg(feature = "kira")]
pub use kira_backend::KiraAudio;

use amadeo_assets::Assets;
use amadeo_ecs::{Service, World};
use amadeo_events::WorldEvents;
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
    /// The lowest [`SoundPlayed`] sequence number **not yet** handed to a backend.
    ///
    /// A half-open bound — "everything below this has played" — rather than "the highest that has".
    /// That is not a stylistic choice: `EventClock` hands out sequence numbers **starting at zero**,
    /// so a `last_played` field initialised to 0 with a `sequence > last_played` filter drops the
    /// very first event a world ever sends. It did, and the symptom was the first footstep in the
    /// game being silent and every one after it working — which is close to undiagnosable by ear.
    ///
    /// # The bug this exists to prevent
    ///
    /// **The render rate is not the tick rate.** `collect_audio` runs in the `Render` stage, the
    /// event buffers swap at the *tick* boundary, and the windowed loop renders as fast as it can —
    /// so a single footstep event sits in the readable buffer across every frame drawn during that
    /// tick. Reading it naively plays one footstep per rendered frame, which at 300 fps against a
    /// 60 Hz tick is five overlapping copies of the same sound.
    ///
    /// `EventRecord::sequence` is strictly increasing across every event type, so a high-water mark
    /// is all that is needed. It lives here rather than in a resource because it is machinery: a
    /// service is outside the state hash (ADR 0009), and how many times a frame happened to be drawn
    /// is exactly the sort of thing that must never reach a replay.
    next_one_shot: u64,
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
            next_one_shot: 0,
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
    let frame = build_frame(world);

    // **Advance the high-water mark before submitting, not after.** These have now been handed over,
    // and a backend that failed on one should not be given it again on the next frame — a failing
    // one-shot re-offered every frame is a stutter on top of whatever the original problem was.
    if let Some(highest) = highest_one_shot_sequence(world)
        && let Some(audio) = world.service_mut::<Audio>()
    {
        audio.next_one_shot = highest + 1;
    }

    submit(world, frame);
}

/// The largest sequence number among the one-shot events this frame will carry.
///
/// Separate from [`build_frame`] because that function is shared with [`describe_audio`], which must
/// not advance anything — describing a world is a question, and a question that consumed the thing it
/// asked about would make an agent's introspection part of the game.
fn highest_one_shot_sequence(world: &World) -> Option<u64> {
    let from = world
        .service::<Audio>()
        .map_or(0, |audio| audio.next_one_shot);
    world
        .read_events::<SoundPlayed>()
        .iter()
        .map(|record| record.sequence)
        .filter(|sequence| *sequence >= from)
        .max()
}

/// Reads the world and works out what should be audible, gains included.
///
/// Shared by [`collect_audio`] and [`describe_audio`] — **one implementation, deliberately.** Two
/// copies of "what should be audible" would drift, and the way that failure presents is an agent
/// being told the game is playing something it is not, which is worse than no answer at all. This is
/// the fifth instance of the same lesson in this engine; see `docs/07`.
fn build_frame(world: &World) -> AudioFrame {
    let Some(listener) = collect_listener(world) else {
        // No listener means nothing can hear anything. A frame of voices with no ears to put them in
        // would make a backend guess at a position, and guessing is what produces a sound that comes
        // from the wrong side.
        //
        // **One-shots are dropped here too, deliberately.** A footstep in a world with no ears is
        // not a footstep heard from nowhere; it is a footstep nobody was there for.
        return AudioFrame::default();
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

    // One-shots that have not been handed over yet, in send order — which `EventRecord::sequence`
    // gives for free and which is the order they actually happened in.
    let from = world
        .service::<Audio>()
        .map_or(0, |audio| audio.next_one_shot);
    let one_shots: Vec<OneShot> = world
        .read_events::<SoundPlayed>()
        .iter()
        .filter(|record| record.sequence >= from)
        .map(|record| OneShot {
            sound: record.event.sound.clone(),
            bus: record.event.bus,
            gain: record.event.gain,
            pitch: record.event.pitch,
            position: if record.event.spatial {
                Some(record.event.position)
            } else {
                None
            },
        })
        .collect();

    let mut frame = AudioFrame {
        listener: Some(listener),
        voices: voices.into_iter().map(|(_, voice)| voice).collect(),
        one_shots,
    };

    // **Bus and master gain are applied here, not in a backend.** A backend that applied them would
    // have to be told them, and two backends could disagree about the order — where a voice arriving
    // with its final gain already in it cannot be misread.
    //
    // It also means `describe_audio` reports the gain a voice would actually be heard at, rather than
    // the one its component authored.
    if let Some(audio) = world.service::<Audio>() {
        for voice in &mut frame.voices {
            voice.gain *= audio.gain_for(voice.bus);
        }
        // The same multiply, for the same reason. Easy to forget precisely because one-shots were
        // added later, and what it sounds like when forgotten is a footstep that ignores the volume
        // slider.
        for one_shot in &mut frame.one_shots {
            one_shot.gain *= audio.gain_for(one_shot.bus);
        }
    }
    frame
}

/// What the world sounds like, as data — the agent's ears, and the answer to "why is it silent".
///
/// # Why this exists at all
///
/// ADR 0060 decided that a sound which will not load is **silent**, with `SoundCache::failures` as
/// the whole diagnosis. That claim is only true if something can *read* the report, and until this
/// existed nothing outside Rust could: an agent or a person asking why a game was quiet had no
/// channel to ask through, which quietly made ADR 0060 a worse decision than it was written as.
///
/// Invariant I5 says it plainly — anything the editor could do, the CLI and RPC can. Silence is the
/// audio equivalent of a blank screen, and `render.describe` has answered that for the screen since
/// M1.
///
/// # It reads the world, not the last frame
///
/// The same choice `render.describe` makes, and here it is load-bearing rather than merely tidy:
/// `NullAudio` remembers the last frame it was given and **a real backend does not**, so reading
/// back from the backend would work headlessly and answer nothing in the game somebody is actually
/// playing.
#[derive(Debug, Clone, PartialEq)]
pub struct AudioDescription {
    /// Whether an [`Audio`] service is installed at all. `false` means no system is running, which
    /// is a different problem from one that is running and silent.
    pub installed: bool,
    /// The backend's name — `"null"` or `"kira"`. **The most common explanation of silence**, since
    /// every headless build installs the null one deliberately.
    pub backend: &'static str,
    /// Master gain.
    pub master: f32,
    /// Per-bus gain, in [`Bus`] order.
    pub buses: [f32; Bus::COUNT],
    /// What should be audible now, gains applied — exactly what a backend would be handed.
    pub frame: AudioFrame,
    /// Asset ids whose samples are decoded and ready.
    pub decoded: Vec<String>,
    /// Ids that would not load, and why. In id order.
    pub failures: Vec<(String, String)>,
    /// The last submission failure, if the last one failed.
    pub last_error: Option<String>,
}

impl AudioDescription {
    /// A one-line explanation of why nothing is audible, or `None` if something should be.
    ///
    /// # Why the engine writes this sentence rather than the caller
    ///
    /// Every one of these causes is invisible from the outside and each has a different fix, and the
    /// order matters: a world with no listener submits no voices *at all*, so "there are no voices"
    /// is a true and useless thing to report when the real answer is "nothing has ears". Working
    /// that out from the fields is exactly the reasoning an agent would have to redo, badly, on
    /// every call.
    #[must_use]
    pub fn silent_because(&self) -> Option<String> {
        if !self.installed {
            return Some(
                "no audio system is installed. A game inserts one with \
                 `app.insert_service(Audio::new(..))` before it runs"
                    .to_string(),
            );
        }
        if self.frame.listener.is_none() {
            return Some(
                "nothing in the world has an `AudioListener`, so there are no ears to hear from \
                 and no voices are submitted at all. Put one on the camera or the character"
                    .to_string(),
            );
        }
        if self.frame.voices.is_empty() {
            return Some(
                "no entity is making a sound. An `AudioSource` needs `playing` true and a `gain` \
                 above zero to become a voice"
                    .to_string(),
            );
        }
        if self.master <= 0.0 {
            return Some("the master gain is zero".to_string());
        }
        if self.frame.voices.iter().all(|voice| voice.gain <= 0.0) {
            return Some(
                "every voice has ended up at zero gain, which means a bus gain is zero".to_string(),
            );
        }
        // Deliberately last, and deliberately not fatal: this is the *normal* headless case, so
        // reporting it above a real fault would bury the real fault.
        if self.backend == "null" {
            return Some(
                "the null backend is installed, which makes no sound by design. Every headless \
                 build uses it; only a windowed build swaps in a real one"
                    .to_string(),
            );
        }
        None
    }
}

/// Reads the world and reports what it sounds like.
///
/// Costs nothing when nobody is asking, exactly as `render.describe` does — it is a query, not a
/// recording, and no part of the audio path pays for it existing.
#[must_use]
pub fn describe_audio(world: &World) -> AudioDescription {
    let frame = build_frame(world);

    let Some(audio) = world.service::<Audio>() else {
        return AudioDescription {
            installed: false,
            backend: "none",
            master: 0.0,
            buses: [0.0; Bus::COUNT],
            frame,
            decoded: Vec::new(),
            failures: Vec::new(),
            last_error: None,
        };
    };

    let (decoded, failures) = match world.service::<SoundCache>() {
        Some(cache) => {
            // Deduplicated and sorted, because two entities can play one sound and a listing that
            // repeated it would read as two. A `BTreeSet` rather than sorting afterwards, so the
            // order is not an accident of how the frame happened to be built.
            let decoded: std::collections::BTreeSet<String> = frame
                .voices
                .iter()
                .map(|voice| &voice.sound)
                .chain(frame.one_shots.iter().map(|shot| &shot.sound))
                .filter(|sound| cache.is_decoded(sound))
                .cloned()
                .collect();
            (
                decoded.into_iter().collect(),
                cache
                    .failures()
                    .map(|(id, failure)| (id.to_string(), failure.to_string()))
                    .collect(),
            )
        }
        // No cache is not a fault. A game may upload its sounds by hand, and a test certainly does.
        None => (Vec::new(), Vec::new()),
    };

    AudioDescription {
        installed: true,
        backend: audio.backend_name(),
        master: audio.master,
        buses: audio.buses,
        frame,
        decoded,
        failures,
        last_error: audio.last_error().map(|error| error.to_string()),
    }
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

/// Decodes every sound this frame names and hands it to the backend, if it does not have it yet.
///
/// The audio counterpart of the renderer's `decode_frame_textures`, and it needs the same shape for
/// the same reason: [`SoundCache`] mutably, `Assets` shared, and [`Audio`] mutably are three entries
/// in one service map, so the cache is taken out of the world for the duration.
///
/// Does nothing if no [`SoundCache`] is installed — a headless test that installed no asset system
/// still submits frames, and its backend simply has nothing to play.
///
/// # Why decoding is here rather than at ADR 0021's load barrier
///
/// The *bytes* are behind the barrier and are resident before the first tick. Decoding them is a
/// pure function of those bytes, so doing it on first use cannot observe anything the barrier was
/// protecting. The same trade `TextureCache` takes, with the same cost: a possible hitch the first
/// time a sound is heard, left untuned until one is measured.
fn ensure_sounds(world: &mut World, frame: &AudioFrame) {
    if !world.has_service::<SoundCache>() || (frame.voices.is_empty() && frame.one_shots.is_empty())
    {
        return;
    }

    // Every id this frame names, from both lists. **A one-shot is the case that suffers most from
    // being loaded late**: a hum that starts a frame after it should have is inaudible, where a
    // footstep that arrives after the frame carrying it is simply never heard at all.
    let wanted: Vec<&str> = frame
        .voices
        .iter()
        .map(|voice| voice.sound.as_str())
        .chain(frame.one_shots.iter().map(|shot| shot.sound.as_str()))
        .collect();

    world.with_service_taken::<SoundCache, ()>(|world, cache| {
        if let Some(assets) = world.service::<Assets>() {
            for sound in &wanted {
                cache.ensure(sound, assets);
            }
        }

        let Some(audio) = world.service_mut::<Audio>() else {
            return;
        };
        for sound in &wanted {
            if audio.has(sound) {
                continue;
            }
            let Some(decoded) = cache.get(sound) else {
                // Missing or undecodable. Silence plus the report in `SoundCache::failures`, which
                // is the whole of the diagnosis — see the note there about why there is no
                // placeholder sound.
                continue;
            };
            // Cloned because the backend takes ownership and the cache keeps its copy for the next
            // backend — a device lost and reacquired re-uploads from here rather than re-reading
            // the file. Once per id, not once per frame: the `has` check above is what makes that
            // true.
            let error = audio.upload(sound, decoded.clone()).err();
            if error.is_some() {
                audio.last_error = error;
            }
        }
    });
}

/// Hands the frame over, recording any failure. Gains are already in it — see [`build_frame`].
fn submit(world: &mut World, frame: AudioFrame) {
    if !world.has_service::<Audio>() {
        return;
    }

    // **Before the submission, not after.** A backend is asked to start a voice during `submit`, so
    // a sound that arrived at the backend afterwards would be missing for exactly the frame that
    // needed it — which is inaudible for a hum and very audible for anything short.
    ensure_sounds(world, &frame);

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

    /// A world with ears and the one-shot event registered.
    fn world_that_can_hear_one_shots() -> World {
        let mut world = world_with_ears();
        world.register_event::<SoundPlayed>();
        world
    }

    fn heard_one_shots(world: &World) -> Vec<String> {
        world
            .service::<Audio>()
            .expect("installed")
            .null_backend()
            .expect("null")
            .one_shots()
            .to_vec()
    }

    #[test]
    fn a_sent_event_becomes_a_one_shot_once_the_buffers_swap() {
        // Double-buffered: sent this tick, readable next (ADR's `amadeo-events` model). That is one
        // tick of latency on a footstep, which is 16 ms and inaudible.
        let mut world = world_that_can_hear_one_shots();
        world.send_event(SoundPlayed::at("footstep", [1.0, 0.0, 2.0]));

        collect_audio(&mut world);
        assert!(
            heard_one_shots(&world).is_empty(),
            "not readable until the buffers swap"
        );

        world.swap_events::<SoundPlayed>();
        collect_audio(&mut world);

        assert_eq!(heard_one_shots(&world), vec!["footstep".to_string()]);
        let frame = last(&world);
        assert_eq!(frame.one_shots[0].position, Some([1.0, 0.0, 2.0]));
    }

    #[test]
    fn the_very_first_one_shot_a_world_ever_sends_is_heard() {
        // **Written after watching it fail.** `EventClock` hands out sequence numbers starting at
        // **zero**, so a high-water mark called "the highest already played", initialised to 0 and
        // filtered with `sequence > mark`, drops event number zero — the first sound the game ever
        // makes. Every one after it works, which is what makes it nearly undiagnosable by ear: you
        // would conclude the *first* footstep of a session was swallowed by something else.
        //
        // The field is now a half-open bound ("everything below this has played") and the filter is
        // `>=`, which is the standard way not to have this bug. This test is what says so.
        let mut world = world_that_can_hear_one_shots();
        assert_eq!(
            world
                .resource::<amadeo_events::EventClock>()
                .expect("registered")
                .sent_count(),
            0,
            "this test is only meaningful in a world that has never sent an event"
        );

        world.send_event(SoundPlayed::at("first", [0.0, 0.0, 0.0]));
        world.swap_events::<SoundPlayed>();
        collect_audio(&mut world);

        assert_eq!(heard_one_shots(&world), vec!["first".to_string()]);
    }

    #[test]
    fn one_event_is_one_sound_however_many_times_the_frame_is_drawn() {
        // **The bug this whole path is shaped around.** `collect_audio` runs in the `Render` stage,
        // buffers swap at the *tick* boundary, and the windowed loop renders as fast as it can — so
        // one footstep event sits in the readable buffer across every frame drawn during that tick.
        // Without the sequence high-water mark, a 300 fps machine plays five overlapping copies and
        // a 60 fps machine plays one, which is the worst kind of bug: it depends on the hardware.
        let mut world = world_that_can_hear_one_shots();
        world.send_event(SoundPlayed::at("footstep", [0.0, 0.0, 0.0]));
        world.swap_events::<SoundPlayed>();

        for _ in 0..5 {
            collect_audio(&mut world);
        }

        assert_eq!(
            heard_one_shots(&world),
            vec!["footstep".to_string()],
            "five renders of one tick must be one footstep"
        );
    }

    #[test]
    fn two_events_in_one_tick_are_two_sounds() {
        // The other half of the same guarantee, and the one a naive "have I seen this?" flag gets
        // wrong: deduplicating too eagerly turns a burst of gunfire into a single shot.
        let mut world = world_that_can_hear_one_shots();
        world.send_event(SoundPlayed::at("shot", [0.0, 0.0, 0.0]));
        world.send_event(SoundPlayed::at("shot", [1.0, 0.0, 0.0]));
        world.send_event(SoundPlayed::everywhere("shell"));
        world.swap_events::<SoundPlayed>();

        collect_audio(&mut world);
        collect_audio(&mut world);

        assert_eq!(heard_one_shots(&world).len(), 3);
    }

    #[test]
    fn a_one_shot_gets_the_bus_and_master_gain_like_everything_else() {
        // Easy to miss because one-shots were added after voices, and what it sounds like when
        // missed is a footstep that ignores the volume slider.
        let mut world = world_that_can_hear_one_shots();
        {
            let audio = world.service_mut::<Audio>().expect("installed");
            audio.master = 0.5;
            audio.buses[Bus::Interface as usize] = 0.5;
        }
        world.send_event(SoundPlayed::everywhere("click"));
        world.swap_events::<SoundPlayed>();

        collect_audio(&mut world);

        let frame = last(&world);
        assert_eq!(frame.one_shots.len(), 1);
        assert!((frame.one_shots[0].gain - 0.25).abs() < 1e-6);
        // `everywhere` puts it on `Interface` on purpose: a menu click must not duck under a
        // waterfall, which is the whole reason that bus is separate.
        assert_eq!(frame.one_shots[0].bus, Bus::Interface);
        assert_eq!(frame.one_shots[0].position, None);
    }

    #[test]
    fn a_one_shot_in_a_world_with_no_ears_is_not_heard() {
        // Not "heard from nowhere" — nobody was there for it. A world with no listener submits an
        // empty frame, and a one-shot smuggled through that would be the one sound in the game that
        // did not need ears.
        let mut world = World::new();
        world.insert_service(Audio::new(Box::new(NullAudio::new())));
        world.register_event::<SoundPlayed>();
        world.send_event(SoundPlayed::at("footstep", [0.0, 0.0, 0.0]));
        world.swap_events::<SoundPlayed>();

        collect_audio(&mut world);
        assert!(heard_one_shots(&world).is_empty());
    }

    #[test]
    fn describing_a_world_does_not_consume_its_one_shots() {
        // **A question must not change the answer.** `describe_audio` shares `build_frame` with
        // `collect_audio`, so it sees the pending one-shot — and it must leave the high-water mark
        // alone, or an agent asking what the game sounds like would silence the next footstep.
        let mut world = world_that_can_hear_one_shots();
        world.send_event(SoundPlayed::at("footstep", [0.0, 0.0, 0.0]));
        world.swap_events::<SoundPlayed>();

        assert_eq!(describe_audio(&world).frame.one_shots.len(), 1);
        assert_eq!(describe_audio(&world).frame.one_shots.len(), 1);

        collect_audio(&mut world);
        assert_eq!(heard_one_shots(&world), vec!["footstep".to_string()]);
    }

    #[test]
    fn a_one_shot_cannot_move_the_state_hash_when_it_plays() {
        // The event itself *is* hashed — that is the point, since deciding to play a footstep is
        // gameplay. What must not move the hash is the **playing**, which happens in the `Render`
        // stage and touches only services.
        let mut world = world_that_can_hear_one_shots();
        world.send_event(SoundPlayed::at("footstep", [0.0, 0.0, 0.0]));
        world.swap_events::<SoundPlayed>();

        let before = world.state_hash();
        collect_audio(&mut world);
        collect_audio(&mut world);
        assert_eq!(before, world.state_hash());
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
