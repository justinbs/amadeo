# 05 — Roadmap

Read the current milestone at the start of every session. Check `STATUS.md` for actual progress.

## How milestones work here

Every milestone ends in something **runnable**, and every milestone has an **exit gate** — a
concrete, falsifiable test. Not "the ECS is done" but "this specific thing works and here is how to
verify it." A milestone is not finished until its gate passes, and gates are not negotiable downward
mid-milestone. If a gate turns out to be wrong, that's an explicit conversation and an ADR, not a
quiet edit.

No time estimates. They would be fiction, and the plan → build → re-plan cycle means scope is expected
to move.

Between each milestone: **a re-planning pass.** Revisit `04-subsystems.md` and `06-open-questions.md`,
write ADRs for what was learned, and adjust the next milestone before starting it. The build phase
teaches things the planning phase cannot.

---

## M0 — The Spine

*Goal: prove the risky foundations. No pretty pictures. The stuff that's ruinous to retrofit.*

**Build**
- Cargo workspace, CI (build + test + clippy + fmt), `.gitignore`, dependency-direction lint.
- `amadeo-math` — vectors, matrices, quaternions, rects, wrapping glam.
- `amadeo-core` — ids, handles, arenas, error model, logging, config layering.
- `amadeo-ecs` — archetype SoA storage, queries, deferred commands, change detection.
- `amadeo-events` — typed double-buffered queues with total ordering.
- `amadeo-input` — action mapping, deterministic sampling, record/replay of action streams.
- `amadeo-app` — fixed-timestep loop, schedules with explicit ordering, plugin registration.
- Determinism harness: seeded RNG resource, world state hashing, golden replay test runner.
- `amadeo-render` — window via winit, wgpu device init, clear to a color, and a **null backend**.
- ✅ **The Q1 spike.** Four approaches prototyped and measured against one shared benchmark;
  resolved by ADR 0011. `spikes/q1-game-logic/`.

**Exit gate** — status as of session 4 (2026-07-31): **M0 complete.**

1. ✅ **A coloured quad moves under keyboard input in a real window.** `cargo run -p quad-demo`.
   Visually confirmed by Justin.
2. ✅ **Record input, replay it, assert identical state hashes at checkpoints.** Two halves, both now
   in CI. *Separate build:* `crates/amadeo-app/tests/golden_replay.rs` against a committed fixture.
   *Separate process:* `amadeo replay games/quad-demo/replays/wander.replay`, which launches the real
   game binary and asserts four checkpoints — closed in session 6, once `amadeo-cli` existed.
3. ✅ **`cargo test` passes in CI, including the golden replay.** 228 tests; fmt, clippy `-D warnings`,
   and rustdoc clean; a dedicated determinism job runs the suite three times in separate processes
   plus once in release.
4. ✅ **Q1 resolved by an ADR backed by measured numbers.** ADR 0011. Four candidates prototyped and
   measured against one shared benchmark in `spikes/q1-game-logic/`. Decision: **game logic is Rust,
   compiled in**; WASM reserved as an escape hatch behind a stated threshold. The premise the
   question was built on — a 30-second rebuild — measured at **0.9–3.2 s** and does not hold.

**Why this gate:** it exercises input → simulation → state → render → replay end to end. If this
works, the spine is real. If determinism is broken, it's broken *here*, when it costs a day instead
of a milestone.

---

## M1 — See and Inspect

*Goal: the collaboration surface goes live. From here on, I can work largely unattended.*

**Build**
- ✅ `amadeo-reflect` — `Value` tree, `TypeInfo` schema, `TypeRegistry`, `#[derive(Reflect)]` and
  `#[derive(StableHash)]`, the metadata vocabulary (ranges, units, docs, ADR 0006 replication), and
  a per-type version number for later migration. ADR 0012.
- ✅ **`Component: Reflect`**, so I8 is structural rather than conventional (ADR 0013). All existing
  components converted to `#[derive(StableHash, Reflect)]`.
- **`Resource: Reflect`** — the other half of I8. Needs `Rng`'s state exposed so `SimRng` can reflect
  (which also retires its `Debug`-based `StableHash`), and map support in `Reflect` for `InputState`.
- Canonical serialization: sorted keys, stable IDs, fixed number formatting.
- 🟡 `amadeo-scene` — the text format is decided and built (ADR 0014, chosen from the four
  hand-written candidates in `spikes/q2-scene-format/`): parser with line-numbered errors, canonical
  byte-stable writer, and the round-trip test that satisfies exit gate 3. Layer 2 lands too —
  `ComponentRegistry` in `amadeo-ecs` builds components by name, and `instantiate` turns a document
  into entities atomically. **Prefab instancing landed in session 8** — ADR 0029 settles both halves
  of Q7: `from` holds an asset id, and an override is a top-level patch on the instance root that
  cannot reach inside. **Still to do:** materialising hierarchy as components (blocked on where
  `Parent` lives — see below).
