# CLAUDE.md — Amadeo Engine

> Read this file, then `STATUS.md`, before doing anything else in this repo.
> `STATUS.md` says where the project actually is right now. This file says how to work in it.

---

## 1. What Amadeo is

Amadeo is a **general-purpose, genre-agnostic game engine designed to be driven equally well by a
human in a graphical editor and by an AI agent through text and RPC.**

Two audiences, one engine, no second-class citizen:

- **Justin** (the human) works through a graphical editor, code, or both.
- **Claude** (the agent) works through text files, a CLI, and a live introspection protocol.

Neither can do something the other cannot. That symmetry is the product, not a feature.

**It is not** a framework for one genre, a rendering demo, or a wrapper over an existing engine.
It is a data-oriented engine core plus optional genre modules.

## 2. Non-negotiable invariants

Breaking any of these is a bug, no matter how convenient. If a task seems to require breaking one,
stop and raise it instead of working around it.

| # | Invariant | Why |
|---|---|---|
| **I1** | **Text files are the only source of truth.** Scenes, prefabs, assets metadata, and config live in human-readable, hand-editable text. The editor is a *client* that reads and writes those files. It never holds private state. | Without this, the agent is locked out of authoring. See `docs/adr/0003`. |
| **I2** | **Serialization is canonical and byte-stable.** Saving an unchanged file produces a byte-identical file. Sorted keys, stable IDs, fixed formatting. `amadeo fmt` is the single authority. | Editor saves and hand-edits must produce clean, reviewable diffs. |
| **I3** | **Simulation is deterministic.** Fixed timestep, seeded RNG, no wall-clock or unordered iteration in gameplay logic. Same inputs + same seed = same state hash, on any machine. | This is the keystone. It buys replay-as-test, headless verification, snapshots, save/load, and time-travel debugging. See `docs/adr/0005`. |
| **I4** | **The engine core contains zero game logic.** No concept of health, jumping, inventory, or damage below the module layer. Genre knowledge lives only in `modules/` and in games. | This is what "genre-agnostic" actually means operationally. |
| **I5** | **Anything the editor can do, the CLI and RPC can do.** The editor is built strictly on top of the same protocol the agent uses. No editor-only capabilities, ever. | Guarantees the agent never falls behind the human. |
| **I6** | **Dependencies flow one way.** The crate graph is a strict DAG (see §4). A lower layer never references a higher one. No cyclic crates, no "just this once." | Keeps the engine comprehensible and testable in isolation. |
| **I7** | **Every subsystem is headless-capable.** Rendering, audio, and input all have null backends. The whole engine must run with no window and no GPU. | Headless is how the agent runs and verifies games, and how CI works. |
| **I8** | **Reflection is not optional, and the schema is closed.** Every component, resource, and event registers a machine-readable schema, **and so does every type those name** — registering one type registers its field types transitively (ADR 0030), so the schema can never name something it cannot describe. If it can't be reflected, it can't be serialized, inspected, or edited. **Enforced by trait bound** — ADR 0013 for components, ADR 0027 for the other two. | One registry powers serialization, the editor, and agent introspection. |

## 3. Tech stack (decided)

- **Language:** Rust (2024 edition), `#![forbid(unsafe_code)]` outside explicitly audited modules.
- **Graphics:** `wgpu` — one API over Vulkan/DX12/Metal, and it targets WebGPU, so a browser export
  path stays open for free. **ADR 0045 settles the "should we write Vulkan directly" question and the
  answer is no**: wgpu's native features are Vulkan's in practice (bindless, multi-draw indirect,
  subgroups, ray query, mesh shaders, BC/ASTC), a wgpu game already looks far better than Amadeo
  does, and **nothing on the list of things that would improve the picture is blocked by the API**.
  The renderer's ceiling is its *feature set*; ADR 0045 orders that work by visual return. The one
  real reason to go native is a **console** target, which is a business decision rather than a
  graphics one.
- **Windowing:** `winit`. **Math:** `glam`. **Physics:** `rapier` (2D+3D) behind engine-owned traits.
- **Editor UI:** `egui` (immediate-mode, in-process, cheap to build). Game UI is a separate,
  retained-mode system — do not confuse the two.
- **Primary target:** native desktop, Windows first. Web export is a later milestone, not a
  parallel obligation.
- **Game logic authoring:** **Rust systems in the game crate.** No scripting layer, no dynamic
  reload. Settled by measured spike — ADR 0011, evidence in `spikes/q1-game-logic/`. WASM is the
  pre-selected escape hatch if a gameplay rebuild ever sustains above 5 s; check by re-running
  `spikes/q1-game-logic/measure.ps1`, not by impression.

Rationale and rejected alternatives: `docs/02-tech-stack.md` and `docs/adr/0002`.

## 4. Repository layout & dependency order

Crates are listed in dependency order. **A crate may only depend on crates above it.**
`✅` exists and is tested. `—` planned, not yet written.

