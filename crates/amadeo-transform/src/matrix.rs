//! A 4×4 matrix, and only as much of one as transform propagation needs.
//!
//! # Why this is here and not in `amadeo-math`
//!
//! `docs/02-tech-stack.md` says maths is glam, wrapped by `amadeo-math` so the engine owns its public
//! surface. That crate does not exist yet, and propagation needs exactly two operations: compose a
//! translation/rotation/scale into a matrix, and multiply two matrices. Designing a whole maths
//! surface backwards from its first caller is how a wrong abstraction gets locked in — so this is the
//! small, obvious thing, and `amadeo-math` arrives when something needs a real surface.
//!
//! # Why scalar arithmetic
//!
//! ADR 0019 keeps `GlobalTransform` out of the state hash, so this arithmetic cannot move a replay
//! on its own. But a gameplay system that reads a `GlobalTransform` and writes the result back into
//! a `Transform` puts it back into hashed state through the side door — "place this child where its
//! parent's hand is" is a real thing to want. Plain scalar `f32` evaluates identically everywhere;
//! a SIMD path is not guaranteed to. The cost of being careful here is nothing.

/// A 4×4 transformation matrix, stored **column-major**.
///
/// Column-major because that is what wgpu and WGSL expect, so a matrix can go to the GPU without
/// being transposed on the way.
///
/// `columns[c][r]` is the entry in column `c`, row `r`.
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct Mat4 {
    /// Four columns of four rows each.
    pub columns: [[f32; 4]; 4],
}

impl Default for Mat4 {
    fn default() -> Self {
        Mat4::IDENTITY
    }
}

impl Mat4 {
    /// The matrix that changes nothing.
    pub const IDENTITY: Mat4 = Mat4 {
        columns: [
            [1.0, 0.0, 0.0, 0.0],
            [0.0, 1.0, 0.0, 0.0],
            [0.0, 0.0, 1.0, 0.0],
            [0.0, 0.0, 0.0, 1.0],
        ],
    };

    /// Builds a matrix that scales, then rotates, then translates.
    ///
    /// That order is the usual one and the one people expect: an object is shaped, then turned, then
    /// put somewhere. Applying translation before rotation would swing the object around the origin
    /// instead of spinning it in place.
    ///
    /// Rotation is Euler angles in **degrees**, applied Z, then X, then Y (ADR 0018).
    #[must_use]
    pub fn from_transform(
        translation: [f32; 3],
        rotation_degrees: [f32; 3],
        scale: [f32; 3],
    ) -> Self {
        let rotation = Mat4::from_euler_degrees(rotation_degrees);

        // Scaling is folded into the rotation's columns rather than done as a separate multiply:
        // each column of a rotation matrix is an axis, so scaling an axis is scaling its column.
        // `take(3)` because the fourth column is translation, which scale must not touch.
        let mut columns = rotation.columns;
        for (column, factor) in columns.iter_mut().take(3).zip(scale.iter()) {
            for value in column.iter_mut() {
                *value *= factor;
            }
        }

        columns[3] = [translation[0], translation[1], translation[2], 1.0];
        Mat4 { columns }
    }

    /// Builds a rotation matrix from Euler angles in degrees, applied Z, then X, then Y.
    #[must_use]
    pub fn from_euler_degrees(degrees: [f32; 3]) -> Self {
        let (sin_x, cos_x) = degrees[0].to_radians().sin_cos();
        let (sin_y, cos_y) = degrees[1].to_radians().sin_cos();
        let (sin_z, cos_z) = degrees[2].to_radians().sin_cos();

        // Written out rather than composed from three matrix multiplies. Three multiplies would be
        // clearer to derive and would also do 192 float operations to produce nine numbers, most of
        // them multiplications by zero -- and this runs per entity per tick.
        Mat4 {
            columns: [
                [
                    cos_y * cos_z + sin_y * sin_x * sin_z,
                    cos_x * sin_z,
                    -sin_y * cos_z + cos_y * sin_x * sin_z,
                    0.0,
                ],
                [
                    -cos_y * sin_z + sin_y * sin_x * cos_z,
                    cos_x * cos_z,
                    sin_y * sin_z + cos_y * sin_x * cos_z,
                    0.0,
                ],
                [sin_y * cos_x, -sin_x, cos_y * cos_x, 0.0],
                [0.0, 0.0, 0.0, 1.0],
            ],
        }
    }