- ✅ **`amadeo-transform`** (ADR 0015) — `Transform` moved out of `amadeo-render`, plus `Parent`.
  Settles where hierarchy lives; scenes now load their tree as real components. `GlobalTransform`
  and propagation landed in session 6 once ADR 0018 settled what a transform is.
- 🟡 `amadeo-agent` v1 — the **read** half exists: `describe` (the schema as JSON), `entity` and
  `query` (the live world), and a deterministic JSON writer whose output is sorted and therefore
  diffable. Still to do: the mutating calls (`world.spawn`, `world.set_component`, `sim.step`,
  `sim.pause`), `events.since`, `scene.load`/`save`, `render.capture`/`describe`, `replay.*`,
  `snapshot.*`. **The transport is built** (ADR 0016, Q14): hand-written JSON parser matching the
  existing writer, JSON-RPC 2.0 over newline-delimited stdio, served from inside the game binary.
  Methods: `describe`, `world.list`, `world.entity`, `world.query`, `schedule.list`, `sim.status` —
  all read-only. The mutating calls and the persistent session wait for M4's editor to need them.
- ✅ **`App` owns a `ComponentRegistry`** (ADR 0016) — `App::register_component::<T>()`, so a game
  registers once. `quad-demo` registers its own `Velocity` and `Player` alongside `Transform` and
  `Quad`, which is what makes `amadeo describe Velocity` describe a *game's* type.
- ✅ Protocol spec in `docs/protocol/v1.md`, versioned, written against the batch method set.
- 🟡 `amadeo-cli` — **built:** `describe`, `query`, `entity`, `schedule`, `status`, `call`, `check`,
  `replay`, `fmt`. **Still to do:** `new`, `run`, `test`. Per ADR 0016 `fmt` runs standalone;
  everything else spawns the game binary via `cargo run -p <package> -- --amadeo-agent` and talks to
  it over stdio. **`amadeo replay` closed M0's carried-over separate-process replay check**, which CI
  now runs in the determinism job against `games/quad-demo/replays/wander.replay`.
- `amadeo-assets` — virtual FS, handles, async load with load-order isolation, hot reload, import
  pipeline, text sidecar metadata, placeholder assets on failure. **Q4 is settled (ADR 0020):** an
  asset is named by a declared `id` in its sidecar, defaulting to the filename stem on import, so
  moving a file breaks nothing. `assets.list` must exist before the id becomes the reference syntax,
  or the first agent to author a scene has to guess.
- ✅ **Transform hierarchy** — `GlobalTransform` plus `propagate_transforms`, excluded from the state
  hash because it is derived (ADR 0019). Still to do in this line: sprite batcher, textures, cameras.
- Game logic layer, per the Q1 decision: **nothing to build.** ADR 0011 settled this as "Rust systems
  in the game crate", which is what `games/quad-demo` already does. `amadeo-script` is not created.
- **`snapshot.take` / `snapshot.restore`, treated as the iteration-loop priority rather than as two
  more RPC methods.** ADR 0011 measured re-simulation, not compilation, as the thing that actually
  degrades the edit→observe loop: 47 ms to reach 30 s of simulated time, 382 ms to reach 5 minutes,
  growing linearly forever. Acceptance test: restoring to tick N beats re-simulating to tick N at
  N = 18 000.
- ✅ **Widen ECS queries past two components.** Done: `iter_triple` and `for_each_triple_mut` (writes
  two, reads one). Four or more is deferred until a real system needs it.

**Exit gate**
1. ✅ **A complete small 2D game — built entirely by Claude with zero editor use and zero human
   intervention.** Something on the order of: player moves, enemies patrol, collision, a score, a
   win state. Authored via text files and RPC only.
   **`games/vault` — the Vault.** All five: the player moves and is stopped by walls, two wardens
   patrol authored routes, six sigils are collected for score, and the run ends in a win or a loss.
   The level is `scenes/vault.scene`, loaded at startup; the sprites are generated from hand-written
   `.pix` text by `cargo run -p vault --bin pix`. `tests/plays_itself.rs` drives all five claims with
   scripted input.
2. ✅ Verification of that game done purely through `inspect`, headless runs, and `render.describe` —
   with screenshots used only for final confirmation.
   `render.describe` was built for this and `tests/verified_without_eyes.rs` is the proof — it found
   a real bug (the score readout overlapping the top wall) that no simulation test could see. The
   game was played, checked, and corrected before anyone looked at it.