```
crates/
✅ amadeo-derive      proc macros: #[derive(Reflect)], #[derive(StableHash)]. No engine deps, so it
                     sits below even amadeo-core. Re-exported next to each trait; never used directly.
✅ amadeo-image       decodes PNG (via the `png` crate) and PPM (hand-written) into TextureData, and
                     encodes PNG for `render.capture`. **`mip_chain` builds a texture's mip levels**,
                     and the one subtle thing about it is that averaging happens in **linear light**:
                     sRGB bytes are a perceptual curve, so averaging them directly makes every level
                     too dark -- the classic mipmap bug, seen as a texture that dims as it recedes.
                     Half black and half white is ~188, not 128. Alpha is coverage, not light, and
                     averages as it is. Uses `powf`, which ADR 0044 bans in a `TerrainSource` -- safe
                     here because it runs at load and its output is pixels, not gameplay state. --
                     width, height, an explicit PixelFormat, and flat pixels. Also no engine deps.
                     ADR 0026: decoding happens at load time *for now*, and the format tag is what
                     makes the eventual import pipeline an addition rather than a rewrite. Holds one
                     of the engine's two non-`thiserror` dependencies; that is why it is its own crate.
                     **Also Radiance `.hdr`, decode and encode** (ADR 0049), as `HdrImage` — floats
                     rather than bytes, and a separate type on purpose: an environment map is
                     decoded, projected onto a cube and convolved twice, so forcing it through
                     `TextureData` would give every consumer a branch it never takes.
                     `PixelFormat` gained **`Rgba8Unorm`** for a texture carrying *data rather than
                     colour*. A normal map read through the sRGB curve has every direction bent, and
                     **nothing in a `.png` says which it is** — the `.ama-meta` sidecar's
                     `color_space` does, and forgetting it is silent (**Q31**).
✅ amadeo-gltf        reads glTF 2.0 (.glb, or .gltf with embedded buffers) into plain data: meshes,
                     materials, and the node hierarchy. Also no engine deps, for amadeo-image's exact
                     reason -- **no `gltf::` type is visible above it** (ADR 0039). Rotations come out
                     as **quaternions**, deliberately: ADR 0018's Euler order belongs to
                     amadeo-transform, and a second implementation of it is the bug that reads as
                     "the imported model is rotated slightly wrong".
✅ amadeo-jobs        background work: a `JobPool` of fixed worker threads and an `Inbox` whose
                     results drain in **key order, never completion order** (ADR 0041). **No
                     dependencies at all**, not even thiserror -- a work-stealing runtime underneath
                     the one thing that must be reproducible is what ADR 0041 refuses. A job owns its
                     inputs (`FnOnce + Send + 'static`) so it cannot borrow the world. Exactly two
                     ways an answer may return: **wait at a barrier** (`wait_for_idle`, which makes
                     parallelism a pure speedup nothing downstream can observe), or **deliver into a
                     Service** that gameplay cannot see (ADR 0009). `pending()` is diagnostics only:
                     a count that depends on machine speed is what makes a replay diverge.
✅ amadeo-noise       deterministic gradient noise (ADR 0044). No deps. **`sin`, `cos` and `powf` are
                     forbidden in anything that decides where the ground is** -- Rust documents their
                     precision as varying by platform, by version, and between two calls in one
                     execution, and ADR 0043 made a chunk's collider gameplay state. Built from
                     `+ - * /`, `floor` and integer hashing, all exactly specified by IEEE 754.
                     Its own crate rather than part of amadeo-voxel because noise is not
                     three-dimensional and a 2D heightmap must not need a mesher (trap 9).
                     `a_grid_of_samples_hashes_to_a_known_number` pins a **literal** hash CI runs on
                     both platforms; it caught a one-ULP constant change nothing else noticed.
✅ amadeo-voxel       signed-distance `Field` -> mesh, by naive surface nets (ADR 0042), plus chunk
                     residency and the terrain source (ADR 0043). No deps at all. Negative is inside;
                     getting the sign backwards makes the mesh inside out and it reads as invisible
                     terrain. Surface nets over marching cubes: fewer triangles, and neither can do
                     sharp corners anyway -- so buildings stay BoxMesh and glTF. **The fourth
                     producer of mesh data**, and nothing above the loader knows.
                     **A chunk needs an apron on BOTH sides, and ADR 0042 only described one.**
                     *Vertices* need the high one: a cell needs its eight corners, so a chunk's last
                     cell needs the next chunk's first sample. *Quads* need a low one: `surface_nets`
                     emits a quad from the four cells around a grid edge, and at a chunk's low face
                     two of them belong to the previous chunk -- so the bridging quads were emitted
                     by neither and every chunk had a one-cell gap around it. A chunk of n cells
                     fills **n+2** samples over n+1 cells, starting one cell BELOW its origin
                     (ADR 0043 §4). **Call `mesh_chunk`**, which gets this right, rather than
                     `surface_nets` on a hand-built field.
                     **A mesh's normals and its winding are independent, and getting one right does
                     not check the other.** Every quad this emitted was wound against its own normal
                     until session 13 -- all three axes, uniformly -- so every voxel surface was
                     inside out. It hid because the tests checked *normals* (from the gradient, always
                     correct) and because nothing had ever *drawn* one: a collider has no winding.
                     A heightfield that is inside out is **invisible from above**, which reads as
                     chunks that failed to stream. `triangles_are_wound_to_match_their_own_normals`
                     compares the two against each other; write that test for any new mesh producer.
                     Residency is integer boxes per viewer, because which chunks exist is gameplay
                     state (ADR 0041 §2). Three nested sets, `collision ⊆ visual ⊆ data`, where
                     `data` is `visual` grown by one chunk -- so the apron is enforced by a test
                     rather than remembered. `ChunkKey` carries `lod` although everything is level 0:
                     resolution is part of a chunk's identity, and Q25 is still open.
🟡 amadeo-terrain     chunked streaming (ADR 0043). `TerrainStreamer::update(viewers) -> TerrainUpdate`.
                     **The core depends only on amadeo-voxel and amadeo-jobs** -- no World, no
                     renderer, no solver -- because the hard part of streaming is *when* work happens
                     and none of it needs an entity. That is what lets ADR 0041's claim be tested
                     with no engine in the build.
                     **Colliders are meshed INLINE and block the tick; meshes go to the job pool.**
                     ADR 0041 §2 as two code paths. `colliders`, `colliders_removed`, `visible_added`
                     and `removed` are all `BTreeSet` differences over residency, so contents AND
                     order are identical at every thread count; `meshes` is the inbox drain and is
                     timing-dependent by design. **Gameplay may read the first four and never the
                     fifth.**
                     **Entities are spawned from `visible_added`, never from mesh arrival** -- an
                     entity is world state, so spawning on arrival puts the entity allocator and the
                     state hash behind machine speed. A chunk with no mesh yet draws nothing.
                     **The collider path must fill the mesh cache too**: a collision chunk is meshed
                     inline and marked known, so the pool never touches it and it never reaches
                     `meshes` -- miss this and the invisible terrain is the ground you stand on.
                     `TerrainStreamer::edit` digs. An edit invalidates **up to eight** chunks (the
                     two-sided apron) and jobs carry an edit **version**, so a mesh from before a dig
                     cannot land after it and refill the hole.
                     The ECS layer (`TerrainViewer`, `TerrainChunk`, `stream_terrain`, `install`) is
                     behind the **`engine` feature**, off by default, which is what preserves the
                     no-engine-deps property above. `stream_terrain` runs **before** `step_physics`.
                     **Edits are authored in the `TerrainEdits` RESOURCE, and the streamer is a cache
                     of it** (ADR 0046, closing Q29). Gameplay writes the resource -- writing the
                     streamer directly puts the hole where neither the state hash nor a save file can
                     see it. Stored flat by **world sample**, not grouped by chunk: a sample near a
                     boundary is read by up to eight chunks, and an owning chunk would leave the other
                     seven meshing it differently. `stream_terrain` syncs when the revision moves, and
                     that is also what re-digs the world after a snapshot restore -- a snapshot
                     restores resources and never services (ADR 0009).
