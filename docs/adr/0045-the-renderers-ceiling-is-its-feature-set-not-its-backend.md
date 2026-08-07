# ADR 0045 — The renderer's ceiling is its feature set, not its backend

**Status:** Accepted · **Date:** 2026-08-07 · **Builds on:** ADR 0002, ADR 0026, ADR 0033, ADR 0034,
ADR 0035, ADR 0038

## Context

`games/scarp` runs, and it looks like a prototype. Justin raised this directly and precisely: the
shading, textures and lighting are *"so basic right now, like really basic"*, this is nowhere near
what a first 3D demo should look like, and — the part that needs an answer rather than reassurance —
**is the backend the reason?**

The premise behind the question is a reasonable one and worth stating fairly:

> wgpu is a wrapper over Vulkan, Metal and DX12. Surely the real graphical gains are underneath it,
> and surely a serious engine ends up writing against those directly.

ADR 0002 chose wgpu, for determinism, for one API across platforms, and for a free browser path. It
never addressed whether that choice **caps how good the picture can get**, or what would make us
revisit it. This ADR answers both, because the answer decides where years of renderer effort go.

## What the research found

### 1. wgpu's native feature set is, in practice, Vulkan's

Running natively, wgpu exposes far more than the WebGPU baseline, behind explicit feature flags. The
list is the modern GPU-driven-renderer toolkit, essentially item for item:

| Capability | wgpu feature |
|---|---|
| Bindless textures and buffers | `TEXTURE_BINDING_ARRAY`, `BUFFER_BINDING_ARRAY`, `STORAGE_RESOURCE_BINDING_ARRAY` |
| Partially-bound / non-uniform indexing | `PARTIALLY_BOUND_BINDING_ARRAY`, `SAMPLED_TEXTURE_AND_STORAGE_BUFFER_ARRAY_NON_UNIFORM_INDEXING` |
| GPU-driven draw submission | `MULTI_DRAW_INDIRECT_COUNT`, `INDIRECT_FIRST_INSTANCE` |
| Hardware ray tracing | `EXPERIMENTAL_RAY_QUERY`, `EXPERIMENTAL_RAY_TRACING_PIPELINES` |
| Mesh shaders | `EXPERIMENTAL_MESH_SHADER` |
| Wave/subgroup intrinsics | `SUBGROUP`, `SUBGROUP_VERTEX`, `SUBGROUP_BARRIER` |
| 64-bit atomics | `SHADER_INT64_ATOMIC_ALL_OPS`, `TEXTURE_INT64_ATOMIC` |
| Compressed textures | `TEXTURE_COMPRESSION_BC`, `ETC2`, `ASTC` (incl. HDR) |

Not everything is stable — ray tracing and mesh shaders are explicitly experimental — but the
direction is settled and the gap is *timing*, not *architecture*. Nothing on the list of things that
would make Amadeo look good is missing.

### 2. Nothing on the "looks basic" list is a backend limitation

This is the part that actually answers the question. Here is what Amadeo's renderer has, against what
a shipping stylised-realistic renderer has:

**Has:** Lambert diffuse, one directional light, one orthogonal shadow map, a constant ambient term,
a base-colour texture, and a post chain of exposure/tonemap/grade/vignette.

**Does not have:** mipmaps, anisotropic filtering, normal mapping, a real BRDF, image-based or sky
lighting, shadow cascades, more than one light, point or spot lights, ambient occlusion, fog,
transparency, anti-aliasing, bloom (declared, not drawn), particles, decals, LOD, or culling.

**Every single item in the second list is implementable on wgpu today, on the stable feature set.**
Not one of them is waiting on an API. The picture is basic because the renderer is six features deep
and a shipping renderer is forty.

### 3. And a wgpu game already looks better than this by a wide margin

**Tiny Glade** (Pounce Light, released September 2024) is built in Rust on Bevy and wgpu, and its
visuals are its selling point — it is widely praised specifically for how it looks. Veloren has
shipped a full open-world game on wgpu across Vulkan, Metal and DX12 backends.

So the claim "you cannot make it look good on wgpu" is refuted by existing product rather than by
argument. What separates Tiny Glade from `games/scarp` is a decade-equivalent of renderer features,
not an API.

### 4. What going native would actually cost, and buy

**Buy:** access to experimental features a few months earlier than wgpu wraps them; some CPU-side
driver overhead, which the frame budget says is not currently anywhere near the bottleneck
(`docs/10`: 125 µs of CPU frame preparation on a 16.67 ms budget); and vendor-specific extensions.