    /// Recovers the Euler angles, in degrees, that [`Mat4::from_euler_degrees`] would build this
    /// from.
    ///
    /// The exact inverse of that function, including its **Z, then X, then Y** order (ADR 0018).
    /// Order matters: nearly every other library composes Euler angles differently — nalgebra's
    /// `from_euler_angles` is roll-pitch-yaw, meaning Z·Y·X — so converting through a library that
    /// disagrees produces rotations that are right for one axis and subtly wrong for the others.
    /// That is the kind of error that reads as "physics is drifting" rather than as a conversion bug.
    ///
    /// Assumes the matrix is a rotation, optionally scaled **uniformly**. A non-uniform scale has no
    /// Euler decomposition, and this will return something plausible and wrong for one — which is why
    /// the only caller is the physics boundary, where rapier hands back pure rotations.
    ///
    /// # Gimbal lock
    ///
    /// When X reaches ±90° the Y and Z rotations become the same motion, so infinitely many pairs
    /// describe the same orientation. This returns one of them rather than failing: the orientation
    /// is correct, but feeding it back in and reading it out again may give different Y and Z. That
    /// is inherent to Euler angles, not to this implementation, and is why ADR 0018 kept them for
    /// *authoring* while everything computed stays a matrix.
    #[must_use]
    pub fn to_euler_degrees(&self) -> [f32; 3] {
        // Undo any uniform scale first, so the trigonometry below sees a pure rotation. Each of the
        // first three columns is an axis scaled by that axis's scale, so its length is the scale.
        let length = |column: [f32; 4]| {
            (column[0] * column[0] + column[1] * column[1] + column[2] * column[2]).sqrt()
        };
        let scale = length(self.columns[0])
            .max(length(self.columns[1]))
            .max(length(self.columns[2]));
        if scale < 1e-6 {
            return [0.0, 0.0, 0.0];
        }
        let at = |column: usize, row: usize| self.columns[column][row] / scale;

        // From `from_euler_degrees`, whose third column is [sin_y*cos_x, -sin_x, cos_y*cos_x]:
        //   x comes straight out of that middle entry,
        //   z from the first two entries of the middle *row* (cos_x*sin_z, cos_x*cos_z),
        //   y from the outer entries of the third column (sin_y*cos_x, cos_y*cos_x).
        // The shared cos_x cancels inside each `atan2`, which is why this needs no division.
        let x = (-at(2, 1)).clamp(-1.0, 1.0).asin();
        let z = at(0, 1).atan2(at(1, 1));
        let y = at(2, 0).atan2(at(2, 2));

        [x.to_degrees(), y.to_degrees(), z.to_degrees()]
    }

    /// Multiplies two matrices: `self` applied *after* `rhs`.
    ///
    /// For a hierarchy this reads as `parent.mul(child)` — the child's local transform first, then
    /// its parent's, which is what "the child is positioned relative to its parent" means.
    #[must_use]
    pub fn mul(&self, rhs: &Mat4) -> Mat4 {
        let mut out = [[0.0_f32; 4]; 4];
        for (column, target) in out.iter_mut().enumerate() {
            for (row, cell) in target.iter_mut().enumerate() {
                // The four products are written out rather than summed in an inner loop, so the
                // addition order is fixed by this source rather than by how a compiler chooses to
                // unroll it. Float addition is not associative, and this is exactly the kind of
                // thing that drifts between builds.
                *cell = self.columns[0][row] * rhs.columns[column][0]
                    + self.columns[1][row] * rhs.columns[column][1]
                    + self.columns[2][row] * rhs.columns[column][2]
                    + self.columns[3][row] * rhs.columns[column][3];
            }
        }
        Mat4 { columns: out }
    }

    /// The translation this matrix applies — its fourth column.
    ///
    /// What a renderer or a "where is this actually" query wants, without decomposing the rest.
    #[must_use]
    pub fn translation(&self) -> [f32; 3] {
        [self.columns[3][0], self.columns[3][1], self.columns[3][2]]
    }

