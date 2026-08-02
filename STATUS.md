# Amadeo — Current Status

**Last updated:** 2026-08-02 (end of session 7)
**Current phase:** **M0 complete. M1 well under way** — reflection, the scene format, the agent's read
layer, the agent protocol and a working `amadeo` CLI, the whole asset layer, and the sprite batcher
have all landed. What remains in M1 is **sprites reaching the GPU**, snapshots, and the small game
that closes the exit gate. Q3, Q4, Q13, Q14, Q16 and Q17 are all closed, so nothing is blocked.
**Remote:** `origin → https://github.com/justinbs/amadeo.git` (private). Green on every job.

> ### ⚠️ Two working rules that changed in session 7 — read before doing anything
>
> 1. **Do not `git push`. Justin pushes.** Commit as much as you like; leave it on the local branch
>    and tell him what is waiting. Checking CI with `gh` after *he* pushes is still right.
> 2. **Consult him on anything hard to reverse** — the test is cost-to-undo, not visibility. An
>    internal mechanism nobody would read still warrants asking if ripping it out later means
>    rewriting a lot. Both rules are in `CLAUDE.md` §5.

---

## Where we are

Sessions 1–2 established scope, stack, and architecture. Session 3 built M0. Session 4 closed it by
resolving Q1. Session 5 built most of M1's foundations. Session 6 resolved six open questions and
built the whole agent transport and CLI. **Session 7 finished `amadeo-assets`, audited the earlier
work, took the target list from three games to eight, built the sprite batcher, and then chased its
cost down through two layers of the ECS.** ADRs 0022–0025.

**Thirteen crates plus one game**, all tested: `amadeo-derive`, `amadeo-core`, `amadeo-reflect`,
`amadeo-ecs`, `amadeo-transform`, `amadeo-events`, `amadeo-assets`, `amadeo-input`, `amadeo-render`,
`amadeo-scene`, `amadeo-agent`, `amadeo-app`, `amadeo-cli`, and `games/quad-demo`.
**610 tests passing**; fmt, clippy
`-D warnings`, and rustdoc all clean. CI runs on Windows and Linux with a dedicated determinism job.

Seven things work end to end today:

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
- **Assets are named, found, and loaded.** `amadeo assets` lists every declared id with the file
  behind it and whether its bytes are resident; `amadeo import` gives a new file a sidecar with an id
  from its filename. A scene declares what it needs in an `assets` block, and `amadeo check` refuses
  one naming an id that does not exist — with "did you mean" when it is close.
- **Loading cannot move a replay.** quad-demo loads a real file at startup and `wander.replay` still
  matches all four checkpoints, because ADR 0009's `Service` split keeps asset state out of the hash
  structurally rather than by convention.
- **Sprites batch into draw calls.** 20,000 fully interleaved sprites collapse to 32 batches in
  2.58 ms (15.5% of a 60 Hz frame); 50,000 tiles on one sheet are a single draw call. Not yet on
  screen — the wgpu backend draws quads, not sprites.

**M0 exit gate: 4 of 4, nothing carried.** Gate item 2's "separate process" half — open since M0
because it needed `amadeo-cli` — closed in session 6: `amadeo replay` plays
`games/quad-demo/replays/wander.replay` through the real game binary in a fresh process, four
checkpoints asserted, and CI runs it in the determinism job.

