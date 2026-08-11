//! Sine and cosine that give the same answer on every machine — ADR 0053.
//!
//! # Why this exists at all
//!
//! ADR 0044 banned `f32::sin`, `f32::cos` and `f32::powf` from anything that decides gameplay state,
//! and the reason is not fussiness. Rust documents their precision as varying **by platform, by
//! version, and between two calls in one execution**, because IEEE 754 requires correct rounding only
//! for `+ - * /` and `sqrt` and lists the transcendentals as merely recommended. A sine in a terrain
//! generator puts Windows and Linux on different ground, and the bug report reads *"the replay does
//! not reproduce on Linux"* while pointing at physics.
//!
//! `amadeo-noise` obeyed that ban by **avoiding** trigonometry. That works for noise and does not
//! work for a camera: placing something at an angle is exactly what trigonometry is for, and there is
//! no way to write an orbit without it. So the ban gets the other half of its answer — not a
//! forbidden operation, but a *specified* one.
//!
//! Everything below is built from `+ - * /` and `floor`, all of which IEEE 754 pins exactly. Two
//! machines agreeing is therefore a property of the arithmetic rather than of the libm they happen
//! to link.
//!
//! # Why the working type is `f64`
//!
//! Not for extra accuracy in the answer — the answer is an `f32`. It is for the **range reduction**.
//! Folding 3000° into the first quadrant in `f32` throws away most of the significant digits before
//! the polynomial ever runs, and the error shows up as a rotation that is visibly wrong at large
//! angles. `f64` addition and multiplication are specified exactly as `f32`'s are, so working wider
//! costs determinism nothing.
//!
//! # What it is accurate to
//!
//! Within a rounding or two of `f32` across the whole circle — about 1e-7 — checked below against
//! the standard library at `f64`, which is the true answer to far more digits than the caller's
//! `f32` can hold. That is far tighter than anything the engine can perceive: at a camera arm of
//! seven metres it is half a micron.
//!
//! Comparing against the standard library at **`f32`** would be the wrong test, and one of the tests
//! below exists to say why: `angle.to_radians()` in `f32` has already lost the digits that decide the
//! answer by the time the angle is large, so past a few turns this module is the *more* accurate of
//! the two and the disagreement is the reference's.
//!
//! # One property worth having on purpose
//!
//! **The cardinal angles are exact.** `sin_cos_degrees(90.0)` returns exactly `(1.0, 0.0)`, and the
//! standard library cannot — `90f32.to_radians().cos()` is about `-4.4e-8`, because the `f32` nearest
//! to π/2 is not π/2. So a quarter turn here produces an *exactly* axis-aligned rotation matrix,
//! where before it produced one with dust in the off-diagonals. That falls out of reducing in degrees
//! and only then converting, rather than converting and then reducing.