    /// A perspective projection: things further away are smaller.
    ///
    /// `fov_degrees` is the **vertical** field of view, matching `amadeo-render`'s
    /// `Projection::Perspective` — vertical rather than horizontal so that widening a window shows
    /// more of the world instead of squashing it, which is the same rule the orthographic path
    /// already follows.
    ///
    /// # The depth range is 0 to 1, not −1 to 1
    ///
    /// OpenGL maps the near plane to −1 and the far plane to +1; Vulkan, DX12, Metal and **WebGPU**
    /// all map near to 0. wgpu follows WebGPU, so this does too. Getting it wrong does not produce
    /// an error — it produces geometry that is clipped at half the distance it should be, which is
    /// the sort of thing that reads as "the far plane setting is broken".
    ///
    /// Reversed depth (near at 1) is the usual precision improvement and is **not** done here: it
    /// needs a matching depth-compare direction and a cleared value of 0, so it is a decision for
    /// when depth precision is an actual problem rather than a speculative one.
    #[must_use]
    pub fn perspective(fov_degrees: f32, aspect: f32, near: f32, far: f32) -> Self {
        // Guarded so a degenerate camera produces a usable matrix rather than infinities that
        // silently turn every vertex into NaN.
        let fov = fov_degrees.clamp(0.001, 179.0).to_radians();
        let aspect = if aspect.abs() < 1e-6 { 1.0 } else { aspect };
        let far = if (far - near).abs() < 1e-6 {
            near + 1.0
        } else {
            far
        };

        // Cotangent of half the vertical field of view: the distance at which the view is one unit
        // tall, which is what turns a world offset into a clip-space one.
        let focal = 1.0 / (fov / 2.0).tan();

        let mut columns = [[0.0_f32; 4]; 4];
        columns[0][0] = focal / aspect;
        columns[1][1] = focal;
        columns[2][2] = far / (near - far);
        columns[2][3] = -1.0;
        columns[3][2] = (near * far) / (near - far);
        Mat4 { columns }
    }

    /// A box-shaped projection: no perspective, so parallel lines stay parallel.
    ///
    /// The box is **centred on the eye** and symmetric, which is what a shadow map for a directional
    /// light wants — the sun has no position, so there is nothing to be off-centre from.
    ///
    /// `half_width` and `half_height` are half the box's extent across, `near` and `far` its extent
    /// along the view direction. Depth maps to 0 at `near` and 1 at `far`, the same WebGPU convention
    /// [`Mat4::perspective`] follows and for the same reason.
    #[must_use]
    pub fn orthographic(half_width: f32, half_height: f32, near: f32, far: f32) -> Self {
        // Guarded the same way `perspective` is: a degenerate box produces a usable matrix rather
        // than infinities that turn every vertex into NaN one frame later.
        let half_width = if half_width.abs() < 1e-6 {
            1.0
        } else {
            half_width
        };
        let half_height = if half_height.abs() < 1e-6 {
            1.0
        } else {
            half_height
        };
        let far = if (far - near).abs() < 1e-6 {
            near + 1.0
        } else {
            far
        };

        let mut columns = [[0.0_f32; 4]; 4];
        columns[0][0] = 1.0 / half_width;
        columns[1][1] = 1.0 / half_height;
        // Negative, because the view looks down its own negative Z: a point further away has a more
        // negative z, and must come out with a *larger* depth.
        columns[2][2] = 1.0 / (near - far);
        columns[3][2] = near / (near - far);
        columns[3][3] = 1.0;
        Mat4 { columns }
    }

