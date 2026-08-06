# ADR 0038 — One shadow map now, and the mode is authored data

**Status:** Accepted · **Date:** 2026-08-06 · **Builds on:** ADR 0026, ADR 0031, ADR 0033, ADR 0034

## Context

M2's exit gate 1 names shadows. Nothing in the engine can cast one, and shadows are the first thing
that will **read** the depth buffer rather than only write to it — which `STATUS.md` had been
carrying as a known wrinkle since the mesh pass landed.

The framing question, for the sixth time in this subsystem: *what data does this imply?*
`RenderBackend` isolates the algorithm completely, so the shader is cheap to change and always has
been. What is not cheap is the field on `DirectionalLight`, because that is an authored, **hashed**
component that scene files carry.

## The vocabulary, because it is what the choice turns on

A **shadow map** is the scene rendered from the light's point of view, storing only depth — how far
the light can see before something blocks it. Shading a pixel then asks *"is anything closer to the
light than me?"*; if so, this pixel is in shadow.

For a **directional** light — the sun — this is awkward in a specific way: it has no position, so one
map has to cover everything visible. Stretched over a large scene, each shadow-map pixel covers a lot
of ground and edges go blocky. **Cascades** fix this by splitting the camera's range into 2–4 slices
with a map each: full detail nearby, coarse in the distance.

## What the research found

**Godot ships single-map ("Orthogonal") as a real supported mode, not a stepping stone**, alongside
2 and 4 PSSM splits. Its own guidance is that Orthogonal is right when the shadow distance is low —
interiors, small scenes, weaker hardware — and splits are for large outdoor views.

That is the finding that decided the scope: **the number of splits is authored data on the light.**
Unity's shadow cascades are a quality setting; Unreal's directional light exposes cascade count
directly. Nobody treats one-versus-many as an architectural fork, because it is not one.

So "one map now" is not a shortcut that has to be undone. It is one value of a field that has to
exist anyway.

## Decision

### 1. `ShadowMode` is an enum on `DirectionalLight`, and it ships with two variants

`Off` and `Orthogonal`. Cascades arrive as a third variant carrying a split count.

This is the same argument `PixelFormat` shipped with under ADR 0026 and the render graph's internal
`TargetFormat` under ADR 0034: **the tag is the load-bearing part**. Getting the shape right once
means cascades become a new variant and a loop, rather than a change to every scene file that has a
sun in it — and to the state hash of every replay containing one.

`Off` is the default. Not only to save the work: a scene with no shadows looks flat but *correct*,
where a scene with a badly-fitted shadow map looks broken. Opting in is the safer direction.

Alongside it, three fields that are quality-versus-cost knobs rather than modes: `shadow_distance`
(the half-extent of the box, and the setting with the most direct effect on quality),
`shadow_resolution`, and `shadow_bias`.

### 2. The box follows the camera and is snapped to a **world-anchored** grid

A directional light has no position, so there is nothing to centre a shadow map on except what the
viewer can see. Centring it on the camera is what keeps the resolution where it is being looked at.

But a box that slides continuously with the camera makes every shadow edge **crawl** — each
shadow-map pixel covers a slightly different patch of world every frame, so edges fizz and swim while
the player walks, with nothing in the scene actually moving.

The fix is to snap the box to a grid **anchored at the world origin**. Snapping relative to the
camera would be snapping to something that moves, which is no snapping at all — this was got wrong
once while deriving it, and the test `a_shadow_box_moves_in_whole_texels` is what pins it: nudge the
camera by a quarter of a texel and a fixed world point must land on exactly the same spot.

### 3. A shadow map is a **distinct format tag** in the render graph, not a flag

`TargetFormat` gains `ShadowMap32` next to `Depth32`. They are the same wgpu format and differ in
what they are for, which turns out to be what matters:

- A shadow map needs `TEXTURE_BINDING`; the scene depth buffer does not, and asking for usages
  nothing needs is not free — some backends pick a less efficient memory layout to satisfy them.
