//! `audio.describe` — what the world sounds like, and why it might not.
//!
//! # Why this is not just "list the voices"
//!
//! `render.describe` answers "what is on screen" for an agent with no eyes. This is its counterpart,
//! and the asymmetry is worth naming: **a blank screen has one obvious symptom and silence has
//! none.** A person or an agent looking at a black window knows something is wrong. Nobody notices
//! that a game is quiet, and when they do, the causes are all invisible from outside — no listener
//! in the world, a source that is not playing, a bus at zero, or the null backend, which every
//! headless build installs *on purpose*.
//!
//! So the reply carries `silent_because`: the engine's own one-line answer, in a deliberate order,
//! rather than a bag of fields each caller would have to reason over. See
//! [`AudioDescription::silent_because`](amadeo_audio::AudioDescription::silent_because) for why the
//! order is the load-bearing part.
//!
//! ADR 0060 is what makes this necessary rather than nice. It decided a sound that will not load is
//! **silent**, with the failure report as the whole diagnosis — and a report nothing can read is not
//! a diagnosis.

use crate::json::Json;
use amadeo_audio::{Bus, describe_audio};
use amadeo_ecs::World;

/// Renders the audio description as JSON.
///
/// Reports `installed: false` rather than an error when a game installed no audio system, for the
/// reason `assets.list` reports an empty catalogue: a game with no sound is an ordinary thing, and
/// making the caller handle a failure for it would be wrong.
#[must_use]
pub fn describe(world: &World) -> Json {
    let description = describe_audio(world);

    let voices: Vec<Json> = description
        .frame
        .voices
        .iter()
        .map(|voice| {
            let mut members = vec![
                // The entity, split the way `render.describe` splits it — an agent correlating this
                // with `world.entity` needs both halves.
                ("entity", Json::Int(i64::from(voice.source.index()))),
                (
                    "generation",
                    Json::Int(i64::from(voice.source.generation())),
                ),
                ("sound", Json::string(&voice.sound)),
                ("bus", Json::string(bus_name(voice.bus))),
                // **After the bus and master multiply**, which is what a backend receives and what
                // the voice is actually heard at. The authored number is on the component and
                // `world.entity` shows that.
                ("gain", Json::Float(f64::from(voice.gain))),
                ("pitch", Json::Float(f64::from(voice.pitch))),
                ("looping", Json::Bool(voice.looping)),
                ("spatial", Json::Bool(voice.position.is_some())),
            ];
            if let Some(position) = voice.position {
                members.push(("position", vector(position)));
            }
            Json::object(members)
        })
        .collect();

    let mut members = vec![
        ("installed", Json::Bool(description.installed)),
        ("backend", Json::string(description.backend)),
        ("master", Json::Float(f64::from(description.master))),
        (
            "buses",
            Json::object(
                [Bus::Effects, Bus::Music, Bus::Dialogue, Bus::Interface]
                    .into_iter()
                    .map(|bus| {
                        (
                            bus_name(bus),
                            Json::Float(f64::from(description.buses[bus as usize])),
                        )
                    })
                    .collect::<Vec<_>>(),
            ),
        ),
        ("voice_count", Json::Int(voices.len() as i64)),
        ("voices", Json::Array(voices)),
        (
            "decoded",
            Json::Array(description.decoded.iter().map(Json::string).collect()),
        ),
    ];

    match &description.frame.listener {
        Some(listener) => members.push((
            "listener",
            Json::object([
                ("position", vector(listener.position)),
                ("forward", vector(listener.forward)),
                ("up", vector(listener.up)),
            ]),
        )),
        // `null` rather than omitted, because "there are no ears" is the single most useful fact in
        // this reply and a missing key is easy to read straight past.
        None => members.push(("listener", Json::Null)),
    }

    // The structured half of ADR 0021's report. Unlike a texture there is no visible stand-in to go
    // with it — ADR 0060 — so this channel is the entire signal.
    if !description.failures.is_empty() {
        members.push((
            "failures",
            Json::Array(
                description
                    .failures
                    .iter()
                    .map(|(id, message)| {
                        Json::object([("id", Json::string(id)), ("message", Json::string(message))])
                    })
                    .collect(),
            ),
        ));
    }

    if let Some(error) = &description.last_error {
        members.push(("last_error", Json::string(error)));
    }

    if let Some(reason) = description.silent_because() {
        members.push(("silent_because", Json::string(reason)));
    }

    Json::object(members)
}

/// The name a bus is known by in a scene file, so the reply and the format agree.
fn bus_name(bus: Bus) -> &'static str {
    match bus {
        Bus::Effects => "Effects",
        Bus::Music => "Music",
        Bus::Dialogue => "Dialogue",
        Bus::Interface => "Interface",
    }
}

fn vector(value: [f32; 3]) -> Json {
    Json::Array(
        value
            .iter()
            .map(|part| Json::Float(f64::from(*part)))
            .collect(),
    )
}

#[cfg(test)]
mod tests {
    use super::*;
    use amadeo_audio::{Audio, AudioListener, AudioSource, NullAudio, collect_audio};
    use amadeo_transform::Transform;

