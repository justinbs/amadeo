# ADR 0073 — Fog is a forward term on the surface shader, not a post-process

**Status:** Accepted · **Date:** 2026-08-16 · **Builds on:** ADR 0034, ADR 0045, ADR 0049 ·
**Settles:** `docs/05`'s M3 exit gate item 5, and the "deliberately missing" note ADR 0034 left

## Context

M3's exit gate item 5 asks for "a dark corridor with a moving flashlight that reads as genuinely
atmospheric", and calls it the renderer's real exam. Fog is most of the first half: without it, the
far end of a corridor is either fully lit or fully black, and neither reads as somewhere you cannot
see into.

ADR 0034 declared fog as part of `Environment` and then deliberately did not build it, on the
grounds that it "needs to know how far away each pixel is — and there is no depth buffer until the
mesh pass lands". That reasoning was about a **post-process** implementation, and it quietly became
the reason fog stayed unbuilt for four milestones after the depth buffer arrived.

Justin chose distance fog plus a real environment map over three alternatives (post-process fog,
volumetric light shafts, and doing only the environment map).

## Decision

### 1. Fog is applied per surface, in `mesh.wgsl`

A fragment shader already knows its own world position, and the camera's world position has been in
the per-camera view uniform since PBR landed (ADR 0048). So the distance a fragment is behind is a
subtraction, and **no depth buffer is involved at all** — the thing ADR 0034 was waiting for was
never needed for this shape of the feature.

It is applied **last**, to the finished lit colour, because air between the eye and a surface adds
its own light and hides what is behind it. Fogging the albedo instead would let a lamp brighten the
haze in front of it, which is not how air works.

### 2. The curve is exponential-squared, and `start` is subtracted rather than dividing a range

`factor = 1 - exp(-((distance - start) * density)^2)`.

A linear start-to-end ramp has a visible edge where it begins and another where it ends, and both
read as a band across the picture. Exponential-squared has no edges anywhere; the squared term is
what keeps near objects nearly untouched while distance closes in quickly, where plain exponential
fog is already noticeable at arm's length.

`start` is subtracted before the curve rather than remapping a range, so it means exactly "nothing
closer than this is fogged" and moving it does not also change how thick the far distance is. Two
numbers that each mean one thing.

### 3. Off is off *exactly*

The shader returns early at zero density rather than computing `mix(colour, fog, 0.0)`. Both are
exact in IEEE 754, and the early return does not depend on that being true of every driver's `mix`.

**This is the claim the whole decision rests on**, because every `.environment` in the repository
defaults to `density 0.0` and three games have captures asserted against their pixels.
`fog_off_is_byte_identical` pins it, with `fog_actually_reaches_the_pixels` and
`fog_thickens_with_distance` as the controls — the first would pass perfectly against a renderer
that ignored the field entirely.

### 4. It lives in `Environment`, and travels in the *view* uniform

Fog is part of a camera's look, so it is authored where the rest of the look is (ADR 0034): a
reflected block in an `.environment` file, so `amadeo fmt`, `amadeo check`, `describe --example` and
a snapshot all work on it with nothing built.

But it is the one field there that is **not** a post-process, so it reaches the shader through the
per-camera `MeshView` uniform rather than the post one. Two extra `vec4`s, with the density packed
into the colour's alpha because WGSL pads a lone `f32` in a uniform to sixteen bytes anyway.

## Consequences

- **Every `.environment` file gained a `fog` block**, which is Q32's shape for the fourth time — and
  for the fourth time the churn was not the problem. `App::asset_problems` named the file, the
  component and the missing field in one line, which is exactly what session 17 built it for.
- **Fog is uniform.** It cannot be thicker near the floor, denser in one room, or shaped by anything.
  Height fog is an additional term in the same function; per-room fog would need a volume, which is a
  different decision.
- **A torch beam still lights no fog**, so there are no visible shafts. That is the *volumetric*
  feature, and it needs this one to exist first: it raymarches through exactly this medium. It stays
  the obvious next step for atmosphere and is deliberately not paid for now.
- **The sky is not fogged**, only surfaces. For an interior that is invisible; for an outdoor scene
  it is the correct behaviour — distant terrain fades towards the fog colour while the sky does not,
  which is what aerial perspective looks like.

## Rejected alternatives

**Post-process fog reading the depth buffer.** The shape ADR 0034 assumed. It fits the existing post
chain and would allow richer curves later — and it requires binding the scene depth buffer as a
texture, which `CLAUDE.md` currently states must *not* happen (the shadow map is its own
`TargetFormat` variant precisely so the scene depth buffer does not ask for `TEXTURE_BINDING`).
Reversing that for a result identical to what four shader lines produce is the wrong trade.

**Volumetric light shafts now.** The strongest possible version of this feature, and the one that
would most improve `games/warren` specifically. Rejected as an order of magnitude more work —
per-pixel raymarching through a shadow map, plus noise or temporal filtering or it bands and crawls
— and because it presupposes this decision anyway.

**Doing only the environment map.** The cheaper half of the same session's work, and it closes a
known defect (`sky ""` leaves nothing lighting an indirect surface at all). It is done as well, and
it does not deliver "a corridor that recedes", which is what the gate names.