— amadeo-math        vectors, matrices, quaternions, rects, curves. No engine deps.
✅ amadeo-core        Tick, FIXED_DT, Rng (PCG32), StableHasher (FNV-1a), StableId/NetId/Authority, and
                     **`sin_cos_degrees` — the engine's own trigonometry** (ADR 0053). ADR 0044 banned
                     the standard library's `sin`/`cos` because Rust does not specify them; this is the
                     other half of that answer, built from `+ - * /` and `floor`, which IEEE 754 pins.
                     `Mat4::from_euler_degrees` uses it, so **composing a rotation is reproducible
                     wherever it happens** rather than only where somebody remembered. Reduces in
                     *degrees* and converts last, which makes the quarter turns exact — and makes it
                     more accurate than `angle.to_radians().sin_cos()`, so a test comparing the two at
                     `f32` is testing the reference. A literal grid hash is pinned on both platforms.
                     It exists here for the reason `Rng` does: determinism-critical things get written
                     rather than depended on.
✅ amadeo-reflect     Value tree, TypeInfo schema, TypeRegistry. ADR 0012. Values include maps with
                     string keys (ADR 0027) — a key type implements ReflectKey, and `to_key` must be
                     injective. Also holds `Reflect for Tick`: a type below this crate cannot
                     implement the trait (I6), so the impl goes where the *trait* lives instead.
✅ amadeo-ecs         archetype SoA storage, resources, services, deferred commands,
                     ComponentRegistry (builds a component from a name + a Value), and queries:
                     `world.query::<(&A, Option<&B>)>()` resolves each column once per archetype
                     (ADR 0025). Read-only; mutation stays with for_each_*_mut.
✅ amadeo-transform   Transform (3D; 2D is its degenerate case, ADR 0018), Parent, GlobalTransform +
                     propagate_transforms, and a scalar Mat4. GlobalTransform is computed, never
                     authored, and DERIVED so it stays out of the state hash (ADR 0019).
✅ amadeo-events      typed double-buffered queues, EventClock total ordering
🟡 amadeo-assets      AssetCatalogue (declared id -> file, ADR 0020), the .ama-meta sidecar format,
                     a sorted directory scan, sidecar generation on import, and byte loading behind
                     ADR 0021's barrier. Asset-root resolution is by marker file (ADR 0022).
                     `load_all_in_parallel` reads files on a `JobPool` and fills the store in **key
                     order at a barrier**, so it is byte-identical to the sequential path, failure
                     messages included (ADR 0041). ADR 0021's rule -- gameplay may not ask whether an
                     asset has loaded -- is what makes that safe.
                     Typed handles, the import/decode pipeline, and hot-reload still to come.