- `assign_transients` reuses one physical texture for two transients whose `(width, height, format)`
  match. Without distinct tags a shadow map and a scene depth buffer of the same size could be handed
  the same texture, and one of them would be missing the usage it needs.

This also resolves the `PooledTexture::bind_group` wrinkle `STATUS.md` recorded. The `Option`
**survives** rather than going away: it turned out there are two kinds of depth texture and only one
is sampled. A shadow map gets a bind group built against a comparison layout; the scene depth buffer
still gets none.

### 4. The shadow pass has no colour attachment, and its ordering is derived

`PassKind::Shadow` is the only pass in the engine that attaches depth and no colour — nothing is
painted, only measured. It declares the shadow map in `writes` and the view pass declares it in
`reads`, so **the order falls out of the dependency** rather than being asserted by writing the
passes in sequence. `NullBackend` compiles the same graph, so a shadow-before-view ordering bug is
catchable with no GPU, which is what ADR 0034 said an internal graph buys.

### 5. Scope, decided rather than asked

- **Only directional lights cast.** Point lights are still to come; M3's horror slice is what
  actually needs them, and gate 1 does not.
- **At most one light casts per view.** Every extra shadow-casting light is another full pass over
  the scene. Choosing between a loop in the shader and a pass per light is the same open question
  this crate already has about lighting generally — answering it here, for shadows only, would be
  answering it in the wrong place.
- **A 1×1 placeholder shadow map is always bound.** Same argument as `TextureCache`'s placeholder:
  the last resort must be something that cannot itself be missing. Otherwise the mesh pipeline needs
  a second variant compiled without the shadow bindings — two pipelines, two shaders that can drift,
  and a 2D game paying for a distinction it cannot observe. A uniform flag says which is bound.

### 6. Two defect-avoidance choices worth recording

**Front faces are culled in the shadow pass**, where the mesh pass culls back faces. This is the
cheapest fix for shadow acne there is: recording the far side of each object moves the stored depth
away from the surface being lit, so a lit surface stops shadowing itself. It costs correctness only
for geometry with no thickness, which is what `shadow_bias` remains for.

**The bias is slope-scaled.** A surface seen edge-on by the light spans much more depth across one
shadow-map texel than one facing it square. One flat bias forces a choice between acne on slopes and
peter-panning on flat ground; scaling it by the surface's angle to the light needs neither.

**Shadow multiplies direct light only, never ambient.** A surface in shadow is still lit by the sky.
Multiplying ambient too would make every shadow a silhouette of pure black.

## Alternatives rejected

**Cascades now (2–4 splits).** The complete answer for open-world scenes immediately, and no second
visit. Rejected on cost and risk rather than on principle: roughly two to three times the work, and
it introduces split selection, a matrix per cascade, and per-cascade texel snapping all at once —
three subtle things where this introduces one. Since the mode is data, nothing about arriving there
later is harder than arriving there now.

**Minimal: hard shadows, no softening, no authored fields.** The fastest route to a gate-1 tick.
Rejected because the fields are the part that touches scene files and the state hash, so deferring
them defers the only expensive thing — which inverts the usual reason to defer.

## Consequences

- **Shadows go blocky over large outdoor scenes** until cascades land. This is inherent to one map
  and is the documented limitation of the mode, not a defect.
- A game that never asks for shadows allocates no shadow map, runs no extra pass, and pays only a
  1×1 texture and a uniform branch — the same posture the depth buffer already takes for 2D.
- `Mat4` gained `orthographic` and `look_along`. Both belong in `amadeo-transform` rather than the
  renderer: a projection is a matrix, not a rendering concept, and `amadeo-physics` or a future
  culling pass will want them too.
- Shadow fitting is arithmetic on the CPU, so **it is checkable headless**. Where the box lands is by
  far the most likely thing to be wrong, and `tests/shadows.rs` checks it with no GPU — leaving
  `capture.rs` to prove only the part that genuinely needs pixels.
