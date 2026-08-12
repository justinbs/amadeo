//! The listening test — the only verification the kira backend has, and it needs a person.
//!
//! ```text
//! cargo test -p amadeo-audio --features kira --test you_can_hear_it -- --ignored --nocapture
//! ```
//!
//! # Why this is `#[ignore]` rather than an ordinary test
//!
//! It opens a real audio device and plays a real sound for about eight seconds. CI has no device, a
//! headless run has no device, and even where one exists **nothing in the process can assert that a
//! sound came out** — the samples leave through the operating system and there is nothing to read
//! back.
//!
//! So this is not a test in the sense the rest of the suite is. It is a **procedure**, written down
//! and runnable, with the acceptance criteria in the output rather than in an assertion. Marking it
//! `#[ignore]` is what keeps it out of `cargo test --workspace` while keeping the invocation in the
//! repository instead of in somebody's shell history.
//!
//! What it *does* assert is everything up to the speaker: that a device opens, that a sound uploads,
//! and that submitting frames never returns an error. Those are real failures and they are caught
//! here. The last step is the listener's.
//!
//! # What you should hear
//!
//! Printed as it runs, so the criteria are in front of you rather than in this comment.

#![cfg(feature = "kira")]

use amadeo_audio::{
    AudioBackend, AudioFrame, Bus, KiraAudio, Listener, OneShot, SoundData, Voice, VoiceTracker,
};
use amadeo_core::sin_cos_degrees;
use amadeo_ecs::World;

/// Roughly how often a game would submit a frame.
const FRAME: std::time::Duration = std::time::Duration::from_millis(16);

/// A one-second mono A440-ish tone that loops without a click.
///
/// 440 Hz over one second is 440 whole cycles, so the last sample runs onto the first cleanly. Built
/// with the engine's own trigonometry for the reason `games/atrium`'s `tone` generator gives.
fn a_tone(hertz: f32, seconds: f32, sample_rate: u32) -> SoundData {
    let frames = (seconds * sample_rate as f32).round() as usize;
    let samples = (0..frames)
        .map(|index| {
            let degrees = 360.0 * hertz * (index as f32 / sample_rate as f32);
            // Quiet. This runs on somebody's actual speakers, possibly with headphones on.
            sin_cos_degrees(degrees).0 * 0.25
        })
        .collect();

    SoundData {
        samples,
        channels: 1,
        sample_rate,
    }
}

#[test]
#[ignore = "opens a real audio device and needs a person to listen; see the module docs"]
fn a_sound_moves_around_the_listener() {
    println!("\n--- the listening test ---");
    println!("You should hear, over about eight seconds:");
    println!("  1. a steady quiet tone that starts in front of you;");
    println!("  2. it circles you — right, behind, left, front — and should track smoothly");
    println!("     rather than jumping between the two channels;");
    println!("  3. it gets quieter as it swings away and louder as it comes back;");
    println!("  4. it stops cleanly at the end, with no click.");
    println!("Headphones make points 2 and 3 much easier to judge.\n");

    let mut backend = match KiraAudio::new() {
        Ok(backend) => backend,
        Err(error) => {
            // Not a failure. This test is run by hand on a machine that may not have a device, and
            // "there is no sound card" is information rather than a bug.
            println!("no audio device on this machine, so there is nothing to hear: {error}");
            return;
        }
    };

    backend
        .upload("circling", a_tone(440.0, 1.0, 48_000))
        .expect("a plain mono tone should upload");
    assert!(backend.has("circling"));

    let mut world = World::new();
    let source = world.spawn();

    // The ears at the origin, facing -Z, which is what an unrotated listener does (ADR 0018).
    let listener = Listener {
        position: [0.0, 0.0, 0.0],
        forward: [0.0, 0.0, -1.0],
        up: [0.0, 1.0, 0.0],
    };

    let seconds = 8.0;
    let steps = (seconds / FRAME.as_secs_f32()).round() as u32;

    for step in 0..steps {
        let progress = step as f32 / steps as f32;
        // Two full circles at three metres, so the pan is unmistakable and repeats — one lap makes
        // it hard to tell a smooth sweep from a lucky sequence of jumps.
        let (sine, cosine) = sin_cos_degrees(progress * 720.0);
        let radius = 3.0;

        let frame = AudioFrame {
            listener: Some(listener),
            one_shots: Vec::new(),
            voices: vec![Voice {
                source,
                sound: "circling".to_string(),
                bus: Bus::Effects,
                gain: 1.0,
                pitch: 1.0,
                looping: true,
                position: Some([sine * radius, 0.0, -cosine * radius]),
            }],
        };

        backend
            .submit(&frame)
            .expect("submitting a frame should never fail once the sound is uploaded");
        std::thread::sleep(FRAME);
    }

    // An empty frame is how a sound stops (ADR 0059): the voice is gone from the state, so the
    // backend stops it. Nobody calls `stop`.
    backend
        .submit(&AudioFrame::default())
        .expect("an empty frame is valid");
    // Long enough for the fade-out to finish before the backend is dropped, which would cut it.
    std::thread::sleep(std::time::Duration::from_millis(200));

    println!("\n--- done. If all four points held, the backend works. ---\n");
}

