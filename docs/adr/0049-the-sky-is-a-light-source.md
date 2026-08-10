# ADR 0049 — The sky is a light source

**Status:** Accepted · **Date:** 2026-08-10 · **Builds on:** ADR 0021, ADR 0026, ADR 0033, ADR 0034,
ADR 0045, ADR 0048 · **Closes:** Q28

## Context

Every surface in the engine carried this, written into `mesh.wgsl`:

```wgsl
let ambient = 0.12;
```

One number, added to everything, regardless of which way a surface faced or what was around it. It
started at `0.03` and was raised to `0.12` when shadow maps arrived, because whole areas of *floor*
became ambient-only and at `0.03` they read as holes in the world rather than as shade. The fix then
was to guess a brighter constant. **Q28** recorded that the real answer was different: the sky is a
light, and ambient was standing in for it.

ADR 0045 ranked replacing it as *"probably the single biggest step towards looking like a real
engine"*, and ADR 0048 made it urgent rather than merely desirable — a metal has no diffuse at all, so
under a constant ambient it renders **black**. Metallic-roughness shipped with metals unusable.

## The choice, and why the more expensive option was taken

Justin was given four options with costs, and chose the third.

1. **An authored ambient colour.** An afternoon's work, and it renames the problem: still one flat
   value everywhere, metals still black.
2. **A sky/ground hemisphere gradient.** Cheap, text-authorable, no new asset kinds. Gets most of what
   outdoor light *is* — things facing up catch sky, things facing down catch bounce. No directional
   detail, and nothing for an indoor scene.
3. **Full image-based lighting from an HDR environment map.** What makes PBR pay off, and what an
   indoor scene needs. Requires cube maps, an HDR decoder, and a prefiltering stage — none of which
   existed.
4. **A procedural sky feeding the lighting.** Most work; sky and light agree by construction.

**Option 3 was chosen**, and the recommendation had been option 2. Two things make that the right
call rather than merely the more ambitious one: M3's exit gate is a **dark atmospheric corridor**,
which a sky gradient does nothing for and an environment map does everything for; and option 2 would
have been thrown away rather than extended, because a hemisphere gradient produces no reflections at
all and reflections are half of what was missing.

## Decision

### 1. An environment is decoded, projected onto a cube, and convolved twice — at load, on the CPU

`.hdr` (Radiance RGBE) → equirectangular `HdrImage` → `Cubemap` → two prefiltered products:

- **Irradiance**, 16 pixels a side: for each direction, the total light reaching a matte surface
  facing it. Irradiance is the smoothest signal in rendering, so resolution here would store noise.
- **A specular chain**, 128 down to 4 across six levels: the environment blurred once per roughness,
  by GGX importance sampling. Karis's split-sum, which is what every real-time renderer now does.

**On the CPU, and that is invariant I7 rather than a preference.** Every subsystem must be
headless-capable; a prefilter needing a device could not run in a headless test or on a machine with
no GPU. `mip_chain` is the precedent. The transcendentals throughout are covered by ADR 0044's
existing carve-out: banned from anything deciding gameplay state, fine at load when the output is
pixels.

**Prefiltering lives in `amadeo-render`, not `amadeo-image`.** `importance_sample_ggx` and
`mesh.wgsl`'s `distribution_ggx` are two views of one model — one generates directions, the other
measures how many point a given way. If they disagreed, a prefiltered reflection would be subtly the
wrong shape for the material shading it, and two crates that cannot see each other is how that
drifts.

### 2. Radiance `.hdr`, not OpenEXR

An ordinary `.png` clips at white, so the sun and the sky beside it would be the same colour — and the
sun is the part that matters. RGBE stores three mantissas and one shared exponent, covering ~`10^38`
in four bytes.

Chosen over OpenEXR because the decoder is two hundred lines with no dependencies where EXR's
reference implementation is a large C++ library, and because `.hdr` is what free environment maps are
distributed as. The cost is real and is stated in the round-trip test rather than hidden behind a
loose tolerance: one exponent serves all three channels and is chosen for the brightest, so a dim
channel beside a bright one is quantised in steps sized for the bright one.

