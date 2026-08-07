//! Gradient noise that gives the same bits on every machine — ADR 0044.
//!
//! Noise is how a generated world gets shape: a formula turning a position into a value, summed at
//! several scales so the result reads as landscape rather than as arithmetic. [`Noise`] is one
//! scale; [`Fbm`] is the sum of several, which is what you almost always want.
//!
//! ```
//! use amadeo_noise::Fbm;
//!
//! let hills = Fbm {
//!     frequency: 0.02,
//!     octaves: 4,
//!     ..Fbm::new(0x5EED)
//! };
//!
//! // Roughly [-1, 1]. Scale and offset it into whatever a world needs.
//! let height = 24.0 + hills.sample_2d(120.0, -80.0) * 12.0;
//! # let _ = height;
//! ```
//!
//! # Why this crate is hand-written, and what it is not allowed to do
//!
//! **`sin`, `cos` and `powf` are forbidden here.** Not discouraged — forbidden, and the reason is
//! worth reading once because the code below looks strange without it.
//!
//! Rust's standard library says of `f32::sin`, `f32::cos` and `f32::powf`:
//!
//! > The precision of this function is non-deterministic. This means it varies by platform, Rust
//! > version, and can even differ within the same execution from one invocation to the next.
//!
//! Not a caveat about the last decimal place. These are calls into the platform's C maths library,
//! and IEEE 754 requires correct rounding only for `+`, `-`, `*`, `/`, `sqrt` and `fma` —
//! trigonometric and transcendental functions are *recommended*, and two conforming implementations
//! need not agree. `f32::sqrt` carries the opposite note and is guaranteed exact.
//!
//! That matters here more than it would in most engines. ADR 0043 made a terrain chunk's **collider**
//! gameplay state — a character stands on it — and a terrain source built on this crate is what
//! decides where that surface is. A sum of sines, which is the obvious way to write rolling hills,
//! would put Windows and Linux on different ground. The state hash would diverge and the symptom
//! would be *"the replay does not reproduce on Linux"*, pointing at physics, at the scheduler, at
//! the job pool — at everything except a trigonometric function inside a terrain generator, because
//! trigonometry is not where anyone expects nondeterminism to live.
//!
//! So everything below is built from multiplication, addition, subtraction, division, comparison and
//! `floor`, over gradients chosen by **integer** hashing. Every one of those is exactly specified, so
//! determinism here is structural rather than tested-for — the same move ADR 0041 made for threads.
//!
//! `mul_add` is absent too, and that is not an oversight: it is correctly rounded and it is *a
//! different number* from `a * b + c`, because it rounds once instead of twice. Writing the multiply
//! and the add separately, always, keeps one answer.
//!
//! # The test that makes this a claim rather than a hope
//!
//! `a_grid_of_samples_hashes_to_a_known_number` asserts a **literal** hash of 512 samples, which CI
//! evaluates on Windows and on Linux. A test that only checked two calls in one process agreeing
//! would pass on a machine where every value was wrong — which is exactly the weakness session 12
//! found in the streamer's own exit-gate test. If a change moves that number, CI goes red on the
//! commit that moved it.
//!
//! # What this crate deliberately does not have
//!
//! No opinion about worlds. It does not know what terrain is, which way is up, or what a chunk is —
//! it turns coordinates into numbers. A `TerrainSource` composing those numbers into a landscape
//! lives in the game that wants that landscape, per ADR 0044 §2.

#![forbid(unsafe_code)]

/// One octave of gradient noise, seeded.
///
/// Cheap to copy and cheap to make: the seed is the whole of its state, so there is no permutation
/// table to build and nothing to keep in sync. Two `Noise` values with the same seed are the same
/// function, on any machine.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub struct Noise {
    seed: u64,
}