#[test]
#[ignore = "opens a real audio device and needs a person to listen; see the module docs"]
fn two_sounds_start_and_stop_independently() {
    println!("\n--- the second listening test ---");
    println!("You should hear, over about six seconds:");
    println!("  1. one steady low tone, from everywhere (no panning);");
    println!("  2. after two seconds a second, higher tone joins it;");
    println!("  3. after two more the *low* one stops and the high one keeps going, unchanged —");
    println!("     it must not restart, stutter, or change volume when the other stops.\n");

    let mut backend = match KiraAudio::new() {
        Ok(backend) => backend,
        Err(error) => {
            println!("no audio device on this machine: {error}");
            return;
        }
    };

    backend
        .upload("low", a_tone(220.0, 1.0, 48_000))
        .expect("uploads");
    backend
        .upload("high", a_tone(330.0, 1.0, 48_000))
        .expect("uploads");

    let mut world = World::new();
    let first = world.spawn();
    let second = world.spawn();

    let listener = Listener {
        position: [0.0; 3],
        forward: [0.0, 0.0, -1.0],
        up: [0.0, 1.0, 0.0],
    };

    // Non-spatial, so nothing pans and the only thing changing is which voices exist. That is what
    // makes point 3 a clean observation: a stutter there is the reconciliation being wrong, and it
    // is the failure `VoiceTracker` exists to prevent.
    let voice = |source, sound: &str| Voice {
        source,
        sound: sound.to_string(),
        bus: Bus::Effects,
        gain: 0.6,
        pitch: 1.0,
        looping: true,
        position: None,
    };

    let play = |backend: &mut KiraAudio, voices: Vec<Voice>, seconds: f32| {
        let steps = (seconds / FRAME.as_secs_f32()).round() as u32;
        for _ in 0..steps {
            backend
                .submit(&AudioFrame {
                    listener: Some(listener),
                    voices: voices.clone(),
                    one_shots: Vec::new(),
                })
                .expect("submitting should not fail");
            std::thread::sleep(FRAME);
        }
    };

    play(&mut backend, vec![voice(first, "low")], 2.0);
    play(
        &mut backend,
        vec![voice(first, "low"), voice(second, "high")],
        2.0,
    );
    play(&mut backend, vec![voice(second, "high")], 2.0);

    backend
        .submit(&AudioFrame::default())
        .expect("an empty frame is valid");
    std::thread::sleep(std::time::Duration::from_millis(200));

    println!("\n--- done. ---\n");
}

