# ADR 0055 — Shadow cascades are concentric layers of one texture array

**Status:** Accepted · **Date:** 2026-08-11 · **Builds on:** ADR 0034, ADR 0038, ADR 0051 ·
**Completes:** ADR 0045's tier 1

## Context

ADR 0038 shipped one shadow map and **reserved the variant this one fills**: `ShadowMode` was written
as an enum precisely so that cascades would arrive as a new spelling rather than as a change to every
scene with a sun in it.

The limitation it named is arithmetic. `games/scarp` fits one 2048² map over a 70 m box, so a
shadow-map texel covers about **7 cm** of ground and edges are visibly blocky. Splitting the camera's
range into rings and giving each its own map puts the resolution where it is looked at.

The split scheme and the per-cascade fitting landed first, separately and deliberately (`7de3242`):
they are the half with real arithmetic in them and are checkable with no GPU, which is invariant I7
paying for itself. **No `Cascaded` variant was added at that point**, because a mode the backend
cannot honour is a half-feature that looks whole — a scene could ask for cascades, get one map, and
read as working.

## Decision

**Four cascades, concentric around the camera, drawn into four layers of one depth texture array.**

Three things were settled with the fitting and are restated here because they are what the rest
follows from: **four, fixed** rather than authored; **concentric boxes** rather than fitted to slices
of the view frustum; and the **practical split scheme** — interpolate between even-by-distance and
even-by-ratio — with the weight authored.

What this ADR adds is the GPU half, and four decisions inside it.

### The blend is a payload on the variant, not a field on the light

```
shadows Cascaded
  blend 0.5
```

ADR 0032 already lets a scene file carry an enum payload, so this needs no format change. And it
**sidesteps Q32**: a light that does not opt into cascades does not change at all, so no existing
`.scene` is invalidated. That is the fourth time a field added to a component would have rewritten
every file spelling it out, and the first time the shape of the change avoided it.

### The shadow map is always an array, even with one layer

`Orthogonal` gets a one-layer array rather than a plain 2D texture. That is what keeps the mesh
shader to **one** declaration and the backend to **one** pipeline: `texture_depth_2d` and
`texture_depth_2d_array` are different binding types, so supporting both would mean two shaders that
can drift apart. Same argument as the 1×1 white placeholder an untextured material binds.

The layer count lives **inside** `TargetFormat::ShadowMap32`, which makes the transient pool do the
right thing for free — it already matches on the whole format, so a one-layer and a four-layer map can
never be handed the same physical texture.

### One pass per cascade

A wgpu render pass attaches exactly one texture view, and a layer of an array is its own view. So
four layers is four passes by construction rather than by choice. It also means the profiler reports
each cascade separately, which is what made the cost measurable — see `docs/10`.

### The bias is per cascade, and this is the trap

A bias is expressed in the light's **clip** depth, which spans that cascade's own box. A near cascade
covering ten metres and a far one covering seventy turn the same authored world-unit offset into very
different clip-space numbers. Sharing one bias means choosing which end to break: too little for the
far cascades, which stipple themselves dark with acne, or too much for the near one, which detaches
shadows from what casts them.

`fit_cascade` divides through each cascade's own depth range, so this comes out correct for free —
and `a_near_cascade_gets_a_larger_bias_than_a_far_one` is what stops that quietly ceasing to be true.

## Consequences

**Measured cost: 71.7 µs → 113.7 µs of GPU time** on the Scarp at 640×360, about 1.6× and still 0.7%
of a 60 Hz budget. The three extra shadow passes each cost *less* than the first, because they draw
the same casters into smaller boxes. `view 0` grew 7 µs, which is the fragment stage's cascade
selection. Full table in `docs/10-frame-budget.md`.

**What it buys**: the near cascade covers about a seventh of the distance at the same resolution, so
a texel near the camera is about 1 cm of ground rather than 7 cm.

**Selection is by radial distance from the camera, not view-space depth.** The cascades are
concentric boxes around the eye rather than slices of the frustum, so how far away a point is *is*
the question. Using depth along the view axis would put a point off to the side into a cascade that
does not reach it.

**`Orthogonal` stays and is still the right choice for an interior.** One map is cheaper, M3's exit
gate is indoor, and `games/atrium` keeps what it has.

**No blending across split boundaries.** A fragment near a boundary can pick a different cascade than
its neighbour, which shows as a line across the ground where resolution changes. The standard fix is
to blend over a small band, at the price of a second sample. Built without it deliberately, because
the plan said to look first — and at these distances the seam is not visible in the Scarp. It is the
first thing to add if it becomes one.

**The wasted half.** A concentric cascade covers the space *behind* the viewer, which is roughly half
of it. Fitting each cascade to its slice of the view frustum would use the resolution better and is
named in the fitting commit as the first thing to revisit if cascades are not sharp enough.

## The bug this shipped through, which is the part worth keeping

The first capture with cascades on came back with a **huge dark wedge across the horizon**. Nothing
failed to compile, nothing failed validation, and every headless test passed.

`sky.wgsl` kept its **own hand-written copy** of the per-view uniform struct, because it and
`mesh.wgsl` read the same buffer at the same binding. Turning `light_view_projection` into an array of
four grew that struct by 192 bytes in one copy and not the other — so the sky shader read the three
vectors that turn a screen position into a world direction from the wrong offsets, and drew the sky
facing somewhere else.

The fix is not to update the copy. `view.wgsl` holds the declaration once and is prepended to both at
pipeline creation, so there is one statement of the layout. This is the same answer `amadeo-snapshot`
gives for borrowing `format_float` from `amadeo-scene`, and the same shape as the winding/normals and
the two-sided-apron findings: **two copies of one fact drift, and a comment saying "keep these in
step" is not a mechanism.**

One copy remains and cannot be removed this way: `GpuMeshView` in Rust. `#[repr(C)]` and a WGSL struct
are two statements of one layout, and only a wrong picture says they disagree.

## Alternatives rejected

**A `cascade_count` field.** A variable-length texture array and a variable shader loop bound, to buy
flexibility nothing has asked for. Adding it later defaulting to four changes no existing file.

**One pass drawing all four layers.** Needs multiview or layered rendering, which wgpu exposes
narrowly and WebGPU does not guarantee. Four passes cost four pass setups, which the measurement
above shows is not where the time goes.

**Sharing `GpuMeshView` with the shadow pass, as before.** The shadow pass draws exactly one cascade,
so it would have to be told which — which is a uniform of its own under another name. It gets a
small one holding just its matrix, and both are filled in the same loop from the same `ShadowData`,
which keeps the "cannot disagree" property that sharing was for.
