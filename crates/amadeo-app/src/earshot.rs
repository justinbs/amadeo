//! Whether a wall stands between a sound and the ears — ADR 0086.
//!
//! # Why this lives here and not in `amadeo-audio`
//!
//! `amadeo-audio` cannot ask. It does not depend on `amadeo-physics` and it must not: a 2D
//! platformer, `games/vault` and every grand-strategy target want spatial sound and do not want a
//! solver in the crate graph to get it. So the audio crate **owns the slot** — `AudioSource::
//! occlusion` — and something above both crates fills it.
//!
//! That inversion is not novel. `Overlay`, `TextureCache`, `MeshCache` and `SkyCache` all work this
//! way, and `amadeo-ui` sits above `amadeo-render` for exactly this reason: the renderer cannot look
//! for a `UiNode`, so it owns a slot and the higher crate fills it. This is the fifth instance.
//!
//! # What it does not do
//!
//! It does not decide how much quieter a blocked sound should be. **A wall does not make a sound
//! quieter, it makes it dull** — it removes the top of the spectrum — and a voice attenuated in gain
//! alone tells a player *"that is further away"* when a game whose antagonist is found by ear needs
//! them to distinguish *far* from *behind that bulkhead*. This writes *how blocked*, and the mix
//! decides what to do about it.

use amadeo_audio::{AudioListener, AudioSource};
use amadeo_ecs::{Entity, World};
use amadeo_physics::{Collider, Physics, Shape, ShapeCast};
use amadeo_transform::{GlobalTransform, Parent, Transform};

/// The label [`occlude_voices`] is registered under.
pub const OCCLUDE_VOICES: &str = "occlude_voices";

/// The radius of the probe swept from a sound to the ears, in world units.
///
/// **Not a zero-width ray**, for `modules/amadeo-interaction`'s reason: a hairline ray threads the
/// gap between two wall panels and reports a clear path where a person would hear a wall. This is
/// how wide a gap has to be before sound is treated as coming through it.
pub const EARSHOT_PROBE: f32 = 0.15;

/// How far `occlusion` may move in one tick.
///
/// **The reason a single cast is not enough.** A cast answers `blocked` or `clear`, so assigning its
/// answer steps a voice from 0.0 to 1.0 in one tick every time the listener crosses a doorway edge —
/// which is an audible click, and which fails `docs/13` §1b's F6 clause (a) by construction.
///
/// Easing is also the *more* correct answer, not merely the usable one: sound diffracts around an
/// edge and the ear integrates over tens of milliseconds, so an instantaneous step is the physically
/// wrong shape as well as the unpleasant one.
///
/// A constant rather than an authored field, because it is a property of hearing rather than of a
/// level. At 60 Hz this crosses the full range in about a third of a second.
pub const OCCLUSION_RATE: f32 = 0.05;

