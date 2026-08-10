# ADR 0048 — Surfaces shade with a real BRDF

**Status:** Accepted · **Date:** 2026-08-10 · **Builds on:** ADR 0033, ADR 0034, ADR 0045, ADR 0047

## Context

ADR 0045 put **metallic-roughness PBR** third on M3's renderer list: *"`Material` has carried
`metallic` and `roughness` since ADR 0033 and the shader reads neither, so every surface reads as
coloured paint rather than as a material."*

That was literally true. `mesh.wgsl` computed `albedo * (light * N·L + ambient)` — Lambert diffuse and
nothing else. Two fields sat on every material, in every `.material` file, in the schema `describe`
reports, and no pixel had ever depended on either.

## Decision

**Cook-Torrance with GGX, Smith height-correlated visibility, and Schlick Fresnel** — the model glTF
2.0 specifies, which ADR 0033 already committed to by choosing the metallic-roughness parameterisation
"so that the importer maps onto it directly rather than through a translation nobody can predict the
losses of". There was no real choice to make here; the value is in having done it, not in which one.

Plus `metallic_roughness_texture`, packed glTF's way: **green is roughness, blue is metallic**, red
unused. Sampled values *multiply* the scalars, exactly as `base_colour_texture` multiplies
`base_colour`, so the scalars stay meaningful and an absent texture is the identity.

The one decision that took thought is below.

## The `PI`, and why existing scenes did not all go dark

An energy-correct BRDF puts a `1 / PI` on the diffuse term. Applied literally, every surface in the
engine would have dropped to about a third of its former brightness and every authored light
`intensity` in every scene would have needed retuning — for no visible benefit, since brightness is
relative.

So **`light_colour` is treated as carrying `PI` times the irradiance**: the light's units absorb the
constant. The final term is `(diffuse + specular) * light_colour * N·L * shadow * PI`, and the
`1 / PI` inside the diffuse cancels it exactly.

This is what most real-time renderers do, and the property that actually matters is preserved: the
**relative** weighting of diffuse against specular is untouched, and that is what decides whether a
material reads as stone or as plastic. Only the absolute scale is conventional, and it was already
conventional before.

## What this changed in the picture, honestly

**Very little, and that is the finding rather than a disappointment.**

Every material in the repository is a rough dielectric — `metallic 0.0`, roughness 0.6 to 0.9 — and a
rough dielectric is precisely the case where a full BRDF and plain Lambert nearly agree. `games/scarp`
is essentially unchanged. `games/atrium` picked up a faint sheen on its floor, because a broad surface
seen at a shallow angle has a rising Fresnel term, and reads very slightly flatter as a result.

One existing test had to change, and the reason is worth recording rather than hiding in a diff.
`a_mesh_actually_reaches_the_pixels` asserted that a red box's green and blue channels stayed below
60 — which encoded *"there is no specular"* as though it were a property of the renderer. A dielectric's
highlight is **white**, not tinted, which is what makes plastic look like plastic; at the default
roughness of 0.5, facing the light head-on, it measures about 0.15 in linear light and lifts green and
blue to roughly 111. The prediction and the measurement agreed to two decimal places, which is the
evidence that the implementation is right rather than merely different.

**Two things are now blocking the visual payoff, and both are already on the list:**

- **Metals are unusable until image-based lighting exists.** A metal has no diffuse at all — light
  reflects or is absorbed, nothing scatters back — so a metal lit only by the ambient constant is
  *black*. That is correct: a metal with nothing to reflect is black. What it should be reflecting is
  the sky, and there is no sky (**Q28**). `a_metal_is_black_under_ambient_because_there_is_no_sky_yet`
  pins this deliberately, so that closing Q28 **breaks that test** and forces it to be revisited.
- **A physically-correct highlight needs a tonemapper, and the default `Environment` is a no-op.** A
  near-mirror facing a light is genuinely a hundred times brighter than white. The HDR target
  (ADR 0034) carries it correctly and then the default look clips it, because ADR 0034 deliberately
  made the default byte-identical to no post-processing at all. That decision was right when nothing
  produced values above 1.0. PBR is what makes it produce them, and it is worth revisiting when
  sky lighting lands rather than on its own.

So: the sockets are wired and the wire is correct, and **the thing that makes it *look* like PBR is
the next item rather than this one.** ADR 0045 said image-based lighting was "probably the single
biggest step towards looking like a real engine". Building this makes that more true, not less.

## Consequences

**Good.**

- `metallic` and `roughness` mean something for the first time since ADR 0033 declared them.
- Imported glTF materials now shade the way the tool that authored them intended, parameters and
  texture packing alike, which is what ADR 0033's choice of model was for.
- The bind group work from ADR 0047 paid off immediately: a third texture was one binding, one shader
  line, and one entry in `MaterialTextures::ids` — no rework.
- Roughness is floored at 0.04. A perfectly smooth surface concentrates its entire highlight into a
  single point, which aliases into a flickering firefly rather than reading as polish.

**Bad, and accepted.**

- **The picture barely changed**, for the reasons above. This is scaffolding for the next two items
  rather than a visible win on its own.
- Metals are effectively unavailable to content until Q28 closes, and nothing stops someone authoring
  one and getting a black surface. The test names the limitation; nothing enforces it.
- The specular clips against the default look. Correct HDR, no tonemap — see above.
- `Material` gained a third texture slot and every `.material` file changed again, which is **Q32**
  becoming concrete rather than theoretical.

**Explicitly not decided here.** Whether the default `Environment` should tonemap; anything about
image-based lighting or a sky model; multiple lights; and transparency, which needs the sorting
ADR 0018 reserved.
