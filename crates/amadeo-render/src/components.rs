//! What the renderer reads off entities.

use amadeo_core::StableHash;
use amadeo_ecs::{Component, Resource};
use amadeo_reflect::Reflect;

/// Where an entity is in 2D space.
///
/// # Where this lives
///
/// A transform is a foundational concept that physics, animation, and the scene tree will all want,
/// so this is **not** its permanent home. It sits here for M0 because the renderer is currently its
/// only consumer, and inventing a crate to hold one struct would be premature. It moves to
/// `amadeo-scene` when the scene tree lands in M1, along with the `Parent`/`Children` hierarchy and
/// the `GlobalTransform` propagation described in ADR 0004.
#[derive(Debug, Clone, Copy, PartialEq, StableHash, Reflect)]
pub struct Transform2d {
    /// Position in world units.
    #[reflect(unit = "world units", sync = "on_change", interpolate = "linear")]
    pub position: [f32; 2],
    /// Rotation in radians, counter-clockwise.
    #[reflect(unit = "rad", sync = "on_change", interpolate = "angular")]
    pub rotation: f32,
    /// Scale multiplier on each axis.
    #[reflect(sync = "on_change", interpolate = "linear")]
    pub scale: [f32; 2],
}

impl Default for Transform2d {
    fn default() -> Self {
        Self {
            position: [0.0, 0.0],
            rotation: 0.0,
            scale: [1.0, 1.0],
        }
    }
}

impl Transform2d {
    /// A transform at a position, unrotated and unscaled.
    #[must_use]
    pub fn at(x: f32, y: f32) -> Self {
        Self {
            position: [x, y],
            ..Self::default()
        }
    }
}

impl Component for Transform2d {}

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
    /// Draw order. Higher values draw on top of lower ones.
    ///
    /// An explicit integer rather than an implied ordering, because iteration order is an
    /// implementation detail and must never decide what appears in front (invariant I3 makes it
    /// reproducible, but reproducible-and-arbitrary is still the wrong thing to rely on).
    pub layer: i32,
}

impl Default for Quad {
    fn default() -> Self {
        Self {
            size: [1.0, 1.0],
            color: [1.0, 1.0, 1.0, 1.0],
            layer: 0,
        }
    }
}

impl Quad {
    /// A quad of a given size and colour on layer zero.
    #[must_use]
    pub fn new(width: f32, height: f32, color: [f32; 4]) -> Self {
        Self {
            size: [width, height],
            color,
            layer: 0,
        }
    }

    /// Places this quad on a draw layer.
    #[must_use]
    pub fn on_layer(mut self, layer: i32) -> Self {
        self.layer = layer;
        self
    }
}

impl Component for Quad {}

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
    fn transform_defaults_to_identity() {
        let transform = Transform2d::default();
        assert_eq!(transform.position, [0.0, 0.0]);
        assert_eq!(transform.scale, [1.0, 1.0]);
        assert_eq!(transform.rotation, 0.0);
    }

    #[test]
    fn transform_at_sets_position_only() {
        let transform = Transform2d::at(3.0, -4.0);
        assert_eq!(transform.position, [3.0, -4.0]);
        assert_eq!(transform.scale, [1.0, 1.0]);
    }

    #[test]
    fn quad_builder_reads_clearly() {
        let quad = Quad::new(2.0, 3.0, [1.0, 0.0, 0.0, 1.0]).on_layer(5);
        assert_eq!(quad.size, [2.0, 3.0]);
        assert_eq!(quad.layer, 5);
    }

    #[test]
    fn components_hash_by_value() {
        assert_eq!(
            stable_hash_of(&Transform2d::at(1.0, 2.0)),
            stable_hash_of(&Transform2d::at(1.0, 2.0))
        );
        assert_ne!(
            stable_hash_of(&Transform2d::at(1.0, 2.0)),
            stable_hash_of(&Transform2d::at(1.0, 2.1))
        );
        // Layer is part of the value, so a layer change is a state change.
        assert_ne!(
            stable_hash_of(&Quad::default()),
            stable_hash_of(&Quad::default().on_layer(1))
        );
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