    /// A view matrix for something at `eye` looking along `direction`.
    ///
    /// **This is a view matrix, not a world matrix** — it takes world space *to* the viewer's space,
    /// which is what a projection expects to be handed. [`Mat4::inverse_rigid`] is the other way of
    /// arriving at one, from a transform; this is the way that is convenient when all you have is a
    /// direction, which is exactly the case for a directional light.
    ///
    /// The viewer looks along its own **negative Z**, the same convention a camera and a
    /// `DirectionalLight` both use, so `direction` is the direction light *travels*.
    ///
    /// `up` only resolves the remaining roll. When it is parallel to `direction` there is no unique
    /// answer, so a different up is substituted rather than returning a matrix full of NaN — a light
    /// pointing straight down is the overwhelmingly common case and must not be the broken one.
    #[must_use]
    pub fn look_along(eye: [f32; 3], direction: [f32; 3], up: [f32; 3]) -> Self {
        let normalise = |v: [f32; 3]| {
            let length = (v[0] * v[0] + v[1] * v[1] + v[2] * v[2]).sqrt();
            if length < 1e-6 {
                None
            } else {
                Some([v[0] / length, v[1] / length, v[2] / length])
            }
        };
        let cross = |a: [f32; 3], b: [f32; 3]| {
            [
                a[1] * b[2] - a[2] * b[1],
                a[2] * b[0] - a[0] * b[2],
                a[0] * b[1] - a[1] * b[0],
            ]
        };
        let dot = |a: [f32; 3], b: [f32; 3]| a[0] * b[0] + a[1] * b[1] + a[2] * b[2];

        // Looking *along* the negative Z axis means the positive Z axis points back the way we came.
        let forward = normalise(direction).unwrap_or([0.0, -1.0, 0.0]);
        let z_axis = [-forward[0], -forward[1], -forward[2]];

        // A light pointing straight down has `up` parallel to its view direction, so the cross
        // product collapses. +Z is an arbitrary but stable substitute; any vector not parallel to
        // the direction gives an equally valid matrix, and they differ only by a roll nobody can see
        // in a depth-only render.
        let x_axis = match normalise(cross(up, z_axis)) {
            Some(axis) => axis,
            None => normalise(cross([0.0, 0.0, 1.0], z_axis)).unwrap_or([1.0, 0.0, 0.0]),
        };
        let y_axis = cross(z_axis, x_axis);

        // The rotation part of a view matrix is the transpose of the viewer's basis, and the
        // translation is the eye moved through it, negated -- the same shape `inverse_rigid` builds.
        let mut columns = [[0.0_f32; 4]; 4];
        for row in 0..3 {
            columns[row][0] = x_axis[row];
            columns[row][1] = y_axis[row];
            columns[row][2] = z_axis[row];
        }
        columns[3][0] = -dot(x_axis, eye);
        columns[3][1] = -dot(y_axis, eye);
        columns[3][2] = -dot(z_axis, eye);
        columns[3][3] = 1.0;
        Mat4 { columns }
    }

    /// The inverse of a matrix that only rotates, scales uniformly, and translates.
    ///
    /// **A camera's view matrix is exactly this**: the world seen from the camera is the inverse of
    /// where the camera is.
    ///
    /// # Why not a general inverse
    ///
    /// A general 4×4 inverse is a cofactor expansion — long, easy to get subtly wrong, and pointless
    /// here, because the only matrices this is ever asked for are built from a translation, a
    /// rotation and a scale. For those the answer is short and obvious: undo the scale, transpose
    /// the rotation, and negate the translation through both.
    ///
    /// Returns `None` when a column has collapsed to zero length, which means a scale of zero on
    /// some axis. That matrix has genuinely no inverse — it flattened the world — so a camera with
    /// a zero scale draws nothing rather than filling the screen with NaN.
    #[must_use]
    pub fn inverse_rigid(&self) -> Option<Mat4> {
        // Each of the first three columns is an axis scaled by that axis's scale, so its length is
        // the scale and dividing by the square of it both normalises and undoes the scaling.
        let mut basis = [[0.0_f32; 3]; 3];
        for (axis, column) in basis.iter_mut().enumerate() {
            let source = self.columns[axis];
            let square = source[0] * source[0] + source[1] * source[1] + source[2] * source[2];
            if square < 1e-12 {
                return None;
            }
            for (row, cell) in column.iter_mut().enumerate() {
                *cell = source[row] / square;
            }
        }

        // Transposing the (unscaled) basis inverts the rotation.
        let translation = self.translation();
        let mut columns = [[0.0_f32; 4]; 4];
        for row in 0..3 {
            for (column, source) in basis.iter().enumerate() {
                columns[row][column] = source[row];
            }
        }
        // The translation moves through the inverted basis, negated.
        for (column, source) in basis.iter().enumerate() {
            columns[3][column] = -(translation[0] * source[0]
                + translation[1] * source[1]
                + translation[2] * source[2]);
        }
        columns[3][3] = 1.0;
        Some(Mat4 { columns })
    }

