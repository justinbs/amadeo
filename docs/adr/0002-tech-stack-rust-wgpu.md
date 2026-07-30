# ADR 0002 — Rust + wgpu as the engine stack

**Status:** Accepted · **Date:** 2026-07-30

## Context

Requirements established in session 1:

- Unified 2D **and** 3D from the start.
- Native desktop first, Windows primary.
- A graphical editor is a first-class deliverable.
- Determinism is a hard invariant (ADR 0005).
- Fast iteration for an AI agent is a functional requirement.

The machine has Node and Java installed and nothing else relevant, so every candidate requires a
toolchain install. That neutralized what initially looked like a strong constraint.

An earlier recommendation in the same session favored TypeScript + WebGPU on the assumption of
browser-first, 2D-first delivery. When the requirements came back as native-first and unified
2D/3D, that recommendation no longer held.

## Decision

**Rust (2024 edition), with:**

| Concern | Choice |
|---|---|
| Graphics | `wgpu` — one API over Vulkan/DX12/Metal, and targets WebGPU |
| Windowing | `winit` |
| Math | `glam`, wrapped by `amadeo-math` |
| Physics | `rapier` 2D+3D, behind engine-owned traits |
| Editor UI | `egui` |
| Game UI | custom retained-mode (`amadeo-ui`) — deliberately not egui |
| Agent protocol | JSON-RPC over stdio + local TCP |

`#![forbid(unsafe_code)]` outside explicitly audited modules.

**Deliberately left open:** how game logic is authored and hot-reloaded (open question Q1). It is not
settled by this ADR and must not be assumed.

## Rationale

1. **No GC.** Predictable frame times, and determinism is achievable without fighting a collector.
   This is the single strongest argument, since determinism underpins the entire AI-native design.
2. **wgpu is the right unified 2D/3D abstraction.** One backend-agnostic API, modern explicit model
   that suits a render graph — and because it targets WebGPU, a browser export path stays open for
   near-free. That recovers the cheap-sharing benefit of a web engine without conceding native
   performance.
3. **rapier offers real determinism.** Cross-run reproducible physics in both 2D and 3D from one
   project. Nothing else in the ecosystem offers this combination.
4. **Strong types suit agent-authored code.** Code that compiles usually also works. The type system
   catches a large share of the errors an agent is most likely to make, before anything runs.
5. **One language across engine, editor, tools, tests, and games.** No binding layer to keep in sync.

## Rejected alternatives

**TypeScript + WebGPU in a Tauri/Electron shell.** Best-in-class iteration speed and observability;
the browser's devtools protocol is effectively a ready-made agent interface. Rejected because a GC'd
language in a browser shell is the wrong foundation for stable-frame-time 3D with physics and skeletal
animation, and the performance ceiling would eventually limit what games we could make. *What we keep:*
its observability lessons — everything the browser gave free (screenshots, live state queries,
console-as-protocol) is now built explicitly in `amadeo-agent`. That is the cost of going native, paid
on purpose.

**C# / .NET 9 + Silk.NET.** The strongest runner-up: far better compile times, real hot reload, large
training corpus, proven for engines. Rejected because GC tail latency conflicts with determinism in
ways requiring permanent vigilance — and writing C# under Rust-like discipline argues for just writing
Rust. Native graphics bindings are also more fragmented than wgpu.

**C++.** Industry standard, maximum control. Rejected on build complexity, memory-safety burden, and
the worst agent ergonomics available — subtle UB is exactly where generated code is most dangerous and
hardest to verify.

**Building on Bevy.** Rust, ECS, wgpu, mature. Rejected because we would inherit its scene format,
scheduler semantics, and editor story, and invariants I1/I2/I3/I5 would be things we fight Bevy to
impose rather than design in. Bevy remains **reference material** — read it for archetype storage,
scheduling, and render graph design. Learn from it; don't depend on it.

**Godot via GDExtension.** Would provide an editor and renderer immediately. Rejected because the
project's core thesis is that editor-owns-truth is the problem; adopting Godot adopts the problem.

## Consequences

**Accepted cost: compile times.** This is the stack's real weakness and it hits the agent loop harder
than the human one. Mitigations: resolve Q1 so gameplay changes never require a full engine rebuild;
keep crates small and the dependency graph shallow; use `cargo check` and `lld` in the dev loop; put
heavy dependencies behind feature flags; make headless deterministic tests the primary verification
path since they're far faster than launching a window.

**Accepted cost: no free browser observability.** `amadeo-agent` must be genuinely good, because there
is no devtools fallback. This raises the priority of that crate from "nice tooling" to "load-bearing."

**Prerequisite:** install the Rust toolchain (`rustup`, stable, `x86_64-pc-windows-msvc`), and likely
VS Build Tools with "Desktop development with C++" for the MSVC linker.

**Left open:** web export (M5) and whether WASM plays a role in game logic (Q1).
