# ADR 0010 — The window and event loop live in the game binary, not the engine

**Status:** Accepted · **Date:** 2026-07-30

## Context

M0's exit gate needed a real window. The obvious place to put a windowing loop is the engine — most
engines expose something like `App::run()` that creates a window, pumps OS events, and drives the
simulation. A fresh reader's instinct is that the engine should own the main loop.

Two things make that wrong here.

**The dependency order forbids it.** `amadeo-render` sits *above* `amadeo-app` (`CLAUDE.md` §4), so
`amadeo-render` may not depend on `amadeo-app`. A loop that both creates a surface and steps the app
needs both, so it cannot live in either without violating invariant I6.

**It would make windowing mandatory.** If `amadeo-app` depended on winit and wgpu, every headless
build — every test run, every CI job, every future dedicated server — would compile ~200 crates it
never uses. Invariant I7 says every subsystem is headless-capable; a required windowing dependency
undermines that in practice even if the code paths technically allow it.

## Decision

**The event loop belongs to the game binary.** `games/quad-demo` owns the winit `ApplicationHandler`,
creates the window, constructs `WgpuBackend`, and drives `App::advance_real_time` and `App::render`.

Two supporting rules:

1. **The GPU backend is opt-in.** `amadeo-render` builds without wgpu by default; the `gpu` feature
   adds it. The abstraction and `NullBackend` compile and test with no GPU at all.
2. **No engine crate depends on a windowing library.** `LiveSource` in `amadeo-input` stores action
   *names and values*, so the platform layer owns the key-to-action mapping and the input crate stays
   device-agnostic. A gamepad or a remapping UI plugs into the same seam later.

## Consequences

- `cargo test --workspace` compiles no windowing or GPU code and stays at a couple of seconds. This
  is the single biggest factor in the everyday iteration loop, and ADR 0002 already flagged compile
  time as this stack's main weakness.
- Every game repeats a modest amount of windowing boilerplate. Acceptable for now; when a second game
  exists and the shape has stopped moving, a `amadeo-shell` crate above `amadeo-render` can hold the
  shared loop **without** any lower crate depending on it. Deliberately not built for one caller.
- The editor (M4) will own its own loop for the same reason, which is fine — it is a separate binary
  and already a client of `amadeo-agent` (ADR 0003).
- A headless agent run needs no window at all: construct `App`, install `Renderer::headless()`, call
  `run_ticks`. That path is already exercised by the determinism suite.

## Rejected alternatives

**`amadeo-app::run_windowed(app)`.** The conventional shape. Rejected because it forces `amadeo-app`
to depend on `amadeo-render` and winit, making windowing mandatory for headless builds and inverting
the crate order.

**A feature flag on `amadeo-app` enabling the windowing path.** Keeps headless builds lean and puts
the loop in the engine. Rejected because feature-gated dependency edges make the crate graph
conditional, which is exactly the thing invariant I6 exists to keep simple — "does A depend on B"
should have one answer, not one per feature combination.

**A separate `amadeo-shell` crate now.** Where this ends up, probably. Rejected *for now* because
there is one caller, and a shared abstraction extracted from a single example is usually the wrong
abstraction. Revisit when a second game exists.