    /// Applies this matrix to a point, dividing through by w.
    ///
    /// Returns `None` when w is zero or negative, which for a projection means the point is **behind
    /// the camera** — dividing anyway would fold it back onto the screen mirrored, which is the
    /// classic "geometry appears when you turn away from it" bug.
    #[must_use]
    pub fn project_point(&self, point: [f32; 3]) -> Option<[f32; 3]> {
        let mut out = [0.0_f32; 4];
        for (row, cell) in out.iter_mut().enumerate() {
            *cell = self.columns[0][row] * point[0]
                + self.columns[1][row] * point[1]
                + self.columns[2][row] * point[2]
                + self.columns[3][row];
        }
        if out[3] <= 1e-6 {
            return None;
        }
        Some([out[0] / out[3], out[1] / out[3], out[2] / out[3]])
    }
}

#[cfg(test)]
mod euler_tests {
    use super::*;

    /// The angles that come back must rebuild the same matrix. Comparing *matrices* rather than
    /// angles is what makes this meaningful — two different angle triples can describe one
    /// orientation, and the orientation is what anything downstream actually uses.
    fn round_trips(degrees: [f32; 3]) {
        let original = Mat4::from_euler_degrees(degrees);
        let rebuilt = Mat4::from_euler_degrees(original.to_euler_degrees());
        for (a, b) in original.columns.iter().zip(rebuilt.columns.iter()) {
            for (x, y) in a.iter().zip(b.iter()) {
                assert!(
                    (x - y).abs() < 1e-4,
                    "{degrees:?} did not round trip: {original:?} vs {rebuilt:?}"
                );
            }
        }
    }

    #[test]
    fn euler_angles_survive_a_round_trip() {
        for degrees in [
            [0.0, 0.0, 0.0],
            [30.0, 0.0, 0.0],
            [0.0, 45.0, 0.0],
            [0.0, 0.0, -60.0],
            [15.0, 25.0, 35.0],
            [-40.0, 170.0, 80.0],
            [12.5, -95.0, -170.0],
        ] {
            round_trips(degrees);
        }
    }

    #[test]
    fn the_order_is_z_then_x_then_y_and_not_the_other_convention() {
        // The trap this function exists to avoid. Composing the same three angles in a different
        // order gives a *different orientation*, so a conversion that quietly used nalgebra's
        // roll-pitch-yaw would be right for one axis and wrong for the others.
        let ours = Mat4::from_euler_degrees([30.0, 40.0, 50.0]);
        // Z·Y·X — nalgebra's order — built from the same numbers.
        let theirs = Mat4::from_euler_degrees([0.0, 0.0, 50.0])
            .mul(&Mat4::from_euler_degrees([0.0, 40.0, 0.0]))
            .mul(&Mat4::from_euler_degrees([30.0, 0.0, 0.0]));

        let differs = ours
            .columns
            .iter()
            .zip(theirs.columns.iter())
            .any(|(a, b)| a.iter().zip(b.iter()).any(|(x, y)| (x - y).abs() > 1e-3));
        assert!(differs, "the two conventions should not agree");
    }

    #[test]
    fn a_uniform_scale_does_not_disturb_the_angles() {
        // rapier hands back pure rotations, but a `GlobalTransform` composed through a scaled parent
        // does not — and reading an angle out of one should still give the angle.
        let scaled = Mat4::from_transform([1.0, 2.0, 3.0], [20.0, -35.0, 10.0], [4.0, 4.0, 4.0]);
        let angles = scaled.to_euler_degrees();
        assert!((angles[0] - 20.0).abs() < 1e-3, "{angles:?}");
        assert!((angles[1] + 35.0).abs() < 1e-3, "{angles:?}");
        assert!((angles[2] - 10.0).abs() < 1e-3, "{angles:?}");
    }

