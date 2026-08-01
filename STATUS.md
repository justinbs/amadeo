# Amadeo — Current Status

**Last updated:** 2026-07-31 (session 6)
**Current phase:** **M0 complete. M1 well under way** — reflection, the scene format, the agent's read
layer, **and the agent protocol plus a working `amadeo` CLI** have all landed. What remains in M1 is
the 2D renderer, assets, snapshots, and the small game that closes the exit gate. Q3 and Q13 both
closed too, so nothing is blocked.
**Remote:** `origin → https://github.com/justinbs/amadeo.git` (private) — **nothing pushed yet**

---

## Where we are

Sessions 1–2 established scope, stack, and architecture. Session 3 built M0. Session 4 closed it by
resolving Q1. Session 5 built most of M1's foundations.

**Twelve crates plus one game**, all tested: `amadeo-derive`, `amadeo-core`, `amadeo-reflect`,
`amadeo-ecs`, `amadeo-transform`, `amadeo-events`, `amadeo-input`, `amadeo-render`, `amadeo-scene`,
`amadeo-agent`, `amadeo-app`, `amadeo-cli`, and `games/quad-demo`. **480 tests passing**; fmt, clippy
`-D warnings`, and rustdoc all clean. CI runs on Windows and Linux with a dedicated determinism job.

Four things work end to end today:

- **The engine runs.** `cargo run -p quad-demo` opens a window with a quad you steer with WASD.
  Deterministic at a fixed 60 Hz, records to a hand-editable `.replay` file, and replays against
  checkpoint state hashes in CI.
- **A text file builds a world.** A `.scene` file (ADR 0014) parses, formats byte-stably, and
  instantiates into a `World` using the engine's real components, hierarchy included.
- **The engine describes itself.** `amadeo_agent::describe` emits the full component schema as
  JSON — names, types, docs, units, ranges, replication — generated from the code, so never stale.
- **The CLI talks to a running game.** `amadeo describe Velocity` describes a type defined in
  `games/quad-demo`, answered over JSON-RPC by the game binary the CLI launched. Also `query`,
  `entity`, `schedule`, `status`, `call`, `check`, `replay`, and a standalone `fmt`.
- **A replay reproduces in a fresh process.** `amadeo replay games/quad-demo/replays/wander.replay`
  launches the game, plays a hand-written recording, and asserts four checkpoint hashes. This is the
  stronger half of the golden-replay claim — the in-process test proves a recording survives a
  rebuild, this proves it survives a new process — and it runs in CI.

**M0 exit gate: 4 of 4, nothing carried.** Gate item 2's "separate process" half — open since M0
because it needed `amadeo-cli` — closed in session 6: `amadeo replay` plays
`games/quad-demo/replays/wander.replay` through the real game binary in a fresh process, four
checkpoints asserted, and CI runs it in the determinism job.

