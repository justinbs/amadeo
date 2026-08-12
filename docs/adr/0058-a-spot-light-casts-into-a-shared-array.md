# ADR 0058 — A spot light casts, into a layer of the shared shadow array

**Status:** Accepted · **Date:** 2026-08-12 · **Builds on:** ADR 0038, ADR 0055, ADR 0057

## Context

ADR 0057 gave the engine lights at a place and said plainly what they could not do: **everything they
lit, they lit through walls.** That was the deliberate half of a scope decision — ship the components
first so their shape settles in scene files before shadows complicate it.

This is the other half, and it is what M3's exit gate is judged on: *"a dark corridor with a moving
flashlight that reads as genuinely atmospheric"*. A flashlight that shines through the crate in front
of it is the moment a horror scene stops working.

## Decision

**A spot light may cast one shadow, into a layer of the same texture array the directional light's
cascades use.**

### One array, because there is nowhere else to put one

All four bind groups are already spoken for — view, shadow map, material textures, environment — and
wgpu guarantees only four. A second shadow texture would have **nowhere to bind**. So spot shadows go
into layers after the cascades, which ADR 0055 already made an array for.

`View::shadow_atlas` is the one place that decides how many layers a view needs and how big they are,
so the graph's declaration, the backend's layer arithmetic and the shader's indexing cannot disagree.

**The cost is a shared resolution.** A texture array has one size for every layer, so a 512-pixel spot
beside a 2048-pixel sun is drawn at 2048. `SpotLight::shadow_resolution` is therefore a *request* and
the largest wins. The fix, if shadow memory ever matters, is a second smaller array plus a bind group
built per frame rather than per texture — more machinery than one number is worth today.

### A `bool`, not a `ShadowMode`

A spot light has exactly one sensible arrangement: one perspective map from where it stands, looking
where it points. Cascades exist to spread resolution over a range a *directional* light cannot bound,
and a spot bounds itself with its range and its cone. Offering `Cascaded` on a spot would be offering
a wrong answer.

### Two casters, and that constant is the expensive one

Each is a full extra pass over the scene's geometry *and* a layer of an array whose size is shared.
`MAX_SHADOW_SPOTS` is **two**: a flashlight plus one fixed light is what the horror slice needs, and
the third light in a scene is still *lit*, just not casting. Which two is decided by the same
nearest-first sort ADR 0057 uses for the lights themselves, applied after the cut so a layer is never
assigned to a light that was dropped.

### The fitting is trivial, and that is the point

A directional light has no position and no bound, so ADR 0055 invents one: a camera-centred box,
snapped to a texel grid, split four ways. A spot light **is** a camera — it stands somewhere, points
somewhere, and stops at its range. Its shadow matrix is `perspective(2 × outer_angle) × look_along`,
and there is no fitting at all.

Two details that differ from the cascaded path and would be wrong if copied:

- **The perspective divide is real.** A cascade's projection is orthographic, so `w` is 1 and
  `mesh.wgsl` skips the divide. A spot's is perspective. Skipping it gives a shadow that stretches
  away from the light and looks like a bias problem.
- **The bias divides through the range, not through `far - near`.** Perspective clip depth is
  compressed towards the far plane, so a world-unit offset is a much larger share of it out at the
  range — which is where precision is worst and where acne appears first.

## Consequences

**The caster list is now the union of every shadow volume**, and getting that wrong was the bug this
shipped through. `shadow_casters` was culled to the *directional* light's box alone, so a scene lit
only by a torch produced an **empty** caster list: every shadow pass cleared its layer, drew nothing,
and every surface came out fully lit. A shadow map with nothing in it does not look broken — it looks
like no shadows, which is exactly what the feature is supposed to change.

The union is deliberately loose. A pass whose own light cannot see a mesh clips it anyway, so a
generous list costs a few vertices where a tight one costs a missing shadow.

**Point lights still cast nothing.** A point light's shadow is a cube — six faces, six passes, and a
sampling path that picks a face from a direction. That is a bigger job than this one and it is not
what the exit gate needs.

**`games/atrium` gained a shadow-casting lantern**, on ADR 0057's rule that a feature nothing uses is
a feature nobody has looked at. Its pool of light and the pillar shadow inside it are visible in the
capture.

**No existing scene changed.** `shadows` defaults to `false`, so a spot light costs a pass and a layer
only when asked, and the Scarp's four-cascade array is byte-identical.

## Alternatives rejected

**A shadow texture per light.** No bind group left, and it would mean a variable number of bindings —
which is a bind group rebuilt whenever the light count changes rather than a layer index.

**One pass drawing every layer.** Needs multiview or layered rendering, which wgpu exposes narrowly
and WebGPU does not guarantee. ADR 0055 rejected it for cascades on the same grounds and the
measurement there showed pass setup is not where the time goes.

**Reusing `ShadowData` with `count = 1`.** It carries four cascades, a cascade count and per-cascade
`far` distances that a spot has no use for, and its bias means something subtly different. Two types
that share a name and disagree about their fields' meaning is worse than two types.