    #[test]
    fn a_collapsed_matrix_reports_no_rotation_rather_than_nan() {
        let flat = Mat4::from_transform([0.0; 3], [45.0, 45.0, 45.0], [0.0, 0.0, 0.0]);
        assert_eq!(flat.to_euler_degrees(), [0.0, 0.0, 0.0]);
    }
}

#[cfg(test)]
mod perspective_tests {
    use super::*;

    fn near_far() -> (f32, f32) {
        (0.1, 100.0)
    }

    #[test]
    fn the_near_plane_maps_to_zero_and_the_far_plane_to_one() {
        // The WebGPU depth convention, and the one thing about this matrix that has no visible
        // symptom other than geometry being clipped at the wrong distance. OpenGL would put the
        // near plane at -1, and copying a matrix from an OpenGL tutorial is exactly how that
        // happens.
        let (near, far) = near_far();
        let projection = Mat4::perspective(60.0, 16.0 / 9.0, near, far);

        // A point straight ahead is at -z, because the camera looks down its own negative z axis.
        let at_near = projection
            .project_point([0.0, 0.0, -near])
            .expect("in front of the camera");
        let at_far = projection
            .project_point([0.0, 0.0, -far])
            .expect("in front of the camera");

        assert!(
            at_near[2].abs() < 1e-4,
            "near should be 0, got {}",
            at_near[2]
        );
        assert!(
            (at_far[2] - 1.0).abs() < 1e-4,
            "far should be 1, got {}",
            at_far[2]
        );
    }

    #[test]
    fn something_behind_the_camera_does_not_project() {
        // Dividing by a negative w folds a point back onto the screen, mirrored — which reads as
        // "geometry appears when I turn away from it" and is very hard to attribute.
        let (near, far) = near_far();
        let projection = Mat4::perspective(60.0, 1.0, near, far);
        assert!(projection.project_point([0.0, 0.0, 5.0]).is_none());
    }

    #[test]
    fn the_vertical_field_of_view_is_what_was_asked_for() {
        // Vertical rather than horizontal, so widening a window shows more world instead of
        // squashing it — the same rule the orthographic path follows.
        let (near, far) = near_far();
        let projection = Mat4::perspective(90.0, 2.0, near, far);

        // At 90° vertical, a point one unit up at one unit away sits exactly on the top edge.
        let edge = projection
            .project_point([0.0, 1.0, -1.0])
            .expect("in front");
        assert!(
            (edge[1] - 1.0).abs() < 1e-4,
            "expected the top edge, got {edge:?}"
        );

        // And the aspect ratio widens rather than stretches: at aspect 2, the same offset
        // horizontally is only half as far across the screen.
        let across = projection
            .project_point([1.0, 0.0, -1.0])
            .expect("in front");
        assert!(
            (across[0] - 0.5).abs() < 1e-4,
            "expected half, got {across:?}"
        );
    }

    #[test]
    fn a_view_matrix_undoes_the_cameras_transform() {
        // The property that makes `inverse_rigid` a view matrix: applying it to the camera's own
        // position gives the origin, because the camera is at the centre of its own view.
        let camera = Mat4::from_transform([3.0, 4.0, 5.0], [0.0, 30.0, 0.0], [1.0, 1.0, 1.0]);
        let view = camera.inverse_rigid().expect("a real transform inverts");

        let at_origin = view.project_point([3.0, 4.0, 5.0]).expect("w is 1 here");
        assert!(
            at_origin.iter().all(|value| value.abs() < 1e-4),
            "the camera should sit at its own origin, got {at_origin:?}"
        );

        // And it really is the inverse: composed either way round, nothing moves.
        let identity = view.mul(&camera);
        for (column, expected) in identity.columns.iter().zip(Mat4::IDENTITY.columns.iter()) {
            for (got, want) in column.iter().zip(expected.iter()) {
                assert!((got - want).abs() < 1e-4, "not an inverse: {identity:?}");
            }
        }
    }

