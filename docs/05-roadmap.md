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
  raycasts, collision events into `amadeo-events`.
- Verified deterministic physics: cross-run reproducibility test, in CI.

**Exit gate**
1. A 3D scene: imported glTF level, dynamic lighting, shadows, a physics-driven character controller
   you can walk around with.
2. A 2D scene from M1 still renders correctly and unchanged — proving coexistence, not replacement.
3. A physics-heavy replay (200+ bodies) reproduces bit-identically across runs and processes.
4. Frame time within a declared budget at a declared scene complexity. Numbers written down.

---

## M3 — Game Feel and Completeness

*Goal: the engine can produce a game you'd actually hand to someone.*

**Build**
- `amadeo-audio` — mixer, buses, 2D/3D spatialization, event-driven playback, null backend.
- `amadeo-anim` — sprite animation, skeletal animation, blend trees, state machines, tweens.
- `amadeo-ui` — retained-mode game UI: flex layout, theming, text rendering, focus navigation,
  authored in scene files and visible to introspection.
- Particles / VFX basics.
- Save/load built on snapshots, with versioning and migration.
- Input remapping UI, controller support.
- First genre modules, prioritised by the target games (`00-vision.md`):
  **`mod-charcontroller3d`** (movement, ground detection) with the **camera rig as a separate module**
  so first- and third-person are both supported — Schedule I and Backrooms are first-person, Palworld
  is third; **`mod-behaviour`** (AI state machines: patrol, pursue, search, flee);
  **`mod-inventory`** (items, stacks, containers); **`mod-interaction`** (look-at, pick up, use).
  2D modules (`mod-tilemap`, `mod-platformer2d`) drop to M7 unless a specific need arises.

**Exit gate**

**A single-player first-person atmospheric horror slice** — Inside the Backrooms in shape, not in
scale. Chosen in session 2 as the smallest genuinely finishable complete game that is also the hardest
test of the renderer. Reasoning in `00-vision.md` § The first game to actually finish.

1. **Complete, not a demo.** Title screen → playable loop → lose state (caught) and win state (escape)
   → pause → save → quit → resume from save. With sound design and music.
2. **Bounded procedural interiors** — assembled from handcrafted room pieces, not one static level.
   Tests the scene composition and prefab-instancing design under real use.
3. **At least one pursuing entity** with distinct AI states (idle, search, pursue, lose interest),
   driven by `mod-behaviour`.
4. **Inventory and interaction** — pick up and use at least a flashlight and a key-type item.
5. **Atmosphere holds up.** A dark corridor with a moving flashlight that reads as genuinely
   atmospheric. This is the renderer's real exam: dynamic lighting, shadow quality, fog, and a
   post-process stack. If this works, the other two art directions are easier.
6. **Audio carries weight.** Spatialised sound, occlusion or at least attenuation, reactive music or
   stingers. Horror lives or dies here, which makes it a good forcing function for the audio system.
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
