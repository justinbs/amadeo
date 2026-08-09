//! Deciding whether something can possibly be seen — M2.5's exit gate 3.
//!
//! # What this is for
//!
//! Every mesh in the world was submitted to the GPU every frame. In `games/scarp` that is 50 chunk
//! meshes of which **30 are behind or beside the camera**, measured through `render.describe`. A
//! frustum test is the standard first answer: six planes, one box test per mesh, and anything
//! entirely outside every plane is never handed to the backend at all.
//!
//! # One implementation, two callers, on purpose
//!
//! [`Frustum`] is used both by the collection pass, which decides what is drawn, and by
//! `render.describe`, which reports what is visible. Those two answering differently is the worst
//! available outcome: `describe` would report culling that did not happen, or miss culling that did,
//! and the gate is *measured through `describe`*. Same type, same arithmetic, no way to drift — the
//! same discipline `MeshData::bounds` follows for the box being tested.
//!
//! # Conservative, deliberately
//!
//! The box test can keep something that is fully outside — a large box straddling a corner is the
//! classic case. It can never **drop** something that is inside. That asymmetry is the right one:
//! drawing a few extra meshes costs a little time, and culling something visible makes geometry
//! vanish as the camera turns, which reads as a streaming or loading bug rather than as a culling
//! one.

use amadeo_transform::Mat4;

/// The six planes bounding what a camera can see.
///
/// Each plane is `[a, b, c, d]` with `a·x + b·y + c·z + d >= 0` **inside**. The normals point
/// inward, so "inside every plane" means "inside the frustum".
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct Frustum {
    planes: [[f32; 4]; 6],
}

impl Frustum {
    /// Extracts the planes from a view-projection matrix.
    ///
    /// # How this works, since it looks like magic
    ///
    /// A point is inside the frustum exactly when its clip-space coordinates satisfy
    /// `-w <= x <= w`, `-w <= y <= w` and `0 <= z <= w` — that is what a projection matrix is *for*.
    /// Each of those six inequalities, written out in terms of the world position, **is** a plane
    /// equation whose coefficients are a sum or difference of two rows of the matrix. So the planes
    /// fall out of the matrix by addition, with no trigonometry and no knowledge of whether the
    /// projection was perspective or orthographic.
    ///
    /// This is the Gribb–Hartmann extraction, and the depth convention matters: `wgpu` clips `z` to
    /// `0..w` rather than `-w..w`, so the near plane is row 2 alone rather than `row3 + row2`. Using
    /// the OpenGL form here would put the near plane in the wrong place and cull things just in front
    /// of the camera.
    ///
    /// The planes are **not normalised**. Nothing here needs a true distance — only the sign — and
    /// skipping it avoids six square roots and any question about a degenerate matrix.
    #[must_use]
    pub fn from_view_projection(view_projection: &Mat4) -> Frustum {
        // Row `i` of a column-major matrix, gathered across the columns.
        let row = |i: usize| {
            [
                view_projection.columns[0][i],
                view_projection.columns[1][i],
                view_projection.columns[2][i],
                view_projection.columns[3][i],
            ]
        };
        let add = |a: [f32; 4], b: [f32; 4]| [a[0] + b[0], a[1] + b[1], a[2] + b[2], a[3] + b[3]];
        let sub = |a: [f32; 4], b: [f32; 4]| [a[0] - b[0], a[1] - b[1], a[2] - b[2], a[3] - b[3]];

        let (x, y, z, w) = (row(0), row(1), row(2), row(3));
        Frustum {
            planes: [
                add(w, x), // left:   x >= -w
                sub(w, x), // right:  x <=  w
                add(w, y), // bottom: y >= -w
                sub(w, y), // top:    y <=  w
                z,         // near:   z >=  0
                sub(w, z), // far:    z <=  w
            ],
        }
    }

    /// Whether an axis-aligned box might be visible.
    ///
    /// `true` means "cannot be ruled out" rather than "is definitely on screen" — see the module
    /// docs on why the error is deliberately in that direction.
    ///
    /// # The one trick
    ///
    /// For each plane, only **one** corner of the box needs testing: the one furthest along the
    /// plane's normal. If even that corner is behind the plane then all eight are, and the box is
    /// entirely outside. Picking it is a sign check per axis, so this is six planes × three
    /// comparisons rather than six × eight corners.
    #[must_use]
    pub fn intersects_aabb(&self, min: [f32; 3], max: [f32; 3]) -> bool {
        for plane in &self.planes {
            // The corner furthest along this plane's normal.
            let corner = [
                if plane[0] >= 0.0 { max[0] } else { min[0] },
                if plane[1] >= 0.0 { max[1] } else { min[1] },
                if plane[2] >= 0.0 { max[2] } else { min[2] },
            ];
            let distance =
                plane[0] * corner[0] + plane[1] * corner[1] + plane[2] * corner[2] + plane[3];
            if distance < 0.0 {
                // Entirely on the outside of this plane, so entirely outside the frustum.
                return false;
            }
        }
        true
    }

    /// Whether a single point is inside.
    ///
    /// Mostly for tests and for answering "is the player on screen"; the box test is what culling
    /// uses.
    #[must_use]
    pub fn contains_point(&self, point: [f32; 3]) -> bool {
        self.intersects_aabb(point, point)
    }
}