    #[test]
    fn a_scaled_camera_still_inverts() {
        let camera = Mat4::from_transform([1.0, 2.0, 3.0], [15.0, 0.0, 45.0], [2.0, 2.0, 2.0]);
        let view = camera.inverse_rigid().expect("uniform scale inverts");
        let identity = view.mul(&camera);
        for (column, expected) in identity.columns.iter().zip(Mat4::IDENTITY.columns.iter()) {
            for (got, want) in column.iter().zip(expected.iter()) {
                assert!((got - want).abs() < 1e-4, "not an inverse: {identity:?}");
            }
        }
    }

    #[test]
    fn a_collapsed_transform_has_no_inverse() {
        // A zero scale flattens the world, so there is genuinely nothing to invert. Returning None
        // is what keeps a misconfigured camera from filling the screen with NaN.
        let flat = Mat4::from_transform([0.0, 0.0, 0.0], [0.0, 0.0, 0.0], [1.0, 0.0, 1.0]);
        assert!(flat.inverse_rigid().is_none());
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    /// Matrices of floats need a tolerance; trigonometry does not land on exact zeros.
    fn close(a: &Mat4, b: &Mat4) -> bool {
        a.columns
            .iter()
            .zip(b.columns.iter())
            .all(|(x, y)| x.iter().zip(y.iter()).all(|(p, q)| (p - q).abs() < 1e-5))
    }

    #[test]
    fn identity_changes_nothing() {
        let m = Mat4::from_transform([1.0, 2.0, 3.0], [10.0, 20.0, 30.0], [2.0, 2.0, 2.0]);
        assert!(close(&Mat4::IDENTITY.mul(&m), &m));
        assert!(close(&m.mul(&Mat4::IDENTITY), &m));
    }

    #[test]
    fn a_transform_with_no_rotation_is_just_scale_and_offset() {
        let m = Mat4::from_transform([5.0, -1.0, 2.0], [0.0, 0.0, 0.0], [2.0, 3.0, 4.0]);

        assert_eq!(m.translation(), [5.0, -1.0, 2.0]);
        assert_eq!(m.columns[0][0], 2.0);
        assert_eq!(m.columns[1][1], 3.0);
        assert_eq!(m.columns[2][2], 4.0);
    }

    #[test]
    fn a_quarter_turn_about_z_maps_x_onto_y() {
        // The 2D case, and the one a 2D game exercises: +90 degrees about Z takes the x axis to y.
        let m = Mat4::from_euler_degrees([0.0, 0.0, 90.0]);
        let x_axis = [m.columns[0][0], m.columns[0][1], m.columns[0][2]];

        assert!((x_axis[0] - 0.0).abs() < 1e-5, "got {x_axis:?}");
        assert!((x_axis[1] - 1.0).abs() < 1e-5, "got {x_axis:?}");
    }

    #[test]
    fn composing_translations_adds_them() {
        let parent = Mat4::from_transform([10.0, 0.0, 0.0], [0.0; 3], [1.0; 3]);
        let child = Mat4::from_transform([0.0, 5.0, 0.0], [0.0; 3], [1.0; 3]);

        assert_eq!(parent.mul(&child).translation(), [10.0, 5.0, 0.0]);
    }

    #[test]
    fn a_parent_rotation_carries_its_child_around() {
        // The reason propagation exists at all: a child offset along x, under a parent turned 90
        // degrees about z, ends up offset along y.
        let parent = Mat4::from_transform([0.0; 3], [0.0, 0.0, 90.0], [1.0; 3]);
        let child = Mat4::from_transform([2.0, 0.0, 0.0], [0.0; 3], [1.0; 3]);

        let world = parent.mul(&child).translation();
        assert!((world[0] - 0.0).abs() < 1e-5, "got {world:?}");
        assert!((world[1] - 2.0).abs() < 1e-5, "got {world:?}");
    }

    #[test]
    fn a_parent_scale_multiplies_a_child_offset() {
        let parent = Mat4::from_transform([0.0; 3], [0.0; 3], [3.0, 3.0, 3.0]);
        let child = Mat4::from_transform([1.0, 2.0, 0.0], [0.0; 3], [1.0; 3]);

        assert_eq!(parent.mul(&child).translation(), [3.0, 6.0, 0.0]);
    }

    #[test]
    fn multiplication_is_not_commutative_and_the_order_is_the_documented_one() {
        // Guards the argument order, which is the single easiest thing to get backwards here.
        let rotate = Mat4::from_transform([0.0; 3], [0.0, 0.0, 90.0], [1.0; 3]);
        let offset = Mat4::from_transform([2.0, 0.0, 0.0], [0.0; 3], [1.0; 3]);

        let rotated_then_offset = rotate.mul(&offset).translation();
        let offset_then_rotated = offset.mul(&rotate).translation();

        assert!((rotated_then_offset[1] - 2.0).abs() < 1e-5);
        assert!((offset_then_rotated[0] - 2.0).abs() < 1e-5);
    }
    #[test]
    fn an_orthographic_box_maps_near_to_zero_and_far_to_one() {
        // The WebGPU depth convention, asserted the same way the perspective test does. Getting this
        // backwards does not error -- it makes everything fail the depth test, which reads as
        // "shadows are missing" rather than as a matrix bug.
        let projection = Mat4::orthographic(10.0, 10.0, 1.0, 51.0);
        let near = projection
            .project_point([0.0, 0.0, -1.0])
            .expect("in front");
        let far = projection
            .project_point([0.0, 0.0, -51.0])
            .expect("in front");
        assert!(
            (near[2] - 0.0).abs() < 1e-5,
            "near should map to 0, got {near:?}"
        );
        assert!(
            (far[2] - 1.0).abs() < 1e-5,
            "far should map to 1, got {far:?}"
        );
    }

    #[test]
    fn an_orthographic_box_does_not_shrink_with_distance() {
        // The whole difference from a perspective projection, and what makes it right for a
        // directional light: the sun's rays are parallel, so its projection must be too.
        let projection = Mat4::orthographic(10.0, 10.0, 1.0, 51.0);
        let near = projection
            .project_point([5.0, 0.0, -2.0])
            .expect("in front");
        let far = projection
            .project_point([5.0, 0.0, -50.0])
            .expect("in front");
        assert!(
            (near[0] - far[0]).abs() < 1e-5,
            "x should not change with depth"
        );
        assert!(
            (near[0] - 0.5).abs() < 1e-5,
            "half the half-width should be 0.5"
        );
    }

    #[test]
    fn look_along_puts_what_it_looks_at_in_front_of_it() {
        // Looking down from above: a point below the eye should land ahead of the viewer, which in
        // this convention means a negative z.
        let view = Mat4::look_along([0.0, 10.0, 0.0], [0.0, -1.0, 0.0], [0.0, 1.0, 0.0]);
        let ahead = view.project_point([0.0, 0.0, 0.0]).expect("finite");
        assert!(
            ahead[2] < 0.0,
            "the ground should be in front of a light above it, got {ahead:?}"
        );
        assert!(
            ahead[0].abs() < 1e-5 && ahead[1].abs() < 1e-5,
            "and centred, got {ahead:?}"
        );
    }

    #[test]
    fn look_along_survives_pointing_straight_down() {
        // The overwhelmingly common case for a sun, and the one where `up` is parallel to the view
        // direction so the usual cross product collapses. It must not be the broken one.
        let view = Mat4::look_along([0.0, 10.0, 0.0], [0.0, -1.0, 0.0], [0.0, 1.0, 0.0]);
        for column in view.columns {
            for cell in column {
                assert!(cell.is_finite(), "a straight-down light produced {view:?}");
            }
        }
    }

    #[test]
    fn look_along_preserves_distance() {
        // A view matrix only rotates and translates, so it must not stretch anything -- a light that
        // scaled the world would put its shadow map's depths on a different scale from its own
        // projection, and the shadows would be uniformly wrong by a factor nobody could see.
        let view = Mat4::look_along([3.0, 4.0, 5.0], [1.0, -2.0, 0.5], [0.0, 1.0, 0.0]);
        let a = view.project_point([0.0, 0.0, 0.0]).expect("finite");
        let b = view.project_point([1.0, 0.0, 0.0]).expect("finite");
        let moved = ((a[0] - b[0]).powi(2) + (a[1] - b[1]).powi(2) + (a[2] - b[2]).powi(2)).sqrt();
        assert!(
            (moved - 1.0).abs() < 1e-5,
            "one unit apart should stay one unit, got {moved}"
        );
    }
}