**M1 exit gate: 1 of 5, with 2 and 4 now reachable.** Gate 3 (scene round-trip byte-identical) is
done. Gates 2 and 4 describe verifying and authoring *through* the CLI and RPC, which now exist —
gate 4 in particular ("`describe` output is sufficient to write a new component without reading
engine source") is testable today by actually doing it. Gate 1 (a complete small 2D game) still needs
the sprite renderer, which ADR 0018 has now unblocked. Gate 5 (golden replays still pass) holds.

**No blockers of any kind.** Q14, Q13, Q4, and two thirds of Q3 all closed in session 6 — every one
of them except Q4 built the same session it was decided.

## The single most important thing to do next

**Sprites reach the screen.** The batcher exists and is measured (ADR 0023), the ECS underneath it is
now fast enough (ADRs 0024 and 0025), and the **wgpu backend still does not draw sprites**.

That needs texture upload, a sampler, and bind groups — and before any of it, a **decoder**, because
`amadeo-assets` deliberately hands over bytes, not pixels. Where the decoder lives is a real choice
and worth thinking about rather than defaulting: `assets/textures/*.ppm` are ASCII PPM precisely so
the first one can be ten hand-checkable lines instead of a dependency, but PNG is what any real game
ships and that means either the `image` crate or a lot of code.

Note this is the first thing in a while with **no open question in front of it**.

### Then, in rough order

- **`Resource: Reflect`** — the other half of I8, deliberately deferred by ADR 0013.
- **`snapshot.take` / `snapshot.restore`** — the iteration-loop priority ADR 0011 identified.
- **Q7 — prefab semantics**, which needs the `from` conflict settled first.

### Also worth knowing

Two questions were raised this session that block nothing today but should not be discovered late:
**Q15** (modding versus ADR 0011, raised by the target list growing) and the **ADR 0014 / ADR 0020
disagreement about `from`** (filed under Q7).

### `amadeo-assets` and the sprite batcher — done, session 7

### `amadeo-assets` — done, in the order STATUS.md previously listed

1. ✅ **A directory scan** producing a catalogue. Sorted walk into a `BTreeMap` (I3), duplicate ids
   refused naming both files, and every problem reported at once rather than the first.
2. ✅ **A missing sidecar generated on import**, id defaulting to the filename stem. Prepare-then-apply,
   so a dry run is the same code path as a real one and nothing is written if anything would fail.
3. ✅ **`assets.list` and `amadeo assets`** — the ADR 0020 requirement, in place before ids became the
   reference syntax. Also `amadeo import`, and `--check` on it so it can gate a commit.
4. ✅ **Loading**, to ADR 0021's rule, plus the barrier and the `assets` block a scene declares in.
5. ✅ **`amadeo check` verifies asset ids**, with `similar_to` giving "did you mean".
6. ✅ **The sprite batcher and ADR 0023**, settling Q3's last third against a measurement.

**One decision came up that STATUS.md had said would not** — see ADR 0022 below.

Then, in order:

1. **`Resource: Reflect`** — the other half of I8, deliberately deferred by ADR 0013. Needs `Rng`'s
   state exposed so `SimRng` can reflect (retiring the `Debug`-based `StableHash` flagged as
   inelegant since M0), and map support in `Reflect` for `InputState`. Also what `world.resources`
   in the protocol is waiting on.
2. **`snapshot.take` / `snapshot.restore`.** The Q1 spike found re-simulation, not compilation, is
   what degrades the iteration loop — 382 ms to reach 5 simulated minutes, growing linearly.
3. **Q7 — prefab override semantics**, which `amadeo-assets` makes reachable and which is the
   hardest design problem left in the scene subsystem.

### ADR 0022, and a correction to what this file said

The previous version of this section claimed the loading half had **no open decisions left in it**.
That was wrong on one point, found immediately on starting the work: a game names its asset directory
with a *relative* path, and the working directory differs in all four ways a game gets started — the
CLI sets it to the project root, `cargo run` from a subdirectory does not, and a packaged binary
could be anywhere.

Researched rather than guessed, per the standing instruction. Bevy answers with an environment-variable
chain (`BEVY_ASSET_ROOT` → `CARGO_MANIFEST_DIR` → executable directory); Godot anchors on a marker
file, defining `res://` as the directory holding `project.godot`. **ADR 0022 takes Godot's approach**,
because this project already has a marker file and `amadeo-cli` already walks up for it — resolving
the game side by a different rule would invent a disagreement about which project we are in. It also
needs no shared code, which matters because `amadeo-cli` deliberately does not depend on `amadeo-app`.

Worth knowing for next time: "no open decisions left" is a claim that should be checked, not trusted.

**Three things are undecided rather than unbuilt**, all in `docs/06-open-questions.md`:

- ~~**Q3 (the last third) — which render pipeline shape.**~~ **Resolved in session 7 — ADR 0023.**
  Sprites batch by `(sort order, texture)`. Decided against measurements, as the question demanded:
  20,000 interleaved sprites collapse to exactly 32 batches, and a whole tilesheet is one draw call.
  The measurement also found that the pipeline shape is *not* currently the limiting factor — Q16 is
  — which is the opposite of what the question expected.
- **Q7 — prefab override semantics.** The format records overrides visibly (the I1 requirement), but
  what they *mean* when a prefab changes under an instance is undesigned. Study Unity's and Godot's
  failure modes first. **Now carries a smaller question that has to be answered first**, found in
  session 7: ADR 0014 says `from` holds a *path* (`from prefabs/door_metal`, pinned by a test) and
  ADR 0020 says it holds an *asset id* (`from wall_concrete`). They are not reconcilable as written —
  a path is not a usable id, because `/` is refused. Nothing is broken today, since prefab
  instancing is refused outright; decide before building it, and supersede whichever ADR loses.
- **Q12 — `Service: Send + Sync`.** Not moot: a `kira` audio manager, an asset loader holding a file
  watcher, and a `wgpu` surface all hit it in M3. Decide when the first real offender lands.
- **Q15 — modding, and whether ADR 0011 still holds.** New in session 7, raised by the target list
  growing. ADR 0011 decided game logic is plain Rust, by measurement — but it measured *iteration
  speed for the developer*, and a mod author cannot rebuild the engine at any speed. The reserved
  WASM hatch is probably the right answer (the Q1 spike measured it bit-identical to native at 1.24×,
  and sandboxed by construction), but the trigger ADR 0011 recorded does not cover this reason.
  **Decide before the module system hardens in M2–M3**, since "what can a mod do" is the same
  question as "what is the module boundary". Nothing today depends on it.

Prefab *instancing* is unbuilt rather than undecided — it needs `amadeo-assets` to resolve
an id, which is why `instantiate` refuses a `from` line with an error saying exactly that.

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
- **Target games: eight of them, extended from three in session 7.** Palworld, Schedule I, Inside the
  Backrooms, **Minecraft, Terraria, Project Zomboid, RimWorld, Stellaris**. Deliberately different
  genres, dimensions, scales, and art directions — used as a prioritisation signal. The intersection
  defines the core; the divergence defines what must stay pluggable. See `docs/00-vision.md`
  § Target games for what the five additions changed; the short version is that 2D became a
  requirement rather than a principle, destructible chunked worlds became a real subsystem, ECS
  throughput and dense UI both moved up sharply, and **modding put ADR 0011 under real pressure
  (Q15)**.
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

- **Q4 resolved — an asset is named by a declared `id` in its sidecar**, not its path and not a GUID.
  Defaults to the filename stem on import, so it reads like a path and survives a move. ADR 0020.
- **Q13 resolved — `ComponentId` is the hash of a component's canonical name**, not its Rust path.
  Moving a component between crates is free; renaming one is a deliberate, visible change. ADR 0017.
- **Q3 resolved, two thirds of it — one 3D `Transform`, and an explicit `SortOrder`.** 2D is the
  degenerate case rather than a separate type; rotation is Euler degrees so it stays hand-writable.
  The pipeline shape is deliberately still open. ADR 0018.
- **Q14 resolved — the game binary hosts the agent; the CLI launches it.** One-shot JSON-RPC over
  stdio, hand-written parser, `App` owns the `ComponentRegistry`. See `docs/adr/0016`.

### How Justin wants to work — stated in session 6, and load-bearing

These are not preferences to weigh; they are instructions. Full versions in `CLAUDE.md` §5 and §6.

- **Research before asking, not instead of asking.** He has no game-engine-development background
  and says he tends to take whichever option is recommended. So a menu of options I have not
  researched is not sharing a decision — it looks like collaboration and is not. When the codebase
  alone cannot settle a trade-off, go read how real engines solve it. He explicitly endorsed the
  time. ADR 0021 is the worked example: the research changed the answer.
- **Pros *and* cons for every option**, including the recommended one.
- **Plain language**, with the vocabulary defined at the point it affects a choice he has to make.
- **Prefer the more complete option over the faster one.** His words: he would rather have a
  complete engine than one that accumulates problems, and does not mind more steps or more time.
  Do not quietly narrow scope to save effort — that is not the trade he is asking for.
- **No `Co-Authored-By: Claude` trailer on commits.** Personal project; he knows. End the message at
  the last line of the body.

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
| **Gotcha — `gh`** | The GitHub CLI is installed but **not on PATH** for tool invocations, the same as `cargo`. It lives at `C:\Program Files\GitHub CLI\gh.exe`; prefix with `$env:PATH = "C:\Program Files\GitHub CLI;$env:PATH"`. Worth knowing because checking CI yourself after a push is faster than waiting to be told it is red. |
| Smart App Control | **Resolved.** It was blocking every binary this project builds — confirmed via event log (3118, policy `{0283ac0f-…}`). Justin disabled it (one-way change on Win11). If a future machine hits `os error 4551`, this is why; see `docs/07-working-with-the-code.md` §5. |
| Gotcha — winget | `winget install` on an already-installed package attempts an *upgrade* and silently ignores `--override`, so it cannot add a workload. Use the VS Installer to modify an existing install. |
| Gotcha — wgpu | This project is on **wgpu 30**, which differs from most material online. Read the crate source under `~/.cargo/registry/src/*/wgpu-30.0.0/src/api/` rather than trusting search results. `docs/07` records the three changes that cost the most time. |
| **Gotcha — line endings** | `core.autocrlf` is **true** by default on Windows and on GitHub's windows-latest runners. It rewrites committed LF into CRLF on checkout, breaking byte comparisons of `.replay` and `.scene` fixtures — invariant I2. `.gitattributes` pins `eol=lf`; **do not remove it**. This machine has `core.autocrlf=false` set locally, which is why it reproduced nowhere here. Tell: only the *Windows* CI jobs fail, because Linux checkout does no conversion. |

## CI

Green as of session 6. Five jobs: `check` (fmt + clippy), `test` on windows-latest and
ubuntu-latest, `determinism` (the suite three times serially, then release, then a separate-process
replay), and `docs`.

**The first push, in session 6, went red 3/5 and stayed red for four commits.** Not a determinism
failure despite looking exactly like one — see the line-endings gotcha above. Worth knowing that the
run before the fix failed *with identical state hashes on both sides of the assertion*; the
simulation was never wrong.

Older commits still show red on GitHub. That is correct and needs no action: CI ran against trees
that had no `.gitattributes`, so re-running them would fail identically. The code in them is fine —
in every red run, `golden_file_replays_to_its_recorded_hashes` (the test that actually asserts state
hashes) passed.

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
- ✅ **`amadeo-cli`** — `describe`, `query`, `entity`, `schedule`, `status`, `call`, `check`,
  `replay`, and `fmt`. The ADR 0016 split is visible in `--help`: `fmt` runs in the CLI and never
  builds anything; everything else launches the game through `cargo run`, so a stale binary is
  rebuilt rather than answering for code that no longer exists.
- ✅ **`amadeo replay`** — the separate-process half of the golden-replay mechanism, and the last
  thing carried over from M0's exit gate. `--replay` and `--seed` are *launch* arguments rather than
  methods, because a recording must be installed before the first tick and `App::with_seed` fixes the
  seed at construction — before the handover is even reached. So a game reads
  `amadeo_app::requested_seed()` before building; one that does not gets a clear seed-mismatch error
  instead of a divergence that looks like a regression. Reports every failing checkpoint, not the
  first. Fixture at `games/quad-demo/replays/wander.replay`, hand-written and then filled in from the
  mismatch report — which is the intended way to author one.
- ✅ **`GlobalTransform` and `propagate_transforms`** (ADR 0019) — waiting since ADR 0015, unblocked
  by ADR 0018 settling what a transform is. Walks up the parent chain per entity rather than keeping
  a depth-sorted work list, because that list is a cache with an invalidation story and hierarchies
  are shallow. A `Parent` cycle falls back to the local transform rather than hanging.

  **`GlobalTransform` is `DERIVED`, so it is excluded from the state hash** — Justin decided this
  directly, and it is the reason matrix arithmetic cannot move a replay. Proven rather than asserted:
  `quad-demo` now carries a `GlobalTransform` on every entity and **both replay fixtures are
  byte-unchanged**. Two tests guard each other — one that propagation does not move the hash, one
  that a real change still does, so neither can pass because hashing quietly broke.

  Also a scalar `Mat4` in `amadeo-transform` rather than creating `amadeo-math` or taking glam:
  propagation needs compose-and-multiply and nothing else, and designing a maths surface backwards
  from its first caller is how a wrong abstraction gets locked in.
- ✅ **`amadeo check`** — validates scene files against the game's *real* schema, which is precisely
  what a standalone tool cannot do. Reports **every** problem in one pass rather than the first:
  `instantiate` stops at the first error because that is right for loading and wrong for checking, so
  `amadeo_scene::validate` collects instead, on a new `ComponentRegistry::validate` that answers
  "would this build?" with no `World` to build into. Diagnostics come back naming an entity id; the
  CLI turns that into `file:line` because it is the side that still has the text. One launch covers
  every file named, since a build per scene would make checking a directory unusable.
- ✅ **Q13 resolved — ADR 0017.** `ComponentId` now hashes a component's canonical name rather than
  its Rust path, so moving a type between crates stopped being a silent replay-invalidating change.
  Cost: two components sharing a canonical name now collide. The registry already refuses that;
  `World::insert` gained a **debug-build guard** for anything unregistered.
- ✅ **Q3 resolved, two thirds — ADR 0018.** One 3D `Transform` (2D is its degenerate case,
  `Transform2d` retired), rotation as **Euler degrees** so it stays hand-writable, and `SortOrder`
  replacing `Quad::layer`. The pipeline shape is deliberately still open and dropped to P2.
- ✅ **`GlobalTransform` and `propagate_transforms`** (ADR 0019) — waiting since ADR 0015, unblocked
  by ADR 0018 settling what a transform is. Walks up the parent chain per entity rather than keeping
  a depth-sorted work list, because that list is a cache with an invalidation story and hierarchies
  are shallow. A `Parent` cycle falls back to the local transform rather than hanging.

  **`GlobalTransform` is `DERIVED`, so it is excluded from the state hash** — Justin decided this
  directly, and it is the reason matrix arithmetic cannot move a replay. Proven rather than asserted:
  `quad-demo` carries a `GlobalTransform` on every entity and **both replay fixtures are
  byte-unchanged**. Two tests guard each other — one that propagation does not move the hash, one
  that a real change still does — so neither can pass because hashing quietly broke.

  Also a scalar `Mat4` in `amadeo-transform` rather than creating `amadeo-math` or taking glam:
  propagation needs compose-and-multiply and nothing else, and designing a maths surface backwards
  from its first caller is how a wrong abstraction gets locked in.
- ✅ **The renderer reads `GlobalTransform`**, so hierarchy reaches the screen. Scale and rotation
  come back out of the **composed matrix**, not the local transform — a matrix's columns are its
  scaled axes, so a column's length is that axis's total scale and its angle the total rotation.
  Without that a parent's turn would move a child but not rotate it.
- ✅ **`.gitattributes`** — the fix for the CI failure, see the CI section above.
- ✅ **Q4 resolved — ADR 0020**, and **ADR 0021** on top of it. Asset identity is a declared `id` in
  a sidecar; the simulation never observes asset *state*.
- 🟡 **`amadeo-assets`, first slice** — the `.ama-meta` sidecar format and the `AssetCatalogue`
  mapping id to file, with duplicate ids refused naming both files. Loading, handles, the import
  pipeline and hot-reload are still to come, to ADR 0021's rule.

### M1 continued (session 7)

- ✅ **`amadeo-assets`, the loading half** — all five steps listed above. The scan reports what it
  could *not* catalogue (unimported files, orphaned sidecars), because ADR 0020 predicted that exact
  confusion by name: asking for `wall` is refused while `wall.png` sits right there in the tree.
  Stored paths are normalised to forward slashes, since they go over the protocol and
  `textures\wall.png` against `textures/wall.png` would need a special case in every cross-platform
  assertion. Dotfiles are not assets, which is the *only* rule about what counts as one — an
  extension allowlist would be genre knowledge and I4 forbids it.
- ✅ **ADR 0022** — the asset root is found by walking up for `amadeo.toml`. See the correction above.
- ✅ **The load barrier**, and the `assets` block a scene declares its requirements in. A missing
  asset is recorded and survivable rather than fatal, per ADR 0021. **Proven, not asserted:**
  quad-demo now loads a real 700-byte file at startup and `wander.replay` still matches all four
  checkpoints, because `Assets` is a `Service` and ADR 0009 excludes those by trait bound.
- ✅ **`amadeo check` validates asset ids**, with near-miss suggestions.
- ✅ **A PCG32 reference cross-check** — see the audit below.
- ✅ **The sprite batcher — ADR 0023, resolving Q3's last third.** A `Sprite` component holding a
  texture *id* (ADR 0020) plus a `region`, so a tilesheet is one texture and one batch. Batches are
  `(sort order, texture)` pairs: layering is never violated, and within one order the relative order
  of *different* textures is explicitly not guaranteed — that is the trade, and `SortOrder` is the
  mechanism for controlling it.

  Decided against numbers, as the question demanded. 20,000 fully interleaved sprites collapse to
  exactly **32** batches — the theoretical minimum that preserves layering — and 50,000 tiles on one
  sheet are **one** draw call. Batch counts are asserted (a pure function of the world, no clock);
  times are printed, with only an algorithmic-collapse ceiling asserted.

  Two things the measurement changed. The first version sorted by `(order, &str)` and was 55% slower;
  keying on an index into a sorted texture table made the sort integer-only. And `SpriteInstance`
  carries the transform's **axes** rather than a size and an angle, which removes a round trip
  through trigonometry on both the CPU and the shader — and is strictly more expressive, since a
  size-and-angle pair cannot represent a sheared or non-uniformly-scaled-then-rotated sprite.

- ✅ **Component ids are compile-time constants now — ADR 0024, resolving Q16.** `Reflect` gained
  `STATIC_NAME` (filled in by the derive) and `STATIC_NAME_HASH` (a `const fn` FNV-1a over it), so
  `ComponentId::of::<T>()` is a constant load rather than a `String` allocation plus a hash on
  **every** component access.

  This is an engine-wide win, not a rendering one — `World::get`, `World::insert`, and every query
  pay it. Sprite collection went **5.13 ms → 3.32 ms** at 20,000 sprites (31% → 20% of a frame), and
  the 50,000-tile case **11.55 ms → 6.77 ms**. Ids are byte-identical: both golden replays and the
  separate-process `amadeo replay` pass unchanged, which is the assertion that matters, since a
  different hash would have invalidated every committed replay at once.

- ✅ **Queries are tuples of terms, and a term may be optional — ADR 0025, resolving Q17.**
  `world.query::<(&Transform, &Sprite, Option<&SortOrder>, Option<&GlobalTransform>)>()`. Each column
  is resolved **once per archetype** instead of once per entity, which is the structural reason
  archetype ECSs are fast and the thing Amadeo's hand-written query methods could not express.

  **Justin chose this**, over hand-writing every shape or a lower-level per-archetype accessor, after
  the trade was put to him with the legibility cost stated. It is the one deliberate piece of clever
  Rust in the ECS — a trait with an associated type plus a macro writing the tuple impls — and the
  module docs explain each part of the machinery next to the code rather than only in the ADR.

  Read-only on purpose: a generic *mutable* query cannot prove two type parameters are different
  columns, so Bevy uses `unsafe` for it, this crate forbids `unsafe`, and the measured problem was
  entirely on the read side. `for_each_pair_mut` and friends are untouched, and a test asserts the
  old and new paths see the same world.

  Sprite collection: **3.32 ms → 2.58 ms** at 20,000 sprites, and **5.13 → 2.58 ms** across ADRs 0024
  and 0025 together — 15.5% of a 60 Hz frame, from 31%.

**Verified green: 610 tests passing; clippy, fmt, and rustdoc all clean under `-D warnings`.**

### The audit Justin asked for, session 7

He asked for the earlier work to be re-checked, since everything before the last two additions was
built on whichever option was recommended. What was checked, and what it found:

**The invariants hold, and two of them hold better than the docs claim.**

- **I3 (determinism).** There is **no `HashMap` or `HashSet` anywhere in the engine** — the only
  occurrences are comments explaining why a `BTreeMap` is used instead. No `Instant::now` or
  `SystemTime` in any engine crate. Transcendental functions (`sin_cos`, `atan2`, `hypot`) appear in
  exactly **two** places, and both are outside the hashed path: `amadeo-transform`'s matrix build,
  which feeds `GlobalTransform` (`DERIVED`, excluded by ADR 0019), and `amadeo-render`'s matrix
  decomposition, which is render-side. That matters more than it looks — IEEE 754 does not specify
  transcendental functions, so `sin` can differ in the last bit between platforms. **ADR 0019's
  decision is load-bearing for cross-platform determinism in a way the ADR does not state.**
- **The safety net is real.** The `test` CI job runs on **both** Windows and Linux and includes the
  golden-replay test, which asserts *committed* hashes. So a hashed path growing a `sin` call would
  fail CI on one platform. That is a genuine cross-platform determinism check and the docs undersell it.
- **I6 (dependency DAG).** Verified crate by crate. Every edge points the right way; no cycles.
- **`World::state_hash` is sound.** Entities sorted by index and generation, components in sorted id
  order, resources in `BTreeMap` order, tick included, services excluded. `DERIVED` components skip
  **their id as well as their value**, which is the subtle half — writing the id would mean adding a
  `GlobalTransform` still moved the hash. The sorted-`component_ids` invariant it relies on is
  enforced by `debug_assert`.
- **The golden replay is not vacuous.** Four distinct checkpoint hashes, with a paired `assert_ne`
  guarding against the hash being constant.

**One real gap, now closed. `Rng` had no known-answer test.** Every existing test was a
*self-consistency* property — same seed gives same sequence, different seeds diverge, outputs in
range. All of them would still pass if the algorithm were subtly wrong (shift by 17 instead of 18),
because a wrong generator is still a perfectly deterministic one: I3 would hold and the statistical
quality PCG was chosen for would be silently gone. `StableHasher` *was* cross-checked against an
independent FNV-1a when written; the generator was going on the claim in its own doc comment.

Closed by `crates/amadeo-core/tests/pcg_reference_vector.rs`. **The result: `Rng` reproduces the
official PCG32 demo output exactly** — seeded `(42, 54)` it emits `a15c02b7, 7b47f409, ba1d3330,
83d2f293, bfa4784b, cbed606e`. So the implementation is genuinely PCG32 XSH-RR 64/32, confirmed
against a published vector rather than against a transcription that could share a mistake with it.
FNV-1a's constants and its xor-then-multiply ordering were checked too, and are correct.

**Smaller things found and fixed in passing:**

- `amadeo-agent`'s lib docs still said there was no JSON-RPC server and no JSON parser. Session 6
  built both.
- `quad-demo`'s `build_simulation` doc comment had been detached from it by a `const` inserted
  between them, so the function was undocumented and `DEFAULT_SEED` was documented as a colour palette.
- `docs/protocol/v1.md` listed `assets.list` as not implemented. Now specified.

**Smaller things found and left alone, deliberately:**

- Three `expect()` calls in `amadeo-app/src/schedule.rs` technically breach the "no `unwrap`/`expect`
  in engine crates" convention. All three are provably unreachable local invariants established a few
  lines above, each with an explanatory message; rewriting them would add unreachable error paths.
  Every other occurrence in the engine is inside a doc-comment example, which is fine.
- `amadeo-app` lists `amadeo-input` in both `[dependencies]` and `[dev-dependencies]`. Harmless.

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

1. `CLAUDE.md` — invariants (§2), what exists (§4), how to verify (§4b), **how to put a choice to
   Justin (§5)**, and the traps (§7).
2. This file: **How Justin wants to work**, **The single most important thing to do next**, and
   **CI**. Those three are the whole handoff; everything else here is background.
3. `docs/07-working-with-the-code.md` — the Rust patterns this engine uses and why, the everyday
   `amadeo` commands, and the golden-replay mechanism. Skip if you already know the codebase.
4. `docs/adr/` — 25 of them now, so read by need rather than in order:
   - **0023** before touching the renderer, **0024** and **0025** before touching `amadeo-ecs`.
     0025 in particular: `world.query` is the API every read path should use, and its module docs
     explain the one piece of deliberately non-boring Rust in the engine.
   - **0005** (determinism), **0008** (ECS storage), **0009** (resource vs service) and **0019**
     (derived components) before touching `amadeo-ecs` or anything that reaches `state_hash`.
   - **0003** and **0004** before touching scenes or the editor; **0014** for the scene format.
   - **0011** before proposing a scripting language or hot reload — decided by *measurement*, so
     reopening it needs numbers, not arguments.
   - **0016** plus `docs/protocol/v1.md` before touching the CLI, the agent, or process boundaries.
   - **0017** before moving or renaming a component (moving is free now; renaming is not).
   - **0018** before touching transforms or draw order; **0020** and **0021** before assets.
5. `docs/06-open-questions.md` — before assuming anything undecided. Nine remain, none blocking.
   **Q15** (modding vs ADR 0011) and the **`from` conflict inside Q7** are the two that were raised
   in session 7 and deliberately left for Justin.

Then `git log --oneline -25`. Commit messages explain *why*, deliberately, and session 6's are long
on purpose — several record a diagnosis that took a while to reach.

**Things that will bite a cold session specifically:**

- **`cargo` is not on PATH for tool invocations.** Prefix with
  `$env:PATH = "$env:USERPROFILE\.cargo\bin;$env:PATH"`.
- **`gh` is not on PATH either.** It is at `C:\Program Files\GitHub CLI\gh.exe`.
- **Windows PowerShell 5.1 reads UTF-8 as ANSI and writes back a BOM.** If you script a file edit,
  use .NET APIs with `UTF8Encoding($false)`, or every em-dash in the repo is silently corrupted.
  Console *display* of em-dashes as mojibake is harmless; a `git diff --stat` showing the whole file
  changed is not.
- **PowerShell here-strings break `git commit -m`** when the message contains quotes — the message
  gets split into pathspecs and the commit fails confusingly. Write the message to a file with the
  Write tool (which emits UTF-8 with no BOM) and use `git commit -F <file>`.
- **Do not push.** See the box at the top of this file.

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

  Then **`GlobalTransform` and `propagate_transforms`** (ADR 0019), waiting since ADR 0015. Justin
  decided directly that a derived component stays **out of the state hash**, which needed a mechanism
  the ECS did not have: `Component::DERIVED`, carried through the type erasure by `Column`, honoured
  by `state_hash`. Named `DERIVED` rather than `HASHED` on purpose — the first states what must be
  *true* so the rule follows from the name, the second describes what it does and invites anyone
  wanting a quieter diff to reach for it. Proven, not asserted: `quad-demo` now carries a
  `GlobalTransform` on every entity and both replay fixtures are byte-unchanged.

  **Then CI, which had been red since the first push and was not what it looked like.** The failing
  assertion had *identical checkpoint hashes on both sides* and differed only in `\n` versus `\r\n`.
  `core.autocrlf` is true by default on GitHub's Windows runners; with no `.gitattributes` it
  rewrote every committed LF on checkout. This machine has it set to `false` locally, which is why
  it reproduced nowhere here across seven different reproductions of CI's exact commands. Fixed with
  `.gitattributes`, verified by two fresh clones under `autocrlf=true` (17 CR bytes before, 0
  after). Worth recording that the toolchain-pin commit immediately before it was a real fix for a
  real defect — `channel = "stable"` was not pinning anything despite its comment promising exactly
  the reproducibility I3 needs — but it was **not** this bug, and I presented it with more adjacency
  to the failure than it earned.

  Finally **Q4 (ADR 0020)** and **ADR 0021**, plus the first slice of `amadeo-assets`. Q4 asked what
  names an asset; the answer follows Q13 one layer up — a path is a *location*, so identity is a
  declared `id` in a sidecar, defaulting to the filename stem so it reads like a path but survives a
  move. ADR 0021 then settled how loading avoids breaking I3, and this one was **researched rather
  than reasoned about** (Justin's standing instruction): the industry pattern is a loading barrier,
  but Bevy chooses it for user experience and tolerates mid-game loads, so adopting it for
  determinism would give the right shape for the wrong reason and would not hold the first time
  someone streams a chunk. The invariant is stronger: gameplay holds an id and never observes asset
  *state*, so there is nothing to branch on.

  519 tests, all five CI jobs green, seventeen commits.
- **S7 (2026-08-02):** **`amadeo-assets` finished** — all five steps the previous handoff listed, in
  its order. The scan, sidecar generation on import, `assets.list` plus `amadeo assets` and
  `amadeo import`, the load barrier with a scene-declared `assets` block, and asset ids validated by
  `amadeo check`.

  **The handoff's claim that the loading half had "no open decisions left in it" was wrong**, and it
  surfaced in the first hour: a relative asset path has to resolve against *something*, and the
  working directory differs in all four ways a game starts. Researched rather than guessed — Bevy
  uses an environment-variable chain, Godot anchors on a marker file — and **ADR 0022** took Godot's
  approach, because this project already has `amadeo.toml` and the CLI already walks up for it.

  Then **the audit Justin asked for**, which is written up in its own section above. Headline: the
  invariants hold, and I3 holds better than the docs claim — there is no `HashMap` anywhere in the
  engine, and all transcendental maths is confined to non-hashed paths, which makes ADR 0019's
  derived-component decision quietly load-bearing for cross-platform determinism. One real gap:
  `Rng` had only self-consistency tests, which would all pass on a subtly wrong generator. Now
  cross-checked, and it **reproduces the official PCG32 demo vector exactly**.

  One unresolved conflict found and deliberately *not* decided alone: ADR 0014 and ADR 0020 disagree
  about whether `from` holds a path or an asset id. Filed under Q7, since it has to be settled before
  prefab instancing.

  578 tests, all four verification commands green, `wander.replay` unchanged.

  **Then the target list grew from three games to eight** — Minecraft, Terraria, Project Zomboid,
  RimWorld, Stellaris added to Palworld, Schedule I, and Inside the Backrooms. Written up in
  `docs/00-vision.md`, and it is a larger change than a list edit: the original three were all 3D,
  all action-paced, all co-op, all rendering-led, and the five additions break every one of those.
  Six consequences, of which two matter most. **2D stopped being a principle being defended and
  became a requirement** — three of the eight are 2D or isometric, which lands the same week the
  sprite batcher does. And **modding became a target-driven requirement**, which puts ADR 0011 under
  a kind of pressure Q1 never evaluated: it decided by measuring the developer's iteration speed, and
  a mod author cannot rebuild the engine at any speed. Filed as **Q15**, deliberately not decided.

  **Then the sprite batcher (ADR 0023), which closed Q3 and then kept going.** The batching rule is
  `(sort order, texture)`: layering exact across orders, grouped by texture within one. 20,000
  interleaved sprites collapse to exactly 32 batches, and a whole tilesheet is one draw call.

  What made the rest of the session was that **the measurement did not agree with the theory.**
  Collecting 20,000 sprites took 5.1 ms, and removing the batcher's own trigonometry moved it by 4% —
  which ruled out the obvious suspect and pointed into the ECS twice over:

  - **ADR 0024** — `ComponentId::of` was allocating a `String` and hashing it *on every call*, on the
    hot path of every component access. Now a compile-time constant via two new `Reflect` consts.
    5.13 → 3.32 ms, and it made the whole engine faster, not just rendering.
  - **ADR 0025** — the ECS could not express an *optional* component in a query, so the renderer fell
    back to `world.get` per entity: 40,000 lookups a frame, which is exactly what archetype storage
    exists to avoid. `world.query::<(&A, &B, Option<&C>)>()` now resolves each column once per
    archetype. 3.32 → 2.58 ms. **Justin chose this design** from three options after research.

  Two near-misses worth keeping. A `static` cache inside a generic function is shared across
  monomorphisations, not per-type — it collapsed every component onto one id and the archetype tests
  caught it instantly. And the throughput fixture gave no entity a `GlobalTransform`, so it was
  measuring a fallback path no shipped game takes; fixing it changed the final number materially.

  **610 tests, all four verification commands green, both replays unchanged throughout** — which
  mattered most for ADR 0024, where a wrong hash would have invalidated every committed replay at
  once.
