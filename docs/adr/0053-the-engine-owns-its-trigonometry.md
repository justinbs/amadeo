# ADR 0053 — The engine owns its trigonometry

**Status:** Accepted · **Date:** 2026-08-11 · **Builds on:** ADR 0018, ADR 0019, ADR 0044

## Context

ADR 0044 banned `f32::sin`, `f32::cos` and `f32::powf` from anything that decides gameplay state.
The reason is not taste: Rust documents their precision as varying **by platform, by version, and
between two calls in one execution**, because IEEE 754 requires correct rounding only for `+ - * /`
and `sqrt` and lists the transcendentals as merely recommended. A sine in a terrain generator puts
Windows and Linux on different ground, and the resulting bug report says *"the replay does not
reproduce on Linux"* while pointing at physics.

`amadeo-noise` obeyed that ban by **avoiding** trigonometry — gradient noise can be built from `floor`
and integer hashing. That was the right answer there and it is not an answer everywhere.

### The ban had a hole, and something was already through it

Session 15 set out to fix a follow camera that would not orbit. Placing a camera at an angle around
a pivot is exactly what sine and cosine are for; there is no arithmetic dodge. Looking for where to
put the deterministic version turned up the fact that **the problem already existed**:

`modules/amadeo-camera`'s `keep_camera_clear` built a matrix from its parent's rotation with
`Mat4::from_transform`, projected the swept distance onto one of that matrix's axes, and wrote the
result into the camera's `Transform` — which is a hashed component. `Mat4::from_euler_degrees` used
`f32::sin_cos`. So the camera's position, and therefore every state hash containing it, depended on
an operation Rust does not specify.

`crates/amadeo-transform/src/matrix.rs` had **described this exact route in its own header** as the
"side door" that would put matrix arithmetic back into hashed state, and had guarded against the
lesser risk (SIMD) while leaving the greater one open. Nothing caught it, and nothing could have:
the Scarp's cross-thread determinism test compares a machine against itself, and the cross-platform
literal hashes that would have caught it are in physics and noise, neither of which rotates anything.

### Why the obvious workaround is worse

The camera's `Transform` is *derived* — recomputed every tick from its parent, its pitch and a physics
sweep, and read by nothing in the simulation. It is in the hash only because `Transform` is one type
shared by everything that has a position. Excluding it would mean per-entity hash exclusion, where
ADR 0019's `DERIVED` is per **type**; that is a real architectural change, expensive to unpick, and
it buys one caller an exemption rather than fixing the arithmetic.

## Decision

**`amadeo-core` ships `sin_cos_degrees`, and `Mat4::from_euler_degrees` uses it.**

Built from `+ - * /` and `floor`, all of which IEEE 754 pins exactly, so two machines agreeing is a
property of the arithmetic rather than of the libm they happen to link. It lives in `amadeo-core`
for the reason that crate's own header already gives for `Rng` and `StableHasher`: things that look
like they should be dependencies are implemented here when they are determinism-critical.

Three implementation choices, all recorded because each has a plausible alternative:

**Reduce in degrees, convert to radians last.** Folding the angle onto one turn and then onto the
first quadrant is exact, because `floor` is exact and `90.0 - 90.0 × 1.0` is zero on the nose. The
usual route — convert to radians, then reduce — carries the rounding error in π/2 into every answer.

**`f64` internally, `f32` out.** Not for accuracy in the result but for the range reduction: folding
a few thousand degrees in `f32` throws away the digits that decide the answer. `f64`'s `+ - * /` are
specified exactly as `f32`'s are, so working wider costs determinism nothing.

**Taylor, not minimax.** A minimax fit reaches the same accuracy two terms sooner, at the price of a
row of constants nobody can check by eye. Reciprocal factorials can be verified by anyone;
`0.008_332_161` cannot. `CLAUDE.md`'s legibility requirement is explicit that this is the trade to
make, and two multiplies is not a real cost.

### Scope: engine-wide rather than camera-only

Put to Justin as a choice, because it is the expensive half. Camera-only would have been safe by
construction — no rendered pixel could move. Engine-wide means every view, model and light matrix
shifts by a bit or so, which can flip a pixel at a silhouette and put the 23 GPU capture tests at
risk.

He chose engine-wide, and the reason it is the better answer is that the camera was not special.
Any system that reads a `GlobalTransform` and writes the result back into a `Transform` — "place this
child where its parent's hand is" — reopens the hole, and asking every future caller to remember is
the arrangement that just failed. Fixing it at the source means no caller has to know.

**The risk did not materialise.** The full suite passed unchanged on the first run: all 23 capture
tests, the pinned rapier state hash, and every golden replay. Rotations in those fixtures are either
zero or quarter turns, where the two implementations agree exactly — and where the new one is
*exactly* right and the old one was not.

## Consequences

**The cardinal angles become exact.** `sin_cos_degrees(90.0)` is exactly `(1.0, 0.0)`; the standard
library's `90f32.to_radians().cos()` is about `-4.4e-8`, because the `f32` nearest π/2 is not π/2. So
a quarter-turn rotation matrix now has exactly the unit axes where it used to carry dust in the
off-diagonals. This is a free improvement rather than the point, but it is the kind of thing that
turns up later as "why is this box very slightly sheared".

**Negative zero is normalised away.** `-0.0` and `0.0` are equal and have different bit patterns, so a
component holding one hashes differently from a component holding the other. Adding zero to the
result — exactly specified — means a caller cannot be surprised by which they got.

**A literal grid hash is pinned, and CI runs it on both platforms.** This is the mechanism that has
already earned its keep once: `amadeo-noise`'s equivalent caught `SCALE_2D` being replaced with
`std::f32::consts::SQRT_2`, which reads as the same number and differs in the last bit. Changing the
polynomial is therefore a decision rather than a rebaseline, because every replay and every pinned
state hash moves with it.

**It is not a licence to use trigonometry freely in gameplay.** ADR 0044 stands for `powf`, which has
no equivalent here, and for the general principle that a generator should prefer arithmetic that is
specified. What changes is that *placing something at an angle* is now a solved problem rather than a
banned one.

**`sin`/`cos` in `f32` remain available and remain wrong for this purpose**, and nothing stops a
future caller reaching for them. The `amadeo-noise` precedent suggests the eventual answer is a lint;
that is not built, and the honest state is that this is enforced by review and by the header comments
at both sites.

## Alternatives rejected

**Exclude the camera's transform from the state hash.** Needs per-entity exclusion where ADR 0019 has
per-type; a large, hard-to-reverse change that exempts one caller instead of fixing the arithmetic.

**A minimax polynomial from a Remez solver.** Faster, and opaque. See above.

**Depend on a fixed-point or software-float crate.** Solves a problem the engine does not have. The
whole difficulty is confined to the transcendentals; `+ - * /` are already exact, and pulling in a
dependency to re-implement arithmetic that works would be a large change for no gain.

**Store rotations as quaternions so no Euler conversion is needed.** ADR 0018 settled this on
hand-editability grounds and nothing here reopens it — and it would not help, since building a
quaternion from an authored angle needs the same trigonometry.