/// Gradients for 3D, the twelve edge midpoints of a cube.
///
/// Perlin's improved set. Every component is `0` or `±1`, so a dot product against one is a couple
/// of additions rather than three multiplications — and, more to the point here, involves no
/// constant that could be rounded differently anywhere.
const GRADIENTS_3D: [[f32; 3]; 12] = [
    [1.0, 1.0, 0.0],
    [-1.0, 1.0, 0.0],
    [1.0, -1.0, 0.0],
    [-1.0, -1.0, 0.0],
    [1.0, 0.0, 1.0],
    [-1.0, 0.0, 1.0],
    [1.0, 0.0, -1.0],
    [-1.0, 0.0, -1.0],
    [0.0, 1.0, 1.0],
    [0.0, -1.0, 1.0],
    [0.0, 1.0, -1.0],
    [0.0, -1.0, -1.0],
];

/// Gradients for 2D: the four axes and the four diagonals.
const GRADIENTS_2D: [[f32; 2]; 8] = [
    [1.0, 0.0],
    [-1.0, 0.0],
    [0.0, 1.0],
    [0.0, -1.0],
    [1.0, 1.0],
    [-1.0, 1.0],
    [1.0, -1.0],
    [-1.0, -1.0],
];

/// Scales raw 3D output to fill `[-1, 1]`.
///
/// Gradient noise built on the twelve edge vectors reaches at most `√3 / 2` in magnitude, so this is
/// `2 / √3`. Written as a literal rather than computed: an `f32` literal is the same bits on every
/// machine, and it keeps `sqrt` out of the hot path for a value that never changes.
const SCALE_3D: f32 = 1.154_700_5;

/// Scales raw 2D output to fill `[-1, 1]`. The 2D gradients reach `√2 / 2`, so this is `√2`.
///
/// From `std` rather than written out, which clippy asks for and which is better anyway: it says
/// what the number *is* instead of what it looks like. Still a compile-time constant, so it costs
/// nothing and is the same bits everywhere.
const SCALE_2D: f32 = std::f32::consts::SQRT_2;

impl Noise {
    /// Noise for a seed. Different seeds are different worlds; the same seed is the same world.
    #[must_use]
    pub fn new(seed: u64) -> Self {
        Self { seed }
    }

    /// The seed this was built with.
    #[must_use]
    pub fn seed(&self) -> u64 {
        self.seed
    }

    /// Noise at a point on a plane, in `[-1, 1]`.
    ///
    /// The 2D one is not the 3D one with a zero — it uses its own gradient set, so a heightmap costs
    /// four corners rather than eight. `CLAUDE.md` trap 9: a 2D game gets the real thing, not the 3D
    /// path with an axis wasted.
    #[must_use]
    pub fn sample_2d(&self, x: f32, y: f32) -> f32 {
        // The lattice cell this point is in, and where it sits inside that cell.
        let (x0, fx) = split(x);
        let (y0, fy) = split(y);
        let (ux, uy) = (fade(fx), fade(fy));

        // Each corner contributes its gradient dotted with the offset from that corner to the point.
        let corner = |ix: i32, iy: i32| {
            let gradient = GRADIENTS_2D[(self.hash(ix, iy, 0) % 8) as usize];
            gradient[0] * (fx - (ix - x0) as f32) + gradient[1] * (fy - (iy - y0) as f32)
        };

        let bottom = lerp(corner(x0, y0), corner(x0 + 1, y0), ux);
        let top = lerp(corner(x0, y0 + 1), corner(x0 + 1, y0 + 1), ux);
        lerp(bottom, top, uy) * SCALE_2D
    }

