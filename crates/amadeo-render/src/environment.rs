//! What a camera's picture looks like: the post-process and atmosphere settings — ADR 0034.
//!
//! # Why this is data rather than code
//!
//! M2 requires that the renderer not bake in a look, and ADR 0034 settled what "configurable" means
//! there: **the engine owns the effects and content configures them**. That is what Godot's
//! `Environment`, Unity's Volume profiles and Unreal's Post Process Volumes all do, and the deciding
//! argument was this project's own invariants rather than anything about rendering. Configuration
//! made of reflected data is authorable in a `.scene`, reported by `describe`, spellable by
//! `describe --example`, validated by `amadeo check`, captured in a snapshot, and visible on a
//! headless run — all for nothing. A pass supplied as code is none of those.
//!
//! # The order is the engine's, and the parameters are yours
//!
//! An [`Environment`] is a fixed set of named blocks, **not** a list content can reorder. The reason
//! is arithmetic rather than taste: exposure scales light before anything looks at it, bloom needs
//! values still above the display range, tonemapping is what collapses that range, and grading and
//! vignetting are corrections applied to the result. A format that let a scene file put tonemapping
//! first would mostly produce wrong pictures, and would have no way to say so.
//!
//! The order, which is also the order the fields are declared in:
//!
//! 1. [`Environment::exposure`] — scale the light
//! 2. [`Environment::bloom`] — bleed the bright parts *(not yet drawn; see below)*
//! 3. [`Environment::tonemap`] — bring it into a range a monitor can show
//! 4. [`Environment::grade`] — contrast, saturation, tint
//! 5. [`Environment::vignette`] — darken the edges
//!
//! # What is deliberately missing
//!
//! **Fog**, which ADR 0034 names, needs to know how far away each pixel is — and there is no depth
//! buffer until the mesh pass lands. Adding it then is a new block plus a new branch in
//! `present.wgsl`, which is exactly the cheap change a reflected type exists for.
//!
//! **Bloom is declared and not yet drawn.** Its fields are here because they are part of the same
//! decision and belong in one round of the schema rather than two; the multi-pass blur that
//! implements it is the next thing to build. [`Bloom::intensity`] defaults to zero, so nothing
//! renders differently in the meantime, and `Environment::wants_bloom` is what will switch the
//! passes on.

use amadeo_core::StableHash;
use amadeo_ecs::{Component, Service};
use amadeo_reflect::Reflect;
use std::collections::BTreeMap;

/// How high-dynamic-range light is brought into a range a screen can display.
///
/// A monitor shows a limited range of brightness. A lit scene does not respect that limit — a lamp
/// is genuinely many times brighter than a wall — so something has to map one onto the other. Doing
/// it by simply clipping anything above the maximum is what makes bright areas turn into flat white
/// blobs; a tonemap curve compresses instead, keeping detail in the highlights.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default, StableHash, Reflect)]
pub enum Tonemap {
    /// Clip anything brighter than the display range.
    ///
    /// **The default, deliberately.** A camera that names no environment renders exactly as it did
    /// before ADR 0034, so adding this system changed no existing picture — which matters, because
    /// both games in this repo have been confirmed on screen and one has tests asserting on captured
    /// pixels.
    #[default]
    None,
    /// The simple curve: `c / (1 + c)`. Never clips, desaturates bright areas noticeably.
    ///
    /// Cheap, predictable, and a reasonable choice for a stylised look.
    Reinhard,
    /// An approximation of the filmic curve used across the film industry.
    ///
    /// Holds highlight detail and keeps colour better than [`Tonemap::Reinhard`]. The right default
    /// for anything aiming at realism, which is what M3's atmospheric slice wants.
    AcesFilmic,
}

/// Light bleeding out of the brightest parts of the picture.
///
/// What makes a lamp look like it is genuinely emitting rather than just being a bright shape.
/// Only values *above* [`Bloom::threshold`] contribute, which is why it has to happen before
/// tonemapping has collapsed them.
#[derive(Debug, Clone, Copy, PartialEq, StableHash, Reflect)]
pub struct Bloom {
    /// How bright a pixel must be before it bleeds. Above 1.0 means "brighter than white".
    #[reflect(min = 0.0, max = 100.0)]
    pub threshold: f32,
    /// How much of the bleed is added back. **Zero is off**, and is the default.
    #[reflect(min = 0.0, max = 4.0)]
    pub intensity: f32,
}

impl Default for Bloom {
    fn default() -> Self {
        Self {
            threshold: 1.0,
            intensity: 0.0,
        }
    }
}

