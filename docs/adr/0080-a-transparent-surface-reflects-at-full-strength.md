# ADR 0080 — A transparent surface reflects at full strength

**Status:** Accepted · **Date:** 2026-08-19 · **Builds on:** ADR 0048, ADR 0077 ·
**Implements:** item 12f (part one) of `docs/13-the-engine-gate.md`

## Context

`games/atrium`'s glazed screen did not read as glass. It read as an empty picture frame, and the
engine gate measured it: the pane changed the pixels behind it by **one level**.

Three explanations were proposed and two were wrong, which is worth recording because the wrong ones
were plausible and one of them was mine.

1. **"The blended pass never binds the environment."** False. `draw_run` in `gpu.rs` is a single
   closure called for both the opaque and the transparent pipeline, and it sets bind group 3 before
   either.
2. **"The specular path does not reach blended surfaces."** False, and I offered the disproof: setting
   the pane `metallic 1.0` moves it from (204,209,205) to (123,133,138). The environment specular is
   reaching it.
3. **The blend mode caps it.** Correct, and it is arithmetic rather than a matter of degree.

`mesh.wgsl` returned the whole lit result into `BlendState::ALPHA_BLENDING`, which is
`src·a + dst·(1−a)`. **The entire shader output is multiplied by coverage** — diffuse, specular and
emissive alike. At `alpha 0.34` the pane could contribute at most 34% of anything, so its pixel was
bounded above by `0.34·S + 0.66·W` where `S` is what the same material renders as opaque and `W` is
the wall behind it. No roughness, no sun and no environment could lift it past that line.

**And the metallic-1.0 experiment was this bug's signature rather than evidence against it.** A mirror
aimed at a bright sky got *darker*, because 34% of a dim reflection replaced 34% of a bright diffuse.

## Decision

### The shader keeps direct light in two halves, and premultiplies only the transmitted one

`direct_light` returns a `LightPair` — diffuse and specular — instead of their sum. Ambient was
already split. The fragment output becomes:

```wgsl
let surface = lit_diffuse * albedo.a + lit_specular + in.emissive;
return vec4<f32>(fogged_premultiplied(surface, albedo.a, in.world_position), albedo.a);
```

- **Diffuse is scaled by alpha.** It is light that scattered *through* the material, so a surface you
  can see through transmits proportionally less of it.
- **Specular is not.** It bounced off the front face and never entered the material. A window at 34%
  opacity reflects a highlight at full strength.
- **Emissive is not either.** Something that glows does not glow less because you can see through it.

### The transparent pipeline blends premultiplied

`One, OneMinusSrcAlpha` rather than `SrcAlpha, OneMinusSrcAlpha`, so the weighting is the shader's to
decide rather than the blender's.

**Unity ships exactly this, on its Alpha blend mode, under the name "Preserve Specular Lighting."**
Unreal's surface-forward translucency is structurally the same. It is a named, solved problem rather
than an invention here.

### Fog gains an alpha argument

`fogged_premultiplied(colour, alpha, world)` mixes toward `fog_colour * alpha`, because in a
premultiplied frame the haze in front of a half-transparent surface covers only that surface's share
of the pixel — whatever shows through behind it was fogged by its own fragment, over its own longer
distance. At alpha 1 it is the plain `mix` it replaced.

## Consequences

- **Every opaque surface is byte-identical, and that is checked rather than argued.** At `alpha = 1`
  the new expression reduces to `diffuse + specular + emissive`, the exact sum it replaced, and the
  opaque pipeline declares no blend state at all so the alpha channel never reaches it. All 49
  pre-existing capture tests — many of which pin exact pixel values — pass unchanged through FXC.
- **The close condition is arithmetic and the test states it that way.**
  `a_highlight_on_glass_beats_what_straight_alpha_could_produce` renders the same scene three times —
  the pane opaque, the pane blended, and the wall alone — and asserts the blended pixel exceeds
  `0.34·S + 0.66·W` in **linear** light. The bound is measured rather than written down, so it cannot
  rot when the BRDF changes. Reverting both halves of this ADR puts the measurement at **0.1070
  against a bound of 0.1071** — exactly on the line, which is what says the bound is the right one.
- **Two vacuity traps were hit writing that test and both are recorded in it.** The first version
  compared two saturated pixels, both 255; there is now an explicit refusal to measure anything above
  250. The second used a **box** as the pane, and a box has a front and a back face — with no depth
  write (ADR 0077) both blend at the same pixel, which made the blended pane come out *brighter* than
  the opaque one. It is a `PlaneMesh` now, and that is the first use of `PlaneMesh` anywhere in this
  repository.
- **It did not finish the job, and the remainder is a different problem.** `games/atrium`'s pane went
  from **1 level** brighter than the wall beside it to **10**. That is the cap lifting. What it now
  reflects is a rotationally symmetric gradient with no features in it, so there is nothing in the
  reflection to recognise — `daylight.rs`'s `sky_colour(up)` is a function of elevation alone, so a
  reflection in it **cannot change as the camera yaws**. Structure and a sun disc in the specular
  chain, excluded from the irradiance, is item 12f part two, and this had to land first or that
  content would have been tuned to compensate for a blend mode.
- **A solid used as glass double-blends.** Every face of a transparent box composites, because the
  transparent pass deliberately does not write depth. `games/atrium`'s `glass_pane` is a thin box and
  is therefore blended twice. Not fixed here; recorded because it makes any arithmetic about a
  transparent object wrong by a factor nobody would guess, and it is the reason the test uses a plane.
