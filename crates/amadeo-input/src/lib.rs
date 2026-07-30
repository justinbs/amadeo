//! Action-based input with deterministic sampling, recording, and replay.
//!
//! # Gameplay reads actions, never keys
//!
//! ```
//! use amadeo_core::Tick;
//! use amadeo_ecs::World;
//! use amadeo_input::{ActionId, InputDriver, InputState, ScriptedSource, install, sample_input};
//!
//! let mut world = World::new();
//!
//! let mut source = ScriptedSource::new();
//! source.press(Tick(0), "jump", true);
//! install(&mut world, InputDriver::new(Box::new(source)));
//!
//! // The app loop runs this once per tick, before gameplay.
//! sample_input(&mut world);
//!
//! let input = world.resource::<InputState>().expect("installed");
//! assert!(input.just_pressed(ActionId::new("jump")));
//! ```
//!
//! # Why this is the determinism boundary
//!
//! [`InputState`] is a resource, so it is part of the world's state hash. The [`InputDriver`] that
//! fills it is a *service*, so it is not. That split is deliberate: the input a run received is
//! simulation state, but where it came from — a keyboard, a replay file, an agent's injection — is
//! not, and recording a run must not change that run's state hash.
//!
//! The consequence is the property everything else rests on: **the simulation cannot tell a live
//! player from a replay.** A recorded run plays back through the same code that produced it, so a
//! replay is a real execution rather than an imitation of one.

mod action;
mod recording;
mod source;
mod state;

pub use action::{ActionId, ActionKind};
pub use recording::{InputChange, Recording, ReplayError};
pub use source::{
    InputDriver, InputSource, LiveSource, NullSource, Recorder, ReplaySource, ScriptedSource,
};
pub use state::InputState;

use amadeo_ecs::World;

/// The label the app layer registers [`sample_input`] under.
///
/// Exported so game systems can declare `.after(amadeo_input::SAMPLE_INPUT)` instead of repeating a
/// string literal that could drift.
pub const SAMPLE_INPUT: &str = "sample_input";

/// Prepares a world for input: adds the [`InputState`] resource and the driver service.
pub fn install(world: &mut World, driver: InputDriver) {
    world.insert_resource(InputState::new());
    world.insert_service(driver);
}

/// Samples this tick's input. Runs once per tick, in `PreSimulation`, before gameplay.
///
/// Three things happen, in this order:
///
/// 1. current values roll into previous ones, so `just_pressed` and `just_released` work;
/// 2. the driver's source applies this tick's changes;
/// 3. the recorder, if attached, writes down whatever changed.
///
/// Does nothing if [`install`] was never called, rather than panicking — a game with no input is a
/// legitimate thing to run headlessly.
pub fn sample_input(world: &mut World) {
    let tick = world.tick();
    world.with_service_taken::<InputDriver, ()>(|world, driver| {
        let Some(state) = world.resource_mut::<InputState>() else {
            return;
        };
        state.begin_tick();
        driver.source.apply(tick, state);
        if let Some(recorder) = &mut driver.recorder {
            recorder.observe(tick, state);
        }
    });
}

#[cfg(test)]
mod tests {
    use super::*;
    use amadeo_core::Tick;

    #[test]
    fn sampling_without_install_is_harmless() {
        let mut world = World::new();
        sample_input(&mut world);
        assert!(world.resource::<InputState>().is_none());
    }

    #[test]
    fn install_adds_state_and_driver() {
        let mut world = World::new();
        install(&mut world, InputDriver::null());
        assert!(world.has_resource::<InputState>());
        assert!(world.has_service::<InputDriver>());
    }

    #[test]
    fn sampling_advances_edge_detection_each_tick() {
        let mut world = World::new();
        let mut source = ScriptedSource::new();
        source.press(Tick(0), "fire", true);
        install(&mut world, InputDriver::new(Box::new(source)));

        sample_input(&mut world);
        let input = world.resource::<InputState>().expect("installed");
        assert!(input.just_pressed(ActionId::new("fire")));

        // Next tick: still held, but no longer a fresh press.
        world.advance_tick();
        sample_input(&mut world);
        let input = world.resource::<InputState>().expect("installed");
        assert!(input.pressed(ActionId::new("fire")));
        assert!(!input.just_pressed(ActionId::new("fire")));
    }

    #[test]
    fn recording_does_not_change_the_state_hash() {
        // A run that is being recorded must produce the same state as the same run unrecorded, or a
        // recorded playthrough would not match its own replay.
        let mut plain = World::new();
        let mut plain_source = ScriptedSource::new();
        plain_source.press(Tick(0), "fire", true);
        install(&mut plain, InputDriver::new(Box::new(plain_source)));

        let mut recorded = World::new();
        let mut recorded_source = ScriptedSource::new();
        recorded_source.press(Tick(0), "fire", true);
        let mut recorder = Recorder::new(0);
        recorder.register_action("fire");
        install(
            &mut recorded,
            InputDriver::new(Box::new(recorded_source)).recording_with(recorder),
        );

        for _ in 0..5 {
            sample_input(&mut plain);
            sample_input(&mut recorded);
            plain.advance_tick();
            recorded.advance_tick();
        }

        assert_eq!(plain.state_hash(), recorded.state_hash());
        // And the recording actually captured something, so this is not vacuous.
        let driver = recorded.service::<InputDriver>().expect("installed");
        let recorder = driver.recorder.as_ref().expect("recording");
        assert_eq!(recorder.recording().change_count(), 1);
    }

    #[test]
    fn input_state_is_part_of_the_state_hash() {
        let mut idle = World::new();
        install(&mut idle, InputDriver::null());
        sample_input(&mut idle);

        let mut pressing = World::new();
        let mut source = ScriptedSource::new();
        source.press(Tick(0), "fire", true);
        install(&mut pressing, InputDriver::new(Box::new(source)));
        sample_input(&mut pressing);

        assert_ne!(idle.state_hash(), pressing.state_hash());
    }
}
