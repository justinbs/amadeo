# Amadeo — Current Status

**Last updated:** 2026-07-30 (session 2)
**Current phase:** Planning (pre-M0). No engine code written yet.
**Remote:** `origin → https://github.com/justinbs/amadeo.git` (private)

---

## Where we are

Sessions 1–2 established scope, stack, and architecture. The repository contains planning documents
only — no crates, no build, nothing runnable yet. This is intentional.

**One prerequisite outstanding:** MSVC build tools (see Environment below). Once installed, M0 can
begin immediately.

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
- **Target game direction: Palworld-like** — third-person 3D, open world, creature collection,
  survival/crafting. Reprioritises modules toward `mod-charcontroller3d`, `mod-behaviour`,
  `mod-inventory`; reclassifies terrain/streaming from non-goal to deferred-but-expected.
  See `docs/00-vision.md` § Target game direction.

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
| **Missing — BLOCKING** | **MSVC build tools.** Verified 2026-07-30: `cargo build` on a bare `cargo new` project fails at the link step (`linking with link.exe failed`). No VS installer and no Windows SDK present. Fix: `winget install Microsoft.VisualStudio.2022.BuildTools --override "--quiet --wait --add Microsoft.VisualStudio.Workload.VCTools --includeRecommended"` |
| Also missing | Python, cmake. Neither is needed. |
| Gotcha | Installers update the persistent PATH but not running processes. VS Code's integrated terminal needs **VS Code itself** restarted, not just a new tab. |

## Next actions

1. **Install MSVC build tools** (command above). This is the only remaining blocker.
2. Verify with `cargo build` on a throwaway `cargo new` project — must link, not just compile.
3. Install the `rust-analyzer` VS Code extension (see `docs/07-working-with-the-code.md`).
4. Read `docs/05-roadmap.md` § M0.
5. Scaffold the workspace: `amadeo-math`, `amadeo-core`, `amadeo-ecs`, `amadeo-app`.
6. Stand up CI early — determinism tests are worthless if they aren't run every commit.
7. Run the M0 hot-reload spike to resolve Q1 with measurements, not opinions.

## Open risks

| Risk | Mitigation |
|---|---|
| Scope is genuinely very large (unified 2D/3D + editor + AI layer ≈ rebuilding Godot). | Vertical slices with hard exit gates. Reuse proven crates for solved problems instead of writing them. Ruthless non-goals list in `docs/00-vision.md`. |
| Rust compile times degrade the agent iteration loop. | Q1 spike exists precisely to solve this. Keep crates small and the dependency graph shallow. |
| Determinism erodes silently as features land. | Golden-replay tests in CI from M0. Every subsystem PR adds one. |
| Editor drifts into being the source of truth. | I1/I5 enforced by making the editor an RPC client with no privileged path. Round-trip byte-stability test in CI. |

## Session log

- **S1 (2026-07-30):** Scope, stack, and architecture decided. Planning docs and 5 ADRs written.
  Repo initialized. No code.
- **S2 (2026-07-30):** GitHub remote added (personal account; note the *global* git identity on this
  machine is a work account, so this repo carries a local override — do not remove it). Rust verified
  installed; MSVC build tools confirmed missing and blocking. Added human-legibility requirement to
  `CLAUDE.md` §6 and created `docs/07-working-with-the-code.md`. Target game direction captured and
  module priorities reordered toward 3D. No code.