✅ amadeo-input       action mapping, InputState, recording/replay, the .replay text format
🟡 amadeo-render      RenderBackend trait, NullBackend, Quad/Sprite/SortOrder, the Camera **component**
                     (ADR 0031: an entity, any number per world, each with a projection, a target,
                     a viewport rect and an order; a FrameData is a list of Views, one per camera),
                     the sprite batcher (ADR 0023: batches are (sort order, texture) pairs), and TextureCache —
                     id -> bytes -> pixels, with a three-step placeholder fallback ending in an
                     image built in code so it cannot itself be missing. wgpu behind `gpu` draws
                     **quads and sprites**: texture upload, a nearest sampler, one bind group per
                     texture, one draw call per batch. WgpuBackend::offscreen renders into a texture
                     it owns instead of a window, and RenderBackend::capture reads it back -- which
                     is what `render.capture` uses and what gave the GPU path its first tests
                     (tests/capture.rs).
                     **Textures on materials landed in session 13**: `Material::base_colour_texture`
                     had existed since ADR 0033 and was read by *nothing*. Surfaces get their own
                     **repeating, filtered, 16x-anisotropic** sampler and a second bind group per
                     texture; sprites keep the clamped nearest one and are **pinned to mip level 0**
                     so hand-authored pixel art stays crisp. An untextured material binds a 1x1
                     **white** placeholder -- white is the identity of the multiply, so one pipeline
                     serves both, and it is deliberately not the magenta "asset missing" one.
                     **Frustum culling (M2.5 gate 3)**: `Frustum::from_view_projection` extracts six
                     planes by Gribb-Hartmann, and the depth convention is load-bearing -- wgpu clips
                     z to `0..w`, so the near plane is one matrix row rather than a sum, and OpenGL's
                     form would cull things just in front of the camera. **One implementation, used
                     by both the collection pass and `render.describe`**, so what is culled and what
                     is reported cannot drift. **TWO lists, not one**: `View::meshes` is culled to the
                     camera and `View::shadow_casters` to the light, because a mesh behind the camera
                     can still cast a shadow into view -- a single list holding the union is correct
                     and culls nothing, since a shadow box is `shadow_distance` in every direction.
                     **GPU timestamp queries (gate 4)**: every pass is timed and attributed by label
                     behind `WgpuBackend::set_gpu_timing`, **off by default** because reading the
                     results stalls the pipeline. `TIMESTAMP_QUERY` is requested by intersecting with
                     `adapter.features()`, never demanded -- at **both** device sites, and the
                     offscreen one is the one measurement actually uses.
                     **The render graph (ADR 0034) is internal and stays that way**: declared passes
                     with reads and writes, order derived from the dependencies, transient targets
                     pooled across frames, one pass per camera. It knows nothing about wgpu, so
                     NullBackend compiles it too and a pass-ordering bug is catchable with no GPU.
                     Every camera draws into a transient and a present pass copies it onward, which
                     is what gives the **windowed** backend capture -- it reads the transient, where
                     an offscreen one reads its destination after the present pass.
                     **Post-processing (ADR 0034)**: the cameras draw into an **HDR** target and a
                     post pass brings it down -- `Environment` is an asset the camera names by id,
                     holding exposure/tonemap/grade/vignette in an engine-defined order. Its file is
                     a scene file, so `amadeo fmt` and `amadeo check` work on it unchanged. The
                     default look is a **byte-identical** no-op.
                     **Bloom is drawn (ADR 0056)**, and it had been authorable-but-ignored since ADR
                     0034 -- a scene could ask for it and silently get nothing, which is Q32's defect
                     shape in the file format rather than in an asset. Three passes at half
                     resolution, composited **before the tonemap**, which is what the HDR target
                     exists for: a glow added after it is a grey wash rather than light. Off by
                     default and byte-identical when off, pinned as bytes. The glow reaches ~8 px;
                     widening it is a **downsample chain**, not more taps. `games/scarp` deliberately
                     leaves it off -- daylight has nothing above the threshold, so it either does
                     nothing or washes the picture out, and both were captured.
                     Fog waits for a depth buffer. Still to come: render targets on a camera, and
                     per-camera post (**Q23** -- one look per frame today, from the camera that draws
                     first).
                     **`PointLight` and `SpotLight` (ADR 0057)** -- lights at a *place*, which the
                     engine had none of until session 15. **Eight per view**, forward, in a uniform
                     array; the ninth is dropped by distance to the light's *reach*, silently.
                     Deferred is ruled out because it fights ADR 0051's MSAA; clustered is the
                     upgrade path and is behind `RenderBackend`. **A point light is a spot whose cone
                     is the whole sphere** (`cone_outer_cos = -1`), so the shader has no branch on
                     kind -- but they stay two *components*, because an author should not be typing
                     `inner_angle` on a bulb. Cosines are computed at collection, never per pixel.
                     `direct_light` in `mesh.wgsl` is the shared BRDF: a sun and a torch differ only
                     in direction and radiance, and two copies would drift into a material that looks
                     right under one and wrong under the other.
                     **A spot light casts (ADR 0058); a point light still does not** -- a point's
                     shadow is a cube, six faces and six passes. Its map is a **layer of the same
                     array the cascades use**, because all four bind groups are taken and a second
                     shadow texture would have nowhere to bind; `View::shadow_atlas` is the one place
                     that decides layers and size, so the graph, the backend and the shader cannot
                     disagree. The cost is a **shared resolution** -- `shadow_resolution` is a
                     request and the largest wins. A `bool` rather than a `ShadowMode`, since a spot
                     bounds itself and has nothing for cascades to spread over. **Two casters max.**
                     Two things not to copy from the cascaded path: the perspective **divide is
                     real** here, and the bias divides through the **range** rather than the depth
                     span, because perspective clip depth is compressed towards the far plane.
                     **`shadow_casters` is the union of every shadow volume** -- culling it to the
                     directional light alone made a torch-lit scene draw an empty shadow map, which
                     reads as no shadows rather than as a bug.
                     **Session 14 finished ADR 0045's tier 1.** Normal mapping (**ADR 0047**):
                     `Vertex` gained a tangent, read from glTF's `TANGENT` when the file has one and
                     generated at load when it does not — which is why **no `mikktspace` dependency**
                     was needed, since the case where its exactness matters is baked art, and baked
                     art exports the frame it was baked against. Metallic-roughness PBR
                     (**ADR 0048**): Cook-Torrance/GGX. Image-based lighting (**ADR 0049**, closing
                     **Q28**): the ambient constant is gone, replaced by an environment prefiltered
                     on the **CPU at load** — invariant I7, since a GPU prefilter could not run
                     headless. Plus a sky pass, and **4× MSAA** (**ADR 0051**), chosen over a post
                     filter because low-poly's only aliasing is silhouettes.
                     **Nothing is backface-culled any more (ADR 0052)**, and that is what fixed
                     "digging down shows the sky": terrain is an open surface with no underside, so
                     culling made it invisible from below. For a *closed* mesh it changes nothing —
                     back faces are always behind front faces — and the Scarp's capture is
                     byte-identical, which was checked rather than assumed.
                     **Shadows (ADR 0038)** are the first thing that *reads* a depth texture rather
                     than only writing one. `ShadowMode` on `DirectionalLight` ships
                     `Off | Orthogonal | Cascaded { blend }` — **ADR 0055 filled the variant ADR 0038
                     reserved**, which is that enum paying off: no `.scene` that did not opt in
                     changed, so Q32 did not bite a fourth time. Four concentric cascades, drawn into
                     four **layers of one depth array**; the layer count lives inside
                     `TargetFormat::ShadowMap32` so the transient pool keeps a one-layer and a
                     four-layer map apart for free. **A shadow map is ALWAYS an array**, one layer
                     when `Orthogonal`, so there is one shader and one pipeline. One pass per
                     cascade, because a render pass attaches one view. **The bias is per cascade and
                     that is the trap** — it is in clip depth, so a near box and a far box turn the
                     same authored offset into very different numbers; `fit_cascade` divides through
                     each box's own range. Selection is by **radial distance**, since the boxes are
                     concentric rather than frustum slices. Costs 71.7 -> 113.7 µs of GPU time on the
                     Scarp and buys a near texel of ~1 cm against ~7 cm (docs/10).
                     **`view.wgsl` is prepended to `mesh.wgsl` and `sky.wgsl`** and must stay that
                     way: they read one buffer at one binding, and the hand-written copies drifted
                     the moment cascades grew the struct -- the sky drew facing the wrong way and
                     nothing failed. `GpuMeshView` in Rust is the one copy left. The box is centred on the camera and snapped to a
                     grid anchored at the **world origin** -- anchoring it on the camera is snapping
                     to something that moves, and shadow edges crawl. A shadow map is its own
                     `TargetFormat` variant, not a flag: it needs `TEXTURE_BINDING` and the scene
                     depth buffer must not ask for it, and the transient pool matches on format.
                     One casting light per view, directional only.