/// Sine and cosine of an angle in degrees, in that order.
///
/// Deterministic across platforms — see this module's documentation for why that is not something
/// `f32::sin_cos` can promise, and why it matters to a component that ends up in the state hash.
///
/// A non-finite input gives non-finite output rather than a panic, on the same reasoning
/// [`f32::sin`] uses: this is arithmetic, and arithmetic on a `NaN` yields a `NaN`.
///
/// ```
/// # use amadeo_core::sin_cos_degrees;
/// let (sin, cos) = sin_cos_degrees(90.0);
/// assert_eq!(sin, 1.0);
/// // Exactly zero, which the standard library's version cannot manage.
/// assert_eq!(cos, 0.0);
/// ```
#[must_use]
pub fn sin_cos_degrees(degrees: f32) -> (f32, f32) {
    if !degrees.is_finite() {
        return (f32::NAN, f32::NAN);
    }
    let degrees = f64::from(degrees);

    // Step 1: fold onto one turn, so the polynomial never sees a large argument. `floor` is exact,
    // so this is a subtraction of two exactly-representable quantities.
    let turns = (degrees / 360.0).floor();
    let within_turn = degrees - 360.0 * turns;

    // Step 2: fold onto the first quadrant, remembering which one it came from. Reducing in
    // *degrees* is what makes the cardinal angles exact: 90.0 - 90.0 * 1.0 is exactly zero, where
    // reducing after converting to radians leaves the rounding error in π/2 behind.
    let quadrant = (within_turn / 90.0).floor();
    let in_quadrant = within_turn - 90.0 * quadrant;
    // `& 3` rather than trusting the range: an input that rounds to exactly 360.0 above would give
    // a fourth quadrant, and wrapping is correct where indexing off the end is a panic.
    let quadrant = (quadrant as i64) & 3;

    // Step 3: now, and only now, into radians. One multiply by a constant.
    const RADIANS_PER_DEGREE: f64 = std::f64::consts::PI / 180.0;
    let x = in_quadrant * RADIANS_PER_DEGREE;

    let (sin, cos) = (first_quadrant_sin(x), first_quadrant_cos(x));

    // Step 4: put the signs back. Rotating the (sin, cos) pair a quarter turn at a time.
    let (sin, cos) = match quadrant {
        0 => (sin, cos),
        1 => (cos, -sin),
        2 => (-sin, -cos),
        _ => (-cos, sin),
    };

    // `+ 0.0` turns a negative zero into a positive one, which IEEE 754 specifies exactly. Both are
    // equal to zero and neither is wrong, but they have *different bit patterns* -- and a component
    // holding one goes into the state hash as a different number from a component holding the other.
    // Normalising here means a caller can never be surprised by which they got.
    (sin as f32 + 0.0, cos as f32 + 0.0)
}

/// Sine of an angle in degrees. See [`sin_cos_degrees`].
#[must_use]
pub fn sin_degrees(degrees: f32) -> f32 {
    sin_cos_degrees(degrees).0
}

/// Cosine of an angle in degrees. See [`sin_cos_degrees`].
#[must_use]
pub fn cos_degrees(degrees: f32) -> f32 {
    sin_cos_degrees(degrees).1
}

// --- The polynomials ---
//
// Taylor series about zero, evaluated by Horner's method -- multiply and add, nothing else.
//
// Taylor rather than a minimax fit, deliberately. A minimax polynomial reaches the same accuracy in
// two fewer terms, at the cost of a row of magic constants nobody can check by eye. These
// coefficients are reciprocal factorials: anyone can verify `1 / 5!` is `1 / 120` and nobody can
// verify that `0.008_332_161` was the right output of a Remez solver. The two saved multiplies are
// not worth the opacity, and `CLAUDE.md`'s legibility requirement is explicit that this is the trade
// to make.
//
// The series run to the fifteenth power, which leaves a truncation error near 6e-12 at the far end
// of the quadrant -- roughly twenty thousand times smaller than the `f32` the caller receives can
// represent. Written as divisions rather than decimals because Rust evaluates them at compile time
// under the same IEEE rules, so they cost nothing and stay readable.

/// Sine on `[0, π/2]`.
///
/// Each line folds in one more term of `x - x³/3! + x⁵/5! - …`, working from the smallest term
/// outward. The divisor on each line is the ratio between neighbouring factorials — `5!/3!` is 20,
/// `7!/5!` is 42 — which is what Horner's method leaves behind when the common factors are taken
/// out. Written as separate statements rather than one nested expression because the nested version
/// is seven brackets deep and `cargo fmt` renders it as a staircase.
fn first_quadrant_sin(x: f64) -> f64 {
    let square = x * x;
    let mut series = 1.0 - square / 210.0; // 15!/13!
    series = 1.0 - square / 156.0 * series; // 13!/11!
    series = 1.0 - square / 110.0 * series; // 11!/9!
    series = 1.0 - square / 72.0 * series; // 9!/7!
    series = 1.0 - square / 42.0 * series; // 7!/5!
    series = 1.0 - square / 20.0 * series; // 5!/3!
    series = 1.0 - square / 6.0 * series; // 3!
    x * series
}