/// Colour correction applied after tonemapping.
///
/// The cheap, always-available half of "the renderer must not bake in a look" — the same picture
/// reads as bleak or warm depending entirely on these three numbers.
#[derive(Debug, Clone, Copy, PartialEq, StableHash, Reflect)]
pub struct Grade {
    /// Pushes values away from mid-grey. `1.0` leaves them alone.
    #[reflect(min = 0.0, max = 4.0)]
    pub contrast: f32,
    /// `0.0` is greyscale, `1.0` unchanged, above that oversaturated.
    #[reflect(min = 0.0, max = 4.0)]
    pub saturation: f32,
    /// Multiplied into the final colour. White leaves it unchanged.
    #[reflect(min = 0.0, max = 4.0)]
    pub tint: [f32; 3],
}

impl Default for Grade {
    fn default() -> Self {
        Self {
            contrast: 1.0,
            saturation: 1.0,
            tint: [1.0, 1.0, 1.0],
        }
    }
}

/// Darkening towards the edges of the picture.
///
/// Pulls the eye to the middle, and is most of what makes a corridor feel enclosed — which is
/// exactly what M3's exit gate 5 is asking for.
#[derive(Debug, Clone, Copy, PartialEq, StableHash, Reflect)]
pub struct Vignette {
    /// How dark the corners go. **Zero is off**, and is the default.
    #[reflect(min = 0.0, max = 1.0)]
    pub intensity: f32,
    /// How far out the darkening starts, as a fraction of the half-diagonal.
    #[reflect(min = 0.0, max = 2.0)]
    pub radius: f32,
}

impl Default for Vignette {
    fn default() -> Self {
        Self {
            intensity: 0.0,
            radius: 0.75,
        }
    }
}

/// Everything about how a camera's picture is finished.
///
/// **An asset, named by an id** (ADR 0034). A [`Camera`](crate::Camera) holds
/// `environment "corridor_dark"`, and the file behind that id is a scene document with a single root
/// carrying one of these — exactly as a prefab is (ADR 0029) and a material will be (ADR 0033). So
/// the parser, the canonical writer, `amadeo fmt`, `amadeo check` and ADR 0032's nested values all
/// work on it without anything being built.
///
/// A camera naming no environment gets [`Environment::default`], which does nothing at all.
///
/// ```
/// # use amadeo_render::{Environment, Tonemap};
/// let mut look = Environment::default();
/// look.tonemap = Tonemap::AcesFilmic;
/// look.vignette.intensity = 0.4;
/// assert!(look.changes_the_picture());
/// ```
///
/// The same thing as a file, which is how it is actually authored:
///
/// ```text
/// scene 1
///
/// entity look "Corridor"
///   Environment
///     exposure 1.2
///     tonemap AcesFilmic
///     vignette
///       intensity 0.4
///       radius 0.6
/// ```
#[derive(Debug, Clone, Copy, PartialEq, StableHash, Reflect)]
pub struct Environment {
    /// Linear multiplier on everything the cameras drew, applied first.
    ///
    /// The photographic control: `2.0` is one stop brighter. Above 1.0 this is what pushes highlights
    /// past the display range, which is what gives bloom and tonemapping anything to work with.
    #[reflect(min = 0.0, max = 100.0)]
    pub exposure: f32,
    /// Light bleeding out of the brightest parts. Runs before tonemapping, on purpose.
    pub bloom: Bloom,
    /// How the result is brought into displayable range.
    pub tonemap: Tonemap,
    /// Contrast, saturation and tint, applied after tonemapping.
    pub grade: Grade,
    /// Edge darkening, applied last because it is about *where* a pixel is rather than its colour.
    pub vignette: Vignette,
}

impl Default for Environment {
    /// The look that does nothing.
    ///
    /// **Hand-written rather than derived, and the reason matters**: a derived `Default` would give
    /// `exposure: 0.0`, which is a black screen. Every "off" value here is off because it is the
    /// *identity* of its operation, not because it is zero — and two of them (`exposure`,
    /// `grade.contrast`) are one rather than zero for exactly that reason.
    fn default() -> Self {
        Self {
            exposure: 1.0,
            bloom: Bloom::default(),
            tonemap: Tonemap::default(),
            grade: Grade::default(),
            vignette: Vignette::default(),
        }
    }
}

impl Environment {
    /// Whether this environment would change the picture at all.
    ///
    /// Not an optimisation — the post pass runs either way, so that the frame has one shape rather
    /// than two. It is here so a test, and `render.describe` later, can say "this camera has a look"
    /// without comparing every field by hand.
    #[must_use]
    pub fn changes_the_picture(&self) -> bool {
        *self != Environment::default()
    }

    /// Whether the bloom passes need to run.
    ///
    /// Separate from [`Environment::changes_the_picture`] because bloom is the one effect that costs
    /// extra *passes* rather than extra arithmetic inside one — so it is worth switching off, where
    /// the rest are not.
    #[must_use]
    pub fn wants_bloom(&self) -> bool {
        self.bloom.intensity > 0.0
    }
}

impl Component for Environment {}