3. ✅ Scene round-trip test in CI: parse → serialize → byte-identical.
   `crates/amadeo-scene/tests/round_trip.rs`, which also asserts ADR 0014's worked example is
   byte-identical to the formatter's output — so the spec cannot drift from the implementation.
4. ⚠️ `amadeo describe` output is sufficient to write a new component and system without reading engine
   source. Tested by actually doing it.
   **Tested, the claim is false, and superseded by ADR 0030 rather than met.** Left worded as
   originally written, because the gate being wrong is the result. The test was a real component and
   system (`Trap` and `spring_traps`, shipped in the Vault). `describe` is sufficient to author
   *content* — it carries every field's type, unit, range and meaning — and it says nothing about how
   to *declare* a component, register one, write a system, or query the world. Write-up in
   `docs/09-gate-4-describe-is-not-enough.md`.

   **What ADR 0030 decided**, from three options Justin was given: `describe` is a **schema, not a
   manual**. The API half stays in `docs/07-working-with-the-code.md`, because **invariant I5** does
   not ask the protocol to carry something the editor cannot do either — the editor will never
   declare a Rust type — and the reply now names the file rather than leaving a reader guessing.
   The *schema* half was a genuine hole and is fixed: resources are first-class, the schema is closed
   over every type a field names, a fixed array reports its length, and `describe <Type> --example`
   emits a minimal valid instance in both the scene and JSON spellings. Pinned by
   `games/vault/tests/gate_four.rs` — the game that found the gap.
5. ✅ Golden replays from M0 still pass.
   And the Vault added its own: `replays/collect-two.replay`, asserted in a separate process by CI.
   A much stronger determinism check than `quad-demo`'s — patrolling wardens, collision against
   forty-four walls, entities despawning mid-run, and a resource changing as a result.

**Why this gate:** it's the first real proof of the AI-native thesis. If I can't build a tiny game
without eyes, the design is wrong and we find out before 3D and the editor pile on top.

---

## M2 — The Third Dimension

**Build**
- ✅ **ADR 0031 on 2D/3D coexistence** — decided *before* code, as this line required. Two passes in
  one render graph, neither built on the other; **and the camera becomes an entity rather than a
  resource**, which turned out to be the expensive half and the one the framing had missed. Closes
  Q3's last third and Q10.
- ✅ **Render graph proper: declared passes, resource dependencies, transient targets.** Runs **once
  per camera**, per ADR 0031. **ADR 0034 settles whether it is a public API — it is not.** Built in
  full, but internal, so `RenderBackend` stays the isolation boundary that made three earlier
  renderer decisions cheap. `NullBackend` compiles the same graph, so the pass order is checkable
  with no GPU; and composing the frame off-screen gave the **windowed** backend `capture`.
- 3D: mesh rendering, PBR materials, directional + point lights, shadow maps, frustum culling.
  ✅ **ADR 0035 decides what a mesh asset is, before the code** — a scene file with one root carrying
  either a **procedural shape** (`BoxMesh { size }`) or vertex data, both producing one `MeshData`.
  So a 3D level is authorable in text from day one and the glTF importer becomes a new *producer*
  rather than a precondition. Vertex layout is fixed at position/normal/UV.
- **Configurable post-process stack and atmosphere** — the eight target games span stylised-realistic
  outdoors, low-poly, dark atmospheric interiors, voxel, and pixel-art sprite work, so the renderer
  must not bake in a look (`00-vision.md` § Divergent). Fog/volumetrics and strong dynamic point
  lighting are requirements here, not polish — M3's horror slice depends on them.
  ✅ **ADR 0034 decides the shape, and the stack is built:** *configurable* here means tunable, not
  extensible. The engine owns the effects and content configures them through an **`Environment`
  asset** held by the camera, carrying named effect blocks in an engine-defined order. This is what
  Godot, Unity and Unreal all ship as their primary answer, and it satisfies I5 and I7 for nothing
  where a code-supplied pass satisfies neither.

  Shipped: an HDR scene target, exposure, tonemapping, colour grading and vignette, with the default
  look a byte-identical no-op. **Still open here:** bloom's blur passes (its fields exist and are
  inert), **fog and volumetrics — which need the depth buffer the mesh pass brings**, and per-camera
  post (Q23). M3's exit gate 5 is the exam this has to pass, and fog is the piece it most needs.
