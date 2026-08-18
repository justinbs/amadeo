# ADR 0077 — Transparency is a declared mode and a second, sorted pass

**Status:** Accepted · **Date:** 2026-08-18 · **Builds on:** ADR 0018, ADR 0033, ADR 0052, ADR 0075 ·
**Implements:** item 11 of `docs/13-the-engine-gate.md`

## Context

Every 3D pipeline in the engine was `blend: None`, with a comment deferring the question: *"transparent
meshes need back-to-front sorting within a `SortOrder`, and doing that before there is anything
transparent to sort would be guessing at the shape of a problem nobody has yet."* That was the right
call when it was written and it stopped being right when `docs/12-the-bar.md` named Project Zomboid,
No Man's Sky and Schedule I as first-class targets. Glass, water, light shafts, ghosts, smoke and
world-space panels are all transparent, and none of them was expressible.

The engine gate's third review split the work in two, and the split is the reason this ADR covers only
half of it. **Alpha cutout** — discarding a fragment whose sampled alpha is below a threshold — is
worth nothing until there is a texture to sample: all 36 texture slots in this repository are empty,
so a cutout material today would cut out a rectangle. It moves to Phase C beside the texture
generator, where there is foliage to cut out on the day it lands. **Blending** needs no texture at
all: a pane of coloured glass, a water surface and a heads-up panel are all `base_colour.a < 1`.

## Decision

### 1. A material declares its alpha mode; it is not inferred from alpha

`Material::alpha_mode` is an `AlphaMode`, defaulting to `Opaque`.

Inferring "alpha below one means blend" was the tempting alternative and it is the pattern
`docs/07` now records **five** instances of in this repository: a derivation standing in for a
decision, right for every case anyone has tried and silently wrong for the next. Concretely it would
mean a material cannot be authored as opaque-but-faded, and that an animation driving alpha through
`0.99` would silently move a surface between two pipelines mid-clip.

ADR 0075's declared defaults are what make this free: `alpha_mode` defaults to `Opaque`, so **not one
of the twelve existing `.material` files changed**, and an opaque scene's capture is byte-identical.

**`AlphaMode` has two variants and the missing one is deliberate.** `Mask` is not there, because
adding a variant a scene can ask for and silently not get is the defect ADR 0056 found in bloom.
ADR 0055's precedent is the right one: fill a variant when you build it.

### 2. The split happens at collection, and the sort with it

`View` gains `transparent`, a second list beside `meshes`, filled by `split_by_alpha` and sorted
**furthest from the eye first**.

Deciding draw order in the *backend* was the alternative. It is rejected for the reason ADR 0038 fits
shadow matrices at collection: order is a fact about the frame rather than about a device, so putting
it here means one implementation, which `NullBackend` also sees — an ordering mistake is catchable
with no GPU.

Two properties of the sort matter more than the sort:

- **`SortOrder` is compared first**, so an author who has said "this pane belongs in front" gets that,
  and distance only decides within an order.
- **Equal distances keep a reproducible order without a tie-break**, because `sort_by` is stable and
  the order it is given is already deterministic. `total_cmp` rather than `partial_cmp` is what stops
  a `NaN` making the comparison inconsistent and the output arbitrary.

**It sorts by origin, which is an approximation and always will be.** Two long panes crossing at right
angles have no correct order at all. Per-triangle sorting is far too slow and order-independent
transparency is a much larger feature; authoring around it is what `SortOrder` is for. Every engine
that does per-object sorting has this limitation.

### 3. The blended pass does not write depth, and draws after the sky

One pipeline descriptor, two pipelines, differing in exactly two states: `blend: ALPHA_BLENDING` and
`depth_write_enabled: false`. It still *tests* depth, so glass behind a wall is correctly hidden.

Both halves are required rather than tuned. Blending is not commutative: a pane drawn before the wall
behind it composites against the background instead of against the wall, and no depth testing fixes
that — the wall's fragments are *rejected* once the nearer glass has written depth. And if the blended
pass wrote depth, two blended surfaces would hide each other in whichever order they happened to
arrive, which is the thing the sort exists to decide.

Drawing after the **sky** rather than merely after the opaque meshes is the other half: a pane drawn
before the sky composites against the clear colour rather than against the horizon behind it.

## Consequences

- **The pipeline layout is untouched**, so nothing about lighting, shadows, IBL, MSAA or the culling
  decision (ADR 0052) differs between an opaque surface and a blended one. A pane of glass is lit.
- **Building it found a real latent defect.** `upload_frame_meshes` iterated `view.meshes` alone, so a
  blended mesh reached the backend and drew nothing, its geometry never having been uploaded. The
  symptom was a transparent surface that was simply *absent*, which reads as blending being broken.
  **`shadow_casters` had the same hole and was safe only by coincidence** — it is culled to the
  light's box where `meshes` is culled to the camera's, so a caster behind the camera has always been
  able to name a mesh no other list mentions. Both are fixed together.
- **Three near-identical instance-packing loops became one function.** Two already existed and had to
  agree about the instance layout and the draw-merging rule; a third would have been the copy that
  drifted.
- **Shadows from a blended surface are not modelled.** A pane of glass casts an opaque shadow, because
  the shadow pass draws depth only and knows nothing about alpha. That is the standard behaviour of a
  simple shadow map, it is wrong for glass, and the fix belongs with alpha cutout — where a threshold
  exists to test.
- **Nothing in the repository uses it yet**, and that is deliberate. This ADR delivers the mechanism
  Phase C's content needs; a game using it is item 13's business, and the engine gate exists precisely
  to stop a feature being declared done on the strength of a test alone.