**M1 exit gate: 1 of 5, with 2 and 4 now reachable.** Gate 3 (scene round-trip byte-identical) is
done. Gates 2 and 4 describe verifying and authoring *through* the CLI and RPC, which now exist —
gate 4 in particular ("`describe` output is sufficient to write a new component without reading
engine source") is testable today by actually doing it. Gate 1 (a complete small 2D game) still needs
the sprite renderer, which ADR 0018 has now unblocked. Gate 5 (golden replays still pass) holds.

**No blockers of any kind.** Q14, Q13, and two thirds of Q3 all closed in session 6, each built the
same session it was decided.

## The single most important thing to do next

**Build `GlobalTransform` and its propagation system.** ADR 0018 unblocked it by settling what a
transform is, and it has been waiting since ADR 0015. Composing a child transform with its parent is
the easy part; what was missing was the decision.

`GlobalTransform` is a **computed 4x4 matrix** — never authored, never written to a scene file, and
therefore free to be whatever the maths wants rather than whatever a human can type. That is what
lets the authored `Transform` stay Euler-degrees and hand-writable.

After that the 2D renderer is unblocked too: it can be written against `Transform` and `SortOrder`
without waiting for the pipeline choice, which ADR 0018 deliberately deferred.

Then, in order:

1. **`Resource: Reflect`** — the other half of I8, deliberately deferred by ADR 0013. Needs `Rng`'s
   state exposed so `SimRng` can reflect (retiring the `Debug`-based `StableHash` flagged as
   inelegant since M0), and map support in `Reflect` for `InputState`. Also what `world.resources`
   in the protocol is waiting on.
2. **`snapshot.take` / `snapshot.restore`.** The Q1 spike found re-simulation, not compilation, is
   what degrades the iteration loop — 382 ms to reach 5 simulated minutes, growing linearly.
3. **2D rendering** — sprite batcher, textures. No longer blocked: `Transform` and `SortOrder` are
   settled, and the pipeline choice is made *inside* the batcher rather than before it.

**Three things are undecided rather than unbuilt**, all in `docs/06-open-questions.md`:

- **Q3 (the last third) — which render pipeline shape.** Dropped to P2 by ADR 0018, which settled the
  two expensive parts. `RenderBackend` isolates what remains, so it blocks nothing; decide it while
  writing the sprite batcher, against a real throughput number.
- **Q7 — prefab override semantics.** The format records overrides visibly (the I1 requirement), but
  what they *mean* when a prefab changes under an instance is undesigned. Study Unity's and Godot's
  failure modes first.
- **Q4 — asset identity: stable paths or GUIDs.** Needs an ADR in M1; blocks `amadeo-assets`, which
  blocks prefab instancing.

Prefab *instancing* is unbuilt rather than undecided — it needs `amadeo-assets` to resolve a path at
all, which is why `instantiate` refuses a `from` line with an error saying exactly that.

## Q1 is resolved — ADR 0011

**Game logic is Rust systems in the game crate.** No scripting layer, no dynamic reload,
no `amadeo-script`.

Four candidates were prototyped and measured against one shared benchmark (a three-state enemy AI
over 64 entities, 1800 ticks). Everything is in `spikes/q1-game-logic/`, re-runnable via
`measure.ps1`.

| | edit → observe | state survives | hash vs native Rust | µs/tick |
|---|---|---|---|---|
| **A** pure Rust | 0.95 s (2.1 s in the real game) | no | reference | 4.6 |
| **B** cdylib | 0.69 s | yes | ✅ identical | 4.6 |
| **C** Luau | 0.4 ms | yes | ❌ **differs** | 109.7 |
| **D** WASM | 0.63 s | yes | ✅ identical | 5.7 |

**The recorded Luau prior was refuted, and it is worth knowing why.** Luau is not nondeterministic —
it reproduces perfectly across processes. But its numbers are `f64` and components are `f32`, so it
computes something *different* from the Rust reference: the two agree at tick 1 and diverge at tick 2.
That kills the prior's central mechanism specifically, because "graduate hot logic from Luau into
Rust" changes behaviour and invalidates every golden replay taken before the move.

**The premise behind the whole question was also wrong at this scale.** Q1 was written to avoid a
feared 30-second rebuild. Measured: **0.9 s** for a gameplay edit, **2.0 s** for `quad-demo` (which
links wgpu and winit), **3.2 s** for an engine-crate edit rebuilding everything downstream. There was
no crisis to solve, so the decision is to not pay a permanent architectural cost for it.

**WASM is reserved, not rejected.** It is bit-identical to native Rust (verified across two
optimisation levels) at 1.24× runtime cost, and it is the same artefact M5's web export needs. ADR
0011 names it as the escape hatch behind a measured threshold — a gameplay rebuild sustaining above
5 s. Check by re-running the spike, not by impression.

### Decided
- Name: **Amadeo**.
- Unified 2D **and** 3D from the start (not 2D-first, and not 3D-only). Restated in session 6: the
  three 3D target games order the work, they do not narrow the engine. `CLAUDE.md` §7 trap 9.
- Native desktop first, Windows as the primary target. Web export deferred to M5.
- Graphical editor **and** full text/code/headless parity are both first-class requirements.
- Stack: Rust + wgpu + winit + glam + rapier + egui. See `docs/adr/0002`.
- Scene tree is the authoring model; ECS is the runtime model. See `docs/adr/0004`.
- Text files are the only source of truth. See `docs/adr/0003`.
- Determinism is a hard invariant, designed in from tick zero. See `docs/adr/0005`.
- **Code must stay legible to a Rust-learning human.** Justin intends to read, debug, and fix the
  codebase himself. Boring Rust over clever Rust; accepted cost in verbosity. `CLAUDE.md` §6.
- **Target games: Palworld, Schedule I, Inside the Backrooms.** Deliberately different genres, scales,
  and art directions — used as a prioritisation signal. The intersection defines the core; the
  divergence defines what must stay pluggable. See `docs/00-vision.md` § Target games.
- **Renderer must not bake in an art style.** Configurable post-process stack, flexible dynamic
  lighting, fog/volumetrics. The three targets span stylised-realistic outdoors, low-poly, and dark
  atmospheric interiors.
- **Camera rig is separate from the character controller** — the targets are a mix of first- and
  third-person.
- **Multiplayer is no longer a non-goal.** All three targets are co-op. Client-server with server
  authority and client prediction (*not* deterministic lockstep). Hooks reserved during M0–M2, netcode
  built at M6. See `docs/adr/0006`.
- **First game to finish: single-player first-person atmospheric horror slice** at M3 — smallest
  genuinely finishable complete game, and the hardest test of the renderer.
- **Game logic is plain Rust in the game crate.** No scripting layer, no hot reload. WASM reserved as
  a pre-selected escape hatch behind a measured threshold. See `docs/adr/0011`.
- **`spikes/` exists** for prototypes that answer a question with a measurement. Separate cargo
  workspaces, frozen once their ADR is written. See `spikes/README.md`.

- **Q13 resolved — `ComponentId` is the hash of a component's canonical name**, not its Rust path.
  Moving a component between crates is free; renaming one is a deliberate, visible change. ADR 0017.
- **Q3 resolved, two thirds of it — one 3D `Transform`, and an explicit `SortOrder`.** 2D is the
  degenerate case rather than a separate type; rotation is Euler degrees so it stays hand-writable.
  The pipeline shape is deliberately still open. ADR 0018.
- **Q14 resolved — the game binary hosts the agent; the CLI launches it.** One-shot JSON-RPC over
  stdio, hand-written parser, `App` owns the `ComponentRegistry`. See `docs/adr/0016`.

### Not yet decided (blocking)

Nothing is blocking. Q14, the last P0, closed in session 6.

## Environment

Verified on this machine (2026-07-30):

| | |
|---|---|
| OS | Windows 11 Pro 26200 |
| CPU | AMD Ryzen 7 5700X3D (8C/16T) |
| GPU | NVIDIA RTX 4060 Ti — Vulkan and DX12 capable, fine for wgpu |
| RAM | 40 GB |
| Installed | Node 24.16, npm 11.13, git 2.53, Java 25 |
| **Rust** | ✅ rustup + rustc 1.97.1 + cargo 1.97.1, target `stable-x86_64-pc-windows-msvc`, in `%USERPROFILE%\.cargo\bin` |
| **MSVC build tools** | ✅ VS Build Tools 2022 17.14.37, MSVC 14.44.35207. Verified 2026-07-30: `cargo build` compiles **and links**, and the binary runs. |
| Editor | ✅ VS Code + rust-analyzer v0.3.2989 |
| **Toolchain status** | ✅ **No blockers.** Compiles, links, runs, tests. |
| Also missing | Python, cmake. Neither is needed. |
| Gotcha — PATH | Installers update the persistent PATH but not running processes. VS Code's integrated terminal needs **VS Code itself** restarted, not just a new tab. |
| Smart App Control | **Resolved.** It was blocking every binary this project builds — confirmed via event log (3118, policy `{0283ac0f-…}`). Justin disabled it (one-way change on Win11). If a future machine hits `os error 4551`, this is why; see `docs/07-working-with-the-code.md` §5. |
| Gotcha — winget | `winget install` on an already-installed package attempts an *upgrade* and silently ignores `--override`, so it cannot add a workload. Use the VS Installer to modify an existing install. |
| Gotcha — wgpu | This project is on **wgpu 30**, which differs from most material online. Read the crate source under `~/.cargo/registry/src/*/wgpu-30.0.0/src/api/` rather than trusting search results. `docs/07` records the three changes that cost the most time. |

## Next actions

**M0 is under way and unblocked.** Done so far, in the order it was built:
- ✅ Cargo workspace, workspace lints (`unsafe_code = "forbid"`), toolchain pinned
- ✅ Q5 resolved: 60 Hz fixed timestep (ADR 0007)
- ✅ ECS storage strategy decided: safe archetype columns, no unsafe (ADR 0008)
- ✅ `amadeo-core`: `Tick`, `FIXED_DT`, hand-written PCG32 `Rng` with stream forking, hand-written
  FNV-1a `StableHasher` (cross-checked against an independent implementation), `StableId` / `NetId` /
  `Authority` (the ADR 0006 hooks).
- ✅ `amadeo-ecs`: generational `Entity` handles, `ComponentId` derived from type *name* (not
  `TypeId`, which is not build-stable), type-erased-but-safe archetype columns, archetype migration
  on component add/remove, `iter` / `for_each_mut` / `for_each_pair_mut` queries, per-row change
  ticks, and `World::state_hash`.
- ✅ CI: fmt, clippy `-D warnings`, tests on Windows + Linux, a **determinism job** that runs the
  suite three times in separate processes plus a release build, and a rustdoc job.

- ✅ `Resource` (simulation state, hashed) and `Service` (engine machinery, **not** hashed) as two
  separate stores on `World`, with the distinction enforced by trait bounds — ADR 0009. Found by a
  failing determinism test rather than by design foresight.
- ✅ `amadeo-events`: typed double-buffered queues, a shared `EventClock` giving a total order across
  event types, and a `WorldEvents` extension trait. Events written on tick N are readable on N+1.
- ✅ `amadeo-app`: `Stage`, `Schedule` with `before`/`after` constraints resolved by topological sort
  with **alphabetical tie-breaking** (so registration order cannot influence results), the
  fixed-timestep loop with both `run_ticks` (deterministic, ignores wall time) and
  `advance_real_time` (accumulator, capped at 8 ticks/frame to prevent a catch-up spiral), and
  `SimRng`.
- ✅ Determinism integration suite (`crates/amadeo-app/tests/determinism.rs`) — 14 tests covering
  repeat-run agreement, per-checkpoint agreement, seed divergence, headless-vs-windowed equivalence,
  real-time-vs-exact-tick equivalence, stall recovery, and event ordering.

- ✅ `amadeo-input`: `ActionId` (gameplay reads named actions, never keys), `InputState` with
  `just_pressed`/`just_released` edge detection, `InputSource` implementations (null, scripted,
  replay), and a `Recorder` that writes change-only recordings.
- ✅ **The replay file format** — the project's first authored text format, built to the rules every
  later format must follow (I1/I2): hand-writable, line-oriented, canonically ordered, byte-stable
  round-trip, LF endings, and parse errors carrying line numbers. Rejects a tick-rate mismatch rather
  than replaying it wrong (ADR 0007).
- ✅ **Golden replay harness** with a committed fixture at
  `crates/amadeo-app/tests/golden/walk_and_jump.replay`. A recording made once is replayed by every
  later build and asserted against checkpoint state hashes. Regenerate deliberately with
  `UPDATE_GOLDEN=1 cargo test -p amadeo-app --test golden_replay`.

- ✅ **Deferred commands** — `Commands` service with `despawn`, `insert`, `remove`, `spawn_with`, and
  a `queue` escape hatch. Systems can now change structure from inside a query. The app flushes after
  every stage, so a change requested in `PreSimulation` is visible in `Simulation`. Commands queued
  *during* a flush wait for the next one, deliberately — an unbounded loop inside one flush would
  hang, which is far worse to diagnose than a one-stage delay.

- ✅ `amadeo-render` **abstraction and null backend** — `Transform`, `Quad`, `Camera2d`, the
  `RenderBackend` trait, `NullBackend` (records what *would* have been drawn, so draw calls are
  assertable with no GPU), and the `render_quads` collection pass. Draw order is by explicit
  `Quad::layer` with a stable sort, never by iteration order.
- ✅ `World::iter_pair` — a read-only two-component query, added because the renderer needed one:
  the mutable version would mark every drawn entity as changed each frame and make change detection
  worthless.

- ✅ **The Q1 spike** (session 4) — four candidates for game-logic authoring and hot reload,
  prototyped against one shared benchmark and measured. Resolved by ADR 0011: **plain Rust**.
  Prototypes and numbers in `spikes/q1-game-logic/`; re-run with `measure.ps1`. Established the
  `spikes/` convention (separate workspaces, frozen after their ADR).

**M0 is complete.** Nothing remains.

### M1 so far (session 5)

- ✅ **Three-component ECS queries** — `iter_triple` and `for_each_triple_mut` (writes two, reads
  one). Added because the Q1 benchmark needed exactly that shape and had to work around it.
- ✅ **`amadeo-reflect`** — the `Value` tree (struct fields sorted by construction, so I2 does not
  depend on anyone remembering), `TypeInfo` schema, `TypeRegistry` (BTreeMap, so anything generated
  from it is diffable), and the metadata vocabulary including ADR 0006's replication annotations.
  ADR 0012.
- ✅ **`amadeo-derive`** — `#[derive(Reflect)]` and `#[derive(StableHash)]`. The second matters more
  than it looks: a hand-written `stable_hash` that forgets a field still compiles and still produces
  a plausible number, while silently excluding part of the simulation from every replay assertion.
- ✅ Two gaps closed in `amadeo-core` found while building the above: `stable_hash_of` was `pub` but
  never re-exported, and `[T; N]` had no `StableHash` impl.
- ✅ **`Component: Reflect`** (ADR 0013) — invariant I8 is now enforced by the compiler rather than by
  remembering. An unreflectable type cannot be a component. Every existing component converted to
  `#[derive(StableHash, Reflect)]`, hand-written hash impls deleted, and `Transform`/`Quad`/
  `Camera2d` annotated with units, ranges, and ADR 0006 replication policies.
- ✅ **Q2 resolved and `amadeo-scene` layer 1 built** (ADR 0014). Justin chose a custom,
  indentation-based format from four hand-written candidates in `spikes/q2-scene-format/`. Parser
  with line-numbered actionable errors, canonical byte-stable writer, and the round-trip test that
  satisfies **M1 exit gate 3**. The ADR's worked example is asserted byte-identical to the
  formatter's output, so the spec cannot drift from the implementation.

- ✅ **Scene layer 2** — `ComponentRegistry` in `amadeo-ecs` builds a component from a *name* and a
  `Value`, using monomorphised function pointers rather than a trait object (ADR 0012 chose a
  non-object-safe `Reflect` deliberately, and this is the way back). It owns the `TypeRegistry`, so
  one `register::<T>()` call satisfies I8 with no way to register the constructor and forget the
  schema. Then `amadeo_scene::instantiate` turns a document into entities **atomically** — any
  failure despawns everything it created, because a half-loaded scene looks like it worked.
- ✅ Numeric leniency in `Reflect`: a scene's `intensity 3` arrives as an integer because the parser
  has no schema, and must still fill an `f32` field. Floats accept any numeric value; integers stay
  strict, since an out-of-range integer is a mistake rather than an approximation.

- ✅ **`amadeo-transform`** (ADR 0015) — a new crate holding `Transform` (moved out of
  `amadeo-render`) and `Parent`. Resolves a straight contradiction between `CLAUDE.md` §4 and
  `docs/04` §3 about where hierarchy lives; the `CLAUDE.md` note was a dependency-direction error,
  since render, physics, and animation all sit *below* `amadeo-scene` and all need transforms.
  Scenes now materialise their nesting as real `Parent` components instead of just recording it.

- ✅ **`amadeo-agent`, read half** — `describe` renders the registry as JSON (Pillar 2: "what can I
  do?"), `entity` and `query` render the live world (Pillar 3: "what did I just do?"), on a
  hand-written JSON writer whose objects are sorted so a dump is diffable. `ComponentRegistry` gained
  a type-erased *reader* to match its inserter, and `World::entities()` lists live entities in a
  stable order so introspection does not show churn that did not happen. All read-only, so looking at
  a world cannot perturb it.

### M1 continued (session 6)

- ✅ **Q14 resolved — ADR 0016**, then built the same session. See the session log below for what
  reading the code changed about the question.
- ✅ **A JSON reader** in `amadeo-agent`, beside the writer that was already there, with a round-trip
  test pinning the two together. Strict — no trailing commas, comments, or `NaN` — plus two
  strictnesses past the spec, each because the alternative hides a bug: **duplicate object keys are
  an error** rather than a silent last-one-wins overwrite into a `BTreeMap`, and **nesting is capped**
  so a few thousand `[` arriving from a pipe is a message rather than a stack overflow.
- ✅ **`App` owns a `ComponentRegistry`**, with `App::register_component::<T>()`. This was the gap
  ADR 0016 found by reading code rather than docs: the registry was built ad hoc in tests and nowhere
  else, and `quad-demo` registered nothing, so `describe` against a real game would have reported an
  empty schema for the game's own types the first time anyone tried it.
- ✅ **The protocol** (`amadeo-agent`) and **the host** (`amadeo-app`), split where I6 forces it —
  `amadeo-agent` sits above `amadeo-app`, so it cannot reach down for `App`. It owns the JSON-RPC
  envelope and the methods needing only a world; `amadeo-app` owns the stdin loop and the methods
  needing the schedule or the tick count. A client never sees the seam. Spec in `docs/protocol/v1.md`.
- ✅ **`quad-demo` hands over in one line**, sharing `build_simulation()` with the windowed path so an
  answer about the inspected world is an answer about the game that actually runs (I7).
- ✅ **`amadeo-cli`** — `describe`, `query`, `entity`, `schedule`, `status`, `call`, `check`, and
  `check`, `replay`, and `fmt`. The ADR 0016 split is visible in `--help`: `fmt` runs in the CLI and never builds anything,
  everything else launches the game through `cargo run` so a stale binary is rebuilt rather than
  answering for code that no longer exists.
- ✅ **`amadeo replay`** — the separate-process half of the golden-replay mechanism, and the last
  thing carried over from M0's exit gate. `--replay` and `--seed` are *launch* arguments rather than
  methods, because a recording must be installed before the first tick and `App::with_seed` fixes the
  seed at construction — before the handover is even reached. So a game reads
  `amadeo_app::requested_seed()` before building; one that does not gets a clear seed-mismatch error
  instead of a divergence that looks like a regression. Reports every failing checkpoint, not the
  first. Fixture at `games/quad-demo/replays/wander.replay`, hand-written and then filled in from the
  mismatch report — which is the intended way to author one.
- ✅ **`amadeo check`** — validates scene files against the game's *real* schema, which is precisely
  what a standalone tool cannot do. Reports **every** problem in one pass rather than the first:
  `instantiate` stops at the first error because that is right for loading and wrong for checking, so
  `amadeo_scene::validate` collects instead, on a new `ComponentRegistry::validate` that answers
  "would this build?" with no `World` to build into. Diagnostics come back naming an entity id; the
  CLI turns that into `file:line` because it is the side that still has the text. One launch covers
  every file named, since a build per scene would make checking a directory unusable.

**Verified green: 480 tests passing; clippy, fmt, and rustdoc all clean under `-D warnings`.**

Two things found by running it rather than by thinking about it:

- **PowerShell's pipe prepends a UTF-8 BOM**, and rejecting it produced an error pointing at an
  invisible character — the least actionable message that parser could produce. A leading U+FEFF is
  now skipped, and only a leading one.
- **`state_hash` goes over the wire as a hex string**, not a number. It is a `u64`, JSON numbers are
  `f64`, and above 2^53 a client silently reads a different value — which would break replay
  assertions in the least visible way available.

### Session 5 detail

**The golden replay did not need regenerating**, which was not guaranteed. The derive sorts fields by
name, so any component whose fields were not already alphabetical changes fingerprint. The committed
fixture happens to use only `Position { x, y }` and `Velocity { x, y }` — alphabetical, scalar, no
arrays — so its hashes are byte-identical. `Transform`, `Quad`, and `Camera2d` *did* change, and
nothing asserts on them. Reasoning in ADR 0013 so nobody re-derives it from scratch.

Carried into M1 rather than counted as done — **now closed, in session 6:**
- A **separate-process** replay check. The golden test replays in-process against a committed
  fixture, which covers "separate build" but not "separate process". `amadeo replay` closes it:
  `games/quad-demo/replays/wander.replay` is played by the real game binary in a fresh process, with
  four checkpoints asserted, and CI runs it in the determinism job. **M0's exit gate is now 4 of 4
  with nothing carried.**

Known gaps deliberately left for later:
- No bundle/spawn-with-components API, so building an entity with N components costs N archetype
  migrations. Correct but wasteful; optimise when it shows up in a profile.
- Query shapes reach three components (`iter_triple`, `for_each_triple_mut` — writes two, reads one),
  added in session 5 because the Q1 benchmark needed exactly that and had to work around it. Four or
  more, or a different mutability split, still needs collect-and-write-back. Extend on demand.
- **`Service` requires `Send + Sync`**, which excludes any non-`Sync` runtime from living in the
  world — found when neither script VM in the Q1 spike could be stored there. Harmless today, will
  bite the audio mixer and asset loader in M3. Filed as **Q12**.
- Events cannot be sent from inside a query closure (the world is already borrowed). Workaround is to
  collect then send, as `bounce` does in the determinism tests. Deferred commands solve the same
  problem for structural changes; an equivalent for events has not been built.
- No parallel system execution. ADR 0005 permits it only where access is provably disjoint, and the
  scheduler does not yet track access patterns.
- `SimRng`'s `StableHash` goes through its `Debug` output, which works but is inelegant. Revisit when
  the reflection registry lands in M1 and can expose the state fields directly.

## Open risks

| Risk | Mitigation |
|---|---|
| Scope is genuinely very large (unified 2D/3D + editor + AI layer ≈ rebuilding Godot). | Vertical slices with hard exit gates. Reuse proven crates for solved problems instead of writing them. Ruthless non-goals list in `docs/00-vision.md`. |
| Rust compile times degrade the agent iteration loop. | **Measured, session 4:** 0.9 s for a gameplay edit, 3.2 s for a full downstream rebuild — not currently a problem (ADR 0011). Now depends on keeping the crate graph small and shallow, which has become load-bearing rather than hygiene. Re-run `spikes/q1-game-logic/measure.ps1` when the engine has grown; WASM is the pre-selected answer if the threshold is crossed. |
| **Re-simulation cost, not compile time, degrades the loop.** Getting back to the moment of interest grows linearly with session length (~21 µs/tick; 382 ms to reach 5 simulated minutes). | Snapshot/restore, promoted to an M1 priority by ADR 0011. |
| Determinism erodes silently as features land. | Golden-replay tests in CI from M0. Every subsystem PR adds one. |
| Editor drifts into being the source of truth. | I1/I5 enforced by making the editor an RPC client with no privileged path. Round-trip byte-stability test in CI. |

## Reading order for a fresh session

If you are starting cold, this is the shortest path to being useful:

1. `CLAUDE.md` — invariants (§2), what exists (§4), how to verify (§4b), traps (§7).
2. This file's **Decided** and **Next actions** sections.
3. `docs/07-working-with-the-code.md` — the Rust patterns this engine uses and why. Skip if you
   already know the codebase.
4. `docs/adr/` — read 0005 (determinism), 0008 (ECS storage), 0009 (resource vs service) before
   touching `amadeo-ecs`. Read 0003 and 0004 before touching anything about scenes or the editor.
   Read **0011** before proposing a scripting language or a hot-reload mechanism — it was decided by
   measurement, and reopening it needs numbers. Read **0016** plus `docs/protocol/v1.md` before
   touching the CLI, the agent, or anything about process boundaries.
5. `docs/06-open-questions.md` — before assuming anything undecided.

Then `git log --oneline -20`. Commit messages explain *why*, deliberately.

## Session log

- **S1 (2026-07-30):** Scope, stack, and architecture decided. Planning docs and ADRs 0001–0005
  written. Repo initialized. No code.
- **S2 (2026-07-30):** Target games captured (Palworld / Schedule I / Inside the Backrooms), module
  priorities reordered toward 3D, and the renderer required to stay art-direction-agnostic.
  **Multiplayer promoted from non-goal to planned M6 with hooks reserved in M0–M2 (ADR 0006)** — the
  largest plan change so far. M3's exit gate set to a horror slice with concrete criteria.
  Human-legibility requirement added to `CLAUDE.md` §6 and `docs/07-working-with-the-code.md`
  created. GitHub remote added (personal account; the *global* git identity on this machine is a
  work account, so this repo carries a local override — do not remove it). Rust verified installed,
  MSVC build tools confirmed missing and blocking, rust-analyzer installed; Smart App Control found
  blocking and disabled by Justin. No engine code.
- **S3 (2026-07-30):** M0 implementation, essentially complete. In order: workspace + CI + `amadeo-core` (ADR 0007 fixed
  timestep, ADR 0008 ECS storage); `amadeo-ecs` archetype storage; `amadeo-events` +
  `amadeo-app` schedules and loop + the resource/service split (ADR 0009, found by a failing test);
  `amadeo-input` + the `.replay` text format + golden replay harness; deferred commands;
  `amadeo-render` abstraction and null backend; the wgpu backend behind an opt-in `gpu` feature; and
  `games/quad-demo`, whose window Justin confirmed working. 228 tests. ADRs 0007-0010 written.
  Visual-design preference recorded in `CLAUDE.md` §6. **Remaining in M0: the Q1 spike only.**
- **S4 (2026-07-31):** **M0 closed.** The Q1 spike, run as a measurement rather than an argument:
  four candidates (pure Rust, hot-reloaded cdylib, embedded Luau, WASM) implementing one shared
  benchmark — a three-state enemy AI over 64 entities — with agreement between them tested by state
  hash rather than by inspection. **ADR 0011: game logic is plain Rust in the game crate**, WASM
  reserved as an escape hatch behind a measured threshold.

  The recorded Luau prior was refuted, and specifically: Luau is perfectly deterministic but its
  `f64` arithmetic computes something *different* from `f32` components, diverging at tick 2. That
  breaks the prior's own central mechanism — graduating a system from Luau to Rust would change its
  behaviour and invalidate every golden replay taken before the move. Luau was also 24× slower, of
  which ~78% turned out to be the marshalling binding rather than the language.

  The question's premise was also wrong at this scale: the feared 30-second rebuild measured at
  0.9–3.2 s. Two engine gaps surfaced along the way — `Service: Send + Sync` excludes any non-`Sync`
  runtime (filed as Q12), and the two-component query limit is now confirmed as a real constraint
  rather than a speculative one. Established the `spikes/` convention. No engine code changed;
  still 228 tests.
- **S5 (2026-07-31):** **M1 begins.** Three-component ECS queries first, closing the gap the Q1 spike
  had exposed. Then the M1 keystone: `amadeo-reflect` and `amadeo-derive`, settling the four
  decisions `docs/04-subsystems.md` §8 flagged as needing to be made before writing any of it — a
  value tree rather than dynamic field access, struct fields sorted by construction so I2 is
  structural rather than remembered, the metadata vocabulary (including ADR 0006's replication
  annotations), and a derived `StableHash` so a forgotten field cannot silently drop simulation state
  out of every replay assertion. ADR 0012. Two latent `amadeo-core` gaps closed on the way. Then
  **ADR 0013: `Component: Reflect`**, turning invariant I8 from a convention into a compiler-enforced
  bound and converting every existing component — the same move ADR 0009 made for
  `Resource: StableHash`, and cheapest at eight components. The golden replay survived, for a reason
  worth reading in ADR 0013 rather than assuming. Finally **Q2**: four scene syntaxes hand-written
  and diffed (`spikes/q2-scene-format/`), where the prescribed criterion turned out not to
  discriminate — diffs are identical in all four — so the spike narrowed it to two and Justin chose
  the custom format. `amadeo-scene` built to it (**ADR 0014**) — parser, canonical writer, and then
  layer 2: `ComponentRegistry` and `instantiate`, so a scene file now loads into a `World` using the
  engine's real components. That surfaced a contradiction between two docs about where hierarchy
  components live, resolved by **ADR 0015** with a new `amadeo-transform` crate — and a second trap
  found on the way, filed as Q13: a component's id is the hash of its *fully-qualified path*, so
  moving a type between crates silently invalidates every state hash containing it. Finished with
  the read half of `amadeo-agent` — `describe`, `entity`, `query`, and a deterministic JSON writer —
  which made Pillar 2 real and surfaced **Q14**: under ADR 0011 a standalone CLI cannot know a
  game's components, so the roadmap's `amadeo-cli` shape needs rethinking before it is written.
  392 tests.
- **S6 (2026-07-31):** **Q14 resolved — ADR 0016 — and then built.** The decision came first and
  alone, deliberately: it fixes the shape of `amadeo-cli` and most of what remained in M1, and was
  worth settling before writing the CLI rather than during.

  Reading the code rather than the roadmap changed the framing twice. First, **option 1 was never a
  competing option** — the game binary is the only process holding the registry, the world, and the
  systems at once, which is the same argument ADR 0010 used to put the event loop there, so hosting
  the agent in the game is the substrate all three options are built on and the only live question is
  what wraps it. Second, **the registry has no home**: `ComponentRegistry` is built ad hoc in tests
  and nowhere else, and `quad-demo` registers nothing, so `describe` would today report an empty
  schema for a real game's own components. ADR 0016 puts the registry on `App` for the same reason
  ADR 0013 made `Component: Reflect` a compiler-enforced bound — registering in one place and
  spawning in another is how a component ends up invisible to the agent.

  Two sub-decisions the question had not asked. **One-shot batch before a live session**: each CLI
  invocation is a fresh deterministic run that exits, which is *more* reproducible than attaching and
  covers M1's exit gate; `sim.step` and the mutating calls wait for M4's editor to actually need a
  connection that outlives one question. And **the JSON parser is hand-written**, joining the writer
  already in `amadeo-agent` — `serde_json` was considered and rejected as the first real dependency
  beyond `thiserror` in a workspace that has hand-rolled PCG32, FNV-1a, and two text formats on
  legibility grounds.

  Then **built all of it**: the JSON reader, the registry on `App`, the protocol in `amadeo-agent`,
  the host in `amadeo-app`, the one-line handover in `quad-demo`, and `amadeo-cli` itself. The thing
  that now works is the point of the whole milestone — `amadeo describe Velocity` describes a type
  defined in `games/quad-demo`, answered over JSON-RPC by a game binary that a CLI which has never
  linked it went and launched. Two bugs were found by running it rather than by reasoning about it: a
  UTF-8 BOM from PowerShell's own pipe producing an error that pointed at an invisible character, and
  `state_hash` needing to be a hex string because a `u64` above 2^53 does not survive JSON's `f64`
  numbers.

  Then **`amadeo check`** on top of it — the first command that could not exist in a standalone CLI
  at all, since validating a component name means knowing which names exist. It needed
  `amadeo_scene::validate`, which collects *every* problem rather than stopping at the first the way
  `instantiate` does: stopping is right for loading and wrong for checking, because an agent fixing a
  file cannot ask a follow-up question and one error per round trip is a functional defect.

  Finally **`amadeo replay`**, which closes the separate-process replay gate carried since M0 —
  the last outstanding item from that milestone. The seed problem it raised turned out to have a
  boring answer: the game asks `requested_seed()` *before* building, rather than the host re-seeding
  afterwards, because a world whose construction consumed randomness would then differ from the one
  recorded and the divergence would look like a real regression.

  Then two decisions that had been waiting, each built the same session it was made. **Q13**
  (ADR 0017): `ComponentId` now hashes a component's canonical name rather than its Rust path, so
  moving a type between crates stopped being a silent replay-invalidating change. Both replays were
  regenerated, and confirmed the diagnosis rather than merely obeying it — only the checkpoint lines
  moved, with byte-identical input streams.

  Then **Q3**, which turned out to be three decisions wearing one question. Reading the code showed
  the framing everyone uses — one pipeline or two — is the *cheapest* of the three to reverse, since
  `RenderBackend` isolates it entirely, while the two expensive ones are about data: what a transform
  is, and what decides draw order. So a three-pipeline spike would have measured the wrong thing.
  **ADR 0018** settles the data half: one 3D `Transform` with 2D as its degenerate case, rotation as
  Euler degrees so it stays hand-writable, and `SortOrder` replacing `Quad::layer`. The pipeline
  choice is deliberately deferred to when the sprite batcher exists and can be measured.

  480 tests, all four §4b commands green, CI replaying a committed recording in a fresh process.
  **`GlobalTransform` propagation is next** — waiting since ADR 0015, and now unblocked.
