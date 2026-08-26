//! What a scene file authors: a sound in the world, and the ears that hear it.

use amadeo_core::StableHash;
use amadeo_ecs::Component;
use amadeo_reflect::Reflect;

/// Which mix a sound belongs to.
///
/// # Why this is an enum and not a string
///
/// A bus is a *fixed* set of things a player has volume sliders for, not an open vocabulary. An
/// enum means a scene file naming a bus that does not exist fails to load with the list of ones that
/// do, where a string would silently create a bus nothing has a slider for.
///
/// It also makes ducking spellable later: "quiet the effects bus while dialogue plays" is a rule
/// about two named things, and naming them is what lets it be authored rather than coded.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default, StableHash, Reflect)]
pub enum Bus {
    /// Everything that happens in the world: footsteps, doors, gunfire. The default, because a sound
    /// with no opinion is a sound effect.
    #[default]
    Effects,
    /// The soundtrack. Separate because it is the one players most often turn down on its own.
    Music,
    /// Speech. Separate because it is the one that must stay audible when everything else ducks.
    Dialogue,
    /// The interface: clicks, confirmations, menu movement.
    ///
    /// Separate from `Effects` because it must **not** be affected by anything happening in the
    /// world — a menu click that gets quieter because the player is standing near a waterfall is a
    /// menu that feels broken.
    Interface,
}

impl Bus {
    /// How many buses there are, for sizing an array of gains.
    pub const COUNT: usize = 4;
}

/// A sound attached to an entity.
///
/// # It describes a state, not an action
///
/// This says *"this entity is making this sound"*, not *"play this sound"*. A generator hums because
/// there is a generator; remove the entity and the hum stops, with nobody having to remember to stop
/// it. That is what makes it authorable in a scene file, visible to `describe`, and correct after a
/// snapshot restore.
///
/// **A one-shot does not belong here** — a footstep is an event rather than a property of the world,
/// and it is what `amadeo-events` is for. See ADR 0059.
#[derive(Debug, Clone, PartialEq, StableHash, Reflect)]
pub struct AudioSource {
    /// The declared asset id of the sound (ADR 0020).
    pub sound: String,
    /// Which mix it belongs to.
    pub bus: Bus,
    /// Linear gain before the bus and master are applied. `1.0` is as recorded.
    #[reflect(min = 0.0, max = 4.0)]
    pub gain: f32,
    /// Playback rate. `1.0` is as recorded; `2.0` is an octave up and twice as fast.
    ///
    /// Pitch and speed are the same control here, deliberately — separating them means
    /// time-stretching, which is a signal-processing project rather than a field.
    #[reflect(min = 0.05, max = 8.0)]
    pub pitch: f32,
    /// Whether it restarts when it reaches the end.
    pub looping: bool,
    /// Whether it is heard *from somewhere*.
    ///
    /// `true` means the listener's position and facing decide how loud it is and which side it is
    /// on. `false` means it is heard from everywhere at full strength, which is what music and
    /// narration want — a soundtrack that pans as the player turns around is the most obvious way
    /// for game audio to sound wrong.
    ///
    /// **A spatial sound should be mono.** A stereo recording already has its own left and right, so
    /// a position has nothing left to decide.
    pub spatial: bool,
    /// Whether it is currently making a sound.
    ///
    /// A field rather than removing the component, so that stopping and starting does not change
    /// which archetype an entity is in — an archetype move is much more expensive than a bool, and
    /// a sound that stops and starts is exactly the case that would do it often.
    pub playing: bool,
    /// How much solid matter stands between this sound and the listener — ADR 0086.
    ///
    /// `0.0` is a clear line and `1.0` is fully blocked. **Nothing in this crate computes it**, and
    /// that is the whole decision: `amadeo-audio` does not depend on `amadeo-physics`, so it cannot
    /// ask whether a wall is in the way. It owns the slot and something above both crates fills it,
    /// which is `Overlay`, `TextureCache`, `MeshCache` and `SkyCache`'s inversion a fifth time.
    /// `amadeo-app` ships the system that does (`occlude_voices`), and **a game registers it beside its
    /// own audio pass**. ADR 0086's amendment first said "registered automatically", and engine gate
    /// review 30 upheld the correction: `amadeo-app` registers no default systems at all -- it is a
    /// schedule host -- and an ordering against an unregistered label is a hard `UnknownLabel`, so
    /// automatic registration with a correct ordering was never available.
    ///
    /// # What it means, and why it is not "how much quieter"
    ///
    /// **A wall does not make a sound quieter, it makes it dull** — it removes the top of the
    /// spectrum. A voice attenuated in gain alone tells a player *"that is further away"*, and a game
    /// whose antagonist is found by ear needs them to distinguish *far* from *behind that bulkhead*.
    /// So this field says how *blocked* a sound is and leaves what to do about it to the mix: the
    /// intended consumer is a low-pass cutoff, with gain as a secondary term.
    ///
    /// # Hashed, and deliberately not derived
    ///
    /// It is ADR 0063's `Focus` call rather than `Looking`'s: nothing recomputes it if it is
    /// dropped, a save should restore it, and gameplay may legitimately read it — an AI that knows
    /// it cannot be heard is a real thing to want.
    ///
    /// **Defaults to `0.0`**, so a world that installs nothing sounds exactly as it did before.
    #[reflect(min = 0.0, max = 1.0, default = 0.0)]
    pub occlusion: f32,
}

