//! What the renderer reads off entities.

use amadeo_core::StableHash;
use amadeo_ecs::{Component, Resource};
use amadeo_reflect::Reflect;

/// A flat coloured rectangle.
///
/// The simplest thing that can be drawn, and enough to prove the whole pipeline: simulation to
/// screen. Textured sprites, materials, and meshes arrive in M1 and M2 respectively.
#[derive(Debug, Clone, Copy, PartialEq, StableHash, Reflect)]
pub struct Quad {
    /// Full width and height in world units, before the transform's scale is applied.
    #[reflect(unit = "world units")]
    pub size: [f32; 2],
    /// Linear RGBA, each channel in `0.0..=1.0`.
    #[reflect(min = 0.0, max = 1.0)]
    pub color: [f32; 4],
}

impl Default for Quad {
    fn default() -> Self {
        Self {
            size: [1.0, 1.0],
            color: [1.0, 1.0, 1.0, 1.0],
        }
    }
}

impl Quad {
    /// A quad of a given size and colour.
    #[must_use]
    pub fn new(width: f32, height: f32, color: [f32; 4]) -> Self {
        Self {
            size: [width, height],
            color,
        }
    }
}

impl Component for Quad {}

/// A textured rectangle.
///
/// The 2D workhorse. Three of the eight target games — Terraria, RimWorld, and Project Zomboid —
/// ship on this rather than on meshes, so it is a first-class path and not a stepping stone to 3D
/// (`docs/00-vision.md` § Target games).
///
/// # Why the texture is a `String`
///
/// It is an asset **id**, not a path and not a handle (ADR 0020), and ids are what a scene file
/// writes and what `amadeo assets` lists. Storing the id itself means the component reads the same
/// in a `.scene`, in a `world.entity` dump, and in memory — no resolution step where the three could
/// disagree.
///
/// The cost is a heap allocation per sprite and a string comparison when batching, which at Terraria
/// scale is a real question rather than a theoretical one. **It was measured rather than assumed** —
/// see `crates/amadeo-render/tests/sprite_throughput.rs` and ADR 0023.
///
/// # It never observes the asset's state
///
/// ADR 0021: gameplay holds an id and nothing else. A `Sprite` whose texture has not loaded is not
/// an error and is not a branch — the renderer draws a placeholder and reports it. Nothing about
/// whether the texture is resident can reach the simulation.
#[derive(Debug, Clone, PartialEq, StableHash, Reflect)]
pub struct Sprite {
    /// The texture's declared asset id (ADR 0020), as `amadeo assets` lists it.
    pub texture: String,
    /// Full width and height in world units, before the transform's scale is applied.
    #[reflect(unit = "world units")]
    pub size: [f32; 2],
    /// Linear RGBA multiplied into the texture. White leaves it unchanged.
    #[reflect(min = 0.0, max = 1.0)]
    pub color: [f32; 4],
    /// Which part of the texture to draw, as `[x, y, width, height]` in `0.0..=1.0`.
    ///
    /// The whole texture by default. This is what makes a tilesheet or a sprite atlas work without a
    /// separate concept: a tile is a region of one shared texture, which is also what lets thousands
    /// of tiles collapse into a single batch.
    #[reflect(min = 0.0, max = 1.0)]
    pub region: [f32; 4],
}

impl Default for Sprite {
    fn default() -> Self {
        Self {
            texture: String::new(),
            size: [1.0, 1.0],
            color: [1.0, 1.0, 1.0, 1.0],
            region: [0.0, 0.0, 1.0, 1.0],
        }
    }
}

impl Sprite {
    /// A sprite drawing a whole texture at a given size.
    #[must_use]
    pub fn new(texture: impl Into<String>, width: f32, height: f32) -> Self {
        Self {
            texture: texture.into(),
            size: [width, height],
            ..Sprite::default()
        }
    }

    /// The same sprite showing only part of its texture — one cell of a tilesheet.
    #[must_use]
    pub fn with_region(mut self, x: f32, y: f32, width: f32, height: f32) -> Self {
        self.region = [x, y, width, height];
        self
    }

    /// The same sprite, tinted.
    #[must_use]
    pub fn with_color(mut self, color: [f32; 4]) -> Self {
        self.color = color;
        self
    }
}

impl Component for Sprite {}

/// What draws on top of what.
///
/// Higher values draw later, so they appear in front. Absent means zero.
// Not a doc comment: this is the description `amadeo describe` prints.
//
// ADR 0018. This used to be `Quad::layer`, and it moved out for two reasons. It is not a property of
// being a rectangle -- a mesh needs it too -- and having a per-primitive layer *and* a shared order
// would mean two things deciding what is in front, which is how "why is this behind that" becomes a
// half-hour question.
//
// Explicit data rather than an implied ordering, because iteration order is an implementation detail
// and must never decide what appears in front. Invariant I3 makes iteration order reproducible, but
// reproducible-and-arbitrary is still the wrong thing to rely on.
//
// In 3D this dominates the depth buffer rather than replacing it: within one `SortOrder`, opaque
// geometry uses depth and transparent geometry sorts back to front. A 2D scene leaves everything at
// one depth and distinguishes purely by this. "UI over the world" is a higher `SortOrder`, not a
// separate concept.
//
// A named field rather than a tuple struct, which is what it started as: the reflection derive turns
// a tuple struct into a newtype, and a newtype has no field name for a scene file to write. Every
// other component in a `.scene` is an indented block of named fields, and `SortOrder` being the one
// exception would be a format wart for the sake of two characters in Rust.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default, StableHash, Reflect)]
pub struct SortOrder {
    /// Higher draws later, so it appears in front.
    pub order: i32,
}