    fn field<'a>(reply: &'a Json, name: &str) -> &'a Json {
        match reply {
            Json::Object(members) => members.get(name).unwrap_or(&Json::Null),
            _ => panic!("the reply should be an object"),
        }
    }

    fn text(reply: &Json, name: &str) -> String {
        match field(reply, name) {
            Json::String(value) => value.clone(),
            other => panic!("`{name}` should be a string, got {other:?}"),
        }
    }

    #[test]
    fn a_world_with_no_audio_system_says_so_rather_than_failing() {
        let world = World::new();
        let reply = describe(&world);

        assert_eq!(field(&reply, "installed"), &Json::Bool(false));
        assert!(text(&reply, "silent_because").contains("no audio system"));
    }

    #[test]
    fn a_world_with_no_ears_is_told_that_first() {
        // **The ordering that matters.** A world with no listener submits no voices at all, so
        // "no entity is making a sound" would be true and would send someone to look at the wrong
        // thing entirely. The missing listener is the cause; the absent voices are the symptom.
        let mut world = World::new();
        world.insert_service(Audio::new(Box::new(NullAudio::new())));
        let hum = world.spawn();
        world.insert(hum, Transform::at(3.0, 0.0));
        world.insert(hum, AudioSource::looping("hum"));

        let reply = describe(&world);
        assert_eq!(field(&reply, "listener"), &Json::Null);

        let reason = text(&reply, "silent_because");
        assert!(reason.contains("AudioListener"), "{reason}");
        assert!(
            !reason.contains("no entity is making a sound"),
            "the symptom must not be reported as the cause: {reason}"
        );
    }

    #[test]
    fn a_playing_source_is_reported_with_its_final_gain() {
        let mut world = World::new();
        world.insert_service(Audio::new(Box::new(NullAudio::new())));
        {
            let audio = world.service_mut::<Audio>().expect("installed");
            audio.master = 0.5;
        }

        let ears = world.spawn();
        world.insert(ears, Transform::at(0.0, 0.0));
        world.insert(ears, AudioListener);

        let hum = world.spawn();
        world.insert(hum, Transform::at(3.0, 0.0));
        world.insert(
            hum,
            AudioSource {
                gain: 0.5,
                ..AudioSource::looping("hum")
            },
        );

        let reply = describe(&world);
        assert_eq!(field(&reply, "voice_count"), &Json::Int(1));

        let Json::Array(voices) = field(&reply, "voices") else {
            panic!("voices should be an array");
        };
        assert_eq!(text(&voices[0], "sound"), "hum");
        assert_eq!(text(&voices[0], "bus"), "Effects");
        assert_eq!(field(&voices[0], "spatial"), &Json::Bool(true));
        // 0.5 authored x 0.5 master. The reply reports what would be *heard*, not what was authored.
        assert_eq!(field(&voices[0], "gain"), &Json::Float(0.25));
    }

    #[test]
    fn the_null_backend_is_reported_last_so_it_never_masks_a_real_fault() {
        // Every headless build installs `NullAudio` deliberately, so "the null backend is installed"
        // is almost always true and almost never the interesting answer. Reporting it above a
        // genuine problem would bury the genuine problem in the case that matters most.
        let mut world = World::new();
        world.insert_service(Audio::new(Box::new(NullAudio::new())));

        // No listener *and* the null backend. The listener is what should be reported.
        let reason = text(&describe(&world), "silent_because");
        assert!(reason.contains("AudioListener"), "{reason}");

        // With ears and a voice, the backend becomes the honest answer.
        let ears = world.spawn();
        world.insert(ears, Transform::at(0.0, 0.0));
        world.insert(ears, AudioListener);
        let hum = world.spawn();
        world.insert(hum, Transform::at(0.0, 0.0));
        world.insert(hum, AudioSource::looping("hum"));

        let reason = text(&describe(&world), "silent_because");
        assert!(reason.contains("null backend"), "{reason}");
    }

    #[test]
    fn describing_agrees_with_what_was_actually_submitted() {
        // **The property that makes this method trustworthy**, and the failure it guards against is
        // the fifth instance of one this engine keeps hitting: two copies of "what should be
        // audible" drifting apart. Here that failure reports a game playing something it is not,
        // which is worse than no answer.
        //
        // They cannot drift, because `collect_audio` and `describe_audio` call one builder — and
        // this is what says so rather than the comment above it.
        let mut world = World::new();
        world.insert_service(Audio::new(Box::new(NullAudio::new())));
        let ears = world.spawn();
        world.insert(ears, Transform::at(1.0, 2.0));
        world.insert(ears, AudioListener);
        for index in 0..3 {
            let source = world.spawn();
            world.insert(source, Transform::at(index as f32, 0.0));
            world.insert(source, AudioSource::looping("hum"));
        }

        collect_audio(&mut world);

        let submitted = world
            .service::<Audio>()
            .expect("installed")
            .null_backend()
            .expect("null")
            .last_frame()
            .expect("a frame")
            .clone();

        assert_eq!(describe_audio(&world).frame, submitted);
    }

    #[test]
    fn describing_cannot_move_the_state_hash() {
        // It is a query. `render.describe` makes the same promise and for the same reason: an
        // introspection call that changed the world would make an agent's questions part of the
        // simulation.
        let mut world = World::new();
        world.insert_service(Audio::new(Box::new(NullAudio::new())));
        let ears = world.spawn();
        world.insert(ears, Transform::at(0.0, 0.0));
        world.insert(ears, AudioListener);

        let before = world.state_hash();
        let _ = describe(&world);
        assert_eq!(before, world.state_hash());
    }
}