    /// Noise at a point in space, in `[-1, 1]`.
    ///
    /// The 3D one is what caves and overhangs need: a 2D heightmap can only produce a surface with
    /// one height per column, so nothing can ever be above anything else.
    #[must_use]
    pub fn sample_3d(&self, x: f32, y: f32, z: f32) -> f32 {
        let (x0, fx) = split(x);
        let (y0, fy) = split(y);
        let (z0, fz) = split(z);
        let (ux, uy, uz) = (fade(fx), fade(fy), fade(fz));

        let corner = |ix: i32, iy: i32, iz: i32| {
            let gradient = GRADIENTS_3D[(self.hash(ix, iy, iz) % 12) as usize];
            gradient[0] * (fx - (ix - x0) as f32)
                + gradient[1] * (fy - (iy - y0) as f32)
                + gradient[2] * (fz - (iz - z0) as f32)
        };

        // Interpolate along x, then y, then z: eight corners collapsing to one value.
        let edge = |iy: i32, iz: i32| lerp(corner(x0, iy, iz), corner(x0 + 1, iy, iz), ux);
        let face = |iz: i32| lerp(edge(y0, iz), edge(y0 + 1, iz), uy);
        lerp(face(z0), face(z0 + 1), uz) * SCALE_3D
    }

    /// Which gradient a lattice corner gets.
    ///
    /// **Integer arithmetic all the way down**, which is the point: wrapping multiplication and
    /// shifts are exactly specified, so two machines pick the same gradient by construction rather
    /// than by luck. The mixing step is MurmurHash3's finaliser, chosen because it is well studied
    /// and short enough to read.
    fn hash(&self, x: i32, y: i32, z: i32) -> u64 {
        // Odd constants with well-spread bits, so neighbouring lattice points do not collide. The
        // first is the golden ratio in 64 bits, which is the usual choice for exactly this job.
        let mut value = self.seed;
        value = value.wrapping_add((i64::from(x) as u64).wrapping_mul(0x9E37_79B9_7F4A_7C15));
        value = value.wrapping_add((i64::from(y) as u64).wrapping_mul(0xBF58_476D_1CE4_E5B9));
        value = value.wrapping_add((i64::from(z) as u64).wrapping_mul(0x94D0_49BB_1331_11EB));

        value ^= value >> 33;
        value = value.wrapping_mul(0xFF51_AFD7_ED55_8CCD);
        value ^= value >> 33;
        value = value.wrapping_mul(0xC4CE_B9FE_1A85_EC53);
        value ^ (value >> 33)
    }
}

/// Several octaves of [`Noise`] summed — "fractional Brownian motion", which is the name the
/// literature uses for the oldest trick in procedural terrain.
///
/// One octave of gradient noise is smooth, rolling and boring. Adding a second at twice the
/// frequency and half the height puts bumps on the hills; a third puts bumps on the bumps. Four is
/// usually enough for ground, and the cost is linear in the count.
///
/// # Reading the fields
///
/// ```text
///   frequency   how zoomed out the first octave is. SMALLER means BIGGER features -- 0.01 gives
///               hills about a hundred units across, 0.1 gives lumps about ten across.
///   octaves     how many layers. Each is finer and fainter than the last.
///   lacunarity  how much finer each layer is than the one before. 2.0 means "twice the detail".
///   gain        how much fainter. 0.5 means "half the height". Above 0.5 gets noisy fast.
/// ```
///
/// The result is divided by the total of all the amplitudes, so it stays in `[-1, 1]` and **adding
/// an octave does not change the overall scale of the world** — only its detail. That is worth
/// having: it means tuning detail cannot silently move the ground.
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct Fbm {
    /// The noise being layered.
    pub noise: Noise,
    /// How many layers. Zero produces a flat zero, which is a legitimate thing to ask for.
    pub octaves: u32,
    /// The first layer's frequency. Smaller means larger features.
    pub frequency: f32,
    /// Frequency multiplier per layer. `2.0` is the conventional choice.
    pub lacunarity: f32,
    /// Amplitude multiplier per layer. `0.5` is the conventional choice.
    pub gain: f32,
}

impl Fbm {
    /// Four octaves of gently rolling noise for a seed.
    ///
    /// Defaults that produce something recognisably landscape-shaped, so a caller changing one field
    /// with struct-update syntax gets a sensible world rather than having to know all five.
    #[must_use]
    pub fn new(seed: u64) -> Self {
        Self {
            noise: Noise::new(seed),
            octaves: 4,
            frequency: 0.01,
            lacunarity: 2.0,
            gain: 0.5,
        }
    }