/// The world-space box containing a mesh's bounds after its model matrix has been applied.
///
/// **All eight corners are transformed**, not the two extremes, because a rotated box's extremes are
/// not the extremes of its image — transforming `min` and `max` alone produces a box that is too
/// small on every axis the rotation touches, and a box that is too small culls things that are on
/// screen. That failure reads as geometry flickering out as the camera turns.
#[must_use]
pub fn transformed_bounds(model: &Mat4, min: [f32; 3], max: [f32; 3]) -> ([f32; 3], [f32; 3]) {
    let mut low = [f32::INFINITY; 3];
    let mut high = [f32::NEG_INFINITY; 3];

    for index in 0..8_usize {
        let pick = |axis: usize| {
            if index & (1 << axis) == 0 {
                min[axis]
            } else {
                max[axis]
            }
        };
        let transformed = model.transform_point4([pick(0), pick(1), pick(2)]);
        for axis in 0..3 {
            low[axis] = low[axis].min(transformed[axis]);
            high[axis] = high[axis].max(transformed[axis]);
        }
    }
    (low, high)
}

#[cfg(test)]
mod tests {
    use super::*;

    /// A camera five units back along +Z, looking down its own −Z at the origin.
    fn looking_at_the_origin() -> Mat4 {
        let projection = Mat4::perspective(60.0, 1.0, 0.1, 100.0);
        let mut eye = Mat4::IDENTITY;
        eye.columns[3] = [0.0, 0.0, 5.0, 1.0];
        projection.mul(&eye.inverse_rigid().expect("a translation inverts"))
    }

    #[test]
    fn something_straight_ahead_is_inside() {
        let frustum = Frustum::from_view_projection(&looking_at_the_origin());
        assert!(frustum.contains_point([0.0, 0.0, 0.0]));
        assert!(frustum.intersects_aabb([-1.0, -1.0, -1.0], [1.0, 1.0, 1.0]));
    }

    #[test]
    fn something_behind_the_camera_is_outside() {
        // The case that decides whether the near plane was extracted with the right depth
        // convention. `wgpu` clips z to 0..w, so the near plane is row 2 alone — using OpenGL's
        // `row3 + row2` here would put it a long way behind the camera and keep everything.
        let frustum = Frustum::from_view_projection(&looking_at_the_origin());
        assert!(!frustum.contains_point([0.0, 0.0, 20.0]));
        assert!(!frustum.intersects_aabb([-1.0, -1.0, 19.0], [1.0, 1.0, 21.0]));
    }

    #[test]
    fn something_far_to_the_side_is_outside() {
        let frustum = Frustum::from_view_projection(&looking_at_the_origin());
        assert!(!frustum.contains_point([500.0, 0.0, 0.0]));
        assert!(!frustum.contains_point([0.0, 500.0, 0.0]));
        assert!(!frustum.intersects_aabb([400.0, -1.0, -1.0], [600.0, 1.0, 1.0]));
    }

    #[test]
    fn something_beyond_the_far_plane_is_outside() {
        let frustum = Frustum::from_view_projection(&looking_at_the_origin());
        assert!(!frustum.contains_point([0.0, 0.0, -500.0]));
    }

    #[test]
    fn a_box_straddling_the_edge_is_kept() {
        // The conservative direction, stated as a test so it is a decision rather than an accident.
        // A box mostly off to the left but reaching into the view must be kept: culling it would
        // make geometry vanish at the screen edge as the camera turns.
        let frustum = Frustum::from_view_projection(&looking_at_the_origin());
        assert!(frustum.intersects_aabb([-50.0, -1.0, -1.0], [-0.5, 1.0, 1.0]));
    }

    #[test]
    fn a_rotated_box_keeps_every_corner_inside_its_world_bounds() {
        // Why `transformed_bounds` walks all eight corners. Under a quarter turn about Y the box's
        // x extent comes from its *z* extent, and transforming min and max alone would report a box
        // too small on both — which culls things that are on screen.
        let mut model = Mat4::IDENTITY;
        // A 90-degree rotation about Y: x maps to -z, z maps to x.
        model.columns[0] = [0.0, 0.0, -1.0, 0.0];
        model.columns[2] = [1.0, 0.0, 0.0, 0.0];

        let (low, high) = transformed_bounds(&model, [-1.0, -2.0, -3.0], [1.0, 2.0, 3.0]);

        // The long axis was z and is now x.
        assert!((low[0] - -3.0).abs() < 1e-5, "{low:?}");
        assert!((high[0] - 3.0).abs() < 1e-5, "{high:?}");
        assert!((low[2] - -1.0).abs() < 1e-5, "{low:?}");
        assert!((high[2] - 1.0).abs() < 1e-5, "{high:?}");
        // Y is untouched by a turn about Y.
        assert!((low[1] - -2.0).abs() < 1e-5, "{low:?}");
    }

    #[test]
    fn an_orthographic_view_culls_too() {
        // The same extraction has to work for a 2D camera and for a shadow map's light view, both of
        // which are orthographic. `w` is constant there, so the side planes come out as plain
        // constants rather than depending on position — which is exactly right.
        let projection = Mat4::orthographic(10.0, 10.0, -100.0, 100.0);
        let frustum = Frustum::from_view_projection(&projection);

        assert!(frustum.contains_point([0.0, 0.0, 0.0]));
        assert!(frustum.contains_point([9.0, -9.0, 0.0]));
        assert!(!frustum.contains_point([11.0, 0.0, 0.0]));
        assert!(!frustum.contains_point([0.0, -11.0, 0.0]));
    }
}