#[test]
#[ignore = "opens a real audio device and needs a person to listen; see the module docs"]
fn one_shots_fire_once_each_and_from_where_they_happened() {
    println!("\n--- the third listening test: one-shots ---");
    println!("You should hear, over about five seconds:");
    println!("  1. eight separate blips, evenly spaced — **eight, not sixteen and not one**.");
    println!("     Each is a single event; hearing doubles means the same one played twice;");
    println!("  2. they walk from your left to your right as they go;");
    println!("  3. then two more from directly in front, with no gap of silence swallowing them;");
    println!("  4. nothing keeps playing afterwards — a one-shot ends by itself.");
    println!("Counting is the point of this one. Headphones for point 2.\n");

    let mut backend = match KiraAudio::new() {
        Ok(backend) => backend,
        Err(error) => {
            println!("no audio device on this machine: {error}");
            return;
        }
    };

    // Short and percussive, so eight of them are countable. A one-second drone would blur.
    backend
        .upload("blip", a_tone(880.0, 0.12, 48_000))
        .expect("uploads");

    let listener = Listener {
        position: [0.0; 3],
        forward: [0.0, 0.0, -1.0],
        up: [0.0, 1.0, 0.0],
    };

    let blip = |x: f32| OneShot {
        sound: "blip".to_string(),
        bus: Bus::Effects,
        gain: 0.8,
        pitch: 1.0,
        position: Some([x, 0.0, -1.0]),
    };

    // Eight events over four seconds, each carried by exactly one frame. Every other frame carries
    // an empty list, which is what a backend must treat as "nothing new happened" rather than as
    // "stop what you started".
    let total_frames = (4.0 / FRAME.as_secs_f32()).round() as u32;
    for step in 0..total_frames {
        let every = total_frames / 8;
        let one_shots = if every > 0 && step % every == 0 {
            let progress = f32::from(u16::try_from(step / every).unwrap_or(0)) / 7.0;
            vec![blip(-3.0 + progress * 6.0)]
        } else {
            Vec::new()
        };

        backend
            .submit(&AudioFrame {
                listener: Some(listener),
                voices: Vec::new(),
                one_shots,
            })
            .expect("submitting should not fail");
        std::thread::sleep(FRAME);
    }

    // Two in one frame, which is what a busy tick produces and the case where a backend that keyed
    // one-shots by anything would collapse them into one.
    backend
        .submit(&AudioFrame {
            listener: Some(listener),
            voices: Vec::new(),
            one_shots: vec![
                OneShot {
                    position: None,
                    ..blip(0.0)
                },
                OneShot {
                    pitch: 0.75,
                    position: None,
                    ..blip(0.0)
                },
            ],
        })
        .expect("submitting should not fail");

    std::thread::sleep(std::time::Duration::from_millis(800));
    println!("\n--- done. If you counted eight then two, the one-shot path works. ---\n");
}

#[test]
fn the_tracker_agrees_with_what_that_procedure_expects() {
    // **This one is not ignored**, and it is the part of the two procedures above that *can* be
    // checked. The listening tests are watching for a behaviour — one voice stopping must not
    // disturb another — and that behaviour is decided by `VoiceTracker`, not by kira.
    //
    // So the claim is testable after all, one layer down: when the low voice goes away, the high
    // one must produce no work at all. If this is red, do not bother listening.
    let mut world = World::new();
    let first = world.spawn();
    let second = world.spawn();

    let voice = |source, sound: &str| Voice {
        source,
        sound: sound.to_string(),
        bus: Bus::Effects,
        gain: 0.6,
        pitch: 1.0,
        looping: true,
        position: None,
    };
    let frame = |voices| AudioFrame {
        listener: Some(Listener {
            position: [0.0; 3],
            forward: [0.0, 0.0, -1.0],
            up: [0.0, 1.0, 0.0],
        }),
        voices,
        one_shots: Vec::new(),
    };

    let mut tracker = VoiceTracker::new();
    tracker.reconcile(&frame(vec![voice(first, "low")]));
    tracker.reconcile(&frame(vec![voice(first, "low"), voice(second, "high")]));

    let changes = tracker.reconcile(&frame(vec![voice(second, "high")]));

    assert_eq!(changes.stopped, vec![first], "only the low voice stops");
    assert!(
        changes.started.is_empty() && changes.updated.is_empty(),
        "the surviving voice must be left completely alone, got {changes:?}"
    );
}
