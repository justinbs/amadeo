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
//! 2. [`Environment::bloom`] — bleed the bright parts
//! 3. [`Environment::tonemap`] — bring it into a range a monitor can show
//! 4. [`Environment::grade`] — contrast, saturation, tint
//! 5. [`Environment::vignette`] — darken the edges
//!
//! # One field here is not a post-process at all
//!
//! [`Environment::fog`] is atmosphere rather than finishing, and it happens in `mesh.wgsl` to each
//! *surface* rather than to the finished picture — because how much air a fragment is behind depends
//! on how far away that fragment is (ADR 0073). It is authored here because it is unarguably part of
//! a camera's look; it travels to the shader in the per-camera view uniform rather than the post
//! one.
//!
//! **This module used to say fog was waiting for a depth buffer.** That was true of a *post-process*
//! implementation and quietly became the reason it stayed unbuilt for four milestones after the
//! depth buffer arrived. A fragment shader already knows its own world position and the camera's, so
//! the distance is a subtraction and no depth buffer is involved.

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
    #[reflect(min = 0.0, max = 100.0, default = 1.0)]
    pub threshold: f32,
    /// How much of the bleed is added back. **Zero is off**, and is the default.
    #[reflect(min = 0.0, max = 4.0, default = 0.0)]
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
    #[reflect(min = 0.0, max = 4.0, default = 1.0)]
    pub contrast: f32,
    /// `0.0` is greyscale, `1.0` unchanged, above that oversaturated.
    #[reflect(min = 0.0, max = 4.0, default = 1.0)]
    pub saturation: f32,
    /// Multiplied into the final colour. White leaves it unchanged.
    #[reflect(min = 0.0, max = 4.0, default = [1.0, 1.0, 1.0])]
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
    #[reflect(min = 0.0, max = 1.0, default = 0.0)]
    pub intensity: f32,
    /// How far out the darkening starts, as a fraction of the half-diagonal.
    #[reflect(min = 0.0, max = 2.0, default = 0.75)]
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

/// Air between the eye and everything else (ADR 0073).
///
/// **The one part of an `Environment` that is not a post-process.** Everything else here happens to
/// the finished picture; fog happens to each *surface*, in `mesh.wgsl`, because how much air a
/// fragment is behind depends on how far away that fragment is. It travels to the shader in the
/// per-camera view uniform rather than the post uniform for the same reason.
///
/// # It is what a dark corridor is made of
///
/// M3's exit gate item 5 asks for "a dark corridor with a moving flashlight that reads as genuinely
/// atmospheric", and fog is most of the first half. Without it a corridor is fully lit or fully
/// black at its far end; with it the end recedes, which is the difference between a room with the
/// lights off and somewhere you cannot see into.
#[derive(Debug, Clone, Copy, PartialEq, StableHash, Reflect)]
pub struct Fog {
    /// What the air itself looks like, in **linear** light — the same convention every light in a
    /// scene file uses, not the sRGB one a `.theme` uses.
    ///
    /// Worth choosing against the scene rather than by taste: fog that is lighter than the darkest
    /// surface makes distance glow, which reads as mist, and fog that is darker makes distance
    /// swallow things, which reads as depth. A horror interior wants the second.
    #[reflect(min = 0.0, max = 4.0, default = [0.0, 0.0, 0.0])]
    pub colour: [f32; 3],
    /// How quickly it closes in, per world unit. **Zero is off**, and is the default.
    ///
    /// Roughly: at `1 / density` metres past [`Fog::start`] a surface is about 63% fogged, and at
    /// twice that it is nearly gone. So `0.05` is a haze that reaches about forty metres and `0.2`
    /// is a corridor you cannot see the end of.
    #[reflect(min = 0.0, max = 1.0, default = 0.0)]
    pub density: f32,
    /// How far from the eye the air begins, in world units.
    ///
    /// Subtracted before the curve rather than dividing the range, so it means exactly "nothing
    /// closer than this is fogged" and moving it does not also change how thick the distance is.
    #[reflect(min = 0.0, max = 200.0, unit = "world units", default = 0.0)]
    pub start: f32,
}