— amadeo-audio       mixer, buses, spatialization (null backend required)
🟡 amadeo-physics    RigidBody/Collider/Velocity/Gravity as reflected, HASHED data, the
                     PhysicsBackend trait, and NullPhysics -- which integrates velocity and gravity
                     for real rather than doing nothing, so a headless determinism test is
                     meaningful without rapier. ADR 0036: `enhanced-determinism` is on
                     **permanently**, so physics is single-threaded and scalar; the rapier version
                     is pinned exactly, because an upgrade may move results and invalidate every
                     replay containing physics. **No rapier type may cross PhysicsBackend** -- not
                     into a component, a scene file, a snapshot, or the state hash. Components are
                     the source of truth and the solver's world is a cache rebuilt from them, which
                     is what makes a physics game snapshot-able. **rapier is wired up behind
                     `--features rapier`** (off by default, like `gpu`): bodies collide, stack and
                     rest, and `tests/rapier_determinism.rs` pins a **literal state hash** that CI
                     runs on Windows *and* Linux -- so a cross-platform divergence turns CI red
                     rather than going unnoticed. `PhysicsBackend::reset` exists because ADR 0028's
                     lesson applies here: a snapshot restores components but not a solver's contact
                     caches, so a restored world would hash identically and then simulate
                     differently. Joints, raycasts and collision events are still to come.
                     **rapier 0.34 uses glam, not nalgebra** -- `Rotation` is a `glam::Quat`, and
                     rapier's own `vector![]` macro still builds an *nalgebra* vector its API will
                     not accept. Use `Vector::new`.
                     **`PhysicsBackend::move_shape` is the second operation (ADR 0037)**: move a
                     shape and slide along what it hits, returning where it ended up and whether it
                     landed on something. It is what a character controller is built on, and it
                     knows nothing about characters. It answers from an index `step` builds, so it
                     MUST be called after `step_physics` in the same tick -- asking first queries an
                     empty index and the shape passes through the level on tick 1 only.
                     **`cast_shape` is the fourth (ADR 0054, closing Q34)**: sweep a shape along a
                     line and report the first thing in the way, or `None` for clear. **A cast is not
                     a move** -- it does not slide, so its answer is ON the line by construction,
                     which is the guarantee `move_shape` cannot give. Borrowing the move to ask this
                     question failed twice: a sideways slide counted as progress, and then a slide
                     *along* the query direction counted as nearly all of it and put the follow
                     camera under the terrain. Takes `&self`, because a cast is a question. Same
                     after-`step` rule as `move_shape`.
                     **`insert_static_mesh` is the third (ADR 0043)**: a triangle mesh handed over
                     ONCE, by id, and held between steps -- because `Shape` is `Copy` and
                     `StableHash` and a world's worth of vertices is exactly what ADR 0042 refuses to
                     hash, and because `BodyState` travels in full every tick. The geometry is
                     derived, so ADR 0019 puts it outside the hash; the seed and the edits that made
                     it are what get hashed. Knows nothing about terrain. **An empty mesh is refused
                     by both backends** and most chunks of a real world are empty, so filter with
                     `StaticMesh::is_empty`. Inserting a known id REPLACES; `reset` drops them all.
— amadeo-anim        sprite anim, skeletal, state machines, tweens
— amadeo-ui          retained-mode game UI: layout, theming, focus navigation
✅ amadeo-snapshot    the .snapshot text format (ADR 0028): capture a whole world to a file and put
                     it back. Sits above amadeo-scene because it borrows that crate's scalar
                     encoding — format_float is subtle and two copies would drift. **It captures the
                     entity allocator's free list**, which state_hash excludes: without it a restored
                     world hashes identically and then spawns different handles.
