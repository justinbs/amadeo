//! What the world is animating, and why it might not be.
//!
//! # Why this exists at all
//!
//! ADR 0066 put two reports into this crate — [`ClipCache::failures`] and [`Animatable::missing`] —
//! and made them the whole diagnosis, because the failures they describe all present as *stillness*.
//! **A report nothing can read is not a diagnosis**, which is the hole ADR 0060 had while it was
//! being written and which `audio.describe` was built to close.
//!
//! This is that lesson applied before it bites rather than after.
//!
//! # Stillness is worse than silence
//!
//! `audio.describe`'s module docs point out that a blank screen has an obvious symptom and silence
//! has none. Animation is a third case and the worst of the three: a thing that is not moving looks
//! exactly like a thing that was authored not to move. Nobody can tell a broken clip from a still
//! scene by looking, and — because animation writes gameplay components — a clip that quietly does
//! nothing changes the state hash of every tick after it.

use crate::clip::AnimationClip;
use crate::play::{Animatable, AnimationPlayer, ClipCache};
use amadeo_ecs::{Entity, World};

/// The RPC method name this answers, and the `amadeo` subcommand that calls it.
///
/// A constant rather than a literal in two crates, for `AUDIO_DESCRIBE`'s reason: the name is
/// protocol, and two spellings of it drift silently into a method that exists on one side only.
pub const ANIM_DESCRIBE: &str = "anim.describe";

/// One entity's player, as an outside caller sees it.
#[derive(Debug, Clone, PartialEq)]
pub struct PlayerReport {
    /// Which entity.
    pub entity: Entity,
    /// The clip's declared asset id.
    pub clip: String,
    /// Whether that id resolved to a clip.
    pub loaded: bool,
    /// Seconds into the clip.
    pub time: f32,
    /// The clip's length, when it loaded.
    pub duration: Option<f32>,
    /// Playback rate.
    pub speed: f32,
    /// Whether it wraps at the end.
    pub looping: bool,
    /// Whether its clock is advancing.
    pub playing: bool,
}

/// What is animating, and everything that would explain why nothing is.
#[derive(Debug, Clone, PartialEq)]
pub struct AnimationDescription {
    /// Whether an [`Animatable`] allow-list is installed.
    ///
    /// `false` means **nothing can be written at all**, which is the setup step a game is most
    /// likely to have missed — `amadeo-anim` sits below `amadeo-app` and cannot install itself
    /// (invariant I6), so unlike the clip cache this one is somebody's line of code.
    pub allow_list_installed: bool,
    /// Whether a [`ClipCache`] is installed. Normally true: `load_scene` installs it.
    pub cache_installed: bool,
    /// The component types clips may write, in name order.
    pub allowed: Vec<String>,
    /// Every player in the world, in entity order.
    pub players: Vec<PlayerReport>,
    /// Clip ids that are loaded and usable, in id order.
    pub loaded: Vec<String>,
    /// Clip ids that would not load, or loaded with problems, and why. In id order.
    pub failures: Vec<(String, String)>,
    /// `Component.field` targets a clip asked for and did not get, in order.
    pub missing: Vec<String>,
}