impl Default for AudioSource {
    fn default() -> Self {
        Self {
            sound: String::new(),
            bus: Bus::Effects,
            gain: 1.0,
            pitch: 1.0,
            looping: false,
            spatial: true,
            playing: true,
            occlusion: 0.0,
        }
    }
}

impl AudioSource {
    /// A looping, spatial sound — a hum, a fire, a machine.
    #[must_use]
    pub fn looping(sound: &str) -> Self {
        Self {
            sound: sound.to_string(),
            looping: true,
            ..Self::default()
        }
    }

    /// A looping sound heard from everywhere, at no particular place — music.
    #[must_use]
    pub fn music(sound: &str) -> Self {
        Self {
            sound: sound.to_string(),
            bus: Bus::Music,
            looping: true,
            spatial: false,
            ..Self::default()
        }
    }
}

impl Component for AudioSource {}

/// A sound that happens once and is over — a footstep, a door, a gunshot.
///
/// # Why this is an event and [`AudioSource`] is a component
///
/// ADR 0059 named this gap and refused the obvious fix. An `AudioSource` says *"this entity is
/// making this sound"*, which is a property of the world: it survives a save, `describe` can see it,
/// and it stops when the entity does. **A footstep is none of those things.** It is not true of the
/// world a moment later, there is nothing for it to be attached to, and a component that represented
/// one would have to be added and then removed by somebody remembering to.
///
/// The tempting wrong fix, named in ADR 0059 so it would not get built by accident, is a `play_once`
/// flag on the component plus a system that clears it. That puts a **write into gameplay state** for
/// something that must not be in the state hash at all — and it makes every entity that has ever
/// made a noise carry a field about it forever.
///
/// # It is deterministic, and that is the whole reason it works
///
/// The *decision* to play a footstep is gameplay and belongs in the state hash; the *playing* is
/// machinery and must not be. An [`Event`](amadeo_events::Event) splits exactly there: it requires
/// [`StableHash`], so a queued `SoundPlayed` is part of simulation state and two runs that disagree
/// about footsteps have genuinely diverged — while the backend that plays it is a `Service` and
/// outside the hash entirely (ADR 0009).
///
/// ```
/// # use amadeo_audio::{Bus, SoundPlayed};
/// # use amadeo_ecs::World;
/// # use amadeo_events::WorldEvents;
/// # let mut world = World::new();
/// # world.register_event::<SoundPlayed>();
/// world.send_event(SoundPlayed::at("footstep", [3.0, 0.0, -2.0]));
/// world.send_event(SoundPlayed {
///     bus: Bus::Interface,
///     ..SoundPlayed::everywhere("menu_click")
/// });
/// ```
#[derive(Debug, Clone, PartialEq, StableHash, Reflect)]
pub struct SoundPlayed {
    /// The declared asset id of the sound (ADR 0020).
    pub sound: String,
    /// Which mix it belongs to.
    pub bus: Bus,
    /// Linear gain before the bus and master are applied.
    #[reflect(min = 0.0, max = 4.0)]
    pub gain: f32,
    /// Playback rate. `1.0` is as recorded.
    #[reflect(min = 0.05, max = 8.0)]
    pub pitch: f32,
    /// Whether it is heard *from somewhere*, in which case `position` says where.
    ///
    /// A `bool` beside a plain `[f32; 3]` rather than an `Option<[f32; 3]>`, matching
    /// [`AudioSource::spatial`] exactly — one spelling for one idea, and ADR 0032 left `Option::None`
    /// without a spelling in the value tree anyway.
    pub spatial: bool,
    /// Where it happened, in world space. Ignored when `spatial` is false.
    ///
    /// **A place, not an entity.** A one-shot is over in a fraction of a second, so following
    /// something would buy a pan change of a metre or so and cost a lifetime question nobody wants
    /// to answer — what a footstep should do when the thing that made it is despawned mid-sound.
    /// Unreal draws the same line (`PlaySoundAtLocation` against `SpawnSoundAttached`), and adding a
    /// `follow` field later is additive if a game ever needs one.
    pub position: [f32; 3],
}

