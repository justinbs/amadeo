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
- 🔬 **The Q1 spike.** Prototype 2–3 game-logic hot-reload approaches. Measure edit→observe latency.
  Write the ADR. This blocks M1.

**Exit gate**
1. A colored quad moves under keyboard input in a real window.
2. Record 600 ticks of input. Replay headless, in a separate process, on a separate build. The state
   hash at ticks 100/300/600 is byte-identical to the windowed run.
3. `cargo test` passes in CI, including the golden replay.
4. Q1 is resolved by an ADR backed by measured numbers.

**Why this gate:** it exercises input → simulation → state → render → replay end to end. If this
works, the spine is real. If determinism is broken, it's broken *here*, when it costs a day instead
of a milestone.

---

## M1 — See and Inspect

*Goal: the collaboration surface goes live. From here on, I can work largely unattended.*

**Build**
- `amadeo-reflect` — derive macro, type registry, metadata vocabulary (ranges, units, docs),
  versioning + migration hooks.
- Canonical serialization: sorted keys, stable IDs, fixed number formatting.
- `amadeo-scene` — the scene/prefab text format (ADR first, with worked examples), parse →
  instantiate → reconstruct → canonical write. Prefab overrides.
- `amadeo-agent` v1 — JSON-RPC server: `world.query`, `world.entity`, `world.spawn`,
  `world.set_component`, `sim.step`, `sim.pause`, `events.since`, `scene.load`/`save`,
  `render.capture`, `render.describe`, `replay.*`, `snapshot.*`.
- Protocol spec in `docs/protocol/`, versioned.
- `amadeo-cli` — `new`, `run`, `check`, `fmt`, `test`, `describe`, `inspect`, `replay`.
- `amadeo-assets` — virtual FS, handles, async load with load-order isolation, hot reload, import
  pipeline, text sidecar metadata, placeholder assets on failure.
- 2D rendering — sprite batcher, textures, cameras, layers/sorting, transform hierarchy.
- Game logic layer, per the Q1 decision.

**Exit gate**
1. **A complete small 2D game — built entirely by Claude with zero editor use and zero human
   intervention.** Something on the order of: player moves, enemies patrol, collision, a score, a
   win state. Authored via text files and RPC only.
2. Verification of that game done purely through `inspect`, headless runs, and `render.describe` —
   with screenshots used only for final confirmation.
3. Scene round-trip test in CI: parse → serialize → byte-identical.
4. `amadeo describe` output is sufficient to write a new component and system without reading engine
   source. Tested by actually doing it.
5. Golden replays from M0 still pass.

**Why this gate:** it's the first real proof of the AI-native thesis. If I can't build a tiny game
without eyes, the design is wrong and we find out before 3D and the editor pile on top.

---

## M2 — The Third Dimension

**Build**
- ADR on 2D/3D coexistence (see `04-subsystems.md` §4) — decided *before* code.
- Render graph proper: declared passes, resource dependencies, transient targets.
- 3D: mesh rendering, PBR materials, directional + point lights, shadow maps, frustum culling.
- glTF import — meshes, materials, scene hierarchy, skins.
- Shader/material strategy per its ADR; WGSL organization and variant handling.
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
- First genre modules, prioritised by the target game direction (`00-vision.md`):
  **`mod-charcontroller3d`** (third-person movement, camera, ground detection), **`mod-behaviour`**
  (AI state machines for creatures), **`mod-inventory`** (items, stacks, containers). 2D modules
  (`mod-tilemap`, `mod-platformer2d`) drop to M6 unless a specific need arises.

**Exit gate**
1. **A small but genuinely complete game.** Title screen → playable loop → win and lose states →
   pause → save → quit → resume from save. With sound and music.
   Per `00-vision.md`, the intended shape is a 3D vertical slice sharing the target game's DNA:
   third-person character in a small handcrafted level, one creature with a few AI states that can be
   approached and befriended, and a working inventory. Deliberately small in scope, deliberately
   aligned in subsystems.
2. Built collaboratively: Justin does some of it, Claude does some of it, in the same repo, with clean
   git history and no merge disasters.
3. Runs at a stable 60fps on this machine, verified against declared budgets.

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

## M6+ — Modules and Actual Games

The engine becomes infrastructure and attention moves to games. Module work continues indefinitely —
per `01-architecture.md`, modules are the vocabulary I compose with, and every good module reduces how
much novel code a new game needs.

Candidates: `mod-topdown`, `mod-dialogue`, `mod-turnbased`, `mod-inventory`, `mod-vfx`,
`mod-pathfinding`.

Deferred items become live options here, in rough order of value: multiplayer (determinism and
snapshots are already in place for rollback), localization, mobile, visual scripting.

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
