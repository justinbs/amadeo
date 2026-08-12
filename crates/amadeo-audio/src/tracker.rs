//! Turning "what should be audible" into "what to start, stop and change".
//!
//! # Why this is here rather than in each backend
//!
//! An [`AudioFrame`] is a **state** (ADR 0059): these are the sounds that should be audible now. A
//! backend has to reconcile that against what it is already playing, and that reconciliation is
//! fiddly in ways that are inaudible until they are not — a voice restarted every frame is a stutter
//! at sixty hertz, and a voice never stopped is a hum that outlives the thing making it.
//!
//! It is also **the only part of a backend that can be tested without a sound card.** Everything else
//! a real backend does ends in a speaker, where neither CI nor a headless run can follow it. So the
//! logic lives here, is exercised headlessly, and a backend is left with the part that genuinely
//! needs a device: start this, stop that, change the other.
//!
//! This does not weaken ADR 0059's contract. A backend still receives a state and is still free to
//! reconcile it however it likes; this is the shared answer for the ones that have no reason to
//! invent their own.

use crate::backend::{AudioFrame, Voice};
use amadeo_ecs::Entity;
use std::collections::BTreeMap;

/// What a backend should do to catch up with a frame.
///
/// Ordered so a backend can apply the fields in the order they are declared without thinking about
/// it: **stop first**, then start, then adjust. Stopping first is what keeps the number of live
/// voices from spiking when a scene swaps one sound for another.
#[derive(Debug, Clone, PartialEq, Default)]
pub struct VoiceChanges {
    /// Voices that should no longer be playing, by the entity that was making them.
    ///
    /// Includes an entity whose sound *changed*: swapping a clip is a stop and a start, because
    /// there is no such thing as continuing a different sound.
    pub stopped: Vec<Entity>,
    /// Voices that were not playing and now should be.
    pub started: Vec<Voice>,
    /// Voices already playing whose gain, pitch or position has moved.
    ///
    /// **Only when something actually changed**, which matters more than it looks: a backend told to
    /// re-apply an identical gain every frame is a backend restarting a tween sixty times a second,
    /// and on most audio libraries that is audible as a sound that never quite settles.
    pub updated: Vec<Voice>,
}

impl VoiceChanges {
    /// Whether there is nothing to do.
    #[must_use]
    pub fn is_empty(&self) -> bool {
        self.stopped.is_empty() && self.started.is_empty() && self.updated.is_empty()
    }
}

/// What one voice looked like the last time a backend was told about it.
#[derive(Debug, Clone, PartialEq)]
struct Playing {
    sound: String,
    gain: f32,
    pitch: f32,
    position: Option<[f32; 3]>,
    looping: bool,
}

impl Playing {
    fn of(voice: &Voice) -> Self {
        Self {
            sound: voice.sound.clone(),
            gain: voice.gain,
            pitch: voice.pitch,
            position: voice.position,
            looping: voice.looping,
        }
    }

    /// Whether this voice needs re-applying, given what it looks like now.
    ///
    /// Compared with a tolerance rather than exactly, because a position that arrives from a
    /// transform wobbles in its last bits every frame — and a backend told to move a sound by a
    /// micrometre sixty times a second is doing work no one can hear.
    fn differs_from(&self, voice: &Voice) -> bool {
        const TOLERANCE: f32 = 1e-4;

        if (self.gain - voice.gain).abs() > TOLERANCE
            || (self.pitch - voice.pitch).abs() > TOLERANCE
            || self.looping != voice.looping
        {
            return true;
        }

        match (self.position, voice.position) {
            (None, None) => false,
            (Some(was), Some(now)) => (0..3).any(|axis| (was[axis] - now[axis]).abs() > TOLERANCE),
            // Gaining or losing a position is a change of kind, not of degree: a sound that stops
            // being spatial has to stop being panned.
            _ => true,
        }
    }
}

/// Remembers what a backend is playing, so a frame can be turned into changes.
///
/// One per backend. Keyed by entity, which is the identity [`Voice::source`] exists to provide.
#[derive(Debug, Default)]
pub struct VoiceTracker {
    /// A `BTreeMap` rather than a hash map, like every other registry in this engine: the change
    /// lists below come out in a total order, so a test can assert on them and two runs agree.
    playing: BTreeMap<Entity, Playing>,
}