**Cost:** three backends to write and maintain instead of one — Vulkan, Metal and DX12 are not
interchangeable and Metal in particular is a different model. Losing the browser target entirely,
which ADR 0002 counted as a free future milestone. And it is a very large amount of code for **one
person to maintain**, which `CLAUDE.md`'s legibility requirement treats as a hard constraint rather
than a preference.

There is exactly **one** honest argument for a native backend, and it is not visual quality: consoles.
wgpu has no PS5 or Switch backend and will not have one. If Amadeo ever targets a console, that is a
native port or a middleware layer, and it is a business decision rather than a graphics one.

## Decision

### 1. wgpu stays, and the reason is that it is not the constraint

The renderer's quality ceiling is set by **the features built on top of it**. All effort goes there.
Writing a Vulkan backend now would consume years and change the picture by approximately nothing.

### 2. `RenderBackend` remains the seam, and it is expected to change shape

ADR 0002's structure already makes this reversible: `RenderBackend` is a trait with `NullBackend` and
`WgpuBackend` behind it, and the render graph (ADR 0034) is API-agnostic — it knows nothing about
wgpu, which is why `NullBackend` compiles the same graph.

**But the trait's current granularity is honest about being early.** `upload_texture`, `upload_mesh`
and `render(&FrameData)` describe a renderer that hands over a list of things to draw. A GPU-driven
renderer — bindless, indirect, culling on the GPU — wants a different shape, and the trait will have
to grow toward it. That is expected work, not a design failure, and it is the reason the seam exists.

### 3. The order features land in is decided here, by visual return rather than by interest

**Tier 1 — the difference between "prototype" and "a game".** Roughly in this order:

1. **Mipmaps and anisotropic filtering.** Without mip levels every texture shimmers at distance,
   which is why terrain currently tiles at a coarse 8 m. This is the cheapest large win available and
   it gates how good texturing is allowed to look.
2. **Normal mapping.** The largest perceived-detail gain per unit of cost in all of real-time
   graphics. Needs tangents, which `Vertex`'s documentation already anticipates generating at load.
3. **A real BRDF — metallic-roughness PBR.** `Material` has carried `metallic` and `roughness` since
   ADR 0033 and the shader reads neither. This is what makes a surface read as *stone* or *metal*
   rather than as coloured paint.
4. **Sky and image-based lighting.** Replaces the hardcoded `0.12` ambient (**Q28**). Probably the
   single biggest step towards looking like a real engine: surfaces pick up sky colour and bounce
   instead of being flat-filled, and it is what makes shadowed areas read as shade rather than as
   holes.
5. **Shadow cascades.** ADR 0038 reserved the enum variant for exactly this. One map over 70 m of
   outdoor scene is visibly blocky, and cascades are the standard fix.

**Tier 2 — needed before a scene can hold real content.** More than one light, and point/spot lights.
Frustum culling and LOD (already M2.5 gates 3 and Q25). Anti-aliasing — jagged edges read as amateur
faster than almost anything else. Transparency with correct sorting. Fog and aerial perspective,
which is cheap and does an enormous amount for outdoor depth.

**Tier 3 — polish and scale.** Bloom (declared in `Environment`, not drawn). Screen-space or
ground-truth ambient occlusion. Decals and particles. GPU-driven submission and bindless — which is
about *scale*, not looks, and is where wgpu's native features above start to matter.

### 4. What would make us revisit this

Named now so the answer is a measurement rather than a mood:

- **A console target.** The one real reason, and not a graphics decision.
- **A required technique wgpu does not expose and has not scheduled.** Tier 1 and Tier 2 contain no
  such thing; check again at Tier 3.
- **Measured driver overhead becoming the frame's bottleneck**, with GPU timestamp queries in place
  (M2.5 gate 4) rather than by impression.

## Consequences

**Good.**

- The direction is written down and the reassurance is evidence rather than assertion: a wgpu game
  already looks far better than this, so the API is demonstrably not the limit.
- Renderer effort has an order, chosen by what changes the picture most, so progress is visible
  rather than diffuse.
- One backend for one maintainer, and the browser milestone stays free.

**Bad, and accepted.**

- **Amadeo will look underwhelming for a while yet, and that is a real cost rather than a
  misunderstanding.** Tier 1 is five substantial pieces of work. Nothing about this ADR makes the
  current picture acceptable for shipping; it says the road there does not go through Vulkan.
- Experimental features arrive months later than they would natively. Ray tracing and mesh shaders
  are the current examples, and neither is on Tier 1 or Tier 2.
- Consoles remain closed until a separate decision opens them.

**Explicitly not decided here.** *When* Tier 1 happens relative to the remaining milestones. M2.5's
gates are about worlds that scale, not about how they look, and M3 is where game feel — which
includes looking like a game — is the stated goal.