🟡 amadeo-scene       the .scene text format (ADR 0014): parser, canonical writer, instantiate into
                     a World, and the `assets` block a scene declares its requirements in (ADR 0021).
                     Prefab instancing landed with ADR 0029: `from` holds an **asset id** (superseding
                     ADR 0014's path grammar), and an override is a top-level *patch* that reaches the
                     instance root and nothing inside it. That is what makes nesting structurally
                     safe. A dangling override refuses to load; a cycle is reported with its chain.
                     ADR 0032 added value nesting: an indented block is a **list** if its lines start
                     with `- ` and **named fields** otherwise (YAML's rule, and no schema needed), so
                     nested structs, maps and enum payloads all write now. `Option::None` and any
                     *empty* field value still have no spelling, deliberately.
✖ amadeo-script      NOT BUILT. ADR 0011: game logic is plain Rust in the game crate.
🟡 amadeo-agent       the protocol: JSON reader and writer, JSON-RPC envelope, and the methods that
                     need only a world + registry (describe, describe.example, render.describe,
                     world.query/entity/list/resources). Read-only. ADR 0030 settles what `describe`
                     is *for*: a **schema, not a manual**, covering components, resources, and every
                     type those name transitively — how to write Rust against the engine stays in
                     docs/07, which the reply points at. `render.capture` is served by the *host* in amadeo-app, since it needs an App.
                     Mutation pending.
                     ADR 0016, spec in docs/protocol/v1.md.
✅ amadeo-app         Stage/Schedule, fixed-timestep loop, SimRng, ComponentRegistry, **`asset_problems`
                     — an asset whose file names a component it then failed to build says which
                     asset, which component and which field.** Not a missing-asset report; those are
                     survivable by design (ADR 0021). This is the narrower case that is always a
                     fault, and it used to be a silent `continue`: adding a field to `Environment`
                     invalidated every `.environment` file and the symptom was a *missing service*
                     three layers away. See **Q32**, whose churn was never the problem. `Profiler`
                     (ADR 0040: per-system timings, **always on**; a wall clock does run inside the
                     tick and that is safe only because a Service is structurally outside the state
                     hash -- see ADR 0009), and the agent
                     *host* — serve_if_requested reads stdin and answers. The host lives here rather
                     than in amadeo-agent because it needs App and I6 forbids reaching down.
— amadeo-editor      graphical editor. A CLIENT of amadeo-agent. No privileged access.
🟡 amadeo-cli         the `amadeo` binary. Built: describe/query/entity/schedule/status/call/check/
                     replay/fmt/assets/import/import-gltf/snapshot/capture (import takes `--assets <dir>` to work on a
                     project whose game will not start -- Q19), plus `--from <file>` on any of them to
                     restore a snapshot before answering, and `describe <Type> --example` for a
                     minimal valid instance in both the scene and JSON spellings.
                     Pending: new/run/test/build/export. ADR 0016: `fmt` is standalone;
                     everything else spawns the game binary in agent mode and talks to it over stdio,
                     because only that process knows the game's components.
modules/             optional, genre-flavored. Core NEVER depends on these. Created by ADR 0037; a
                     module may depend on engine crates and on other modules, and no engine crate may
                     ever depend on a module (I6, one level up).
🟡 amadeo-camera      the second module. A third-person `FollowCamera` that **sweeps a sphere and pulls
                     itself in** rather than sitting inside a wall (Q27). **Does not depend on
                     amadeo-character** — trap 10 says a camera rig must not assume a character
                     exists, and this follows a `Parent`, whatever that is. `install` declares both
                     orderings because both fail *silently*: the mouse turn before anything reads the
                     parent's rotation, and the sweep **after `step_physics`**, since `move_shape`
                     answers from an index that step builds and asking earlier finds open space
                     everywhere. **Two sweeps, not one** — the second goes *upward* to the pivot,
                     because a cast that starts inside geometry has no reliable answer and the pivot
                     is inside the ceiling in any tunnel. The result is **projected onto the axis
                     asked for**, because `move_shape` slides -- **superseded: both sweeps are now
                     `cast_shape` (ADR 0054)**, so there is no slide to reinterpret and no projection
                     left. Snap in, ease out.
                     **It is an ARM, and pitch is an angle around the pivot** — session 15, and the
                     thing it shipped without. A camera at a fixed `[0, height, distance]` that only
                     *rotates* when you tilt points at the ground below itself and loses whatever it
                     was following; the position has to come from the pitch. `height` therefore means
                     **the point the camera aims at**, not how high it floats. `CameraArm` holds the
                     smoothed arm length, because that must survive to the next tick and cannot be
                     read back out of a transform once the arm leans — `CharacterController` /
                     `CharacterMotion` again (ADR 0037).
                     **Both sweeps `.ignoring()` the parent**, and *that* was the flicker: a sweep
                     starting inside the followed body's own collider makes rapier report
                     `sliding_down_slope` and cancel the motion, intermittently, so the camera never
                     reached its authored distance at all. The orbit needs `sin`/`cos` of the pitch in
                     a **hashed** component, which is what ADR 0053 exists for.
                     It lived in `games/scarp` first, on the rule that something moves to `modules/`
                     when a *second* game wants it — `games/atrium` is what moved it.
🟡 amadeo-character   the first module. `CharacterController` (speed, acceleration, jump, turn, slope,
                     step height) and `CharacterMotion` (velocity, grounded), driven by named input
                     actions, moved by `PhysicsBackend::move_shape`. `install(&mut app)?` registers
                     both components, `step_physics`, and its own system **after** it -- the ordering
                     is load-bearing, see the physics entry. Not gated on `rapier`: against
                     `NullPhysics` the character walks through walls, which is deliberately the
                     control case its tests assert. Still to come: crouching, coyote time, and
                     imparting velocity to dynamic bodies (see ADR 0037's consequences).
games/               actual games built with the engine
  quad-demo          M0's exit gate: a steerable quad, plus the replay fixture CI asserts on.
  vault              M1's exit gate: a complete small 2D game. The level is scenes/vault.scene;
                     the sprites are generated from hand-written .pix text by
                     `cargo run -p vault --bin pix`. Its tests are the milestone's proof —
                     plays_itself.rs drives the game with scripted input, and
                     verified_without_eyes.rs checks the screen through render.describe.
                     NOTE it has two binaries, so it sets `default-run` — without that
                     `cargo run -p vault` is ambiguous and every CLI command against it fails.
  scarp            M2.5's exit gate: a generated world you walk on and dig into
                   (`cargo run -p scarp`). **Nothing is authored but the player, the camera and the
                   sun** -- the ground is a function of the seed, streamed in chunks. `Highlands` is
                   its `TerrainSource` and lives here rather than in the engine because a world's
                   *shape* is content (ADR 0044 §2). Gate 2 is
                   `a_walk_reproduces_at_every_thread_count`, five worlds in **lockstep** at 1/2/3/5/8
                   workers. Building it found four engine defects; see STATUS.
  atrium           M2's demo: a lit 3D room with shadows and a character you walk around in
                   (`cargo run -p atrium`). The room, its meshes, its materials and its look are all
                   text; the follow camera is a **child entity of the player** and nothing else,
                   which is ADR 0031's claim cashed. **Enables `amadeo-physics/rapier`**, so feature
                   unification turns rapier on for `cargo test --workspace` — deliberate, since a
                   demo whose walls do not stop you is not a demo.
docs/                design docs and ADRs
spikes/              separate cargo workspaces holding the evidence behind an ADR. Frozen once
                     written; excluded from the engine workspace. See spikes/README.md.
```

**Note:** an earlier version of this section said `Transform` would move to `amadeo-scene` with the
hierarchy components. **That was wrong** and ADR 0015 corrects it: `amadeo-render`, `amadeo-physics`,
and `amadeo-anim` all sit *below* `amadeo-scene` and all need transforms, so I6 makes that placement
impossible. They live in `amadeo-transform`.

**Careful:** `ComponentId` **and `ResourceId`** are the hash of a type's **canonical name**
(`Reflect::type_name`), not its Rust path — ADR 0017. So *moving* one between crates is free, and
**renaming one changes its id and every state hash containing it**. `#[reflect(name = "...")]`
renames the Rust type without changing identity. Two components may not share a canonical name; the
registry refuses it. `ServiceId` keeps the Rust path, deliberately: a service is not reflected, is in
no state hash (ADR 0009), and is named by no file.

## 4b. Verifying the build

Everything must be green before a commit. These four are what CI runs:

```
cargo fmt --all --check
cargo clippy --workspace --all-targets --all-features -- -D warnings
cargo test --workspace
RUSTDOCFLAGS="-D warnings" cargo doc --workspace --no-deps
```

Golden replays live in `crates/amadeo-app/tests/golden/`. If one fails, read
`docs/07-working-with-the-code.md` § Golden replays **before** regenerating it.

**A fifth check, and only when a `.wgsl` file changed:**

```
WGPU_BACKEND=dx12 WGPU_DX12_COMPILER=fxc cargo test -p amadeo-render --all-features --test capture
```

Windows CI has no GPU, so it uses WARP and compiles shaders through **FXC**, which is far stricter
than the DXC or Vulkan path a real GPU takes -- a shader that builds on your machine can fail every
GPU test at once on CI. This runs the same compiler locally. Ubuntu CI is no help: with no software
fallback it skips these tests entirely, so a green Ubuntu job says nothing about a shader.
See `docs/07` for the one that caught this out.

**Where does new code go?** If it needs to know what a game *is about*, it belongs in `modules/` or
`games/`. If it's a mechanism with no opinion about genre, it belongs in a crate. When in doubt,
put it higher up the stack — pushing things down later is easy; pulling them out is not.

## 5. Working agreement for sessions

**At the start of a session:**
1. Read `CLAUDE.md`, then `STATUS.md`, then the current milestone section of `docs/05-roadmap.md`.
2. Run `git log --oneline -15` to see what actually happened last.
3. Check `docs/06-open-questions.md` — if the task depends on an open question, resolve it with
   Justin *before* writing code that assumes an answer.

**During a session:**
- Any decision that constrains future work gets an ADR in `docs/adr/`. Cheap to write, saves entire
  sessions of re-litigation. Number sequentially, never edit a decided ADR — supersede it.

- **When to put a choice to Justin: anything hard to reverse.** Stated by him in session 7, choosing
  this deliberately over the narrower "only things I'd read or write often". **The test is cost to
  undo, not visibility** — an internal mechanism nobody would look at still warrants asking if
  ripping it out later would mean rewriting a lot. Genuinely cheap-to-change internals can still be
  decided alone and flagged in the summary, as ADR 0022 was.

- **How to put a choice to Justin.** He has no game-engine-development background and has said he
  tends to take whichever option is recommended. That means offering a menu is not sharing the
  decision — the burden is on the recommendation to have been *earned*. So:
  - **Research before asking, not instead of asking.** When the codebase alone cannot settle a
    trade-off, go read how real engines solve it. He explicitly endorsed spending the time.
  - **Pros *and* cons for every option**, including the recommended one. A list of upsides with one
    "(Recommended)" tag is not a decision aid.
  - **Plain language.** Define the vocabulary — "sprite batcher", "render graph", "gimbal lock" —
    at the point where it affects a choice he has to make.
  - **Prefer the more complete option over the faster one.** Stated directly in session 6: he would
    rather have a complete engine than one that accumulates problems, and does not mind more steps.
    Do not quietly narrow scope to save effort; that is not the trade he wants.
- Prefer a working vertical slice over a complete horizontal layer. Every milestone must end with
  something runnable.
- Write the determinism test alongside the feature, not after. Retrofitting determinism is the
  single most expensive mistake available in this project.

**At the end of a session:**
1. Update `STATUS.md`: what landed, what broke, what's next, any new sharp edges.
2. Update `docs/06-open-questions.md` — remove resolved, add discovered.
3. Commit. Message body should explain *why*, not restate the diff.
   **Sole authorship, and no co-authorship of any kind.** Restated and widened in session 14: no
   `Co-Authored-By` trailer, no "generated by" line, no sign-off in a message, and **no attribution
   in a code comment or doc either**. Commits are authored under Justin's name alone. It is his
   personal project and his GitHub history; he knows what worked on it. This overrides the default
   Claude Code convention of appending a trailer. End the message at the last line of the body.
4. **Pushing is allowed** — changed in session 14, reversing the session-7 rule. Run the four checks
   in §4b, commit, push, then verify with `gh run list` rather than waiting to be told it went red.
   The gate existed when Actions minutes were scarce; the repository is public now and CI is free.
   **This does not widen anything else**: decisions that are expensive to undo are still Justin's,
   per §5's rule on hard-to-reverse choices.

## 6. Conventions

- **Errors:** `thiserror` for library crates, `anyhow` only in `amadeo-cli` and `games/`. No
  `unwrap()` or `expect()` in engine crates outside tests — return typed errors. Error messages must
  include actionable context (entity id, system name, asset path). Both a human and an agent read
  these; a bad error message is a real defect.
- **Naming:** components are nouns (`Transform`, `Velocity`). Systems are verb phrases
  (`integrate_velocity`, `resolve_collisions`). Events are past tense (`EntitySpawned`, `DamageDealt`).
- **Data layout:** structure-of-arrays over arrays-of-structs in ECS storage. Components are plain
  data — no methods with side effects, no `Rc`/`RefCell` in components.
- **Tests:** unit tests inline. Determinism and golden-replay tests in `tests/`. Every subsystem
  needs a headless test. No test may depend on frame timing or wall-clock.
- **Docs:** every public item gets a doc comment. Doc comments are the agent's API surface — treat
  them as load-bearing, not decoration.

### Legibility for a Rust-learning human — a hard requirement

Justin wants to be able to **read, debug, and fix this codebase himself**, including in sessions where
Claude isn't involved or has gotten stuck. He is not yet a Rust expert. This is a stated project
requirement, not a preference, and it constrains how code gets written:

- **Boring Rust beats clever Rust.** Prefer explicit types, plain functions, and obvious control flow.
  Avoid deep generic nesting, trait gymnastics, complex lifetime puzzles, and macro magic unless there
  is a real, stated reason. Where an exotic construct is genuinely necessary, comment *why* — not what.
- **Comment the non-obvious Rust, not the obvious code.** `// the Arc is here because the asset loader
  touches this from a worker thread` is useful. `// increment the counter` is noise.
- **No unexplained idioms.** If a construct would make someone with three months of Rust stop and
  squint (`impl Trait` in odd positions, `PhantomData`, interior mutability, `unsafe`), it needs a
  one-line explanation next to it.
- **Explain in prose when introducing a pattern.** When a session introduces a new architectural
  pattern, add it to `docs/07-working-with-the-code.md` with a short worked example. That file is
  Justin's map into the codebase and must stay current.
- **Commit messages explain why.** He will read git history to understand how things came to be.
- **Errors must be actionable by a human too**, not just structured for an agent. Same standard, both
  audiences.

The trade: some code will be slightly more verbose or slightly less optimal than peak-idiomatic Rust.
That is an accepted and deliberate cost. A codebase only one author can maintain has already failed
this project's core goal.

### Visual design: do not ship the default "AI app" look

Applies to the editor (M4), the game UI system and its default theme (M3), any tooling UI, and any
document or page produced for this project. Justin raised this explicitly and disliked the house style
that LLM-generated interfaces converge on.

**Avoid the tell-tale defaults:**
- `Inter` / `system-ui` / `-apple-system` as the typeface, and font stacks chosen by not choosing
- purple-to-blue gradients, and gradient text
- uniform large border radii on everything; glassmorphism; frosted translucent panels
- centred hero layouts with vast empty margins
- emoji as section markers or button icons
- soft grey drop shadows on floating white cards
- the generic "clean minimal SaaS" arrangement applied regardless of what the thing is

**Aim for instead:**
- A typeface picked deliberately, with some character. For a game engine, the right references are
  professional creative tools — Blender, Houdini, Ableton, Reaper, Nuke — not landing pages.
- **Information density over whitespace.** This is a tool for people doing sustained detailed work.
  Dense, legible, and quick to scan beats airy and sparse. Pro tools look busy because they are.
- Deliberate, slightly idiosyncratic colour. Committed choices, not hedged neutrals.
- Sharp or mixed corner treatments; visible structure; real dividers rather than implied ones.
- Personality. It is allowed to look like *something* rather than like nothing.

If a design decision could be described as "what an AI would produce by default", that is the signal
to choose differently. When in doubt, look at how a mature creative tool solves the same problem.

## 7. Traps specific to this project

Things that will quietly destroy the design if allowed:

1. **Editor convenience creep.** "Just store this one thing in editor state." No — see I1. Every
   piece of editor state that isn't in a file is a capability the agent loses.
2. **Nondeterminism leaks.** `HashMap` iteration, `Instant::now()` in gameplay, unsorted parallel
   writes, uninitialized float garbage. Each one silently voids replay testing. Use ordered maps in
   simulation paths.
3. **Genre logic drifting downward.** A `Health` component in `amadeo-ecs` breaks I4 and starts the
   slide toward a single-genre engine.
4. **The scene format becoming a serializer dump.** If the format is whatever the serializer happens
   to emit, humans stop being able to write it. The format is a designed artifact with its own spec.
5. **Skipping reflection registration.** Ships fine, then the editor and the agent can't see the
   type, and you find out three milestones later.
6. **Building breadth before the spine works.** Ten half-subsystems can't run a game. One thin
   working slice can.
7. **Forgetting the reserved multiplayer hooks.** Six of the eight target games are co-op or
   multiplayer. ADR 0006 reserves
   network identity, replication metadata, and authority during M0–M2 — while those systems are being
   written for the first time. Skipping them means a sweep across every component later. Equally: do
   **not** build transport or prediction code before M6; that's scope creep in the other direction.
8. **Baking an art style into the renderer.** The target games span stylised-realistic outdoors,
   low-poly, and dark atmospheric interiors. A pipeline tuned for one is a pipeline that can't do the
   others. Post-process and lighting stay configurable.
9. **Letting 2D become second-class.** Amadeo supports 2D and 3D equally — a 2D game is a genre, and
   I4 says genres are not privileged. Doing 3D earlier is fine; shipping a 2D feature that is worse
   than its 3D equivalent, or foreclosing 2D with a design choice, is not. Raised by Justin in
   session 6 when the target list was all-3D; **session 7 settled it by adding Terraria, RimWorld,
   and Project Zomboid**, so 2D and isometric are now target requirements rather than a principle
   being defended. See `docs/00-vision.md` § Target games.
10. **Assuming a game has a character, a camera behind it, and a 3D world.** Of the eight targets,
   Stellaris has no character at all, three are 2D or isometric, and three have fully destructible
   chunked worlds. A character controller belongs in `modules/`, not in the core, and the camera rig
   must not assume a character exists.
11. **Designing the module boundary without thinking about mods.** Four of the eight targets are
   defined by their modding ecosystems, which is in real tension with ADR 0011's "game logic is plain
   Rust". Nothing needs deciding yet, but "what can a mod do" is the same question as "what is the
   module boundary" — see **Q15**. Retrofitting a sandbox boundary is much worse than designing to one.

## 8. Reading order for the design docs

| Doc | Read it when |
|---|---|
| `docs/00-vision.md` | You need to know what we're building and what we're deliberately not. |
| `docs/01-architecture.md` | You're placing new code or changing structure. |
| `docs/02-tech-stack.md` | You're questioning a stack choice. |
| `docs/03-ai-native-design.md` | You're touching agent tooling, determinism, or introspection. **Highest-value doc in the repo.** |
| `docs/04-subsystems.md` | You're about to build a subsystem. Per-system requirements and decisions. |
| `docs/05-roadmap.md` | Start of every session. Milestones and their exit gates. |
| `docs/06-open-questions.md` | Before assuming any undecided thing. |
| `docs/07-working-with-the-code.md` | Setup, commands, and the Rust patterns this engine uses. **Justin's map into the codebase — keep it current.** |
| `docs/08-assets.md` | You're adding an asset, or wondering why it isn't showing up. |
| `docs/09-gate-4-describe-is-not-enough.md` | You are about to rely on `describe` telling you how to *write* something, rather than what data exists. Records what M1 exit gate 4 found, and how ADR 0030 closed it. |
| `docs/10-frame-budget.md` | You want to know what a frame costs, or you are about to claim something is fast. M2 exit gate 4's numbers, measured and re-runnable. |
| `docs/adr/` | You want to know why something is the way it is. |
