# 02 — Tech Stack

Decision record: `adr/0002-tech-stack-rust-wgpu.md`. This document holds the detail and the
alternatives that were rejected.

## Constraints that drove the choice

Ranked, because they conflict:

1. **Unified 2D and 3D from the start** (user requirement) — rules out lightweight 2D-only stacks.
2. **Native desktop first** (user requirement) — rules out browser-primary stacks.
3. **A graphical editor is a first-class deliverable** — needs a viable in-process native GUI story.
4. **Determinism is an invariant** — needs control over floating point, memory, and iteration order,
   plus a physics engine that promises reproducibility.
5. **Fast iteration for an AI agent** — the one place the chosen stack is weakest, and therefore the
   thing we design hardest against.
6. **Available tooling on this machine** — Node and Java only; any choice requires an install, so
   this constraint carries less weight than it first appeared to.

## The stack

| Layer | Choice | Why this one |
|---|---|---|
| Language | **Rust** (2024 edition) | No GC, so frame times are predictable and determinism is achievable. Strong types mean generated code that compiles usually also *works* — this matters more for an agent than a human. One language across engine, editor, tools, and tests. Mature game ecosystem to borrow from. |
| Graphics | **wgpu** | One API over Vulkan/DX12/Metal, so unified 2D/3D is a single codebase. Modern explicit API (bind groups, render passes) suits a render graph. Crucially it also targets **WebGPU**, so a browser export path exists later for near-free — that recovers the cheap-sharing benefit of a web engine without giving up native. |
| Windowing / events | **winit** | De facto standard, what wgpu examples assume, handles the Windows path well. |
| Math | **glam** | SIMD-accelerated, game-shaped API, ubiquitous. `amadeo-math` wraps it so we own the public surface and can swap or add fixed-point later. |
| Physics | **rapier** (2D and 3D) | Both dimensions from one project with a consistent API. Has an explicitly deterministic mode with cross-platform reproducibility — nothing else in Rust offers this. Wrapped behind our own traits so it stays replaceable. |
| Editor GUI | **egui** | Immediate-mode, renders through wgpu (so it shares our device), trivially embeddable, extremely fast to build tooling in. An editor is a lot of UI; egui minimizes the cost per panel. |
| Game UI | **`amadeo-ui`, custom, retained-mode** | Deliberately *not* egui. Game UI needs styling, animation, controller focus navigation, and to be authored in scene files. Immediate-mode is wrong for that. Do not conflate these two systems. |
| Serialization | **custom canonical text format** via `amadeo-reflect` | See `adr/0003`. Off-the-shelf serializers optimize for round-trip fidelity, not for being hand-written or diffed. The format is a designed artifact. |
| Asset import | `image`, `gltf`, `symphonia`, `rodio`/`kira` (audio TBD) | Standard, well-maintained. Import happens once into an internal format; runtime never parses source assets. |
| RPC / agent protocol | **JSON-RPC over stdio and a local TCP socket** | Boring on purpose. Trivially inspectable, trivially scriptable, no codegen needed for a client, works from a shell. |
| Tests | `cargo test` + golden replay harness | Behavior tests come from replays, not from mocks. |

## Rejected alternatives

### TypeScript + WebGPU (Electron/Tauri shell)
**Initially recommended, then rejected when the requirements landed.** Worth recording honestly.

Genuinely excellent on iteration speed (sub-second reload) and observability — the browser's devtools
protocol is already an agent-driving interface, and screenshots are free. Zero install for the user.

Rejected because: "native desktop first" and "unified 2D/3D" together push past what a GC'd
scripting language in a browser shell does well. Real 3D scenes with skeletal animation and physics
at stable frame times means fighting GC pauses forever, and the perf ceiling would eventually cap the
kinds of games we could make. Choosing it would have optimized my convenience over the engine's
capability.

*What we keep from it:* the observability lessons. Everything the browser gave us for free —
screenshot on demand, queryable live state, console-as-protocol — we now build explicitly in
`amadeo-agent`. That's the price of going native, and it's paid deliberately.

### C# / .NET 9 + Silk.NET
The strongest runner-up, and not a bad choice. Compile times 3–10x better than Rust, real hot reload
in the toolchain, huge training corpus, and Godot/Unity/Stride/MonoGame prove it works for engines.

Rejected because: GC tail latency conflicts with the determinism invariant in ways that require
constant vigilance (struct-only ECS, pooling everywhere) — and if you're going to write C# like it's
Rust, write Rust. The native graphics binding ecosystem is also more fragmented than wgpu, which
matters a lot for a unified 2D/3D renderer.

### C++ with hand-rolled everything
Maximum control and the industry default. Rejected on build system pain, memory-safety burden, and
the worst agent ergonomics of any option — subtle UB is exactly the failure mode where generated code
is most dangerous and least verifiable.

### Building on top of Bevy
Tempting: Bevy is Rust, ECS, wgpu, and mature. Rejected because we would inherit its scene format,
its scheduler semantics, and its editor story — and I1/I2/I3/I5 are all things we'd be fighting Bevy
to impose rather than designing in. We do, however, treat Bevy as **reference material**: read it for
archetype storage, scheduling, and render graph design. Learning from it is encouraged; depending on
it is not.

### Godot as a host (GDExtension)
Would give an editor and renderer immediately. Rejected because the whole thesis is that the
editor-owns-truth model is the problem; adopting Godot adopts the problem.

## The known weakness, stated plainly

**Rust compile times are the biggest risk this stack carries**, and they hit the agent loop harder
than the human one. A 30-second rebuild between "change enemy speed" and "see enemy speed" is
tolerable for a person and corrosive for an agent doing twenty iterations.

Mitigations, in order of importance:
1. **Open question Q1** exists to solve exactly this. Game logic must not require a full engine
   rebuild. Resolved by measured spike in M0.
2. Small crates and a shallow dependency graph, so incremental rebuilds touch little.
3. Use `cargo check` and a fast linker (`lld`) in the dev loop; reserve full builds for runs.
4. Split heavy dependencies behind feature flags so a headless test build stays lean.
5. Headless deterministic tests as the primary verification path — they're far faster than launching
   a window and looking at it.

## Prerequisites to install before M0

```bash
rustup toolchain install stable
```

Also likely needed: Visual Studio Build Tools with "Desktop development with C++" (for the MSVC
linker that some crates require). Not needed: Python, cmake, Node.