    /// Layered noise at a point on a plane, in `[-1, 1]`.
    #[must_use]
    pub fn sample_2d(&self, x: f32, y: f32) -> f32 {
        let mut total = 0.0;
        let mut amplitude = 1.0;
        let mut frequency = self.frequency;
        let mut normaliser = 0.0;

        for _ in 0..self.octaves {
            total += self.noise.sample_2d(x * frequency, y * frequency) * amplitude;
            normaliser += amplitude;
            amplitude *= self.gain;
            frequency *= self.lacunarity;
        }
        normalise(total, normaliser)
    }

    /// Layered noise at a point in space, in `[-1, 1]`.
    #[must_use]
    pub fn sample_3d(&self, x: f32, y: f32, z: f32) -> f32 {
        let mut total = 0.0;
        let mut amplitude = 1.0;
        let mut frequency = self.frequency;
        let mut normaliser = 0.0;

        for _ in 0..self.octaves {
            total += self
                .noise
                .sample_3d(x * frequency, y * frequency, z * frequency)
                * amplitude;
            normaliser += amplitude;
            amplitude *= self.gain;
            frequency *= self.lacunarity;
        }
        normalise(total, normaliser)
    }
}

/// Divides by the summed amplitude, guarding the zero-octave case.
///
/// A guard rather than a `debug_assert`: zero octaves is a legitimate request — "give me flat" — and
/// it must not produce a `NaN` that then propagates silently into a collider.
fn normalise(total: f32, normaliser: f32) -> f32 {
    if normaliser == 0.0 {
        return 0.0;
    }
    total / normaliser
}

/// Splits a coordinate into its lattice cell and the position within it.
///
/// `floor` is an IEEE 754 integral-rounding operation and is exactly specified, so this is one of
/// the permitted operations under ADR 0044 §1 — the same reason `ChunkKey::containing` is allowed to
/// use it in residency, which is also gameplay state.
fn split(value: f32) -> (i32, f32) {
    let base = value.floor();
    // A saturating cast, which is Rust's defined behaviour for `as` between floats and integers. A
    // coordinate beyond `i32` is a world far larger than anything this engine can hold, and clamping
    // repeats the edge cell rather than wrapping into a different one.
    (base as i32, value - base)
}

/// Perlin's fade curve, `6t⁵ − 15t⁴ + 10t³`.
///
/// Smooths the interpolation so the lattice does not show. Its first *and second* derivatives are
/// zero at both ends, which is what a plain smoothstep gets wrong — with smoothstep the surface
/// normals change abruptly at cell boundaries and lit terrain shows a visible grid.
///
/// Written in Horner form: five multiplications and two additions, and nothing else.
fn fade(t: f32) -> f32 {
    t * t * t * (t * (t * 6.0 - 15.0) + 10.0)
}

/// Linear interpolation. `a + t * (b - a)` rather than `a * (1 - t) + b * t` — one multiplication
/// fewer, and it returns exactly `a` at `t = 0`.
fn lerp(a: f32, b: f32, t: f32) -> f32 {
    a + t * (b - a)
}

#[cfg(test)]
mod tests {
    use super::*;

    /// FNV-1a over the raw bits of a sequence of samples.
    ///
    /// Written here rather than borrowed from `amadeo-core`'s `StableHasher` because this crate has
    /// no dependencies (ADR 0044 §2) — and because hashing the *bits* is the point: two values that
    /// differ in the last place must hash differently, which is exactly what a tolerance-based
    /// comparison would forgive.
    fn hash_bits(values: &[f32]) -> u64 {
        let mut hash: u64 = 0xCBF2_9CE4_8422_2325;
        for value in values {
            for byte in value.to_bits().to_le_bytes() {
                hash ^= u64::from(byte);
                hash = hash.wrapping_mul(0x0000_0100_0000_01B3);
            }
        }
        hash
    }