impl VoiceTracker {
    /// A tracker that believes nothing is playing.
    #[must_use]
    pub fn new() -> Self {
        Self::default()
    }

    /// How many voices it believes are playing.
    #[must_use]
    pub fn live(&self) -> usize {
        self.playing.len()
    }

    /// Works out what has to change for the world to sound like `frame`, and records the result.
    ///
    /// **Records it immediately**, on the assumption the caller applies every change. A backend that
    /// failed halfway would leave this believing something is playing that is not — which is the
    /// right trade, because the alternative is a tracker that has to be told about each success and
    /// a backend that has to remember to tell it.
    pub fn reconcile(&mut self, frame: &AudioFrame) -> VoiceChanges {
        let mut changes = VoiceChanges::default();
        let mut seen: BTreeMap<Entity, Playing> = BTreeMap::new();

        for voice in &frame.voices {
            match self.playing.get(&voice.source) {
                // Same entity, same sound: it is the one already playing.
                Some(was) if was.sound == voice.sound => {
                    if was.differs_from(voice) {
                        changes.updated.push(voice.clone());
                    }
                }
                // Same entity, **different sound**: there is no continuing a different clip, so this
                // is a stop and a start. Easy to miss, and what it sounds like when missed is a
                // source that changed its sound and did not.
                Some(_) => {
                    changes.stopped.push(voice.source);
                    changes.started.push(voice.clone());
                }
                None => changes.started.push(voice.clone()),
            }
            seen.insert(voice.source, Playing::of(voice));
        }

        // Anything that was playing and is not in the frame has gone — the entity was despawned, its
        // source stopped, or it moved out of earshot. This is what makes the collection pass
        // declarative: nobody had to remember to stop it.
        for entity in self.playing.keys() {
            if !seen.contains_key(entity) {
                changes.stopped.push(*entity);
            }
        }

        self.playing = seen;
        changes
    }

