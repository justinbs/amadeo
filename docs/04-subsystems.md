# 04 — Subsystems: Key Areas to Settle Before Building

Each entry states the subsystem's job, the decisions that must be made before writing it, the
decisions already made, and the milestone it lands in.

**Legend:** ✅ decided · ⚠️ needs a decision · 🔬 needs a spike (measure, don't argue)

The first six sections correspond to the areas in the original brief. The sections after that are
areas the brief didn't list but that shape the architecture just as much — several of them, especially
**Determinism & Time**, **Reflection**, and **Scene Format**, must be settled *before* the ones in the
original list, because the others depend on them.

---

## 1. Core Application Layer — `amadeo-core`, `amadeo-app` · M0

**Job:** process lifecycle, the main loop, plugin/module registration, configuration, logging, IDs and
handles, arena allocation, the error model.

✅ Fixed-timestep loop with decoupled render, spec'd in `01-architecture.md`.
✅ Plugin model: a module registers components, systems, node types, and assets. No privileged access.
✅ Headless is a first-class mode, not a flag bolted on (I7). Null backends for render/audio/input.
✅ Error model: `thiserror` in libraries, no panics in engine crates, context-rich messages.

⚠️ **Module load order and determinism.** Registration order must not affect simulation order —
otherwise determinism depends on module list order. Resolve with explicit system ordering labels
rather than registration sequence.
⚠️ **Config layering.** Engine defaults → project config → user config → CLI overrides. Needs a
defined precedence and a way to dump the effective config (the agent needs to know what's actually
in force).
⚠️ **Threading model.** How many pools, what runs off the sim thread (asset loading, audio mixing,
render submission), and how results re-enter the deterministic zone in a fixed order. This is where
determinism is most often lost; decide it before adding the first background task.

---

## 2. Event System — `amadeo-events` · M0

**Job:** decoupled typed communication between systems, and the semantic activity log.

✅ Typed, double-buffered queues. No immediate-dispatch callbacks in simulation — they make ordering
implicit and allow reentrancy.
✅ Write during tick N, read during tick N+1 by default; same-tick reads allowed only across declared
stage boundaries.
✅ Events are exposed to the agent as a per-tick log (`events.since`). A cheap semantic diff of what
happened, much cheaper than comparing full state.

⚠️ **Retention policy.** How many ticks of event history to keep in memory, and whether that's
configurable per event type. Needed for time-travel debugging and for `events.since` to be useful.
⚠️ **Ordering guarantee within a tick.** Insertion order per type is easy; a total order across types
needs a sequence number. Decide whether the agent-facing log needs total order — it probably does, for
causality reading.
⚠️ **Command buffers vs events.** Structural changes (spawn/despawn/add component) are deferred
commands, not events. Keep the two concepts strictly separate; conflating them is a common source of
ordering bugs.

---

## 3. Entity Component System — `amadeo-ecs` · M0

**Job:** the runtime data model. Everything else is downstream of it.

✅ Archetype-based storage, structure-of-arrays. Chosen for cache behavior *and* for uniformity —
uniform layout with known schemas is what makes automated reasoning about state possible.
✅ Components are plain data. No side-effecting methods, no `Rc`/`RefCell`.
✅ Hierarchy is data: `Parent`/`Children` + `LocalTransform` → `GlobalTransform` via a propagation
system.
✅ Structural changes go through deferred command buffers, merged in deterministic order.

⚠️ **Entity ID scheme.** Generational indices are the obvious choice. But the scene format needs
*stable, human-meaningful* IDs that survive reordering and merges (I2) — so there are two ID spaces
(authoring identity vs runtime handle) and a mapping between them. Design both together; retrofitting
stable authoring IDs is painful. **Also design the network identity space here** (ADR 0006) — a third
space, shared across processes. Nearly free while designing the other two, invasive later.
⚠️ **Authority.** An explicit notion of who owns an entity, per ADR 0006, even though it is always
`Local` until M6. Systems written against it from the start stay correct; systems that assume universal
write access all need revisiting.
⚠️ **Query API ergonomics.** This is the single most-used API in the engine, by both authors. Worth
prototyping two or three shapes and judging which reads best, rather than copying one from another
engine reflexively.
⚠️ **Change detection.** Per-component tick stamps enable "only run when changed" and make the
agent's `snapshot.diff` cheap. Costs memory and bookkeeping. Probably worth it — decide in M0.
⚠️ **Relations beyond parent/child.** Godot has one tree; real games want many relationships
(equipped-by, targeting, owned-by). Decide whether to support general entity relations now or model
them as plain components. Leaning: plain components first, revisit if it hurts.
🔬 **Archetype fragmentation.** Many small archetypes destroys iteration performance. Measure with a
realistic component mix before committing to the storage strategy.

---

## 4. Rendering Pipeline — `amadeo-render` · M0 (window) → M1 (2D) → M2 (3D)

**Job:** get pixels on screen for both 2D and 3D from one coherent architecture.

✅ wgpu backend. Render graph architecture (declared passes with resource dependencies) rather than a
hardcoded pipeline — this is what makes unified 2D/3D and later additions tractable.
✅ Rendering reads simulation state, never writes it.
✅ Must be fully disableable (null backend) for headless runs.

⚠️ **How 2D and 3D coexist.** The real decision of this subsystem. Options: (a) 2D as orthographic 3D
with sprites as textured quads — one pipeline, elegant, but 2D-specific optimizations get awkward;
(b) two separate pipelines sharing the render graph and device — more code, better per-mode quality;
(c) 2D as a compositing layer over the 3D pass. Leaning **(a) with a specialized sprite batcher**, but
this deserves an ADR before M1 because it's expensive to reverse.
⚠️ **Sorting and layering.** 2D needs painter's-order layers; 3D needs depth plus separate transparent
sorting. A unified scheme that serves both without surprising either.
⚠️ **Material and shader model.** Hand-written WGSL, or a material graph, or a preprocessor with
includes and variants? Shader variant explosion is a classic engine tarpit. Decide the strategy before
writing the second shader, not the twentieth.
⚠️ **Camera model.** Multiple cameras, render targets, viewports, and how 2D UI/HUD cameras compose
with world cameras.
⚠️ **`render.describe` support.** Screen-space bounds of visible entities must be extractable, since
that's a primary agent verification channel. Design it in — it needs data the culling pass already
computes, so it's nearly free if planned and awkward if not.
🔬 **Sprite batching throughput.** Target a concrete number (e.g. 20k sprites at 60fps) and measure
early. Numbers, not vibes.

---

## 5. Asset Management — `amadeo-assets` · M1

**Job:** get data from disk into memory, correctly, asynchronously, and hot-reloadably.

✅ Import pipeline: source assets (`.png`, `.gltf`, `.wav`) are compiled once into internal formats.
The runtime never parses source formats.
✅ Asset metadata is text and hand-editable (I1) — import settings live next to the asset, in a
`.ama-meta` sidecar or similar.
✅ Handle-based access with async loading. Hot-reload via filesystem watch.

⚠️ **Load timing must not affect simulation.** If spawn order or behavior depends on which asset
finished loading first, determinism dies. Likely answer: simulation blocks on a declared asset set
before a scene becomes active, so load order is never observable. Decide before the first async load.
⚠️ **Identity: paths or GUIDs?** Paths are human-readable and diff-friendly (good for I1) but break on
move/rename. GUIDs survive moves but are opaque and are exactly what makes Unity's files unreadable.
Leaning: **stable paths as the primary identity**, with a rename-tracking tool. Needs an ADR.
⚠️ **Cache invalidation and build determinism.** Same source in, same compiled bytes out — otherwise
asset builds churn and can't be cached or verified.
⚠️ **Dependency graph.** A material references textures; a prefab references meshes. Needs cycle
detection and a defined unload policy.
⚠️ **Missing/failed assets.** Placeholder assets and a structured error rather than a crash — the
agent must be able to keep working and *see* what's broken.

---

## 6. Physics & Math — `amadeo-math` · M0, `amadeo-physics` · M2

**Job:** spatial math, collision, dynamics.

✅ `glam` wrapped by `amadeo-math`, so we own the public surface.
✅ `rapier` for both 2D and 3D, behind engine-owned traits so it's replaceable.
✅ Physics steps inside the deterministic zone at fixed dt.

⚠️ **Determinism configuration.** rapier has a deterministic mode with real constraints (enable
`enhanced-determinism`, fixed iteration counts, ordered body insertion). Verify cross-run
reproducibility with a test *before* building anything on top of it — this is a load-bearing
assumption.
⚠️ **f32 vs f64 vs fixed-point in simulation.** f32 is fastest and standard; strict cross-*platform*
determinism eventually wants fixed-point. Since we're Windows-first and single-platform for now, f32
is fine — but note it in the ADR so the future decision is informed rather than surprising.
⚠️ **How much physics is engine vs module.** Character controllers, one-way platforms, coyote time,
and jump buffering are *genre* concerns (I4) and belong in modules, not in `amadeo-physics`. Draw this
line explicitly; it's where engines usually leak genre logic downward.
⚠️ **2D and 3D in one project.** Whether a project picks one dimension at build time (feature flag) or
can use both simultaneously. Simpler: pick one per project. Decide in M2.

---

# Areas the original brief didn't list

These are not lower priority. Several outrank the list above in ordering.

## 7. Determinism & Time — cross-cutting · M0 · **build this first**

**Job:** the property that everything in `03-ai-native-design.md` depends on.

✅ Fixed timestep, `tick` as the only clock in simulation, seeded RNG as a world resource,
ordered collections in simulation paths, deterministic command merge. `adr/0005`.

⚠️ **Where the seed lives and how it forks.** Systems needing randomness must draw from a
deterministic per-system or per-entity stream, not a shared mutable global — otherwise system
execution order leaks into results.
⚠️ **State hashing.** What's included, what's excluded (render caches, timings), and how stable it is
across builds. This hash is the assertion in every golden replay test, so its definition is critical.
⚠️ **Snapshot format and cost.** Full-world serialization per snapshot is simple but expensive;
delta snapshots are cheap but complex. Start full, measure, optimize if needed.
⚠️ **Fixed dt value.** 60Hz is conventional. 120Hz gives better physics fidelity at 2x cost. Pick and
document; changing it later invalidates every recorded replay.

## 8. Reflection & Schema — `amadeo-reflect` · M0/M1 · **build this second**

**Job:** the single registry that powers serialization, the editor inspector, and agent introspection.

Everything above depends on it, which is why it's early. See `03-ai-native-design.md` Pillar 2.

⚠️ **Derive macro vs manual registration.** A `#[derive(Component, Reflect)]` macro is the ergonomic
answer and adds proc-macro compile cost. Almost certainly worth it.
⚠️ **How rich is the metadata?** Field names and types are the minimum. Ranges, units, tooltips, and
enum variants are what make generated inspectors and agent guidance genuinely good. Decide the
attribute vocabulary early — adding it later means touching every component.
⚠️ **Replication annotations** (ADR 0006). Sync policy, interpolation hint, and authority belong in this
same vocabulary. Add them in M1 while components are being authored and their semantics are freshest —
not in a later sweep across the entire engine. Unused until M6, and that is fine.
⚠️ **Versioning and migration.** When a component gains or renames a field, old scene files must still
load. Needs a version tag and migration hooks, or every format change breaks every saved project.

## 9. Scene & Prefab Format — `amadeo-scene` · M1 · **build this third**

**Job:** the shared authoring surface. The literal thing Justin and Claude both write to.

✅ Text, hand-writable, canonical, byte-stable, stable IDs. `adr/0003`, `adr/0004`.

✅ **The concrete syntax is decided: a custom, indentation-based, line-oriented format** (ADR 0014),
chosen by Justin from four hand-written candidates in `spikes/q2-scene-format/`. The spike's main
empirical finding was a negative one — diff behaviour, the criterion everyone expected to decide it,
is identical across all four. What decided it was compactness (roughly half of RON), keeping the tree
visible in the file (which ruled TOML out), and owning the error messages, which Pillar 5 makes a
functional requirement. **If ever revisited, the fallback is TOML, not KDL.**

✅ **Parser and canonical writer exist** with line-numbered errors and a byte-stable round-trip test.
⚠️ **Layer 2 is not built:** binding parsed values to real component types via the reflection
registry, narrowing numbers to declared widths, and instantiating a document into a `World`.
⚠️ **Prefab override semantics.** The hardest problem in this subsystem, and where Unity is genuinely
bad. An instance overriding some fields of a prefab, prefabs nesting inside prefabs, changes to a
prefab propagating to instances that haven't overridden that field. Design carefully; leaning toward
explicit, visible-in-text overrides with no hidden state.
⚠️ **Merge friendliness.** Two authors editing one scene. Line-oriented, stable-ordered, one-property-
per-line formatting makes git merges tractable. This is a formatting constraint driven by
collaboration, and it's why "whatever the serializer emits" is not acceptable.
⚠️ **Scene composition.** Sub-scene instancing, additive loading, streaming.

## 10. Game Logic Authoring & Hot Reload — **no crate** · resolved in M0

**Job:** how gameplay code gets written and how fast a change becomes visible.

✅ **Resolved by measured spike — ADR 0011.** Game logic is **Rust systems in the game crate**. There
is no `amadeo-script`, no scripting VM, and no dynamic reload. Four candidates were prototyped and
measured in `spikes/q1-game-logic/`; the headline is that a one-line gameplay edit rebuilds in
0.9–2.0 s, so the compile-time crisis this subsystem was invented to solve does not exist at this
scale.

✅ **The escape hatch is chosen in advance.** If a gameplay rebuild sustains above 5 s, or getting
back to the state of interest sustains above 2 s once snapshots exist, the answer is **WASM via
wasmtime** — measured bit-identical to native Rust at 1.24× runtime cost. Re-run
`spikes/q1-game-logic/measure.ps1` before invoking it; the trigger is a number, not a feeling.

⚠️ **The real iteration-loop investment is snapshot/restore**, not reload. Re-simulating to the point
of interest is the only cost that grows with session length (linear, ~21 µs/tick). Promoted to an M1
priority in `05-roadmap.md`.

⚠️ **Luau remains available outside the deterministic zone.** Its `f64` arithmetic does not agree
with `f32` components, which rules it out for simulation — but menus, quest triggers, and dialogue
are not simulation, and its 0.4 ms reload is real. A separate ADR at M3 if it is wanted.

⚠️ **Keeping the crate graph small and shallow is now load-bearing.** ADR 0011 rests on the measured
rebuild times, and those degrade if the graph grows wide or deep.

## 11. Input — `amadeo-input` · M0

**Job:** devices in, deterministic actions out.

✅ Action-based abstraction (`Jump`, `MoveX`), never raw key checks in gameplay. Required for replay
determinism *and* for remapping and controller support.
✅ Input is sampled once per simulation tick from either a live device or a replay stream — the
simulation cannot tell the difference. This is what makes replays work.

⚠️ **Action map format.** Text, reflected, hand-editable, editor-editable. Same requirements as scenes.
⚠️ **Analog, deadzones, and buffering.** Deadzone shaping and input buffering (fighting-game style)
are game-feel concerns — decide what's engine and what's module.
⚠️ **Recording fidelity.** Record actions (compact, robust) rather than raw device events. Actions are
the deterministic boundary.

## 12. Audio — `amadeo-audio` · M3

**Job:** sound. Consistently underestimated, and a huge share of perceived game quality.

⚠️ **Library choice.** `kira` (game-oriented, good mixing/tweening) vs `rodio` (simpler) vs direct
`cpal`. Leaning kira.
⚠️ **Audio must not affect simulation.** Mixing runs on its own thread at its own rate; simulation
fires events, audio consumes them. If simulation ever waits on audio, determinism is gone.
⚠️ Buses, ducking, 2D/3D spatialization, and a null backend for headless (I7).

## 13. Game UI — `amadeo-ui` · M3

**Job:** menus, HUD, dialogue boxes, inventory screens.

**Explicitly not egui.** egui is the *editor's* UI. Game UI must be authored in scene files, themed,
animated, and navigable by controller. Different requirements, different system. Conflating them is a
mistake worth naming twice.

⚠️ **Layout model.** Flexbox-like, constraint-based, or anchor-based. Flex is familiar to both authors
and well-understood.
⚠️ **Retained vs immediate mode.** Retained, to be authorable in scene files and inspectable by the
agent. Immediate-mode UI is invisible to introspection, which breaks the whole observability story.
⚠️ **Focus navigation.** Controller/keyboard focus traversal. Always an afterthought, always painful
to add later.
⚠️ **Text rendering and font handling.** Deceptively large: shaping, atlasing, SDF vs raster.

## 14. Animation — `amadeo-anim` · M3

⚠️ Sprite animation, skeletal animation (glTF skins), blend trees, state machines, and tweens are four
fairly separate systems. Decide which are engine and which are modules.
⚠️ Animation state machines and gameplay state machines want to be the same abstraction. Deciding this
early avoids two parallel half-systems.
⚠️ Must be tick-deterministic — animation driving gameplay (hitboxes on frames) is common and must
reproduce exactly.

## 15. Save/Load & Serialization of Live State — M3

✅ Falls out of determinism: a save is a snapshot, optionally plus the replay log.
⚠️ Save format versioning and migration across engine and game changes.
⚠️ What's excluded from saves (caches, handles, derived state) — needs a reflection-level annotation.

## 16. Editor — `amadeo-editor` · M4

**Job:** make the engine pleasant for a human, without ever becoming the source of truth.

✅ Built as an RPC client of `amadeo-agent` (I5). No privileged access. Structurally enforced by the
crate graph.
✅ Inspector widgets generated from the reflection registry, not hand-written per component.

⚠️ **In-process or separate process?** Separate is architecturally purer (proves the protocol is
sufficient) and better for crash isolation. In-process is simpler and faster. Leaning separate,
precisely because it forces the protocol to be complete — if the editor needs something the protocol
lacks, that's a bug in the protocol, and we find out immediately.
⚠️ **Undo/redo.** Must be expressed as protocol operations so both authors' changes are undoable, and
so undo history never becomes hidden editor state.
⚠️ **Play-in-editor.** Runs the real loop with real determinism; not a special mode with different
semantics.
⚠️ **Gizmos**, multi-select, snapping, and tilemap painting — where a GUI genuinely beats text, and
worth real investment.

## 17. Build, Packaging & Export — M5

⚠️ Asset bundling and compression; a reproducible content build.
⚠️ Windows native export first; web (wasm) export second — cheap because wgpu targets WebGPU, but not
free (threading, filesystem, and audio all differ).
⚠️ Project templates via `amadeo new`.

## 18. Observability & Profiling — M0 (hooks) → M5 (tooling)

✅ Per-system timings exposed via `profile.frame`.
⚠️ **Declared frame budgets per system**, so a regression is an automatic failure rather than a slow
realization. This is how an agent catches performance problems it cannot feel.
⚠️ `tracing` spans throughout, plus a chrome-trace export.
⚠️ Memory tracking per subsystem.

## 19. Documentation as Data — cross-cutting, continuous

⚠️ `amadeo describe` emits the machine-readable API surface (Pillar 2). Should run in CI so it can't
rot.
⚠️ Doc comments are the agent's primary API surface — treat them as load-bearing.
⚠️ A cookbook of worked examples ("how do I make a thing patrol") is disproportionately valuable to an
agent, because a working example beats a signature every time.

---

## 20. Networking — `amadeo-net` · hooks M0–M2, built M6

**Job:** co-op multiplayer. All three target games require it (`00-vision.md`).

✅ **Client-server with server authority and client prediction** — explicitly not deterministic
lockstep. ADR 0006.
✅ Hooks reserved during M0–M2; no transport code before M6. The cheap-now vs expensive-later split is
tabulated in ADR 0006.
✅ Dedicated server comes almost free from invariant I7 (everything headless-capable).

⚠️ Transport choice (QUIC vs a reliable-UDP library) — M6.
⚠️ Interest management for open worlds — matters at Palworld scale, not for bounded interiors.
⚠️ **How the agent interface layer behaves across a client/server split.** Does `world.query` target the
server's authoritative state or a client's predicted state? Both are legitimately useful. Worth deciding
before M6, since it determines how debuggable networked gameplay is — and networked gameplay is the
hardest thing in this project to debug without good introspection.

---

## Recommended build order

The order matters more than the list. Dependencies run downward:

```
1. math, core, ECS, determinism & time     ← the spine, M0. Nothing works without it.
2. reflection & schema                     ← unlocks serialization, editor, and agent at once
3. events, input, app loop                 ← M0
4. Q1 spike: game logic & hot reload       ← M0. ✅ Resolved by measurement (ADR 0011): no crate.
5. scene format + agent interface layer    ← M1. The collaboration surface goes live here.
6. assets, 2D rendering                    ← M1
7. 3D rendering, physics                   ← M2
8. audio, animation, UI, save/load         ← M3
9. editor                                  ← M4
10. build/export, profiling tools          ← M5
11. genre modules, then actual games       ← M6+
```

Items 1, 2, and 4 are the ones that are ruinously expensive to retrofit. Everything else can be built,
thrown away, and rebuilt without much loss — which is exactly why the early milestones should move
slowly and carefully and the later ones can move fast.