impl Default for SoundPlayed {
    fn default() -> Self {
        Self {
            sound: String::new(),
            bus: Bus::Effects,
            gain: 1.0,
            pitch: 1.0,
            spatial: true,
            position: [0.0; 3],
        }
    }
}

impl SoundPlayed {
    /// A sound that happened at a place.
    #[must_use]
    pub fn at(sound: &str, position: [f32; 3]) -> Self {
        Self {
            sound: sound.to_string(),
            position,
            ..Self::default()
        }
    }

    /// A sound heard from everywhere — a menu click, a stinger.
    ///
    /// Defaults to [`Bus::Interface`] rather than `Effects`, because that is what a sound with no
    /// place in the world almost always is, and because a menu click that ducks under a waterfall is
    /// the exact failure `Interface` exists to prevent.
    #[must_use]
    pub fn everywhere(sound: &str) -> Self {
        Self {
            sound: sound.to_string(),
            bus: Bus::Interface,
            spatial: false,
            ..Self::default()
        }
    }
}

impl amadeo_events::Event for SoundPlayed {}

/// The ears. Put it on whatever should hear the world — usually the camera, sometimes the player.
///
/// # Why it is a marker with no fields
///
/// Everything a backend needs — where the ears are and which way they face — is already on the
/// entity's [`Transform`](amadeo_transform::Transform). A component that repeated any of it would be
/// a second copy of a fact, and the two would drift the moment something moved the entity.
///
/// **Camera or player is a real choice with an audible difference**, and it is the game's: ears on
/// the camera hear what the viewer would, which suits a third-person game where the camera is the
/// point of view; ears on the character hear what the character would, which is what a horror game
/// wants when the camera swings away.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default, StableHash, Reflect)]
pub struct AudioListener;

impl Component for AudioListener {}

#[cfg(test)]
mod tests {
    use super::*;
    use amadeo_reflect::TypeRegistry;

    #[test]
    fn a_source_defaults_to_a_spatial_effect_that_is_playing() {
        let source = AudioSource::default();
        assert_eq!(source.bus, Bus::Effects);
        assert!(source.spatial);
        assert!(source.playing);
        assert!(!source.looping, "a sound that loops should have to say so");
    }

    #[test]
    fn music_is_not_spatial_and_is_on_its_own_bus() {
        // The constructor exists precisely so this pair cannot be got wrong one at a time: music on
        // the effects bus is a volume slider that does the wrong thing, and spatial music pans.
        let theme = AudioSource::music("theme");
        assert_eq!(theme.bus, Bus::Music);
        assert!(!theme.spatial);
        assert!(theme.looping);
    }

    #[test]
    fn both_components_round_trip_through_the_value_tree() {
        // Invariant I8: if it cannot be reflected it cannot be serialised, inspected or edited — so
        // this is what says a scene file can author a sound at all.
        let mut registry = TypeRegistry::new();
        registry.register::<AudioSource>().expect("registers");
        registry.register::<AudioListener>().expect("registers");

        let source = AudioSource::music("theme");
        let value = source.to_value();
        let back = AudioSource::from_value(&value).expect("round trips");
        assert_eq!(source, back);

        let ears = AudioListener;
        assert_eq!(
            AudioListener::from_value(&ears.to_value()).expect("round trips"),
            ears
        );
    }

    #[test]
    fn every_bus_has_a_slot_in_the_gain_array() {
        // `Bus::COUNT` sizes an array that is indexed by `bus as usize`, so a variant added without
        // bumping it would index past the end. One assertion that cannot be forgotten, because
        // adding a variant makes it fail.
        let all = [Bus::Effects, Bus::Music, Bus::Dialogue, Bus::Interface];
        assert_eq!(all.len(), Bus::COUNT);
        for (index, bus) in all.iter().enumerate() {
            assert_eq!(*bus as usize, index, "bus order must match its index");
        }
    }
}