/// Cosine on `[0, π/2]`.
///
/// The same shape as [`first_quadrant_sin`], over `1 - x²/2! + x⁴/4! - …`.
fn first_quadrant_cos(x: f64) -> f64 {
    let square = x * x;
    let mut series = 1.0 - square / 182.0; // 14!/12!
    series = 1.0 - square / 132.0 * series; // 12!/10!
    series = 1.0 - square / 90.0 * series; // 10!/8!
    series = 1.0 - square / 56.0 * series; // 8!/6!
    series = 1.0 - square / 30.0 * series; // 6!/4!
    series = 1.0 - square / 12.0 * series; // 4!/2!
    1.0 - square / 2.0 * series // 2!
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::hash::{StableHash, StableHasher};

    /// How close to the standard library is close enough: one `f32` ulp near 1.0.
    const TOLERANCE: f32 = 1.2e-7;

    #[test]
    fn the_cardinal_angles_are_exact() {
        // The property the standard library cannot offer, and the reason a quarter-turn rotation
        // matrix comes out exactly axis-aligned rather than nearly so.
        assert_eq!(sin_cos_degrees(0.0), (0.0, 1.0));
        assert_eq!(sin_cos_degrees(90.0), (1.0, 0.0));
        assert_eq!(sin_cos_degrees(180.0), (0.0, -1.0));
        assert_eq!(sin_cos_degrees(270.0), (-1.0, 0.0));
        assert_eq!(sin_cos_degrees(360.0), (0.0, 1.0));
        assert_eq!(sin_cos_degrees(-90.0), (-1.0, 0.0));
    }

    #[test]
    fn no_result_is_a_negative_zero() {
        // Equal to zero and yet a different bit pattern, so a component holding one hashes
        // differently from a component holding the other. Worth pinning rather than discovering.
        for angle in [0.0, 90.0, 180.0, 270.0, -180.0, 540.0] {
            let (sin, cos) = sin_cos_degrees(angle);
            assert!(sin != 0.0 || sin.is_sign_positive(), "sin({angle}) is -0.0");
            assert!(cos != 0.0 || cos.is_sign_positive(), "cos({angle}) is -0.0");
        }
    }

    #[test]
    fn the_known_angles_come_out_right() {
        // Worked out by hand rather than read off the implementation, which is the only version of
        // this test worth having.
        let (sin, cos) = sin_cos_degrees(30.0);
        assert!((sin - 0.5).abs() < TOLERANCE, "sin 30 = {sin}");
        assert!(
            (cos - 3f32.sqrt() / 2.0).abs() < TOLERANCE,
            "cos 30 = {cos}"
        );

        let (sin, cos) = sin_cos_degrees(45.0);
        assert!(
            (sin - 2f32.sqrt() / 2.0).abs() < TOLERANCE,
            "sin 45 = {sin}"
        );
        assert!(
            (cos - 2f32.sqrt() / 2.0).abs() < TOLERANCE,
            "cos 45 = {cos}"
        );

        let (sin, cos) = sin_cos_degrees(60.0);
        assert!(
            (sin - 3f32.sqrt() / 2.0).abs() < TOLERANCE,
            "sin 60 = {sin}"
        );
        assert!((cos - 0.5).abs() < TOLERANCE, "cos 60 = {cos}");
    }

    #[test]
    fn it_agrees_with_a_high_precision_reference_all_the_way_round() {
        // The reference is the standard library at **`f64`**, which is the true answer to far more
        // digits than an `f32` can hold. Several turns in each direction, at a step that is not a
        // divisor of 90, so the quadrant boundaries are approached from both sides rather than
        // landed on.
        //
        // Two roundings to `f32` can disagree by two ulps in the worst case -- one from this
        // module's result and one from the reference's -- which is where the bound comes from.
        let mut worst = 0.0f32;
        let mut worst_at = 0.0f32;
        let mut angle = -1100.0f32;
        while angle <= 1100.0 {
            let (sin, cos) = sin_cos_degrees(angle);
            let (want_sin, want_cos) = f64::from(angle).to_radians().sin_cos();
            let error = (sin - want_sin as f32)
                .abs()
                .max((cos - want_cos as f32).abs());
            if error > worst {
                worst = error;
                worst_at = angle;
            }
            angle += 0.7;
        }
        assert!(
            worst <= 2.0 * TOLERANCE,
            "worst disagreement with a high-precision reference was {worst} at {worst_at} degrees"
        );
    }

    #[test]
    fn reducing_in_degrees_beats_the_obvious_f32_route_at_large_angles() {
        // **Why the reduction happens in degrees and the conversion to radians happens last**, shown
        // rather than asserted in a comment.
        //
        // `angle.to_radians().sin()` at an `f32` of a thousand degrees has already thrown away the
        // digits that decide the answer: seventeen-odd radians carries its `f32` rounding as about a
        // millionth of a radian of argument error, and the sine of the wrong argument is the wrong
        // sine. Folding onto one turn first means the polynomial never sees a large number.
        //
        // This is not a claim that the standard library is badly implemented. It is a claim about
        // *the route*, and it is the same class of mistake as `SCALE_2D` in `amadeo-noise`: an
        // expression that looks equivalent and is not.
        let angle = -1070.602f32;
        let truth = f64::from(angle).to_radians().sin() as f32;

        let ours = sin_degrees(angle);
        let naive = angle.to_radians().sin();

        assert!(
            (ours - truth).abs() < (naive - truth).abs(),
            "reducing in degrees should be closer to the truth: ours {ours}, naive {naive}, \
             truth {truth}"
        );
    }

    #[test]
    fn the_identity_holds_everywhere() {
        // Independent of the standard library entirely, so it still says something if both
        // implementations were wrong in the same way.
        let mut angle = -400.0f32;
        while angle <= 400.0 {
            let (sin, cos) = sin_cos_degrees(angle);
            let identity = sin * sin + cos * cos;
            assert!(
                (identity - 1.0).abs() < 2.0 * TOLERANCE,
                "sin^2 + cos^2 was {identity} at {angle} degrees"
            );
            angle += 0.37;
        }
    }

    #[test]
    fn a_non_finite_angle_gives_a_non_finite_answer() {
        for angle in [f32::NAN, f32::INFINITY, f32::NEG_INFINITY] {
            let (sin, cos) = sin_cos_degrees(angle);
            assert!(sin.is_nan() && cos.is_nan(), "{angle} gave ({sin}, {cos})");
        }
    }

    #[test]
    fn a_grid_of_angles_hashes_to_a_known_number() {
        // **The test that earns its keep across platforms**, and the reason is on record:
        // `amadeo-noise`'s equivalent caught a constant being replaced by `std::f32::consts::SQRT_2`,
        // which reads as the same number and differs in the last bit. Nothing else in the workspace
        // noticed.
        //
        // A literal, so CI running on Windows *and* Linux is what checks the cross-platform claim
        // this module exists to make. If this fails after a deliberate change to the polynomial,
        // the new number is fine -- but every replay and every pinned state hash moves with it, so
        // it is a decision rather than a rebaseline.
        let mut hasher = StableHasher::new();
        let mut step = 0i32;
        while step < 2000 {
            // Degrees chosen to land on and between quadrant boundaries, and to go negative.
            let angle = (step as f32) * 0.37 - 370.0;
            let (sin, cos) = sin_cos_degrees(angle);
            sin.stable_hash(&mut hasher);
            cos.stable_hash(&mut hasher);
            step += 1;
        }
        assert_eq!(
            hasher.finish(),
            PINNED_GRID_HASH,
            "the deterministic trig moved. Every state hash containing a rotation moves with it."
        );
    }

    /// The hash `a_grid_of_angles_hashes_to_a_known_number` pins. See that test before changing it.
    const PINNED_GRID_HASH: u64 = 903_495_041_332_774_617;
}
