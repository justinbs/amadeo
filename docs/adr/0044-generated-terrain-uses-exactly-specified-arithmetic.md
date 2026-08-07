# ADR 0044 — Generated terrain is built from exactly-specified arithmetic, in a crate of its own

**Status:** Accepted · **Date:** 2026-08-07 · **Builds on:** ADR 0036, ADR 0041, ADR 0042, ADR 0043

## Context

M2.5's exit gate 1 needs "a generated terrain world you can walk around". Everything under it is
built — residency, the source/edit data model, per-chunk meshing, static colliders, the streamer and
the ECS layer — and the only [`TerrainSource`] the engine ships is `FlatGround`, a plane. A world you
can walk around needs shape, and shape means **noise**: a formula turning a position into a value,
composed at several scales so the result looks like landscape rather than like arithmetic.

Writing one is a hundred lines. Deciding what it is allowed to be made of turned out to be the
decision.

## What the research found, and it changes the constraint

**Rust's standard library documents `f32::sin`, `f32::cos` and `f32::powf` as non-deterministic.**
Verbatim, from `doc.rust-lang.org/std/primitive.f32.html`:

> Unspecified precision. The precision of this function is non-deterministic. This means it varies by
> platform, Rust version, and can even differ within the same execution from one invocation to the
> next.

That last clause is the alarming one. It is not merely that Windows and Linux may disagree — the same
binary is permitted to answer differently twice, because these are FFI calls into the platform's C
math library and an implementation may dispatch on CPU features or use a vectorised path for some
call sites and not others.

**`f32::sqrt`, by contrast, carries the opposite note**, and it is a guarantee rather than an
observation:

> The result of this operation is guaranteed to be the rounded infinite-precision result. It is
> specified by IEEE 754 as `squareRoot` and guaranteed not to change.

This is IEEE 754's own division of the world. The standard **requires** correct rounding for `+`,
`-`, `*`, `/`, `sqrt` and `fma`, and lists transcendental and trigonometric functions as
*recommended* — a conforming implementation need not provide them at all, and two that do are not
required to agree. It is the same seam ADR 0036 hit from the other side when it pinned rapier's
version exactly and turned `enhanced-determinism` on permanently.

## Why this is an invariant question rather than a numerical-quality one

A `TerrainSource` is not decoration. **ADR 0043 made a chunk's collider gameplay state** — a
character stands on it, so where the surface is decides where the character ends up — and
`TerrainSource::sample` is the function that decides where the surface is.

So a source built on `sin` puts two machines on different ground. The state hash would diverge, the
cross-platform determinism job would go red, and the reported symptom would be *"the replay does not
reproduce on Linux"* — pointing at physics, at the scheduler, at the job pool, at everything except a
trigonometric function inside a terrain generator, which nobody would think to suspect because
trigonometry is not where nondeterminism is expected to live.

That is precisely invariant **I3**, and the trap is that it has **no symptom on one machine**. Every
test in this repo would pass. `CLAUDE.md` trap 2 lists the known nondeterminism leaks — `HashMap`
iteration, `Instant::now()`, unsorted parallel writes, uninitialised floats. This is a fifth one, and
it is the least visible of them, because the offending code looks like ordinary mathematics.

## Decision

### 1. A `TerrainSource` may use only exactly-specified floating-point operations

Permitted: `+`, `-`, `*`, `/`, `sqrt`, comparison, `floor`/`ceil`/`trunc`/`round` (all IEEE 754
integral-rounding operations), `abs`, `min`/`max`, and integer arithmetic of every kind.

Forbidden: `sin`, `cos`, `tan`, `exp`, `ln`, `powf`, `hypot`, `atan2`, and every other transcendental
— along with `powi`, whose lowering is a compiler decision rather than a specified operation.

`mul_add` is permitted but not used: it is correctly rounded, and it is *a different number* from
`a * b + c` because it rounds once instead of twice. Mixing the two forms for the same expression on
different platforms — which is exactly what a compiler is allowed to do when it contracts a multiply
and an add — is a divergence. Writing the multiplication and the addition separately, and never
`mul_add`, keeps one answer.