/// Writes how blocked each spatial voice is, from the listener's position.
///
/// Does nothing at all unless the world has **both** a [`Physics`] and an
/// [`Audio`](amadeo_audio::Audio) service, so a game with no solver, or no sound, pays one branch.
///
/// # Run it after `step_physics`
///
/// ADR 0054's rule: a backend answers from an index the step builds, so a cast made before it
/// queries an empty world and reports every path clear — which is the exact defect this exists to
/// remove, restored silently.
pub fn occlude_voices(world: &mut World) {
    if world.service::<Physics>().is_none() {
        return;
    }
    let Some((listener, ears)) = listener_at(world) else {
        // No listener means nothing is being heard from anywhere, so there is nothing to block.
        return;
    };
    // **The listener's own body is ignored, and forgetting it makes everything sound blocked.**
    // A cast ending at the ears passes through whatever the ears are attached to just before it
    // arrives, so every voice in the world reads as fully occluded -- which is indistinguishable
    // from working, because the whole game just gets quieter. `modules/amadeo-interaction` hit the
    // same defect from the other end in session 18: the ears are usually a child of a character and
    // have no collider of their own, so it is the *body* that has to be named.
    let body = body_of(world, listener);

    // Collected before the cast, because `cast_shape` borrows the service and writing the component
    // borrows the world. Three phases, which is the same shape `move_the_warden` uses.
    let sources: Vec<(Entity, [f32; 3], f32)> = world
        .query::<(&AudioSource, &Transform, Option<&GlobalTransform>)>()
        .filter(|(_, (source, _, _))| source.spatial && source.playing)
        .map(|(entity, (source, transform, global))| {
            let at = match global {
                Some(global) => global.translation(),
                None => transform.translation,
            };
            (entity, at, source.occlusion)
        })
        .collect();

    let mut settled: Vec<(Entity, f32)> = Vec::with_capacity(sources.len());
    if let Some(physics) = world.service::<Physics>() {
        for (entity, at, was) in sources {
            let motion = [ears[0] - at[0], ears[1] - at[1], ears[2] - at[2]];
            let cast = ShapeCast {
                skin: 0.0,
                ignore: body,
                ..ShapeCast::new(
                    Shape::Sphere {
                        radius: EARSHOT_PROBE,
                    },
                    at,
                    motion,
                )
            };
            // `None` is a clear path. Anything else is something solid between the two, and unlike
            // a sight line there is no "it is the thing I was aiming at" exception — a body between
            // a sound and the ears muffles it whoever it belongs to.
            let target = if physics.cast_shape(&cast).is_some() {
                1.0
            } else {
                0.0
            };
            settled.push((entity, ease(was, target)));
        }
    }

    for (entity, occlusion) in settled {
        if let Some(source) = world.get_mut::<AudioSource>(entity) {
            source.occlusion = occlusion;
        }
    }
}

/// Moves `from` towards `towards` by at most [`OCCLUSION_RATE`].
fn ease(from: f32, towards: f32) -> f32 {
    let step = towards - from;
    if step.abs() <= OCCLUSION_RATE {
        towards
    } else {
        from + OCCLUSION_RATE * step.signum()
    }
}

/// Where the ears are and which entity carries them, if there are any.
fn listener_at(world: &World) -> Option<(Entity, [f32; 3])> {
    world
        .query::<(&AudioListener, &Transform, Option<&GlobalTransform>)>()
        .map(|(entity, (_, transform, global))| {
            let at = match global {
                Some(global) => global.translation(),
                None => transform.translation,
            };
            (entity, at)
        })
        .next()
}

/// The nearest entity at or above `entity` that has a collider.
///
/// An `AudioListener` is normally a **child** — ears on a camera on a character — and a child like
/// that has no collider of its own, so ignoring the listener itself would ignore nothing. This is
/// `modules/amadeo-interaction`'s `body_of` for the same reason, and that module found it the hard
/// way: every cast came back at zero distance against the parent and the symptom was silence rather
/// than an error.
///
/// The loop is bounded rather than `while`, because a `Parent` cycle would otherwise hang the tick.
/// Sixteen is deeper than any hierarchy this engine has reason to build.
fn body_of(world: &World, entity: Entity) -> Option<Entity> {
    let mut at = entity;
    for _ in 0..16 {
        if world.get::<Collider>(at).is_some() {
            return Some(at);
        }
        at = world.get::<Parent>(at)?.0;
    }
    None
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn easing_never_jumps_further_than_the_rate() {
        assert!((ease(0.0, 1.0) - OCCLUSION_RATE).abs() < 1e-6);
        assert!((ease(1.0, 0.0) - (1.0 - OCCLUSION_RATE)).abs() < 1e-6);
    }

    #[test]
    fn easing_settles_exactly_rather_than_oscillating() {
        // Without the `<=` branch a value within one step of its target overshoots and comes back,
        // which is a voice that never stops moving and a backend that never stops being told about
        // it — `VoiceTracker`'s "an unchanged frame must produce no work at all", broken.
        assert_eq!(ease(0.99, 1.0), 1.0);
        assert_eq!(ease(1.0, 1.0), 1.0);
    }
}
