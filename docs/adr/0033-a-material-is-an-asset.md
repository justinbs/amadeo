# ADR 0033 — A material is an asset, and shaders are text with a preprocessor

**Status:** Accepted · **Date:** 2026-08-04 · **Decides:** `docs/04-subsystems.md` §4's material and
shader model · **Builds on:** ADR 0020, ADR 0023, ADR 0029, ADR 0032

## Context

`docs/04` §4 calls this "the next expensive decision here" and asks a question about shaders:
hand-written WGSL, a material graph, or a preprocessor with includes and variants? It also sets the
deadline — *decide before the second shader family, not the twentieth*. There are two shaders today
(`quad.wgsl`, `sprite.wgsl`) and PBR is what makes it a family, so the deadline is now.

**The question is aimed at the cheap half, and that is a pattern rather than an accident.** ADR 0018
found it for transforms, ADR 0031 found it again for cameras: `RenderBackend` isolates rendering
*mechanism* so completely that no scene file, no schema and no state hash can observe which was
chosen. The expensive decision is always the **data** sitting next to it.

Here that data is a material. It is reflected, it appears in scene files, and it is in every state
hash and every snapshot — so where it lives is the thing that is hard to undo.

**This was blocked until this session.** The natural inline shape,
`Material { base_colour, metallic, texture }` nested under a mesh, was unrepresentable until ADR 0032
gave the scene format nested values. Deciding against a format that could not hold the alternative
would have prejudged the answer.

## Decision

### 1. A material is an **asset**, named by an id

A `.material` file with a declared id, exactly as a prefab is under ADR 0029 and a texture is under
ADR 0020. The mesh component holds the id:

```text
entity w1 "Wall" from wall_tile
  override Mesh
    material "stone_rough"
```

Three arguments, in order of weight.

**A material is shared by construction.** The Vault's forty-four wall tiles use one. As inline
component data that is forty-four copies of a struct in every state hash and every snapshot, and one
palette change means editing forty-four entities. As an id it is forty-four short strings and one
file. This is the same argument that made prefabs assets, and it is stronger here — a prefab is a
template you *instance*, a material is a value you genuinely *share*.

**ADR 0023's batching rule extends to it, and only ids make that cheap.** Sprites batch by
`(sort order, texture)`; meshes will batch by `(sort order, material)`. That rule was chosen against
a measurement — 20,000 sprites collapse to 32 batches — and comparing an id is a string compare where
comparing a material struct is a deep compare of colours, factors and texture names, on the path the
whole batcher exists to keep cheap.

**The entire asset toolchain applies for nothing.** `amadeo check` validates the reference and offers
"did you mean" on a typo, ADR 0021's barrier makes it resident before the first tick, `amadeo assets`
lists it, `amadeo import` gives it a sidecar, and moving the file breaks nothing. None of that has to
be built.

Godot reaches the same answer — materials are resources with their own files.

### 2. The material file is a scene file

No new format. A `.material` is a scene document with a single root carrying one `Material` component,
which is what a prefab already is (ADR 0029). So the parser, the canonical writer, `amadeo fmt`,
`amadeo check` and the nested values of ADR 0032 all work on it the day it exists.

That also means a material's *fields* are ordinary reflected data: the schema knows them, `describe`
reports them, and `describe.example` can emit a valid one.

### 3. Shaders are hand-written WGSL, with a preprocessor and no graph

**Decided alone and flagged**, per `CLAUDE.md` §5's rule for genuinely cheap-to-change internals:
`RenderBackend` isolates this, so nothing outside the wgpu backend can observe it.

- **`#include`**, so `pbr.wgsl` and `sprite.wgsl` can share a lighting function rather than copying
  one. Copying is how two shaders silently disagree about the same maths.
- **`#ifdef` and shader defines**, so one shader source covers "with a normal map" and "without"
  rather than two files that drift.
- **A pipeline cache keyed by the set of defines**, so a variant is compiled once.

This is Bevy's shape, arrived at after they hit the variant problem for real: conditional compilation
plus a specialisation key, rather than a pipeline per combination.

**A material graph is rejected outright.** It is an editor-sized project — node types, serialisation,
a code generator, a UI — before the first triangle renders. It is what a team with a tools programmer
builds, and this project has one person who is learning Rust. If it is ever wanted it is additive: a
graph that *emits* WGSL sits on top of this rather than replacing it.

### 4. What a `Material` holds is deliberately not decided here

Its fields depend on the PBR model, which does not exist. This ADR decides **where a material lives
and how shaders are organised**; the field list arrives with mesh rendering and needs no ADR, because
adding a field to a reflected type is exactly the cheap change the schema is for.

## Consequences

**Good:**

- Repeated material assignment costs an id, and one file changes every surface using it.
- Nothing new to build for validation, loading, listing, or formatting.
- Batching by `(sort order, material)` stays as cheap as ADR 0023 measured.
- A material can be authored, checked and diffed before any renderer reads it — the same property
  that let `vault.scene` be written before the sprite path worked.

**Bad, and accepted:**

- **A one-off material needs its own file.** Godot answers this with inline sub-resources; this does
  not. The cost is real for a single tinted object, and the answer is a small file with a clear name.
  If it becomes a genuine irritation, the fix is ADR 0029's override shape — an id plus an inline
  patch — which is already designed and tested and can be added without changing what exists.
- **A material shares one id namespace with every other asset**, which is ADR 0029's recorded
  papercut recurring exactly as predicted. `stone` the texture and `stone` the material collide.
- **Shader defines are a real tarpit if used carelessly.** The rule is that a define exists because a
  material *field* implies it, so the variant count is bounded by the material model rather than by
  imagination.

## What was rejected

- **A `Material` component with inline fields.** Everything about an entity in one place, no
  indirection, no second file — genuinely nicer to read for a small game. Rejected on the three
  arguments above, of which the state-hash one is decisive: sharing by copying puts every copy in
  every snapshot.
- **Both, from the start** — an id with an inline override. Strictly more capable, and the semantics
  are already designed. Rejected as paying ADR 0029's complexity cost before anything has asked for
  it; prefabs earned it by needing instancing, and materials have not earned it yet.
- **Deferring until meshes land.** ADR 0011 and ADR 0023 were both settled well by waiting for
  something real to measure, so this had a real case. Rejected because the data choice shapes the
  scene format, and building meshes against a placeholder means authoring content twice — and because
  §4's deadline is the second shader family, which is the next thing to be written.
- **A material graph.** See 3.