impl AnimationDescription {
    /// A one-line explanation of why nothing is animating, or `None` if something should be.
    ///
    /// # Why the engine writes this sentence rather than the caller
    ///
    /// `AudioDescription::silent_because`'s argument, and the order is the load-bearing part again.
    /// It goes from **what was asked for** to **what was provided**, because a world that authored
    /// no animation is not a world with a broken animation system, and reporting a missing service
    /// first would be reporting machinery at a game that never wanted any.
    #[must_use]
    pub fn still_because(&self) -> Option<String> {
        if self.players.is_empty() {
            return Some(
                "nothing in the world has an `AnimationPlayer`, so nothing is asking to animate"
                    .to_string(),
            );
        }
        if self
            .players
            .iter()
            .all(|player| !player.playing || player.clip.is_empty())
        {
            return Some(
                "every `AnimationPlayer` is either stopped or names no clip. `playing` is a field \
                 rather than a missing component, so a stopped player still appears here"
                    .to_string(),
            );
        }
        if !self.allow_list_installed {
            return Some(
                "no `Animatable` service is installed, so no component may be written. A game \
                 inserts one and calls `allow::<T>()` for each type its clips animate — this crate \
                 sits below `amadeo-app` and cannot install it"
                    .to_string(),
            );
        }
        if !self.cache_installed {
            return Some(
                "no `ClipCache` service is installed. `App::load_scene` installs one when any \
                 entity names a clip, so this means the scene was not loaded through it"
                    .to_string(),
            );
        }
        if self.players.iter().all(|player| !player.loaded) {
            return Some(format!(
                "no clip any player names could be loaded. See `failures`: {}",
                self.failures
                    .iter()
                    .map(|(id, why)| format!("`{id}` — {why}"))
                    .collect::<Vec<_>>()
                    .join("; ")
            ));
        }
        if self.allowed.is_empty() {
            return Some(
                "the `Animatable` allow-list is empty, so every track resolves to nothing. Call \
                 `allow::<T>()` for each component type the clips animate"
                    .to_string(),
            );
        }
        None
    }
}

