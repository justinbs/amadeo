# ADR 0047 — Tangents come from the file, or are generated at load

**Status:** Accepted · **Date:** 2026-08-10 · **Builds on:** ADR 0026, ADR 0033, ADR 0035, ADR 0039,
ADR 0045

## Context

ADR 0045 ordered M3's renderer work by visual return and put **normal mapping** second, after mipmaps:
*"the largest perceived-detail gain per unit of cost in all of real-time graphics."* This is that item.

A normal map is an image that stores, per pixel, which way a surface leans — as though the flat
triangle were finely bumpy. Lighting that per-pixel direction instead of the triangle's own is what
puts grooves between bricks and grain in wood with no extra geometry. Every 3D surface in Amadeo
currently shades from one flat normal per triangle corner, which is a large part of why `games/scarp`
and `games/atrium` read as prototypes.

Three things had to be decided, and only the first is genuinely hard to reverse.

## 1. Where tangents come from

A normal map's directions are stored **in tangent space** — relative to the surface, not the world.
That is what lets one image tile across a curved wall: "lean left" means the same thing everywhere on
the surface. Converting that to a world direction needs to know which way "left" points at each
vertex, which the normal alone does not say: the normal fixes which way is *out* and leaves the
surface free to spin around it. The missing piece is the **tangent**.

`Vertex`'s documentation has said since ADR 0035 that tangents would be "generated at load from the
UVs, which is what glTF itself permits for a model that omits them". That was the right instinct and
it is half an answer, because it does not say *which* generation algorithm — and the algorithms
disagree.

### What the research found

The industry standard is **MikkTSpace** (Morten Mikkelsen). It matters for a specific reason: a normal
map is *baked* against a particular tangent frame, and if the renderer computes a different one, every
bump is lit slightly wrong. The glTF 2.0 specification says that when tangents are absent, clients
*should* compute them "using default MikkTSpace algorithms".

