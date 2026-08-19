# ADR 0083 — Ambient occlusion is a baked channel and a screen-space pass, and it multiplies ambient only

**Status:** accepted
**Date:** session 22
**Supersedes:** nothing. Fills a hole ADR 0048 left open and ADR 0033 never named.

---

## Context

Engine gate review 12 ranked this first of thirteen findings, above every other visual defect it
measured, and its reason was arithmetic rather than taste: **nothing else on the list changes as many
pixels.**

The engine has no ambient occlusion of any kind. Grepped and confirmed: no SSAO, no GTAO, no HBAO, no
baked occlusion, and `Material` had no occlusion field. `mesh.wgsl` samples the metallic-roughness
texture and reads `packed.g` and `packed.b` while **discarding `packed.r`** — which in glTF 2.0's
packing, the packing this engine deliberately adopted so an imported material maps across without
translation, is the occlusion channel. `Material::metallic_roughness_texture`'s own documentation
said "red is unused".

The consequence is in every frame the project has ever produced, and it is why they read as
composited rather than rendered:

- a pillar meets the floor with **zero** darkening;
- a table leg meets the floor with zero darkening;
- a two-wall corner reads the same value as the flats either side of it;
- and a joint cut into a stone slab receives exactly as much ambient light as the face beside it,
  which is why generated stone reads as a picture of stone printed on a flat sheet.

Godot, Unity's URP and Unreal all ship ambient occlusion on by default or one checkbox away, and it
is consistently rated the highest value-for-cost graphics setting there is. Its absence is not a
missing luxury; it is the reason surfaces do not sit on each other.

## Decision

**Three parts. The third is the one that is a decision rather than an implementation.**

### 1. The red channel of the metallic-roughness texture is occlusion

