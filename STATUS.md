# Amadeo — Current Status

**Last updated:** 2026-07-30 (session 2)
**Current phase:** Planning complete. **M0 not started.** No engine code written yet.
**Remote:** `origin → https://github.com/justinbs/amadeo.git` (private)

---

## Where we are

Sessions 1–2 established scope, stack, and architecture. The repository contains planning documents
only — no crates, no build, nothing runnable yet. This is intentional.

**Toolchain is verified working end to end** (compile, link, run). No prerequisites outstanding.
M0 can begin immediately.

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

### Not yet decided (blocking)
- **Q1: How game logic is authored and hot-reloaded.** Rust dylib vs WASM vs embedded scripting.
  This determines the entire iteration loop and must be settled by a measured spike in M0 before
  any subsystem work depends on it. See `docs/06-open-questions.md`.

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
| **Toolchain status** | ⚠️ **Compiles but cannot execute — see Smart App Control below.** |
| Also missing | Python, cmake. Neither is needed. |
| Gotcha — PATH | Installers update the persistent PATH but not running processes. VS Code's integrated terminal needs **VS Code itself** restarted, not just a new tab. |
| **BLOCKER — Smart App Control** | Enabled and enforced. Blocks **every binary this project builds**, anywhere on disk — confirmed via event log (3118 Smart App Control Block, policy `{0283ac0f-…}`). `cargo check` and `cargo build` work; `cargo test`, `cargo clippy`, and running the engine are blocked. No workaround exists; it must be turned off (Windows Security → App & browser control → Smart App Control → Off), which is **one-way** and is Justin's call alone. Full detail and the corrected earlier analysis in `docs/07-working-with-the-code.md` §5. |
| Gotcha — winget | `winget install` on an already-installed package attempts an *upgrade* and silently ignores `--override`, so it cannot add a workload. Use the VS Installer to modify an existing install. |

## Next actions

**M0 is under way.** Workspace scaffolded; `amadeo-core` written and type-checking.

**Blocked on one thing:** Smart App Control (see Environment). Code can be written and type-checked,
but no test can run, so nothing can be *verified*. Justin must decide whether to disable SAC.

Done so far in M0:
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

**Verified green: 228 tests passing; clippy, fmt, and rustdoc all clean under `-D warnings`.**

Remaining in M0:
1. **The wgpu backend and a window.** `amadeo-render` deliberately has no wgpu or winit dependency
   yet — the abstraction and null backend build and test with no GPU (invariant I7). The real backend
   goes behind a feature flag so headless builds and CI stay lean. This is the last piece of the exit
   gate: a coloured quad moving under live keyboard input. **Adds ~200 transitive crates and will
   slow builds noticeably** (ADR 0002's known weakness).
2. A `separate process` replay check. The golden test currently replays in-process against a
   committed fixture, which covers "separate build" but not "separate process". Closing that needs
   `amadeo-cli`, which is M1 work — noted so the gap is not forgotten.
3. **Q1 spike** (game logic hot-reload) — resolve with measurements. Blocks M1.

Known gaps deliberately left for later:
- No bundle/spawn-with-components API, so building an entity with N components costs N archetype
  migrations. Correct but wasteful; optimise when it shows up in a profile.
- Query shapes are limited to one and two components. Extend when a real system needs more, not
  speculatively.
- Events cannot be sent from inside a query closure (the world is already borrowed). Current
  workaround is to collect then send, as `bounce` does in the determinism tests. Deferred commands
  will fix this properly.
- No parallel system execution. ADR 0005 permits it only where access is provably disjoint, and the
  scheduler does not yet track access patterns.
- `SimRng`'s `StableHash` goes through its `Debug` output, which works but is inelegant. Revisit when
  the reflection registry lands in M1 and can expose the state fields directly.

## Open risks

| Risk | Mitigation |
|---|---|
| Scope is genuinely very large (unified 2D/3D + editor + AI layer ≈ rebuilding Godot). | Vertical slices with hard exit gates. Reuse proven crates for solved problems instead of writing them. Ruthless non-goals list in `docs/00-vision.md`. |
| Rust compile times degrade the agent iteration loop. | Q1 spike exists precisely to solve this. Keep crates small and the dependency graph shallow. |
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
5. `docs/06-open-questions.md` — before assuming anything undecided.

Then `git log --oneline -20`. Commit messages explain *why*, deliberately.

## Session log

- **S1 (2026-07-30):** Scope, stack, and architecture decided. Planning docs and ADRs 0001–0005
  written. Repo initialized. No code.
- **S2 (2026-07-30):** Target games captured (Palworld / Schedule I / Inside the Backrooms);
  multiplayer promoted from non-goal to planned M6 with hooks reserved (ADR 0006); M3's exit gate set
  to a horror slice. Human-legibility requirement added to `CLAUDE.md` §6. GitHub remote added.
  Toolchain verified; Smart App Control found blocking and disabled by Justin. No engine code.
- **S3 (2026-07-30):** M0 implementation. In order: workspace + CI + `amadeo-core` (ADR 0007 fixed
  timestep, ADR 0008 ECS storage); `amadeo-ecs` archetype storage; `amadeo-events` +
  `amadeo-app` schedules and loop + the resource/service split (ADR 0009, found by a failing test);
  `amadeo-input` + the `.replay` text format + golden replay harness; deferred commands;
  `amadeo-render` abstraction and null backend. 228 tests. Remaining: wgpu backend, then the Q1 spike.
  Visual-design preference recorded in `CLAUDE.md` §6.
- **S2 (2026-07-30):** GitHub remote added (personal account; note the *global* git identity on this
  machine is a work account, so this repo carries a local override — do not remove it). Rust verified
  installed; MSVC build tools confirmed missing and blocking; rust-analyzer installed. Added
  human-legibility requirement to `CLAUDE.md` §6 and created `docs/07-working-with-the-code.md`.
  Three target games captured, module priorities reordered toward 3D, renderer required to stay
  art-direction-agnostic. **Multiplayer promoted from non-goal to planned M6 with hooks reserved in
  M0–M2 (ADR 0006)** — the largest plan change so far. M3's exit gate set to a horror slice with
  concrete criteria. No code.