The reference implementation is roughly 1900 lines of dense C. Bevy vendors a Rust port as
`bevy_mikktspace` — and has since added a **second, faster Gram-Schmidt path** alongside it, because
the real algorithm is slow enough to be a reported problem
([bevyengine/bevy#17834](https://github.com/bevyengine/bevy/issues/17834)). So even the engine that
took the dependency now offers a way around it.

### The thing that dissolves the trade-off

**glTF can carry `TANGENT` as a vertex attribute, and `amadeo-gltf` was throwing it away.**

Blender and Substance can export the tangent frame they baked against. So the case where MikkTSpace
correctness actually matters — imported art with a baked normal map — is a case where the *file
already has the right answer* and the engine only had to read it. Computing a frame in that case is
choosing to guess when the answer was supplied.

That leaves generation as a fallback for geometry with no file to ask: `BoxMesh`, `PlaneMesh`, and
terrain. Their UVs are flat and axis-aligned, and on flat axis-aligned UVs the two algorithms agree
exactly.

### Decision

**Read `TANGENT` from glTF when it is present. Generate a Gram-Schmidt frame at load when it is not.
Take no `mikktspace` dependency.**

Generation is all-or-nothing per primitive rather than per vertex: a tangent frame is only consistent
across a surface if one method produced the whole of it, and mixing two puts a visible lighting seam
where they meet.

`Vertex` grows a fourth attribute, `tangent: [f32; 4]` — `xyz` direction plus a `w` handedness sign,
which is glTF's own encoding so an imported tangent maps straight across. The sign is what mirrored
UVs need: mirroring a texture across a character's centre line flips handedness on one side, and a
mesh that could not express that would light half a face inside out.

This does change the **fixed vertex layout** ADR 0035 §3 pinned. That is the reversible part of an
otherwise settled decision, and the cost is 16 bytes per vertex on data currently measured in
hundreds of kilobytes.

### What would reverse this

Baked art showing visible artefacts *that its exporter did not write tangents for*. The first fix is
to turn on tangent export, not to change the engine. If some pipeline genuinely cannot, taking the
dependency is an additive change: meshes are regenerated at load every run, nothing is baked into the
repository, and no asset would need re-authoring.

## 2. A normal map is not colour, and the sidecar is what says so

sRGB is a **perceptual curve**: art files spend more of their 256 steps where the eye can tell them
apart. A normal map's bytes are not light at all — they are a direction packed into `0..1`. Decoding
one through the sRGB curve bends every direction it stores, and the surface is lit as though its bumps
face somewhere they do not. Subtly and pervasively wrong, with no error anywhere.

A `.png` holding a normal map is byte-for-byte indistinguishable from one holding colour, so the
decoder cannot tell. Something has to declare it.

**Decision: the asset's `.ama-meta` sidecar declares it**, with `color_space = "linear"`, and
`PixelFormat::Rgba8Unorm` carries it from there. This is exactly what ADR 0026 anticipated — that
variant's documentation named this case and this mechanism before either existed — and the sidecar's
`settings` map already held free-form keys, so **no file format changed**.

The alternative was to infer it from the slot: anything bound as `normal_texture` is data, whatever
its sidecar says. Rejected because it does not generalise — roughness, occlusion and mask maps are all
coming and all linear — and because `TextureCache` is keyed by id, so an id used in two slots would
need two decodes of one image.

The cost is real and is not yet paid: **a sidecar that forgets the line is silently wrong.** Unity and
Godot both solve this with an importer warning, which is the right answer here too and needs a
diagnostics path from a material to the asset it names. Recorded as **Q31**.

## 3. Group 2 became a combination rather than a texture

The mesh shader's bind group 2 held one texture and one sampler, made once per texture at upload. A
material now names two images, and PBR will bring more.

**Decision: one bind group per *combination* of a material's textures, cached on the combination and
built before the pass opens.**

The obvious alternative — a fourth bind group for the normal map — works today and is a dead end
tomorrow: wgpu guarantees only four bind groups, and 0, 1 and 2 are already the view, the shadow map
and this. Metallic-roughness would have nowhere to go.

Two consequences worth knowing. Uploading a texture now **drops every cached combination naming it**,
because a bind group holds the view it was built from; without that, a surface drawn while its texture
was still a placeholder would keep the placeholder forever. And the cache is built up-front rather
than lazily in the draw loop, because creating one needs `&mut self` while a render pass holds the
encoder borrowed — which is a borrow-checker fact that happens to also be the faster arrangement.

## Consequences

**Good.**

- Imported art gets the tangent frame it was baked against, without 1900 lines of ported C and without
  a third non-`thiserror` dependency. `CLAUDE.md`'s legibility requirement did real work here.
- Every existing mesh producer gets tangents for free, through one shared generator with one set of
  tests — rather than four implementations that can disagree.
- Bind group 2 is now shaped for the material model rather than for one texture, so metallic-roughness
  is a binding and a shader line rather than a rework.
- `PixelFormat` earned its keep. ADR 0026 put the tag in "before it is needed"; the payment came due
  three milestones later and cost one variant and one match arm.

**Bad, and accepted.**

- **A sidecar missing `color_space = "linear"` is silently wrong** — Q31, above. This is the sharpest
  edge the feature ships with.
- **Terrain gets an approximate frame, not a correct one.** Its UVs are a planar projection from world
  x/z, so a vertical face has zero UV area and no solution for where `u` points; those vertices fall
  back to an arbitrary axis in the surface. Valid rather than `NaN`, and wrong rather than right. The
  real fix for this and for the UV stretching that shares its cause is **triplanar mapping**, which
  derives its own frame per axis and needs no vertex tangents at all.
- Every field added to `Material` rewrites every `.material` file, because reflection requires all
  fields. Five files this time; PBR will do it again. Worth its own decision eventually — see **Q32**.
- The vertex layout ADR 0035 called fixed has changed once. It is still fixed; it is just fixed at
  four attributes now.

**Explicitly not decided here.** Whether a `Material` chooses its own sampler or wrap mode; whether
normal maps get triplanar treatment on terrain; and how many texture slots a material ends up with.