- Culling architecture must not preclude later world streaming (see non-goals table).
- glTF import — meshes, materials, scene hierarchy, skins.
- ✅ **ADR 0033 on the material and shader model** — decided before the second shader family, as
  `docs/04-subsystems.md` §4 required. A material is an **asset** with an id, its file a scene file
  with one root; shaders are hand-written WGSL with `#include`, `#ifdef` and a pipeline cache keyed
  by the defines. No material graph. What a `Material` *holds* arrives with meshes.
- `amadeo-physics` — rapier 2D and 3D behind engine traits. Rigid bodies, colliders, joints,
  raycasts, collision events into `amadeo-events`. **The other half of M2, and not yet started.**
- Verified deterministic physics: cross-run reproducibility test, in CI.
  ✅ **ADR 0036 settles how, before the crate exists.** `enhanced-determinism` is on **permanently**,
  so physics is single-threaded and scalar; physics state **is** in the state hash, which is what
  makes gate 3 meaningful; and the rapier version is pinned exactly, so an upgrade is a deliberate
  replay regeneration rather than a mystery. Gate 4's frame-time budget must be measured knowing
  physics uses one core — and if physics is the limit, the answers are fewer bodies, better culling
  or sleeping inactive bodies, **not** relaxing this.

**Exit gate**
1. A 3D scene: imported glTF level, dynamic lighting, shadows, a physics-driven character controller
   you can walk around with.
2. A 2D scene from M1 still renders correctly and unchanged — proving coexistence, not replacement.
3. A physics-heavy replay (200+ bodies) reproduces bit-identically across runs and processes.
4. Frame time within a declared budget at a declared scene complexity. Numbers written down.

✅ **M2 is complete — all four gates met** (session 10).

---

## M2.5 — Worlds That Scale

*Goal: the engine can carry a world rather than a room.*

**Why this exists, and why it is numbered .5.** M2 proved 3D works at the scale of eleven boxes.
Five of the eight target games — Palworld, Minecraft, Terraria, Project Zomboid, Stellaris — need
something the Atrium says nothing about: a world larger than what is on screen, streamed, and cheap
enough to draw when most of it is not visible. Building M3's game-feel polish on a renderer that
draws every object every frame would be polishing the wrong layer.

**Numbered 2.5 rather than renumbering M3 onward** because `docs/adr/` is immutable by this project's
own rule and 24 references to M3–M7 live in decided ADRs. Renumbering would strand every one of them
in documents nobody is allowed to correct.

**Build**
- ✅ **ADR 0041 settles the threading model, resolving Q9** — the oldest open architectural question,
  and decided before the first background task exactly as Q9 demanded. Parallelism is allowed only in
  shapes where determinism is structural: jobs that own their inputs and return through a barrier or
  into a `Service`, and an `Inbox` that drains in **key order rather than completion order**.
  `amadeo-jobs` is built and has no dependencies at all.
- ✅ **Background asset loading.** The first consumer of `amadeo-jobs`, and **byte-identical** to the
  sequential path, failure messages included. ADR 0021's load barrier already forbade gameplay from
  observing load state, which is precisely what made this safe — groundwork laid three milestones
  early for a different reason.
- ✅ **`par_for_each_mut`** — within-system parallel iteration whose closure is `Fn + Sync`, so a
  captured accumulator does not compile. The `rayon` question was answered by measurement rather than
  argument: 1.29× at 2,048 rows, 3.35× at 16,384, 5.42× at 131,072, so a persistent pool would only
  help where this should not be used at all. No dependency taken.
- ✅ **Surface-nets terrain meshing** — `amadeo-voxel`, the **fourth producer of mesh data** after
  `BoxMesh`, `PlaneMesh` and `GltfPart`, which is ADR 0035's bet paying off a third time.
  **ADR 0042** settles the data model: a generated base plus sparse **hashed** edits, so an untouched
  world costs nothing to hash and a save file is a seed plus a diff.