This is a real constraint and not a theoretical one: it rules out the obvious way to write rolling
hills, which is a sum of sines.

### 2. The generator is `amadeo-noise`, a crate with no dependencies

Alongside `amadeo-jobs`, `amadeo-voxel`, `amadeo-image` and `amadeo-gltf` at the bottom of the graph.

**Its own crate rather than a module in `amadeo-voxel`**, for one reason that decided it: noise is not
three-dimensional. Terraria, RimWorld and Project Zomboid are on the target list, and a 2D game
wanting a heightmap should not depend on a surface-nets mesher to get one. `CLAUDE.md` trap 9 names
letting 2D become second-class, and a dependency edge is exactly how that happens quietly.

**Engine rather than game**, which is the part that was genuinely open. The argument that settled it
is not reuse — it is *where the test lives*. What this crate ships is not "a hill formula", it is a
guarantee: **the same coordinate gives the same bits on every machine.** A guarantee needs a test
somewhere CI runs it on both platforms, and the CI determinism job covers engine crates. In a game,
the property would be untested, invisible, and would rot the first time somebody reached for `sin`
because it read more naturally.

So: `noise.sample_3d(x, y, z)` is engine. *This world has ridges at that amplitude and caves above
that threshold* is content, and lives in the game that wants it.

### 3. The claim is pinned to a literal number, on both platforms

`amadeo-noise` asserts a **literal hash of a grid of samples**, the way
`tests/rapier_determinism.rs` pins a literal state hash. A test asserting only that two calls in one
process agree would pass on a machine where every value was wrong — it is the same "watch it fail"
gap session 12 found in `the_thread_count_cannot_reach_the_colliders`, where the test named after the
exit gate did not fail when the implementation was broken on purpose.

A number in the source that CI evaluates on Windows *and* Linux is the only version of this claim
that is evidence. If a future change moves it, CI goes red on the commit that moved it, with the
divergence attributed rather than discovered three milestones later inside a replay.

### 4. Gradient noise, hashed from integers

The lattice gradient at a corner comes from an **integer hash** of its coordinates and the seed —
wrapping integer arithmetic, exactly specified — indexed into a fixed table of gradient vectors. The
interpolation is Perlin's fade polynomial, `6t⁵ − 15t⁴ + 10t³`, which is multiplications and
additions and nothing else.

Every step is therefore in §1's permitted set by construction rather than by review. Gradient noise
rather than value noise because value noise is visibly blobby and axis-aligned, and the cost
difference is a table lookup.

## Consequences

**Good.**

- A generated world is part of the deterministic core rather than an exception to it. Two machines
  agree about the ground, and a terrain replay is worth the same as any other.
- The rule is checkable by reading: a `TerrainSource` containing `sin` is wrong on sight, with no
  measurement needed.
- 2D gets noise without a voxel dependency, so trap 9 stays shut.
- The seed reaches the world's shape, so ADR 0042's "a save file is a seed plus a diff" is literally
  true of the terrain a game ships.

**Bad, and accepted.**

- Hand-written noise is slower than a tuned crate and slower than a sum of sines. Meshing already
  happens on the job pool (ADR 0041) where it is a pure speedup, so this buys frame-time headroom to
  spend. If it ever stops being enough, the fix is a cheaper *exact* formulation, not a transcendental.
- The engine now ships a generator with taste in it — gradient noise, this fade curve, this gradient
  table. Those are visible choices where "call the platform's sine" would have felt like none. Stated
  plainly rather than hidden: a different noise is a different world, and changing the algorithm
  changes every generated world that exists. It is versioned by the literal test in §3.
- The ban is enforced by review and by that test, not by the compiler. A game could write its own
  `TerrainSource` with a `sin` in it and nothing would stop it. The mitigation is that the trait's
  documentation now says so at the point of implementation, and `docs/07` carries the worked reason.

**Deliberately not decided here.** What the *demo's* world looks like — how many octaves, how tall,
whether it has caves — is content, and changing it costs a constant in a game crate. And **Q25** (LOD
across chunks) is untouched: a coarser chunk samples the same source at a wider spacing, which this
supports already.
