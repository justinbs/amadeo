# Amadeo — Current Status

**Last updated:** 2026-07-30
**Current phase:** Planning (pre-M0). No engine code written yet.

---

## Where we are

Session 1 established scope, stack, and architecture. The repository contains planning documents
only — no crates, no build, nothing runnable yet. This is intentional.

### Decided
- Name: **Amadeo**.
- Unified 2D **and** 3D from the start (not 2D-first).
- Native desktop first, Windows as the primary target. Web export deferred to M5.
- Graphical editor **and** full text/code/headless parity are both first-class requirements.
- Stack: Rust + wgpu + winit + glam + rapier + egui. See `docs/adr/0002`.
- Scene tree is the authoring model; ECS is the runtime model. See `docs/adr/0004`.
- Text files are the only source of truth. See `docs/adr/0003`.
- Determinism is a hard invariant, designed in from tick zero. See `docs/adr/0005`.

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
| **Missing** | **Rust toolchain — required before M0. Install via `rustup`.** |
| Also missing | Python, cmake, C/C++ compiler. Some crates want MSVC build tools; install "Desktop development with C++" from VS Build Tools if linking fails. |

## Next actions

1. Install the Rust toolchain (`rustup`, stable, `x86_64-pc-windows-msvc`). Confirm `cargo --version`.
2. Read `docs/05-roadmap.md` § M0.
3. Run the M0 hot-reload spike to resolve Q1 with measurements, not opinions.
4. Scaffold the workspace: `amadeo-math`, `amadeo-core`, `amadeo-ecs`, `amadeo-app`.
5. Stand up CI early — determinism tests are worthless if they aren't run every commit.

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