- 🟡 **Chunked streaming.** Which chunks are active is decided **deterministically** from the
  player's position; the work is parallel; the simulation blocks on colliders it needs. ADR 0041 §2
  is the rule, and it is the thing most likely to be got wrong.
  - ✅ **Residency** — `ChunkKey`, `Viewer`, `Residency`, integer boxes per viewer (**ADR 0043**).
    Three nested sets, `collision ⊆ visual ⊆ data`, which turns the apron from something to remember
    into something a test enforces.
  - ✅ **The terrain source** — ADR 0042's generated base plus sparse edits, keyed by *world* sample
    coordinate so two chunks cannot disagree about a sample they share, and per-chunk fill and mesh.
  - ✅ **Static trimesh colliders** — geometry reaches the solver by **id**, not as a component,
    because `Shape` is `Copy` and `StableHash` and ADR 0042 will not have vertices in the state hash.
  - ✅ **ADR 0043 §4 amends ADR 0042 §2**: a chunk needs an apron on **both** sides, because the
    quads bridging two chunks were being emitted by neither.
  - ✅ **The pipeline** — `amadeo-terrain`. Meshes on the job pool, colliders meshed **inline** so
    the simulation blocks on ground it needs. Core has no engine dependencies at all.
  - ✅ **The ECS layer** — `TerrainViewer`, `TerrainChunk`, `stream_terrain`, `install`, behind the
    `engine` feature. Entities spawn from residency, never from mesh arrival.
  - ✅ **Digging** — `TerrainStreamer::edit`, invalidating up to eight chunks, with an edit version
    so a mesh from before a dig cannot land after it.
  - **Where edits live — Q29.** They are unhashed today, so a snapshot loses them. ADR 0042 §4's
    "component on a chunk entity" cannot be implemented as written, because chunk entities are now
    despawned by streaming.
  - ✅ **A game that assembles it** — `games/scarp`. Nothing is authored but the player, the camera
    and the sun; the ground is a function of the seed. **Exit gates 1 and 2 are met.**
  - **Q25 is now better posed and still open**: may a chunk's mesh depend on its neighbours'
    *resolutions*? ADR 0043 pinning colliders to one level made the seam a purely visual problem.
- ✅ **ADR 0044 — generated terrain uses exactly-specified arithmetic.** `f32::sin`, `cos` and `powf`
  are documented as non-deterministic across platforms *and across calls*, and a `TerrainSource`
  decides where a collider is. `amadeo-noise` is built from `+ - * /` and `floor` over integer
  hashing, with a literal sample hash CI checks on both platforms.
- ✅ **Frustum culling.** `Frustum` extracts six planes from a view-projection matrix and tests a
  mesh's world box against them. **One implementation, used by both the collection pass and
  `render.describe`**, so what is culled and what is reported cannot drift apart.
  **Two lists, not one**: the colour pass draws what the camera can see, the shadow pass what the
  light can. The first attempt kept a single list holding the union and culled *nothing* — a shadow
  box is `shadow_distance` in every direction, which in the Scarp is 140 units across a world 112
  wide, so every mesh was inside it.
- **Level of detail**, at least for terrain chunks — **Q25**, deliberately left open by ADR 0042
  because the honest options depend on how streaming ends up shaped.
- **`amadeo-math` over glam.** `Mat4` is hand-written scalar — fine for eleven objects, not for
  meshing a field. `docs/02` already specifies glam wrapped so the engine owns its surface.
- ✅ **GPU timestamp queries.** M2's gate 4 could not measure GPU time at all. Now every render pass
  is timed and attributed by label, behind `set_gpu_timing` — off by default, because reading the
  results stalls the pipeline and a profiler may stall where a game may not.
- **More than one light**, and **textures on materials** — currently colours only, and an imported
  model arrives untextured.

**Exit gate**
1. ✅ **A generated terrain world you can walk around**, streamed in chunks, with collision that
   works and shadows that land on it. `cargo run -p scarp`. Confirmed headlessly by
   `walks_on_generated_ground.rs` and visually by `amadeo capture`, which is what found the
   surface-nets winding defect.
2. ✅ **A replay of that world reproduces bit-identically** across runs, processes, *and thread
   counts* — the last being the one that proves ADR 0041 rather than assuming it.
   `a_walk_reproduces_at_every_thread_count` advances five worlds at 1, 2, 3, 5 and 8 workers **in
   lockstep**, comparing state hashes every tick for 480 ticks, over a walk with a turn and a dig in
   it. Watched failing against a deliberate ADR 0041 §2 violation.
3. ✅ **Frustum culling demonstrably reduces draw calls**, measured through `render.describe` rather
   than believed. The Scarp: **50 meshes exist, 20 are in view, 20 are submitted** — thirty fewer
   draw calls, a 60% reduction. `culling_reduces_draw_calls.rs` measures both numbers from one
   running world, and the rendered PNG is **byte-identical** to the pre-culling one, which is what
   correct culling looks like.
4. ✅ **Frame time within budget at open-world complexity, with GPU time measured this time.**
   `TIMESTAMP_QUERY` in the wgpu backend, requested only where the adapter offers it. The Scarp at
   640×360: **61 µs of GPU time** — shadow 9.2, view 24.6, post 4.1, present 4.1 — against a 16.67 ms
   budget, which is 0.4%. Appended to `docs/10-frame-budget.md`.

---

## M3 — Game Feel and Completeness

