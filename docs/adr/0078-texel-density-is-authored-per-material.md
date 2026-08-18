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
