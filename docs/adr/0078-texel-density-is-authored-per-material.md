# ADR 0078 — Texel density is authored per material

**Status:** Accepted · **Date:** 2026-08-18 · **Builds on:** ADR 0033, ADR 0075 ·
**Implements:** item 10 of `docs/13-the-engine-gate.md`

## Context

The engine had no texture-density control of any kind. `mesh.wgsl` read `out.uv = vertex.uv;`, and
every mesh producer emits UVs running 0 to 1 across a face — a `BoxMesh` per face, an `ArchMesh`
normalised over its perimeter and length. So one image stretched across a 12 m wall and across a
0.4 m crate at a thirty-fold difference in density.

The first engine review flagged this as a blocker for the texture work rather than a nicety, and it
was right for a reason worth stating plainly: **attaching textures without it makes the games look
worse than having none.** A wall whose stone is thirty times the size of a crate's does not read as
art, it reads as a bug. Every art pipeline in the industry has a texel-density standard for exactly
this; Unity exposes it as `_MainTex_ST`, Unreal as U/V tiling on a `TexCoord` node.

`games/scarp` had already hit it and worked around it privately, baking world-scaled UVs into its
terrain generator's vertices behind a `TEXTURE_TILE` constant — in one game, in a code path no
material sampled.

## Decision

### 1. A material carries `uv_scale`, and it multiplies the mesh's own UVs

`Material::uv_scale` is a `[f32; 2]` defaulting to `[1.0, 1.0]`, multiplied into the interpolated
coordinate in the vertex shader.

**Per material rather than per mesh**, because density is a property of the *surface* rather than of
the geometry: the same `BoxMesh` is a wall in one place and a crate in another, and it is the stone
that has a size. It also means the mesh producers stay unchanged, which keeps ADR 0035's promise that
a new shape is one branch in a loader.

ADR 0075's declared defaults make it free: `[1, 1]` is the identity, so **no existing `.material`
changed** and every capture stayed byte-identical.

### 2. It rides in the instance buffer as its own field

`GpuMeshInstance` gains a `uv_scale: [f32; 4]` at vertex attribute location 11, `xy` used.

Two spare lanes already existed — `emissive.w` and `surface.w` — and packing the two halves of one
number into two unrelated fields would have cost nothing and been exactly the kind of cleverness
`CLAUDE.md` §6 rules out. Sixteen bytes per instance is not the constraint here.

## Consequences

- **The second tracked measurement in `docs/13` moved.** Material texture slots that are `""` went
  from **36 of 36** to **43 of 45** — `games/atrium`'s floor and plinth now sample a real image. That
  is the first textured pixel ever drawn in a game in this engine: mipmaps in linear light, 16×
  anisotropic filtering and the whole sampler path have been written and tested since session 14 and
  had never reached a picture.
- **`games/atrium` generates its own texture**, `--bin surfaces`, which is the fourth instance of the
  `pix`/`tone`/`turf`/`gloom` pattern and is `docs/12-the-bar.md` §3 in practice: Claude authored the
  game's texture rather than asking for one.
- **The demonstration is the point and it is a slab pattern for a reason.** A 20 m floor at
  `uv_scale 5` and a 3 m plinth at `uv_scale 0.75` both land on 2 m slabs. Noise at two densities
  looks like noise; a grid is either obviously right or obviously wrong at a glance, which is what
  makes the failure this ADR prevents visible at all.
- **The tiling-noise routine now exists twice**, in `games/scarp`'s `turf` and `games/atrium`'s
  `surfaces`. That is deliberate and it is the moment *before* promotion: `amadeo_noise` is
  non-periodic by design, because a world wants a function over the whole plane and a tile wants a
  wrap. A third user is when it should move into the engine.
- **`games/scarp`'s `TEXTURE_TILE` workaround is now redundant** and should go when that game's
  terrain next gets a material that samples anything.
- **Non-uniform scale on a part still has no answer**, and this does not change that. `uv_scale`
  scales coordinates, not geometry; the inverse-transpose normal rule ADR 0074 kept out of the format
  stays out.

---

## Amendment, same session — §3: the producers emit UVs in metres

**The first version of this ADR fixed the half of texel density that lives in the material and left
the half that lives in the mesh.** The engine gate's sixth review found it, and the evidence was in the
two files the ADR itself had produced: `paving.material` and `plinth_stone.material` were
byte-identical apart from one number.

**Two materials, for one stone, because the objects were different sizes.** That is the failure this
ADR exists to prevent, moved up one level — a game with fifty differently-sized props made of the same
stone would have needed fifty materials.

And it was wrong *within* a single object, by arithmetic rather than by eye. `BoxMesh` emitted UVs
running 0..1 across **every** face regardless of that face's dimensions, so the Atrium's 3 × 1 × 3
plinth carried 2.0 m square slabs on its top and slabs **2.0 m wide by 0.67 m tall** on its sides.

### The missing half was never in the material

In a DCC, "texel density" means the mesh's UVs are already proportional to surface area, and Unity's
`_MainTex_ST` and Unreal's `TexCoord` tiling multiply *that*. Amadeo's procedural producers emitted
0..1 per face, which is the one convention under which a material-level multiplier cannot work.

So **`BoxMesh`, `PlaneMesh`, `WedgeMesh`, `StairMesh` and `CylinderMesh` emit UVs in mesh-local
metres**, and `uv_scale` is a **repeats-per-metre** figure. One material now covers a wall and a crate
at the same stone size, and a non-square face is right for free.

- **`StairMesh` gets it without being touched**, because it composes `BoxMesh` blocks.
- **A wedge's slope is measured along its incline**, not its footprint, or a steep ramp compresses its
  texture in exactly the direction the eye notices.
- **A cylinder's side is developable** — you can unroll it — so arc length is a real distance rather
  than an analogy, which is why it joins the flat producers. It uses the mean radius, so a frustum
  gets one consistent circumference instead of a texture that slides as the radius changes.
- **`ArchMesh` and `SphereMesh` are not converted.** An arch is developable and should follow; a
  sphere is doubly curved and has no distortion-free mapping at all, which is a separate decision.
  Neither is sampled by anything today.
- **`GltfPart` is deliberately untouched.** An imported mesh carries the UVs its DCC authored, which is
  the same split Unity and Unreal live with.

**It was cheapest to change now and would never have been cheaper**: two materials in the repository
sampled a texture. After a texture generator it is every material in every game, plus every capture
that samples one.