*Goal: the engine can produce a game you'd actually hand to someone.*

**Build**
- **The renderer grows up — ADR 0045.** M0–M2.5 built the renderer's *skeleton*: a graph, a depth
  buffer, one light, one shadow map, a post chain, and as of session 13 a base-colour texture. That is
  six features where a shipping renderer has forty, and it is why the M2.5 demo looks like a
  prototype. **The backend is not the reason** — everything below is implementable on wgpu's stable
  feature set today, and ADR 0045 has the evidence. In order of visual return:
  1. ✅ **Mipmaps and anisotropic filtering.** `amadeo_image::mip_chain` builds the levels on the
     CPU — averaging in **linear light**, not in sRGB, which is the classic mipmap bug and shows as
     textures that dim as they recede. Surfaces sample with `mipmap_filter: Linear` and 16× aniso;
     sprites are pinned to level 0 so hand-authored pixel art stays crisp.
     **⚠ Corrected in session 20.** This entry used to claim the payoff was "immediate and
     measurable elsewhere: `games/scarp`'s texture tile was 8 m only because there were no mipmaps,
     and is 4 m now". **The Scarp's ground is not textured.** `turf.material` has
     `base_colour_texture ""`, and `TEXTURE_TILE` scales UVs that nothing samples. The engine review
     found the same thing across the whole repository: **every texture slot of every material is
     empty**, so items 1–3 here are written, tested, and exercised by no content at all.
  2. **Normal mapping** — the largest perceived-detail gain per unit of cost in real-time graphics.
  3. **Metallic-roughness PBR** — `Material` has carried the fields since ADR 0033 and the shader
     reads neither, so every surface reads as coloured paint rather than as a material.
  4. **Sky and image-based lighting** — replaces the hardcoded `0.12` ambient (**Q28**). Probably the
     single biggest step towards looking like a real engine.
  5. **Shadow cascades** — ADR 0038 reserved the enum variant. One map over an outdoor scene is
     visibly blocky.
  6. Then: more than one light, point and spot lights, anti-aliasing, transparency, fog and aerial
     perspective, bloom (declared in `Environment`, never drawn), and ambient occlusion.

  **This is what exit gate 5 below actually requires**, and it is the milestone's real renderer exam.
- ✅ `amadeo-audio` — buses, 3D spatialisation, one-shots as events, `NullAudio` and a kira backend
  (ADRs 0059–0061). Still open: ducking, occlusion, compressed audio, a voice cap.
- 🟡 `amadeo-anim` — **a clip animates a reflected field (ADR 0066)**: a track names a component and a
  field by name, so nothing in the crate knows about any component type and adding an animatable
  property is never engine work. Hashed, because a clip that moves a `Transform` is a moving platform.
  `games/atrium`'s lantern sweeps and its lamp flickers, both authored in `.anim` text.
  Still to come: skeletal animation and skinning, blending, and a state machine.
- ✅ `amadeo-ui` — retained-mode game UI: **anchors plus flow** rather than flex (ADR 0062, and the
  reasoning is that a HUD is a placement problem and a menu is a flow problem), text shaped with
  `cosmic-text`, a glyph atlas, focus navigation by **authored order** (ADR 0063), and theming by
  named tokens (ADR 0064). Authored in scene files and visible to introspection, as required.
  Still open: drawing the focus differently, and pointer navigation.
- Particles / VFX basics.
- ✅ Save/load built on snapshots. `games/atrium`'s pause menu saves and resumes, and a resumed game
  is proven to be the same game as one that never stopped. **Versioning landed with ADR 0069**: the
  same file read leniently, with the integrity check made *conditional on a layout fingerprint*
  rather than dropped — so a player who has not updated keeps the full check, and a save still
  survives a component gaining, losing or renaming a field. Renames are a text file of `old -> new`.
  Still open: real per-version migrations, which are the only thing that survives a field changing
  *meaning* rather than name and which nothing needs yet (`TypeInfo::version` is written into every
  file so they stay additive), and **where a save file should live — Q38**.