    /// Forgets everything, as though nothing were playing.
    ///
    /// For a backend that has been torn down and rebuilt — a device lost and reacquired — where
    /// every voice has genuinely stopped and the next frame should start them all again.
    pub fn clear(&mut self) {
        self.playing.clear();
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::backend::Listener;
    use crate::components::Bus;
    use amadeo_ecs::World;

    fn voice(source: Entity, sound: &str) -> Voice {
        Voice {
            source,
            sound: sound.to_string(),
            bus: Bus::Effects,
            gain: 1.0,
            pitch: 1.0,
            looping: true,
            position: Some([0.0, 0.0, 0.0]),
        }
    }

    fn frame(voices: Vec<Voice>) -> AudioFrame {
        AudioFrame {
            // Always empty here, and that is the point: a one-shot is not reconciled, so nothing in
            // this file should ever look at one. If a `VoiceTracker` change ever needs this field,
            // that change is in the wrong place.
            one_shots: Vec::new(),
            listener: Some(Listener {
                position: [0.0; 3],
                forward: [0.0, 0.0, -1.0],
                up: [0.0, 1.0, 0.0],
            }),
            voices,
        }
    }

    #[test]
    fn a_new_voice_starts() {
        let mut world = World::new();
        let hum = world.spawn();
        let mut tracker = VoiceTracker::new();

        let changes = tracker.reconcile(&frame(vec![voice(hum, "hum")]));
        assert_eq!(changes.started.len(), 1);
        assert!(changes.stopped.is_empty() && changes.updated.is_empty());
        assert_eq!(tracker.live(), 1);
    }

    #[test]
    fn an_unchanged_voice_does_nothing_at_all() {
        // **The property that makes this worth having.** The same frame twice must produce no work:
        // a backend told to restart or re-tween an identical voice every frame is a stutter or a
        // sound that never settles, and both are the sort of thing that gets blamed on the library.
        let mut world = World::new();
        let hum = world.spawn();
        let mut tracker = VoiceTracker::new();

        let steady = frame(vec![voice(hum, "hum")]);
        tracker.reconcile(&steady);
        let again = tracker.reconcile(&steady);

        assert!(
            again.is_empty(),
            "an unchanged frame should ask a backend to do nothing, got {again:?}"
        );
    }

    #[test]
    fn a_vanished_voice_stops() {
        // The declarative half: the entity was despawned or its source stopped, and nobody had to
        // remember to stop the sound.
        let mut world = World::new();
        let hum = world.spawn();
        let mut tracker = VoiceTracker::new();

        tracker.reconcile(&frame(vec![voice(hum, "hum")]));
        let changes = tracker.reconcile(&frame(Vec::new()));

        assert_eq!(changes.stopped, vec![hum]);
        assert_eq!(tracker.live(), 0);
    }

    #[test]
    fn changing_the_sound_is_a_stop_and_a_start() {
        // **The case most likely to be got wrong**, because it looks like an update: the entity is
        // the same and it is still playing something. But there is no continuing a different clip —
        // treating it as an update leaves the old sound running and silently ignores the new one.
        let mut world = World::new();
        let radio = world.spawn();
        let mut tracker = VoiceTracker::new();

        tracker.reconcile(&frame(vec![voice(radio, "static")]));
        let changes = tracker.reconcile(&frame(vec![voice(radio, "music")]));

        assert_eq!(changes.stopped, vec![radio]);
        assert_eq!(changes.started.len(), 1);
        assert_eq!(changes.started[0].sound, "music");
        assert!(changes.updated.is_empty());
    }

    #[test]
    fn a_moved_voice_is_updated_rather_than_restarted() {
        // A sound that moves must keep playing. Restarting it is the most obvious possible bug and
        // the easiest to write, because "the voice changed" and "the voice is new" are one branch
        // apart.
        let mut world = World::new();
        let bee = world.spawn();
        let mut tracker = VoiceTracker::new();

        tracker.reconcile(&frame(vec![voice(bee, "buzz")]));
        let moved = Voice {
            position: Some([5.0, 0.0, 0.0]),
            ..voice(bee, "buzz")
        };
        let changes = tracker.reconcile(&frame(vec![moved]));

        assert!(changes.started.is_empty() && changes.stopped.is_empty());
        assert_eq!(changes.updated.len(), 1);
        assert_eq!(changes.updated[0].position, Some([5.0, 0.0, 0.0]));
    }

    #[test]
    fn a_position_that_wobbles_in_its_last_bits_is_not_a_change() {
        // A position arrives from a transform, and a transform recomputed every tick moves in its
        // last bits even when nothing is moving. Without a tolerance, every spatial sound in the
        // world would be re-positioned sixty times a second forever.
        let mut world = World::new();
        let bee = world.spawn();
        let mut tracker = VoiceTracker::new();

        tracker.reconcile(&frame(vec![voice(bee, "buzz")]));
        let jittered = Voice {
            position: Some([1e-7, -1e-7, 1e-7]),
            ..voice(bee, "buzz")
        };
        assert!(tracker.reconcile(&frame(vec![jittered])).is_empty());
    }

    #[test]
    fn losing_a_position_is_a_change_even_at_the_same_place() {
        // Spatial to non-spatial is a change of *kind*: a sound that stops being placed has to stop
        // being panned, and comparing only the coordinates would miss it entirely.
        let mut world = World::new();
        let voice_over = world.spawn();
        let mut tracker = VoiceTracker::new();

        tracker.reconcile(&frame(vec![voice(voice_over, "line")]));
        let everywhere = Voice {
            position: None,
            ..voice(voice_over, "line")
        };
        let changes = tracker.reconcile(&frame(vec![everywhere]));

        assert_eq!(changes.updated.len(), 1);
        assert_eq!(changes.updated[0].position, None);
    }

    #[test]
    fn clearing_makes_the_next_frame_start_everything_again() {
        // For a device lost and reacquired: every voice has genuinely stopped, so the tracker must
        // not believe otherwise or the next frame would start nothing and the world would go silent.
        let mut world = World::new();
        let hum = world.spawn();
        let mut tracker = VoiceTracker::new();

        let steady = frame(vec![voice(hum, "hum")]);
        tracker.reconcile(&steady);
        tracker.clear();

        let changes = tracker.reconcile(&steady);
        assert_eq!(changes.started.len(), 1);
    }
}
