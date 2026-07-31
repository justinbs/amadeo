# Amadeo — Current Status

**Last updated:** 2026-07-31 (session 5, in progress)
**Current phase:** **M0 complete. M1 under way** — reflection landed, the collaboration surface next.
**Remote:** `origin → https://github.com/justinbs/amadeo.git` (private)

---

## Where we are

Sessions 1–2 established scope, stack, and architecture. Session 3 built M0. Session 4 closed it by
resolving Q1.

Six engine crates plus one game exist and are tested: `amadeo-core`, `amadeo-ecs`, `amadeo-events`,
`amadeo-input`, `amadeo-render`, `amadeo-app`, and `games/quad-demo`. **228 tests passing**; fmt,
clippy `-D warnings`, and rustdoc all clean. CI runs on Windows and Linux with a dedicated
determinism job.

**The engine runs.** `cargo run -p quad-demo` opens a window with a quad you steer with WASD —
confirmed working. It simulates deterministically at a fixed 60 Hz, records a session to a
hand-editable text replay file, and replays it against checkpoint state hashes in CI.

**M0 exit gate: 4 of 4.** See `docs/05-roadmap.md` § M0. Gate item 2 is met in the "separate build"
sense; the "separate process" half is carried into M1 because it needs `amadeo-cli`.

**No blockers.** Toolchain verified end to end.

## The single most important thing to do next

**Decide where the hierarchy components live — it is a blocker, and it is a genuine contradiction
between two docs rather than an oversight.**

`CLAUDE.md` §4 says `Parent`/`Children` move to `amadeo-scene` alongside `Transform2d` in M1.
`docs/04-subsystems.md` §3 lists hierarchy under `amadeo-ecs`. **Those cannot both be right**:
`amadeo-render` sits *below* `amadeo-scene` in the crate order, so M2's transform propagation could
never reach a `Parent` that lived up in `amadeo-scene`.

It blocks real work now. `instantiate` creates a scene's entities but **records** the parent
relationships rather than materialising them, because there is no component to put them in. A scene
that loses its hierarchy on load is not finished. Needs an ADR, and it probably wants `Transform2d`
moved in the same change.

Then, in order:

1. **`Resource: Reflect`** — the other half of I8, deliberately deferred by ADR 0013. Needs `Rng`'s
   state exposed so `SimRng` can reflect (retiring the `Debug`-based `StableHash` flagged as
   inelegant since M0), and map support in `Reflect` for `InputState`.
2. **`snapshot.take` / `snapshot.restore`.** The Q1 spike found re-simulation, not compilation, is
   what degrades the iteration loop — 382 ms to reach 5 simulated minutes, growing linearly.
3. **`amadeo-agent` v1 and `amadeo-cli`**, which together turn the reflection registry into
   `amadeo describe` and make Pillar 2 real rather than latent. `amadeo-cli` also closes M0's
   carried-over separate-process replay check.

**Two things are undecided rather than unbuilt.** Q7 — prefab override *semantics* — is the nearest:
the format records overrides visibly (the I1 requirement) but what they mean when a prefab changes
under an instance is undesigned; `docs/06` suggests studying Unity's and Godot's failure modes first.
And prefab *instancing* needs `amadeo-assets` to resolve a path at all, which is why `instantiate`
currently refuses a `from` line with an error that says exactly that.

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
- Unified 2D **and** 3D from the start (not 2D-first).
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

### Not yet decided (blocking)

Nothing is blocking. Q1, the last P0, closed in session 4.

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

- ✅ `amadeo-render` **abstraction and null backend** — `Transform2d`, `Quad`, `Camera2d`, the
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
  `#[derive(StableHash, Reflect)]`, hand-written hash impls deleted, and `Transform2d`/`Quad`/
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

**Verified green: 355 tests passing; clippy, fmt, and rustdoc all clean under `-D warnings`.**

**The golden replay did not need regenerating**, which was not guaranteed. The derive sorts fields by
name, so any component whose fields were not already alphabetical changes fingerprint. The committed
fixture happens to use only `Position { x, y }` and `Velocity { x, y }` — alphabetical, scalar, no
arrays — so its hashes are byte-identical. `Transform2d`, `Quad`, and `Camera2d` *did* change, and
nothing asserts on them. Reasoning in ADR 0013 so nobody re-derives it from scratch.

Carried into M1 rather than counted as done:
- A **separate-process** replay check. The golden test replays in-process against a committed
  fixture, which covers "separate build" but not "separate process". Closing it needs `amadeo-cli`.

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
   measurement, and reopening it needs numbers.
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
  engine's real components. Surfaced a blocker: the two docs disagree about where `Parent` lives.
  355 tests.