- Input remapping UI, controller support.
- First genre modules, prioritised by the target games (`00-vision.md`):
  - ✅ **movement and ground detection** — `modules/amadeo-character`, ADR 0037. Still to come:
    crouching, coyote time, imparting velocity to dynamic bodies.
  - ✅ **the camera rig, as its own module, with both perspectives** — `modules/amadeo-camera` has a
    third-person `FollowCamera` and a `FirstPersonCamera`, separate components sharing one aiming
    system. Schedule I and Backrooms are first-person, Palworld is third, and neither is privileged.
  - ✅ **`mod-behaviour`** — a **state machine over named facts** (ADR 0068). The game writes
    `"sees_player"` and reads `"pursue"`; the module knows neither. Chosen over a behaviour tree for
    legibility — "why is it doing that" is one field — and because the expensive half is the
    *boundary*, which is the same whichever sequencer sits on top. `games/atrium` has a watcher that
    notices you, chases, searches and gives up, so the module was designed against a real user.
  - ✅ **`mod-interaction`** (look-at, use) — `modules/amadeo-interaction`. A sphere swept forward
    from an `Interactor`, because a ray demands the player aim at a door handle exactly. Built on
    `ShapeHit::entity`, which was added for it: a cast used to say where it stopped and not what it
    stopped against. Still to come: a held item, which is `mod-inventory`'s half.
  - ✅ **`mod-inventory`** (items, stacks, containers) — `modules/amadeo-inventory`, **ADR 0070**.
    An item is an **entity**, always, and a stack is one entity with a count, so "a stack of fifty
    arrows is one row" was never a property of values. **Storing one removes its `Transform`**,
    which is sufficient because every pass that draws or simulates a thing already requires one —
    read rather than reasoned about. `games/atrium` has a brass key you walk up to, pick up, carry
    through a save, and drop. Still to come: equipment, weight, a grid layout.
  2D modules (`mod-tilemap`, `mod-platformer2d`) drop to M7 unless a specific need arises.

**Exit gate**

**A single-player first-person atmospheric horror slice** — Inside the Backrooms in shape, not in
scale. Chosen in session 2 as the smallest genuinely finishable complete game that is also the hardest
test of the renderer. Reasoning in `00-vision.md` § The first game to actually finish.

1. ✅ **Complete, not a demo.** Title screen → playable loop → lose state (caught) and win state
   (escape) → pause → save → quit → resume from save. With sound design and music.
   `games/warren` has all of it: five screens authored in `hud.scene`, both endings, a HUD, a save
   slot, and a way to start over. The sound is placeholder rather than sound *design* — six
   generated clips, meant to be replaced by dropping real `.wav` files in with the same ids — and
   there is no music.
2. ✅ **Bounded procedural interiors** — assembled from handcrafted room pieces, not one static
   level. **ADR 0071 for the artefact, ADR 0072 for what building it found.** `games/warren`'s
   `layout` binary writes a scene from a seeded room graph over **eleven** prefab pieces, always
   connected and always looped; it emits a **file** rather than a seed, so the formatter, the
   validator, prefab instancing and the editor all work on the result and a person can move a door
   by hand.
   A layout also chooses five **landmarks** out of the graph — where you wake up, the way out (the
   furthest room), the key (the largest detour, so provably off the shortest route), the torch (one
   door away) and the warden (half way along) — and the game boots into the result.
   **It tested the prefab-instancing design under real use, which is what this gate item is for, and
   the design failed.** A prefab has exactly one root, so a piece with two colliders must put them on
   children — and `step_physics` was storing world poses into child transforms, so every generated
   interior had been scattered across a hundred metres since the day the generator was written. It
   was invisible to `amadeo check`, to a green suite, and to a capture taken at tick 1. ADR 0072.
3. ✅ **At least one pursuing entity** with distinct AI states (idle, search, pursue, lose interest),
   driven by `mod-behaviour`. `games/warren`'s warden: four states over one named fact, authored in
   the scene, slower than the player so a chase can be won.
4. **Inventory and interaction** — pick up and use at least a flashlight and a key-type item.
5. 🟡 **Atmosphere holds up.** A dark corridor with a moving flashlight that reads as genuinely
   atmospheric. This is the renderer's real exam: dynamic lighting, shadow quality, fog, and a
   post-process stack. If this works, the other two art directions are easier.
   **Fog landed with ADR 0073** — a forward term on the surface shader rather than a post-process,
   which is why it did not need the depth buffer ADR 0034 said it was waiting for. Off by default
   and byte-identical when off. `games/warren` also has a real environment map now (`--bin gloom`),
   so an indirect surface is lit rather than exactly black.
   **Still open, and it is the biggest remaining visual step: volumetric light shafts** — the torch
   beam is not visible in the air, which is most of what a horror flashlight *is*. ADR 0073 records
   why it was not paid for now and that it raymarches through exactly the fog this added.
6. 🟡 **Audio carries weight.** Spatialised sound, occlusion or at least attenuation, reactive music
   or stingers. Horror lives or dies here, which makes it a good forcing function for the audio
   system.
   `games/warren` has the spatialisation and the stingers: a **looping breath on the warden**, so
   distance and direction tell you where it is without seeing it, plus footsteps, a chime and a
   sting for each ending. Attenuation and panning are kira's, which is why ADR 0059 chose it.
   **Still open: occlusion** — the warden is exactly as loud through a wall as through a doorway,
   which in a game made of corridors is the thing most worth fixing next — and reactive music, of
   which there is none at all.
