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
| **I8** | **Reflection is not optional.** Every component and resource registers a machine-readable schema. If it can't be reflected, it can't be serialized, inspected, or edited. | One registry powers serialization, the editor, and agent introspection. |

## 3. Tech stack (decided)

- **Language:** Rust (2024 edition), `#![forbid(unsafe_code)]` outside explicitly audited modules.
- **Graphics:** `wgpu` — one API over Vulkan/DX12/Metal, and it targets WebGPU, so a browser export
  path stays open for free.
- **Windowing:** `winit`. **Math:** `glam`. **Physics:** `rapier` (2D+3D) behind engine-owned traits.
- **Editor UI:** `egui` (immediate-mode, in-process, cheap to build). Game UI is a separate,
  retained-mode system — do not confuse the two.
- **Primary target:** native desktop, Windows first. Web export is a later milestone, not a
  parallel obligation.
- **Game logic authoring:** **UNDECIDED — this is the highest-priority open question.** See
  `docs/06-open-questions.md` Q1. Do not assume an answer; do not build around one until it's settled.

Rationale and rejected alternatives: `docs/02-tech-stack.md` and `docs/adr/0002`.

## 4. Repository layout & dependency order

Crates are listed in dependency order. **A crate may only depend on crates above it.**

```
crates/
  amadeo-math        vectors, matrices, quaternions, rects, curves. No engine deps.
  amadeo-core        ids, handles, arenas, error model, logging, time, config
  amadeo-reflect     type registry, schema emission, canonical (de)serialization
  amadeo-ecs         archetype SoA storage, queries, schedules, change detection, commands
  amadeo-events      typed event bus, deferred queues, documented ordering
  amadeo-assets      virtual FS, async load, import pipeline, cache, hot-reload watch
  amadeo-input       device -> action mapping, deterministic input capture/replay
  amadeo-render      render graph over wgpu, 2D batcher + 3D pipeline, materials, cameras
  amadeo-audio       mixer, buses, spatialization (null backend required)
  amadeo-physics     rapier integration behind engine traits
  amadeo-anim        sprite anim, skeletal, state machines, tweens
  amadeo-ui          retained-mode game UI: layout, theming, focus navigation
  amadeo-scene       scene-tree authoring model, prefabs, instancing, text format
  amadeo-script      game-logic host (shape pending Q1)
  amadeo-agent       Agent Interface Layer: RPC, introspection, snapshots, replay, capture
  amadeo-app         plugin/module registration, main loop, lifecycle
  amadeo-editor      graphical editor. A CLIENT of amadeo-agent. No privileged access.
  amadeo-cli         the `amadeo` binary: new/run/check/fmt/test/replay/inspect/build/export
modules/             optional, genre-flavored. Core NEVER depends on these.
games/               actual games built with the engine
docs/                design docs and ADRs
```

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
- Prefer a working vertical slice over a complete horizontal layer. Every milestone must end with
  something runnable.
- Write the determinism test alongside the feature, not after. Retrofitting determinism is the
  single most expensive mistake available in this project.

**At the end of a session:**
1. Update `STATUS.md`: what landed, what broke, what's next, any new sharp edges.
2. Update `docs/06-open-questions.md` — remove resolved, add discovered.
3. Commit. Message body should explain *why*, not restate the diff.

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
| `docs/adr/` | You want to know why something is the way it is. |
