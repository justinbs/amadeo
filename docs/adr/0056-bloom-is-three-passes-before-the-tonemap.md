# ADR 0056 — Bloom is three passes, and it happens before the tonemap

**Status:** Accepted · **Date:** 2026-08-12 · **Builds on:** ADR 0034

## Context

`Environment::Bloom` has existed since ADR 0034 with two authored fields, a schema, reflection, and a
line in every `.environment` file this repo ships. **Nothing read it.** A scene could write

```
bloom
  intensity 0.8
  threshold 1.0
```

and get exactly nothing, with no error, no warning, and a `describe` that reported the field as
present and meaningful.

That is the same defect shape as the asset that failed to build in silence (**Q32**) and the sidecar
that forgets `color_space` (**Q31**): the file format promises something the engine does not deliver,
and the only way to find out is to look at a picture and wonder. ADR 0034 named it as deferred rather
than dropped; this is the delivery.

## Decision

**Three graph passes — bright, blur x, blur y — at half resolution, composited by the post pass
between exposure and tonemapping.**

### It is passes, not lines in `post.wgsl`

Every other effect in the chain is arithmetic on one pixel. Bloom is the only one that needs a
pixel's **neighbours**, over a radius wide enough to read as a glow, which no amount of work at one
pixel can produce. `post.wgsl` already said so in a comment; this makes it true.

Separable, so two 1D blurs cost `2n` samples where one 2D blur of the same radius costs `n²`.

### Three transients, not two ping-ponged

The obvious arrangement blurs A into B and then B back into A. That makes the resource's readers and
writers circular, and the graph derives pass order from exactly that relationship — so it would
either refuse to resolve or resolve wrongly. Three names cost nothing, because the transient pool is
already free to hand two of them one physical texture once their lifetimes are known.

### Added before the tonemap, and that is the whole reason for the HDR target

A glow added *after* tonemapping sits on top of an already-compressed picture and reads as a grey
wash. Added before it, the glow is **light**: the curve compresses it along with everything else, and
a bright glow blows out the way a bright thing does. This is what ADR 0034's high-dynamic-range scene
target was introduced for, and until now nothing exercised it.

The bright pass applies exposure for the same reason — "bright" has to mean bright *after* exposure,
or a scene that dialled exposure down would still bloom.

### Half resolution, and the radius that follows from it

A glow has no detail to lose, so halving each axis quarters the work and the bilinear filter on the
way back up is free smoothing. Nine taps then reach four half-res texels — **eight full-resolution
pixels**. That is a tight bright halo rather than a broad haze, and it is the honest ceiling of one
blur at one resolution.

**Widening it is not more taps.** A nine-tap Gaussian stretched over thirty pixels samples a smooth
function far too sparsely and bands. The way to widen is a **downsample chain**: blur into a half,
then a quarter, then an eighth, adding back on the way up, which grows the radius geometrically as
the cost shrinks. That is the next step and it is a change to these three passes alone — nothing
above `bloom.wgsl` knows how the glow was made.

### A black placeholder, not a second pipeline

When bloom is off, the post pass binds a 1×1 black texture at the glow slot. **Black is the identity
of an addition**, exactly as white is of a multiply for an untextured material — so one post pipeline
serves both, rather than a bloomed one and a plain one that can drift apart.

## Consequences

**Off by default and byte-identical when off.** `intensity` defaults to zero, the graph declares no
bloom transients and no bloom passes, and `bloom_off_is_byte_identical_to_before_it_existed` pins
that as bytes rather than as "close" — because "close" is also what an accidental extra full-screen
pass would be. The Scarp's capture is unchanged, which was checked rather than assumed.

**A fourth shader now shares a hand-written uniform declaration**, so `post_uniform.wgsl` exists for
the reason `view.wgsl` does one ADR earlier: `post.wgsl` and `bloom.wgsl` read the same buffer at the
same binding, and two copies of one layout drift. The Rust `GpuPost` remains the copy that cannot be
shared this way.

**The Scarp does not use it, deliberately.** Its daylight scene has nothing above the threshold after
exposure, so bloom at a sensible threshold changes not one byte — and at a threshold low enough to
catch something, it washes the whole picture out. Turning on an effect that either does nothing or
makes the picture worse is not an improvement, so the file keeps `intensity 0.0`.

**What it is actually for is M3's exit gate**: a dark corridor with a moving flashlight, where a
handful of genuinely bright sources sit in a mostly dark frame. That is the case bloom exists to
serve and the case this engine does not have a scene for yet.

## Alternatives rejected

**Compositing by writing back into the scene target with additive blending**, so `post.wgsl` needs no
second input. It would make the scene target both read and written by passes that must be ordered
against each other, which is the same circularity the three-transient decision avoids.

**Doing the bright pass at full resolution.** Costs four times as much to produce an image that is
about to be blurred into a glow with no detail in it.

**A luminance-weighted threshold** rather than the brightest channel. A saturated red light would
bloom about a fifth as much as a white one of the same intensity, which is wrong for anything
stylised and wrong for a warning light.