/// Every environment the game has loaded, by asset id.
///
/// # Why the renderer holds these but does not read the files
///
/// An environment's file is a *scene* file, and `amadeo-scene` sits **above** `amadeo-render` in the
/// crate graph — so by invariant I6 this crate cannot parse its own asset. The same split that
/// `TextureCache` has: this crate owns the cache and the type, and something higher up fills it.
/// `App::load_environments` is that something.
///
/// # It is a `Service`, so it cannot move a replay
///
/// ADR 0009 excludes services from `World::state_hash` by trait bound, and ADR 0021 requires that
/// the simulation never observe asset state. A camera holds an *id* and nothing else; whether the
/// environment behind it has loaded is invisible to gameplay, and an id that never resolves renders
/// with the default look rather than failing.
#[derive(Debug, Clone, Default)]
pub struct EnvironmentCache {
    /// Ordered, like every other registry in this engine, so listing it is reproducible (I3).
    loaded: BTreeMap<String, Environment>,
}

impl Service for EnvironmentCache {}

impl EnvironmentCache {
    /// An empty cache.
    #[must_use]
    pub fn new() -> Self {
        Self::default()
    }

    /// Records an environment under an id, replacing any earlier one.
    pub fn insert(&mut self, id: impl Into<String>, environment: Environment) {
        self.loaded.insert(id.into(), environment);
    }

    /// The environment an id names.
    ///
    /// **Never fails.** An empty id means "no environment", and an id that has not loaded is not an
    /// error either — both give [`Environment::default`], which renders exactly as a game with no
    /// post-processing at all. That is ADR 0021's rule: a missing asset is visible and survivable,
    /// never fatal, and here the visible form is simply the unprocessed picture.
    #[must_use]
    pub fn get(&self, id: &str) -> Environment {
        if id.is_empty() {
            return Environment::default();
        }
        self.loaded.get(id).copied().unwrap_or_default()
    }

    /// Whether an id has actually been loaded.
    ///
    /// The honest question behind [`EnvironmentCache::get`]'s forgiving answer: a camera whose look
    /// is silently the default because its file is missing looks identical to one that asked for
    /// nothing, and this is what tells them apart.
    #[must_use]
    pub fn is_loaded(&self, id: &str) -> bool {
        self.loaded.contains_key(id)
    }

    /// Every loaded id, in order.
    pub fn ids(&self) -> impl Iterator<Item = &str> {
        self.loaded.keys().map(String::as_str)
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use amadeo_reflect::Reflect;

    #[test]
    fn the_default_environment_does_nothing() {
        // The property that let this ship without changing either game's confirmed appearance.
        let look = Environment::default();
        assert!(!look.changes_the_picture());
        assert!(!look.wants_bloom());
        assert_eq!(look.exposure, 1.0);
        assert_eq!(look.tonemap, Tonemap::None);
        assert_eq!(look.grade.contrast, 1.0);
        assert_eq!(look.vignette.intensity, 0.0);
    }

    #[test]
    fn an_unloaded_or_empty_id_gives_the_default_rather_than_failing() {
        // ADR 0021: the simulation never observes asset state, so a missing environment is a look
        // rather than an error.
        let cache = EnvironmentCache::new();
        assert_eq!(cache.get(""), Environment::default());
        assert_eq!(cache.get("corridor_dark"), Environment::default());
        // But the two cases are still distinguishable when someone asks directly.
        assert!(!cache.is_loaded("corridor_dark"));
    }

    #[test]
    fn a_loaded_environment_comes_back() {
        let mut cache = EnvironmentCache::new();
        let look = Environment {
            tonemap: Tonemap::AcesFilmic,
            ..Environment::default()
        };
        cache.insert("corridor_dark", look);

        assert_eq!(cache.get("corridor_dark").tonemap, Tonemap::AcesFilmic);
        assert!(cache.is_loaded("corridor_dark"));
        assert_eq!(cache.ids().collect::<Vec<_>>(), ["corridor_dark"]);
    }

    #[test]
    fn an_environment_round_trips_through_the_value_tree() {
        // I8: if it cannot be reflected it cannot be serialised, inspected, or edited — and this one
        // is authored as a file, so the round trip is the whole mechanism rather than a nicety.
        let look = Environment {
            exposure: 1.4,
            tonemap: Tonemap::Reinhard,
            bloom: Bloom {
                threshold: 0.8,
                intensity: 0.5,
            },
            grade: Grade {
                tint: [0.9, 0.95, 1.1],
                ..Grade::default()
            },
            vignette: Vignette {
                intensity: 0.35,
                ..Vignette::default()
            },
        };

        let value = look.to_value();
        let back = Environment::from_value(&value).expect("round trips");
        assert_eq!(back, look);
    }

    #[test]
    fn bloom_is_off_until_its_intensity_is_raised() {
        // Which is what decides whether the blur passes are in the graph at all.
        let mut look = Environment::default();
        look.bloom.threshold = 0.5;
        assert!(!look.wants_bloom(), "threshold alone must not switch it on");
        look.bloom.intensity = 0.2;
        assert!(look.wants_bloom());
    }
}
