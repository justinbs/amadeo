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
}