    #[test]
    fn a_grid_of_samples_hashes_to_a_known_number() {
        // **The claim this crate exists to make, pinned to a literal** -- the same move
        // `tests/rapier_determinism.rs` makes for physics, and for the same reason: CI runs this on
        // Windows *and* Linux, so a platform disagreement turns CI red rather than surfacing three
        // milestones later as "the replay does not reproduce".
        //
        // A test asserting only that two calls agree in one process would pass on a machine where
        // every value was wrong. That is the weakness session 12 found in the streamer's own
        // exit-gate test, and this is the shape that does not have it.
        //
        // If this number moves, the noise changed, and so did every world generated from it. That is
        // a deliberate, reviewable event -- see ADR 0044's consequences.
        //
        // **It has already earned its keep once.** `SCALE_2D` was first written out as the literal
        // `1.414_213_6` and then replaced with `std::f32::consts::SQRT_2`, which reads better and
        // looks like the same number. It is not: the two differ in the last bit, every 2D sample
        // moved with it, and this assertion was the only thing in the workspace that noticed. That
        // is the size of change a terrain collider is built on.
        let field = Fbm::new(0x5EED);
        let mut samples = Vec::new();
        for i in 0..8 {
            for j in 0..8 {
                for k in 0..8 {
                    let (x, y, z) = (i as f32 * 3.5, j as f32 * 3.5, k as f32 * 3.5);
                    samples.push(field.sample_3d(x, y, z));
                }
            }
        }
        for i in 0..8 {
            for j in 0..8 {
                samples.push(field.sample_2d(i as f32 * 7.25, j as f32 * 7.25));
            }
        }

        assert_eq!(samples.len(), 576);
        assert_eq!(
            hash_bits(&samples),
            0x099B_3808_3FA2_C821,
            "the noise has changed, and so has every world generated from it"
        );
    }

    #[test]
    fn the_same_point_always_gives_the_same_value() {
        // I3 at its smallest. Necessary and, as the test above says, nowhere near sufficient.
        let field = Fbm::new(42);
        for point in [[0.0, 0.0, 0.0], [13.5, -7.25, 91.0], [-1000.0, 4.0, 0.5]] {
            assert_eq!(
                field.sample_3d(point[0], point[1], point[2]),
                field.sample_3d(point[0], point[1], point[2])
            );
        }
    }

    #[test]
    fn different_seeds_give_different_worlds() {
        // A seed that did not reach the output would make every world identical -- which looks fine
        // in a screenshot of one world and is discovered only when somebody asks for a second.
        let a = Fbm::new(1);
        let b = Fbm::new(2);
        let differences = (0..64)
            .filter(|i| {
                let x = *i as f32 * 6.5;
                a.sample_2d(x, x) != b.sample_2d(x, x)
            })
            .count();
        assert!(
            differences > 60,
            "only {differences} of 64 samples differ between seeds"
        );
    }

    #[test]
    fn values_stay_inside_the_declared_range() {
        // The doc comment says [-1, 1]. A caller scaling this into a world height will produce
        // terrain outside the chunk it was generated for if that is a lie.
        let field = Fbm::new(0xABCD);
        for i in 0..40 {
            for j in 0..40 {
                let (x, z) = (i as f32 * 2.75 - 55.0, j as f32 * 2.75 - 55.0);
                let value = field.sample_2d(x, z);
                assert!(
                    (-1.0..=1.0).contains(&value),
                    "sample_2d({x}, {z}) = {value} is outside [-1, 1]"
                );
                let value = field.sample_3d(x, 3.0, z);
                assert!(
                    (-1.0..=1.0).contains(&value),
                    "sample_3d({x}, 3, {z}) = {value} is outside [-1, 1]"
                );
            }
        }
    }