/// Reads the world and reports what it is animating.
///
/// # It reads the world rather than the last frame
///
/// `describe_audio`'s rule, for the same reason: a report built from what a backend was last handed
/// would answer a question about machinery. This answers a question about the game.
#[must_use]
pub fn describe_animation(world: &World) -> AnimationDescription {
    let cache = world.service::<ClipCache>();
    let animatable = world.service::<Animatable>();

    let mut players: Vec<PlayerReport> = world
        .query::<(&AnimationPlayer,)>()
        .map(|(entity, (player,))| {
            let clip: Option<&AnimationClip> =
                cache.and_then(|cache| cache.get(&player.clip)).filter(|_| {
                    // An empty id is "no clip", not "a clip called nothing" — the same spelling
                    // every other asset field in the engine uses for none.
                    !player.clip.is_empty()
                });
            PlayerReport {
                entity,
                clip: player.clip.clone(),
                loaded: clip.is_some(),
                time: player.time,
                duration: clip.map(|clip| clip.duration),
                speed: player.speed,
                looping: player.looping,
                playing: player.playing,
            }
        })
        .collect();
    // Entity order, so two runs of the same world report the same list. `query` already yields in
    // storage order, which is not the same thing.
    players.sort_by_key(|player| (player.entity.index(), player.entity.generation()));

    AnimationDescription {
        allow_list_installed: animatable.is_some(),
        cache_installed: cache.is_some(),
        allowed: animatable.map(Animatable::allowed).unwrap_or_default(),
        players,
        loaded: cache.map(ClipCache::loaded).unwrap_or_default(),
        failures: cache
            .map(|cache| {
                cache
                    .failures()
                    .iter()
                    .map(|(id, why)| (id.clone(), why.clone()))
                    .collect()
            })
            .unwrap_or_default(),
        missing: animatable
            .map(|animatable| animatable.missing().iter().cloned().collect())
            .unwrap_or_default(),
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::clip::{Interpolation, Key, Track};
    use amadeo_core::StableHash;
    use amadeo_ecs::Component;
    use amadeo_reflect::Reflect;

    #[derive(Debug, Clone, Copy, PartialEq, Default, StableHash, Reflect)]
    struct Thing {
        /// How tall.
        height: f32,
    }
    impl Component for Thing {}

    fn clip() -> AnimationClip {
        AnimationClip {
            duration: 1.0,
            tracks: vec![Track {
                component: "Thing".to_string(),
                field: "height".to_string(),
                interpolation: Interpolation::Linear,
                keys: vec![
                    Key {
                        time: 0.0,
                        value: vec![0.0],
                    },
                    Key {
                        time: 1.0,
                        value: vec![1.0],
                    },
                ],
            }],
        }
    }

    /// A fully wired world with one player.
    fn working() -> World {
        let mut world = World::new();
        let mut cache = ClipCache::new();
        cache.insert("test", clip());
        world.insert_service(cache);

        let mut animatable = Animatable::new();
        animatable.allow::<Thing>();
        world.insert_service(animatable);

        let entity = world.spawn();
        world.insert(entity, Thing::default());
        world.insert(entity, AnimationPlayer::looping("test"));
        world
    }

    #[test]
    fn a_working_world_has_nothing_to_explain() {
        let description = describe_animation(&working());
        assert_eq!(description.still_because(), None);
        assert_eq!(description.players.len(), 1);
        assert!(description.players[0].loaded);
        assert_eq!(description.players[0].duration, Some(1.0));
        assert_eq!(description.allowed, vec!["Thing".to_string()]);
    }

    #[test]
    fn a_world_that_animates_nothing_says_so_first() {
        // **The order matters, and this is why it starts here.** A world with no players is not a
        // world with a broken animation system, and leading with a missing service would report
        // machinery at a game that never asked for any.
        let world = World::new();
        let description = describe_animation(&world);
        let why = description.still_because().expect("nothing is animating");
        assert!(why.contains("AnimationPlayer"), "{why}");
    }

    #[test]
    fn a_stopped_player_is_reported_as_stopped_rather_than_as_broken() {
        let mut world = working();
        let entity = world
            .query::<(&AnimationPlayer,)>()
            .map(|(entity, _)| entity)
            .next()
            .expect("one player");
        world.insert(
            entity,
            AnimationPlayer {
                playing: false,
                ..AnimationPlayer::looping("test")
            },
        );

        let why = describe_animation(&world)
            .still_because()
            .expect("nothing is animating");
        assert!(why.contains("stopped"), "{why}");
    }

    #[test]
    fn the_forgotten_setup_step_is_named() {
        // **The most likely real fault.** `amadeo-anim` sits below `amadeo-app` and cannot install
        // its own allow-list, so it is a line in a game's setup that can simply be absent — and the
        // symptom is a world where everything is loaded, playing, and completely still.
        let mut world = working();
        world.remove_service::<Animatable>();

        let why = describe_animation(&world)
            .still_because()
            .expect("nothing is animating");
        assert!(why.contains("Animatable"), "{why}");
        assert!(why.contains("allow"), "{why}");
    }

    #[test]
    fn a_clip_that_would_not_load_is_quoted_with_its_reason() {
        // ADR 0066 made the report the whole diagnosis. This is the report being readable.
        let mut world = working();
        let entity = world
            .query::<(&AnimationPlayer,)>()
            .map(|(entity, _)| entity)
            .next()
            .expect("one player");
        world.insert(entity, AnimationPlayer::looping("absent"));
        if let Some(cache) = world.service_mut::<ClipCache>() {
            cache.fail("absent", "no asset declares this id");
        }

        let description = describe_animation(&world);
        let why = description.still_because().expect("nothing is animating");
        assert!(why.contains("absent"), "{why}");
        assert!(why.contains("no asset declares this id"), "{why}");
        assert!(!description.players[0].loaded);
    }

    #[test]
    fn a_target_a_clip_asked_for_and_did_not_get_is_listed() {
        // The second report ADR 0066 created, and the one with no other symptom at all: the clip
        // loaded, the player is playing, and one track writes nowhere.
        let mut world = working();
        if let Some(cache) = world.service_mut::<ClipCache>() {
            cache.insert(
                "test",
                AnimationClip {
                    duration: 1.0,
                    tracks: vec![Track {
                        component: "Elsewhere".to_string(),
                        field: "somewhere".to_string(),
                        interpolation: Interpolation::Linear,
                        keys: vec![Key {
                            time: 0.0,
                            value: vec![1.0],
                        }],
                    }],
                },
            );
        }
        crate::play::animate(&mut world);

        let description = describe_animation(&world);
        // Not "still", because a player is running — which is exactly why this needs its own field
        // rather than a sentence. Something *is* animating; one target is not.
        assert_eq!(description.still_because(), None);
        assert_eq!(description.missing, vec!["Elsewhere.somewhere".to_string()]);
    }

    #[test]
    fn describing_reports_the_same_thing_twice() {
        // Ordered output, so an agent diffing two calls sees a change in the world rather than a
        // change in a hash map's mood.
        let world = working();
        assert_eq!(describe_animation(&world), describe_animation(&world));
    }
}
