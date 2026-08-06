# ADR 0039 — glTF: geometry stays art, the scene graph becomes text

**Status:** Accepted · **Date:** 2026-08-06 · **Builds on:** ADR 0018, ADR 0020, ADR 0021, ADR 0026,
ADR 0033, ADR 0035

## Context

M2's exit gate 1 names "an imported glTF level", and it is the last unbuilt part of that gate.

**ADR 0035 deliberately deferred this decision** — it said the glTF importer "is a producer of
`MeshData` and nothing above it changes when it lands, so it needs no decision now, which is the
property this ADR exists to buy." That property held: everything below is additive.

But "import a glTF" is ambiguous in a way that matters here, because a glTF file is **not a model**.
It is a whole scene: a node hierarchy with transforms, meshes, materials, textures, sometimes
animation and skins. So the real question is not *how* to read one. It is **which parts of a glTF
become engine text, and which stay art** — because invariant I1 says text files are the only source
of truth.

Half of that is already settled by precedent. A `.png` is opaque bytes and ADR 0026 accepted it,
because a PNG is *source art* made in another tool rather than authored engine data. A `.glb` made in
Blender is the same kind of thing.

## What the research found

**The `gltf` crate (gltf-rs) is the mature Rust answer** and exposes exactly the three things this
needs: mesh geometry, materials, and the node hierarchy with transforms. It is what Bevy builds on.

**Godot converts an imported glTF into its own *scene* format**, preserving the node hierarchy, while
geometry stays a resource rather than becoming text. Unity and Unreal do the equivalent. Nobody
serialises vertex arrays into their human-editable format, and nobody leaves the hierarchy locked
inside the interchange file either.

**ADR 0035 had already noted the thing that makes a runtime path viable at all**: glTF was designed
for runtime delivery, its buffers are laid out to be copied straight into GPU buffers, and Bevy loads
it directly. So "read the geometry at load time" is genuinely available here in a way that "load FBX
at runtime" never is.

One fact from the codebase decided the rest. **ADR 0035 promised a `.mesh` file could hold "either a
procedural shape or vertex data", and the vertex-data half was never built** — `MeshData` is not
reflected, so only `BoxMesh` and `PlaneMesh` load. Any option that writes geometry as text has to
build that first, and then live with it.

## Decision

### 1. An import produces text for the layout and leaves the geometry alone

`amadeo import-gltf level.glb` writes:

- a **`.scene`** file carrying the node hierarchy as nested entities;
- a **`.material`** file per glTF material;
- a **`.mesh`** file per glTF **primitive**, each a short pointer into the source file;
- a **sidecar** for the `.glb` itself, so the asset catalogue can find it (ADR 0020).

The `.glb` stays in the project as source art, beside the PNGs.

**This is I1 satisfied where I1 actually bites.** What a person or an agent authors is layout and
materials — where the wall goes, what colour it is, what is parented to what. Nobody hand-edits
vertex positions. A twenty-thousand-triangle model written out as text is megabytes of numbers:
diffable in principle, unreadable in practice, and a permanent weight on the repository.

### 2. A **primitive** is what corresponds to an Amadeo mesh, not a glTF *mesh*

A glTF mesh holds one primitive per material; an Amadeo `Mesh` draws one thing with one material.
Getting this backwards silently loses every material but the first, which looks like an art problem
rather than an importer bug.

A node whose mesh has several primitives becomes one entity plus a child per extra primitive, at the
identity transform.

### 3. The indirection is a `GltfPart` component in a `.mesh` file, not a field on `Mesh`

Something has to say *which* piece of a file a mesh id refers to. Three ways were available:

- **A compound id string** (`"level#3"`). Rejected: it hides structure inside a name, which is the
  exact defect ADR 0030 called out when a fixed array's length existed only inside its type name.
- **A new field on `Mesh`**. Rejected: `Mesh` is authored in every existing scene file, and all of
  them would have had to grow a field that means nothing to a procedural shape.
- **A component in the `.mesh` asset**, which is what this takes. A `.mesh` file already *is* the
  indirection from a name to a shape; it carries a `GltfPart` instead of a `BoxMesh`. Mesh ids stay
  flat, `Mesh` is untouched, and the mapping is a four-line text file anyone can read.

This makes `GltfPart` **the third producer of `MeshData`**, alongside `BoxMesh` and `PlaneMesh` —
exactly the additive shape ADR 0035 predicted.

### 4. The parser lives in its own crate, `amadeo-gltf`, with no engine dependencies

The same precedent `amadeo-image` set for the `png` crate, and for the same stated reason: a heavy
external parser lives in exactly one place, and **no `gltf::` type is visible above that boundary**.

glTF is a large specification with a large crate behind it. An engine whose renderer named
`gltf::Primitive` in its own API would have made an interchange format part of its public surface.

Rotations come out as **quaternions** rather than ADR 0018's Euler degrees, deliberately: the
conversion belongs to `amadeo-transform`, which owns the exact Z-then-X-then-Y order. A second
implementation of that convention is the bug that reads as "the imported model is rotated slightly
wrong". `Mat4::from_quaternion` was added for it.

### 5. The generated text is run through the canonical writer, not written canonically by hand

The importer builds scene text and then **parses it and re-emits it** through `amadeo_scene::to_text`.

Invariant I2 says `amadeo fmt` is the single authority on canonical form. A generator that
reimplemented the rules would be a second authority, and the moment the two disagreed every file it
wrote would fail `amadeo fmt --check` in CI. They *did* disagree, over a trailing blank line, the
first time this ran. Parsing its own output also turns a generator bug that produced unparseable text
into a failure at import time, naming the file.

### 6. Re-importing overwrites

Hand edits to generated files are lost. Stated rather than worked around: the generated scene is a
**starting point** to copy from or instance, and merging hand edits into regenerated output is the
feature every engine gets wrong and Unity is notorious for.

## Alternatives rejected

**A runtime loader only** — a `Mesh` naming a `.glb` id, with placement still hand-written. The
smallest option, exactly what ADR 0035 literally predicted, and what Bevy does. Rejected because it
gives *geometry* rather than a *level*: every object would still be hand-placed, and gate 1 asks for
an imported level. Note that this option is a strict subset of what was built — the runtime loader
exists either way.

**Everything to text, vertex data included.** Nothing binary would enter the project and I1 would
hold with no exception at all. Rejected on the cost, which is real and permanent: it needs the
reflected `MeshData` ADR 0035 promised, and it turns every model into megabytes of numbers that are
diffable in principle and unreadable in practice. The exception a `.glb` asks for is the one a `.png`
already has.

## Consequences

- **The engine links a glTF parser** — `amadeo-app` does, to read geometry at load time. That is a
  real dependency cost, isolated behind one crate. The *importer* half is in `amadeo-cli` only.
- **`MeshData` is still not reflected**, so ADR 0035's "vertex data" half remains unbuilt. This
  decision means nothing needs it, which is a better position than building it to satisfy a promise.
- **Textures are not imported.** A glTF's images stay unread; a generated `.material` carries colours
  and nothing else. ADR 0026's decode path already handles PNGs, so wiring them up is additive — but
  it is unbuilt and a textured model will import untextured.
- **Animations, skins and cameras in a glTF are ignored.** Skins need `amadeo-anim`, which does not
  exist. A camera in a glTF is nearly always the exporter's viewport rather than something a game
  wants.
- The scene format needed **no change at all** to express an imported hierarchy, because nested
  entities already meant parenting. That is ADR 0014 and ADR 0032 paying off rather than luck.