glTF's packing, followed rather than invented, exactly as ADR 0033 followed it for the other two
channels. `Material::occlusion_strength` is glTF's `occlusionTexture.strength`: `mix(1.0, packed.r,
strength)`, so `0.0` ignores the map and `1.0` takes it as authored.

The placeholder texture is white, so a material naming no map samples red 1.0 and is unoccluded at
every strength. **That is what makes this free to add**: no existing content changes.

### 2. A screen-space pass supplies what a texture cannot

A baked channel darkens a joint *within* one surface. It knows nothing about the pillar standing on
the floor, because the two are separate meshes with separate textures and neither one's UV space
contains the other. Contact between objects needs a screen-space pass, and that pass is a separate
piece of work with its own cost and its own ordering constraints.

**Both are wanted and neither replaces the other**, which is the same division every comparable
engine makes: baked AO carries the fine, high-frequency, view-independent detail a screen-space pass
is too coarse to resolve, and the screen-space pass carries the large, dynamic, cross-object
occlusion no bake can anticipate.

### 3. Occlusion multiplies the **ambient** terms only — never the sun, never a lamp

This is the part that is a decision, and it is the one most often got wrong.

Ambient light is what arrives from the whole environment at once. A point down inside a joint sees
only a slice of sky, because the stone either side blocks the rest — so it genuinely receives less
ambient light, and scaling it is a physical statement.

A direct light is one direction. Either it reaches this point or the shadow map already said it does
not. **Darkening it again paints a second, softer, wrong shadow on top of the real one**, and the
result is a surface that looks dirty rather than shaped.

Both halves of ambient are occluded together — diffuse and specular. A recess that sees less sky also
*reflects* less of it, so occluding the diffuse half alone leaves a specular sheen sitting in the
bottom of a joint, which is worse than no occlusion at all because it puts the highlight in the one
place light cannot reach.

**The corollary is that ambient occlusion cannot be a post-process**, and that is worth stating
because a post-process is the cheap and tempting shape. A pass over the finished image has no way to
tell the sun's contribution from the sky's; it would darken direct light, emissive surfaces and the
sky itself. It is the shape that makes AO read as smeared grime.

## Consequences

- `Material` gains one field, and by ADR 0075 a file may omit it. All fourteen `.material` files in
  the repository name it, keeping the property session 21 established.
- The instance data does not grow: `surface` was `[metallic, roughness, normal_strength, unused]` and
  the fourth lane was there for exactly this.
- **A generator that writes this texture must fill red.** The channel was written as zero by
  `games/atrium`'s `surfaces.rs` and read by nothing, and the only thing standing between those two
  facts was that neither file had changed yet — reading red against a map full of zeroes would have
  turned every stone surface in the game black in ambient light. The generator fills it in the same
  commit, because either change alone is a defect.
- Occlusion is **presentation**, so ADR 0019 keeps it out of the state hash and ADR 0009's rule about
  services does not need invoking. No replay, snapshot or determinism test is affected.
- The screen-space half is authored on the **look** — `Environment::ambient_occlusion`, with
  intensity, radius in world units, and a self-occlusion bias. Zero intensity is off, and off means
  the graph declares no transients and no passes at all, so a 2D game pays nothing and every capture
  taken before this existed is byte-identical.
- **It costs a second pass over the geometry.** Occlusion has to be known while a surface is being
  shaded, and the view pass writes depth and shades at once — so depth must be laid down first, on
  its own. That is a real cost and it is **not yet measured**; `docs/13` item 19 is where the
  numbers go, and no claim about the cost should be made until they exist.
- The prepass is single-sampled where the scene's depth is 4×, because a multisampled depth texture
  cannot be sampled without a resolve and wgpu has none. Occlusion is smeared over half a metre, so
  there is nothing at the sub-pixel scale for anti-aliasing it to say.
- **Radius is in world units, not pixels**, so a room does not change shape when somebody resizes
  the window.
- The occlusion map is bound on **group 0**, the per-view group. Four bind groups is the whole
  budget and all four were already spoken for, so there was no fifth to put it in.

## Alternatives rejected

**Infer occlusion from the normal map's cavity.** A normal map's gradient does encode where the
recesses are, and deriving occlusion from it would need no new channel. Rejected on ADR 0077's
precedent: this would be a derivation standing in for a decision, the pattern `docs/07` now records
several instances of. It also cannot express occlusion that is not relief — a painted-on soot line, a
bake from a high-poly model — and it would tie two dials together so that flattening the relief would
silently brighten the recesses.

**A separate occlusion texture rather than a channel.** Cleaner to author by hand, and one more
texture binding, one more upload and one more cache entry per material. glTF allows both and ships
the packed form overwhelmingly, and the packed form is what every exporter writes. Nothing here needs
the separation.

**Screen-space only, no baked channel.** Would have avoided a material field. It also throws away the
fine detail a 512² map resolves and a half-resolution screen-space pass does not, and it leaves the
red channel of every imported glTF material silently discarded — which is the state this ADR exists
to end.

**HBAO or GTAO for the screen-space half.** Both are better and both cost several times the samples.
The Alchemy/SAO family reconstructs its normals from depth, so it needs no normal buffer and no
second prepass target, and it is what a forward renderer can afford beside a shadow pass it is
already paying for. `RenderBackend` isolates the whole thing, so upgrading is a local change.

**A bilateral (depth-weighted) blur.** Would stop occlusion bleeding across a silhouette, at the cost
of reading depth again per tap and a kernel that stops being separable. The bleed is a few pixels of
an already soft quantity multiplied into ambient light alone. Revisit if a silhouette against a
bright background shows a dark fringe.

---

## What building it cost, kept because two of the three were silent

**The graph rejected the frame and the symptom was a black screen.** The prepass declared its target
through `with_depth` and not in `writes`, and `compile` builds its writer table from `writes` alone —
so the occlusion pass read something no pass wrote, `NeverWritten` came back, and `render` returned an
error. A rejected graph draws *nothing*, so a 1920 × 1080 capture came back as pure black with no
other clue. The shadow pass had already solved this by declaring its map in `writes` and binding it as
depth; the fix was to copy it.

**The estimator was wrong by about a factor of four and looked like a wiring failure.** Normalising by
`radius` on dimensional grounds throttled the whole effect to a few per cent, and the first Atrium
capture with occlusion on probed **byte-identical** to the one without. Three debug renders separated
the possibilities — the prepass depth was right, the reconstructed normals were right, and the
estimator was quietly returning almost exactly one. `docs/07`'s rule held: when a capture disagrees
with your model of the code, measure the intermediate rather than reason about the whole.

**A derivative cannot sit behind an early return.** `dpdx` asks what the neighbouring pixels in the
same quad hold, so it is defined only where the whole quad reaches it. Both early returns the shader
started with had to go — one moved below the derivative, one removed entirely in favour of letting
intensity multiply through. FXC rejects this outright, which is the compiler Windows CI uses and the
reason `CLAUDE.md` §4b has a fifth check.