    #[test]
    fn the_surface_is_continuous_rather_than_stepped() {
        // What separates gradient noise from a random number per lattice point. Terrain built on
        // something discontinuous is terrain a character catches on, and the symptom reads as a
        // physics bug.
        let field = Fbm::new(7);
        let mut previous = field.sample_2d(0.0, 0.0);
        for step in 1..2000 {
            let value = field.sample_2d(step as f32 * 0.05, 0.0);
            assert!(
                (value - previous).abs() < 0.05,
                "jump of {} at step {step}",
                (value - previous).abs()
            );
            previous = value;
        }
    }

    #[test]
    fn one_octave_is_smoother_than_five() {
        // That `octaves` does what its documentation claims, measured rather than asserted.
        //
        // # The obvious measurement is the wrong one, which this test found the hard way
        //
        // Summing `|f(x + step) - f(x)|` over a long sweep -- total variation -- says five octaves
        // are *smoother* than one. That is not a bug, it is the amplitude normalisation working as
        // documented: the divisor keeps the overall range at [-1, 1], so adding octaves moves energy
        // from the base shape into the detail rather than adding any. The big smooth swing shrinks by
        // more than the fine wiggles add.
        //
        // Detail is high-frequency content, so measure that directly: the **second difference**, how
        // much the slope changes from step to step. A smooth base contributes almost none of it and
        // fine octaves contribute nearly all of it.
        //
        // A tight threshold is safe here in a way it would not be in a timing test. Everything this
        // crate does is exactly-specified arithmetic (ADR 0044 §1), so both numbers are identical on
        // every machine -- there is no runner variance to leave headroom for.
        let detail = |octaves: u32| {
            let field = Fbm {
                octaves,
                frequency: 0.01,
                ..Fbm::new(3)
            };
            let at = |i: i32| field.sample_2d(i as f32 * 0.5, 0.0);
            (1..500)
                .map(|i| (at(i + 1) - 2.0 * at(i) + at(i - 1)).abs())
                .sum::<f32>()
        };
        assert!(
            detail(5) > detail(1) * 4.0,
            "five octaves ({}) should carry far more fine detail than one ({})",
            detail(5),
            detail(1)
        );
    }

    #[test]
    fn zero_octaves_is_flat_rather_than_not_a_number() {
        // The division guard. A NaN here would flow into a signed-distance field, out of the mesher
        // as a NaN vertex, and into a collider -- where rapier's failure is a long way from its
        // cause.
        let field = Fbm {
            octaves: 0,
            ..Fbm::new(1)
        };
        assert_eq!(field.sample_2d(3.0, 4.0), 0.0);
        assert_eq!(field.sample_3d(3.0, 4.0, 5.0), 0.0);
    }

    #[test]
    fn noise_is_zero_at_lattice_points() {
        // A defining property of gradient noise, and a genuine sanity check on the implementation:
        // at an integer point every offset is zero, so every dot product is zero. Getting the corner
        // offsets wrong is the most common way to write this incorrectly, and it shows up here and
        // essentially nowhere else.
        let noise = Noise::new(99);
        for point in [[0, 0, 0], [3, -2, 7], [-11, 5, 0]] {
            let value = noise.sample_3d(point[0] as f32, point[1] as f32, point[2] as f32);
            assert!(value.abs() < 1e-6, "{point:?} gave {value}");
        }
    }

    #[test]
    fn a_lattice_corner_gets_a_stable_gradient() {
        // The hash is the only integer-domain step, and it is what makes the whole thing
        // reproducible. Two seeds apart must not collide on a whole region.
        let a = Noise::new(0);
        let b = Noise::new(1);
        let collisions = (0..100)
            .filter(|i| a.hash(*i, 0, 0) % 12 == b.hash(*i, 0, 0) % 12)
            .count();
        assert!(
            collisions < 25,
            "{collisions} of 100 corners chose the same gradient for two seeds"
        );
    }
}
