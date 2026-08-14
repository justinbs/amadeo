//! `anim.describe` — what the world is animating, and why it might not be.
//!
//! # Stillness is the worst of the three symptoms
//!
//! `render.describe` answers "what is on screen" for an agent with no eyes, and `audio.describe`
//! exists because **a blank screen has one obvious symptom and silence has none**. Animation is a
//! third case and worse than either: a thing that is not moving looks exactly like a thing that was
//! authored not to move. Nobody can tell a broken clip from a still scene by looking at it.
//!
//! And unlike silence it is not cosmetic. Animation writes gameplay components (ADR 0066), so a clip
//! that quietly does nothing changes the state hash of every tick after it.
//!
//! ADR 0066 put two reports into `amadeo-anim` — `ClipCache::failures` and `Animatable::missing` —
//! and made them the whole diagnosis. **A report nothing can read is not a diagnosis**, which is
//! the hole ADR 0060 had while it was being written and which `audio.describe` was built to close.
//! This is that lesson applied before it bit rather than after.

use crate::json::Json;
use amadeo_anim::describe_animation;
use amadeo_ecs::World;

/// Renders the animation description as JSON.
///
/// Reports the fields rather than an error when a game has no animation at all, for the reason
/// `assets.list` reports an empty catalogue: a game that animates nothing is an ordinary thing.
#[must_use]
pub fn describe(world: &World) -> Json {
    let description = describe_animation(world);

    let players: Vec<Json> = description
        .players
        .iter()
        .map(|player| {
            let mut members = vec![
                // Split the way `render.describe` and `audio.describe` split it — an agent
                // correlating this with `world.entity` needs both halves.
                ("entity", Json::Int(i64::from(player.entity.index()))),
                (
                    "generation",
                    Json::Int(i64::from(player.entity.generation())),
                ),
                ("clip", Json::string(&player.clip)),
                // **The field that answers most questions.** A player naming a clip that did not
                // load is playing nothing, and there is no other way to tell from outside.
                ("loaded", Json::Bool(player.loaded)),
                ("time", Json::Float(f64::from(player.time))),
                ("speed", Json::Float(f64::from(player.speed))),
                ("looping", Json::Bool(player.looping)),
                ("playing", Json::Bool(player.playing)),
            ];
            if let Some(duration) = player.duration {
                members.push(("duration", Json::Float(f64::from(duration))));
            }
            Json::object(members)
        })
        .collect();

    let failures: Vec<Json> = description
        .failures
        .iter()
        .map(|(id, why)| Json::object([("id", Json::string(id)), ("problem", Json::string(why))]))
        .collect();

    let mut members = vec![
        (
            "allow_list_installed",
            Json::Bool(description.allow_list_installed),
        ),
        ("cache_installed", Json::Bool(description.cache_installed)),
        (
            "allowed",
            Json::Array(description.allowed.iter().map(Json::string).collect()),
        ),
        ("players", Json::Array(players)),
        (
            "loaded",
            Json::Array(description.loaded.iter().map(Json::string).collect()),
        ),
        ("failures", Json::Array(failures)),
        // `Component.field` targets a clip asked for and did not get. **Separate from
        // `still_because`**, because a world can be animating perfectly and still have one track
        // writing nowhere — which has no symptom at all.
        (
            "missing_targets",
            Json::Array(description.missing.iter().map(Json::string).collect()),
        ),
    ];

    if let Some(why) = description.still_because() {
        members.push(("still_because", Json::string(&why)));
    }

    Json::object(members)
}

#[cfg(test)]
mod tests {
    use super::*;
    use amadeo_anim::{Animatable, AnimationClip, AnimationPlayer, ClipCache};
    use amadeo_core::StableHash;
    use amadeo_ecs::Component;
    use amadeo_reflect::Reflect;

    #[derive(Debug, Clone, Copy, PartialEq, Default, StableHash, Reflect)]
    struct Thing {
        /// How tall.
        height: f32,
    }
    impl Component for Thing {}

    #[test]
    fn an_empty_world_says_nothing_is_asking_to_animate() {
        let text = describe(&World::new()).to_compact();
        assert!(text.contains("\"still_because\""), "{text}");
        assert!(text.contains("AnimationPlayer"), "{text}");
        assert!(text.contains("\"players\":[]"), "{text}");
    }

    #[test]
    fn a_working_world_omits_the_explanation_entirely() {
        // Absent rather than null. A caller checks for the key, which is the same shape
        // `audio.describe` uses for `silent_because`.
        let mut world = World::new();
        let mut cache = ClipCache::new();
        cache.insert(
            "test",
            AnimationClip {
                duration: 1.0,
                tracks: Vec::new(),
            },
        );
        world.insert_service(cache);

        let mut animatable = Animatable::new();
        animatable.allow::<Thing>();
        world.insert_service(animatable);

        let entity = world.spawn();
        world.insert(entity, Thing::default());
        world.insert(entity, AnimationPlayer::looping("test"));

        let text = describe(&world).to_compact();
        assert!(!text.contains("still_because"), "{text}");
        assert!(text.contains("\"loaded\":true"), "{text}");
        assert!(text.contains("\"duration\":1"), "{text}");
        assert!(text.contains("\"allowed\":[\"Thing\"]"), "{text}");
    }

    #[test]
    fn a_clip_that_would_not_load_is_reported_with_its_reason() {
        // The report ADR 0066 made the whole diagnosis, reaching an agent.
        let mut world = World::new();
        let mut cache = ClipCache::new();
        cache.fail("absent", "no asset declares this id");
        world.insert_service(cache);
        world.insert_service(Animatable::new());

        let entity = world.spawn();
        world.insert(entity, AnimationPlayer::looping("absent"));

        let text = describe(&world).to_compact();
        assert!(text.contains("no asset declares this id"), "{text}");
        assert!(text.contains("\"loaded\":false"), "{text}");
    }
}