impl Default for Fog {
    fn default() -> Self {
        Self {
            colour: [0.0, 0.0, 0.0],
            density: 0.0,
            start: 0.0,
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
// **No longer `Copy`**, since ADR 0049 gave this a `sky` asset id and a `String` cannot be. That is
// a real if small cost: an `Environment` is now cloned where it used to be copied, once per view per
// frame. Measured against the alternative — an interned id, or a fixed-size array of bytes — which
// would buy back a handful of nanoseconds in exchange for a type nobody could read.
#[derive(Debug, Clone, PartialEq, StableHash, Reflect)]
pub struct Environment {
    /// Linear multiplier on everything the cameras drew, applied first.
    ///
    /// The photographic control: `2.0` is one stop brighter. Above 1.0 this is what pushes highlights
    /// past the display range, which is what gives bloom and tonemapping anything to work with.
    #[reflect(min = 0.0, max = 100.0, default = 1.0)]
    pub exposure: f32,
    /// Light bleeding out of the brightest parts. Runs before tonemapping, on purpose.
    #[reflect(default = Bloom::default())]
    pub bloom: Bloom,
    /// How the result is brought into displayable range.
    #[reflect(default = Tonemap::default())]
    pub tonemap: Tonemap,
    /// Contrast, saturation and tint, applied after tonemapping.
    #[reflect(default = Grade::default())]
    pub grade: Grade,
    /// Edge darkening, applied last because it is about *where* a pixel is rather than its colour.
    #[reflect(default = Vignette::default())]
    pub vignette: Vignette,
    /// Air between the eye and everything else (ADR 0073).
    ///
    /// **The only field here that is not a post-process**, and the only one the mesh shader reads.
    /// Off by default, so a scene that authors none is byte-identical.
    #[reflect(default = Fog::default())]
    pub fog: Fog,
    /// Declared asset id of the `.hdr` environment map this look lights surfaces with (ADR 0049).
    /// **Empty means none**, and falls back to a plain neutral sky.
    ///
    /// # Why the sky lives on the look rather than on a light
    ///
    /// **Q28 asked exactly this**, and the answer is that a `DirectionalLight` is a *direct* light —
    /// one direction, one colour, casting a shadow — whereas an environment map is the **indirect**
    /// half: everything arriving from everywhere else. They are different quantities that happen to
    /// both be called lighting.
    ///
    /// `Environment` is already "what this camera sees the world as" and is already an asset with a
    /// cache behind it (ADR 0034), so this needed no new asset kind, no new component and no new
    /// loading path. A world may hold several lights; it has one look per view, which is what an
    /// environment map is — one surrounding, not one source among several.
    ///
    /// Cheap to move if that turns out wrong: it is one field and the handful of `.environment`
    /// files that name it.
    #[reflect(default = String::new())]
    pub sky: String,
    /// How much of [`Environment::sky`] reaches surfaces as ambient light — and **only** that.
    ///
    /// The drawn backdrop is unaffected: the sky pass reads the map at full brightness whatever this
    /// says. `1.0` is the map used as authored, so a scene that omits it is byte-identical.
    ///
    /// # Why the sky's two jobs are two numbers
    ///
    /// An environment map is a **picture** and a **light**, and until this field existed one scalar
    /// was both. The Atrium is where that stopped working: the map had to be scaled to `0.34` to keep
    /// it from washing the room out as fill, and at `0.34` the daylight visible through the oculus
    /// was *darker than the sunlit floor beneath it* — a sky you are looking straight at, dimmer than
    /// a surface it is lighting. Turning it up to fix the picture blew out the fill it was tuned for.
    /// There is no value that is right for both, because they are not the same quantity.
    ///
    /// **Every comparable engine splits these, independently**, which is about as strong a signal as
    /// engine design offers: Unity separates the skybox material's exposure from Environment
    /// Lighting's *Intensity Multiplier*; Unreal separates the sky's brightness from the Sky Light's
    /// *Intensity Scale*; Godot separates `sky_energy_multiplier` from `ambient_light_energy`.
    ///
    /// So the `.hdr` holds **the sky's real colour** and this holds how much of it bounces back in.
    /// Baking the fill into the file instead is what makes a map unusable as a backdrop, and it is
    /// what `games/scarp` and `games/warren` both still do — both left alone deliberately, since the
    /// default is the identity and neither has a sky anyone can see.
    #[reflect(min = 0.0, max = 100.0, default = 1.0)]
    pub sky_ambient: f32,
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
            fog: Fog::default(),
            sky: String::new(),
            sky_ambient: 1.0,
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
        self.loaded.get(id).cloned().unwrap_or_default()
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
            fog: Fog {
                colour: [0.04, 0.05, 0.06],
                density: 0.08,
                start: 2.5,
            },
            sky: "overcast_afternoon".to_string(),
            sky_ambient: 0.6,
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