impl SortOrder {
    /// A sort order.
    #[must_use]
    pub fn new(order: i32) -> Self {
        Self { order }
    }
}

impl Component for SortOrder {}

/// An orthographic 2D camera.
///
/// A [`Resource`] rather than a component for M0: one camera, and it is simulation state because
/// gameplay moves it. Multiple cameras and render targets become components in M2, when the render
/// graph can express more than one pass.
#[derive(Debug, Clone, Copy, PartialEq, StableHash, Reflect)]
pub struct Camera2d {
    /// World-space point at the centre of the view.
    #[reflect(unit = "world units")]
    pub center: [f32; 2],
    /// How many world units tall the view is. Width follows from the viewport's aspect ratio, so
    /// resizing the window widens the view rather than stretching it.
    #[reflect(min = 0.1, max = 1000.0, unit = "world units")]
    pub height: f32,
}

impl Default for Camera2d {
    fn default() -> Self {
        Self {
            center: [0.0, 0.0],
            height: 10.0,
        }
    }
}

impl Resource for Camera2d {}

impl Camera2d {
    /// Converts a world position to normalised device coordinates for a given viewport.
    ///
    /// Returns x and y in `-1.0..=1.0`, with y pointing up — the convention wgpu uses.
    #[must_use]
    pub fn world_to_ndc(&self, world: [f32; 2], viewport: (u32, u32)) -> [f32; 2] {
        let (width, height) = viewport;
        // A zero-sized viewport happens legitimately when a window is minimised. Falling back to a
        // square avoids a division by zero producing NaN coordinates.
        let aspect = if height == 0 || width == 0 {
            1.0
        } else {
            width as f32 / height as f32
        };
        let half_height = self.height / 2.0;
        let half_width = half_height * aspect;

        [
            (world[0] - self.center[0]) / half_width,
            (world[1] - self.center[1]) / half_height,
        ]
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use amadeo_core::hash::stable_hash_of;

    #[test]
    fn quad_builder_reads_clearly() {
        let quad = Quad::new(2.0, 3.0, [1.0, 0.0, 0.0, 1.0]);
        assert_eq!(quad.size, [2.0, 3.0]);
        assert_eq!(quad.color, [1.0, 0.0, 0.0, 1.0]);
    }

    #[test]
    fn components_hash_by_value() {
        assert_ne!(
            stable_hash_of(&Quad::default()),
            stable_hash_of(&Quad::new(2.0, 2.0, [1.0, 1.0, 1.0, 1.0]))
        );
        // Sort order is a component in its own right (ADR 0018), and part of the state hash --
        // moving something in front of something else is a real change.
        assert_ne!(
            stable_hash_of(&SortOrder::default()),
            stable_hash_of(&SortOrder::new(1))
        );
    }

    #[test]
    fn sort_order_defaults_to_zero() {
        // The renderer treats an absent SortOrder as this, so the two must agree.
        assert_eq!(SortOrder::default(), SortOrder::new(0));
    }

    #[test]
    fn camera_maps_its_centre_to_the_origin() {
        let camera = Camera2d {
            center: [5.0, 5.0],
            height: 10.0,
        };
        assert_eq!(camera.world_to_ndc([5.0, 5.0], (800, 600)), [0.0, 0.0]);
    }

    #[test]
    fn camera_maps_vertical_extents_to_the_edges() {
        let camera = Camera2d {
            center: [0.0, 0.0],
            height: 10.0,
        };
        // Half the height above centre is the top of the screen.
        assert_eq!(camera.world_to_ndc([0.0, 5.0], (800, 600))[1], 1.0);
        assert_eq!(camera.world_to_ndc([0.0, -5.0], (800, 600))[1], -1.0);
    }

    #[test]
    fn camera_widens_rather_than_stretching() {
        // A wider viewport must show more world, not distort what is already visible.
        let camera = Camera2d::default();
        let square = camera.world_to_ndc([5.0, 0.0], (600, 600))[0];
        let wide = camera.world_to_ndc([5.0, 0.0], (1200, 600))[0];

        assert_eq!(square, 1.0, "at a 1:1 aspect, x=5 is the right edge");
        assert!(
            wide < square,
            "a wider viewport should push the same point inward, got {wide}"
        );
    }

    #[test]
    fn zero_sized_viewport_does_not_produce_nan() {
        // Minimising a window legitimately reports a zero-sized surface.
        let camera = Camera2d::default();
        let ndc = camera.world_to_ndc([1.0, 1.0], (0, 0));
        assert!(ndc[0].is_finite() && ndc[1].is_finite(), "{ndc:?}");
    }
}