7. Built collaboratively: Justin does some of it, Claude does some of it, in the same repo, with clean
   git history and no merge disasters.
8. Runs at a stable 60fps on this machine, verified against declared budgets.

---

## M4 — The Editor

*Goal: full human authoring, with parity preserved.*

**Build**
- `amadeo-editor` as a separate-process RPC client (see `04-subsystems.md` §16).
- Scene tree panel, generated inspector, asset browser, viewport with camera controls.
- Transform gizmos, multi-select, snapping.
- Play-in-editor using the real loop and real determinism.
- Undo/redo expressed as protocol operations.
- Tilemap painting tools.
- Live tweaking of a running game, with the option to persist changes back to text.

**Exit gate**
1. Justin builds a complete level using **only** the editor, never opening a text file.
2. Claude builds an equivalent level using **only** text and RPC, never opening the editor.
3. Both scenes load identically, round-trip byte-stably, and produce reviewable diffs when the other
   party edits them.
4. **Protocol completeness audit:** the editor uses no capability the CLI lacks. Any gap found is
   filed as a protocol bug and closed.

**Why gate 4 matters:** this is the milestone where parity is most likely to quietly break. The audit
is the enforcement mechanism for I5.

---

## M5 — Ship It

**Build**
- Windows native export: packaging, asset bundling, reproducible content builds.
- Web export via wasm/WebGPU (the wgpu payoff).
- Profiling tooling: frame budgets as CI assertions, chrome-trace export, memory tracking.
- `amadeo new` project templates.
- Generated API documentation, checked in CI so it can't rot.
- Crash reporting with replay attachment — a bug report that reproduces itself.

**Exit gate**
1. A distributable Windows build of the M3 game that runs on a machine without a toolchain.
2. The same game running in a browser.
3. A performance regression introduced deliberately is caught automatically by budget assertions.

---

## M6 — Co-op Multiplayer

*Goal: make the co-op the target games all require. Additive, because the hooks were reserved in
M0–M2 (ADR 0006).*

**Build**
- Transport and connection lifecycle (join, leave, timeout, reconnect).
- Snapshot delta encoding, bandwidth management, interest management.
- Client-side prediction and server reconciliation for movement and interaction.
- Server-authoritative physics; the dedicated server comes largely free from invariant I7.
- Replication driven by the `amadeo-reflect` annotations added back in M1.

**Exit gate**
1. The M3 horror slice runs in 2–4 player co-op, listen-server and dedicated-server.
2. Movement feels correct on a client with 100ms simulated latency.
3. A client joining mid-session receives correct world state.
4. Single-player still works, unchanged, through the same code path.

**Scope discipline:** ADR 0006 authorises no networking machinery before this milestone. If earlier
milestones start growing transport or prediction code, push it back here.

---

## M7+ — Modules and Actual Games

The engine becomes infrastructure and attention moves to games. Module work continues indefinitely —
per `01-architecture.md`, modules are the vocabulary I compose with, and every good module reduces how
much novel code a new game needs.

Toward the target games: `mod-worldclock` (day/night, NPC schedules — Schedule I and Palworld both need
it), `mod-crafting`, `mod-building` (base/property placement), `mod-creature` (companion AI, taming),
`mod-pathfinding`, `mod-dialogue`, `mod-vfx`. 2D modules (`mod-tilemap`, `mod-topdown`) land here.

Remaining deferred items become live options: terrain and world streaming (needed for a Palworld-scale
open world), localization, mobile, visual scripting.

---

## Scope reality check

Unified 2D/3D + a full graphical editor + an AI-native introspection layer is, honestly, Godot-scale
ambition. Godot has had many contributors over many years.

This is achievable at the pace of a human/AI pair only by holding three lines:

1. **Reuse aggressively for solved problems.** rapier, wgpu, winit, glam, egui, glTF, symphonia. We
   write the engine, not the dependencies. Every "let's write our own X" needs to justify itself
   against an existing crate.
2. **Non-goals stay non-goals.** `00-vision.md` has the list. The list is a load-bearing part of the
   plan, and the temptation to relax it will be constant.
3. **Vertical slices, always.** A working thin game beats ten impressive half-subsystems, every
   single time. The gates above exist to enforce this.

The realistic risk is not failure — it's a beautiful, half-finished engine that never runs a game.
Every gate above is designed against exactly that outcome.