### 3. A sky is named on the `Environment` asset — Q28's second half

A `DirectionalLight` is a **direct** light: one direction, one colour, casting a shadow. An
environment map is the **indirect** half — everything arriving from everywhere else. They are
different quantities that happen to share the word "lighting".

`Environment` is already "what this camera sees the world as" and already an asset with a cache
(ADR 0034), so this needed no new asset kind, no new component and no new loading path. A world may
hold several lights; it has one look per view, which is what a surrounding *is*.

Cheap to reverse: one field and the three `.environment` files that name it.

### 4. Bind group 3, and `Rgba16Float`

Group 3 is the last slot wgpu guarantees, and it is the right home rather than the only one left:
0 is the view, 1 the shadow map, 2 the material's textures. An environment changes **per camera**
like the first two, not per draw like the third, so it is set once per view.

`Rgba16Float` rather than `Rgba32Float`, and not to save memory: **32-bit float textures are not
filterable in wgpu's base feature set**, and an environment is sampled with smooth interpolation
across faces and between roughness levels. Halving each channel is twenty lines of bit arithmetic
rather than a dependency.

The environment BRDF is **Lazarov's analytic fit** rather than Unreal's lookup texture — four
instructions, and it saves both a texture and a binding.

### 5. A camera naming no sky gets exactly what it had before

The fallback is a uniform cube map of `0.12`, the constant it replaces. A game that asks for nothing
shades exactly as it did, rather than going black and making image-based lighting look like a
regression.

## Consequences

**Good.**

- **Q28 is closed.** Shadowed areas read as shade rather than as holes, because the sky fills them.
- **Metals work**, which ADR 0048 shipped without. A metal under a blue sky reads blue.
- Surfaces pick up the colour of what surrounds them, which no constant could ever do.
- `PixelFormat`, the sidecar's free-form settings, and `EnvironmentCache` all absorbed this with no
  format change — three earlier designs paying off at once.

**Bad, and accepted.**

- **The sky is not drawn.** This makes the sky a *light source*; painting it behind the scene is a
  separate pass, and until it exists the background is still a flat clear colour. Probably the largest
  remaining visual gap, and cheap relative to what this cost.
- **The sun in a generated sky and the sun in a scene must agree by hand.** `games/scarp`'s
  `bin/sky.rs` carries a `SUN_DIRECTION` matching its `DirectionalLight`, and nothing holds them
  together. If they drift, shadows fall one way and the bright part of the sky sits another — which
  reads as nothing being wrong and everything looking slightly off. Documented in both places, which
  is not a mechanism.
- **Prefiltering costs seconds at load** for a real environment map. Acceptable now, behind ADR 0021's
  barrier where gameplay cannot observe it. The answer when it stops being acceptable is an import
  pipeline that caches the prefiltered result, which ADR 0026 already anticipates.
- `Environment` is **no longer `Copy`**, since it holds a `String`. It is cloned once per view per
  frame instead of copied.
- Only one environment per frame still, which is **Q23** — unchanged by this, but more visible now
  that an environment carries lighting rather than only a post-processing chain.

**Two defects this found, both by looking at the picture.**

- **The feature rendered nothing at first.** Every game installs `TextureCache` by hand, so `SkyCache`
  followed that precedent — and nothing installed it, so every frame silently fell back to the neutral
  sky. No error, no failing test, and a capture identical to the one before. It now installs itself
  beside `EnvironmentCache`. **A capability that goes inert when a setup line is missing is a shape
  this project keeps rediscovering**, and the fix is to remove the line rather than remember it.
- **Adding `sky` to `Environment` broke every `.environment` file**, and the symptom was not a parse
  error: the file was skipped in silence, `found` came back empty, and the failure surfaced three
  layers away as a *missing service*. That is **Q32** arriving one session after it was filed as
  theoretical, and `docs/07`'s "an asset that will not parse is skipped in silence" exactly.

**Explicitly not decided here.** Drawing the sky; whether the default `Environment` should tonemap;
multiple environments per frame (Q23); and whether prefiltering moves to an import step.
