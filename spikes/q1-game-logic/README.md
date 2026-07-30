# Q1 spike — how is game logic authored and hot-reloaded?

Evidence for **`docs/adr/0011`**. Read the ADR for the decision; this file is the method and the
raw numbers.

Measured 2026-07-31 on: AMD Ryzen 7 5700X3D (8C/16T), 40 GB, Windows 11 Pro 26200, rustc 1.97.1.

---

## Method

Four candidates run **the same world, the same player path, and the same enemy behaviour**. Only the
mechanism that authors and reloads the enemy AI differs. The shared half lives in `scenario/`; each
candidate supplies exactly one system.

The benchmark task is a three-state enemy AI (patrol / pursue / search) over 64 enemies for 1800
ticks (30 s of simulated time at 60 Hz). It was chosen because it is the smallest honestly
non-trivial gameplay system, and because it previews real work — `mod-behaviour` in M3 needs it.
It exercises component read+write, a resource, the engine RNG, branching control flow, and a tunable
constant. Full specification in `scenario/src/lib.rs`.

**Agreement is tested by state hash, not by inspection.** Two candidates that produce the same
`World::state_hash` after 1800 ticks computed the same simulation. **State survival across a reload
is tested the same way**: run 1800 ticks with a reload at tick 900, and the final hash must equal
the uninterrupted run's. Any state loss, re-initialisation, or RNG divergence breaks it.

```
pwsh -File measure.ps1          # everything, ~4 minutes
pwsh -File measure.ps1 -Quick   # skip the rebuild samples
```

## The candidates

| Dir | Candidate |
|---|---|
| `a-rust/` | **A** — gameplay compiled into the binary. The baseline. |
| `b-cdylib-logic/`, `b-cdylib-host/` | **B** — gameplay as a `cdylib`, reloaded via `libloading`. |
| `c-luau/` | **C** — gameplay in Luau, embedded via `mlua`. |
| `d-wasm-logic/`, `d-wasm-host/` | **D** — gameplay compiled to WASM, hosted by `wasmtime`. |

---

## Results

### edit → observe, the number that matters most

| | rebuild | reload swap | back to tick 1800 | **total** |
|---|---|---|---|---|
| **A** pure Rust | 0.90 s | — | 47 ms (restart + re-simulate) | **≈ 0.95 s** |
| **A** in the real game¹ | 2.03 s | — | restart | **≈ 2.1 s** |
| **B** cdylib | 0.66 s | 25 ms | 0 — state preserved | **≈ 0.69 s** |
| **C** Luau | 0 — nothing compiles | 0.42 ms | 0 — state preserved | **≈ 0.4 ms** |
| **D** WASM | 0.61 s | 20 ms | 0 — state preserved | **≈ 0.63 s** |

¹ `cargo build -p quad-demo`, which links wgpu and winit. Editing a core engine crate and rebuilding
everything downstream: **3.24 s**.

### Agreement with native Rust — state hash after 1800 ticks

| | hash | |
|---|---|---|
| **A** | `4ce822099ee1fa4a` | reference |
| **B** | `4ce822099ee1fa4a` | ✅ identical |
| **C** | `8b4e0eb7c9d2e2fa` | ❌ **differs** |
| **D** | `4ce822099ee1fa4a` | ✅ identical |

All four agree on the *qualitative* outcome (41 patrolling, 4 pursuing, 19 searching). C's numbers
drift.

**C diverges at tick 2**, not gradually. Tick 1 matches exactly, because every enemy starts on a
whole-number offset from its waypoint and the arithmetic is exact in both precisions. From tick 2
the positions are irrational and Luau's `f64` intermediates round differently from Rust's `f32`.

**C is still deterministic** — five separate processes produced `8b4e0eb7c9d2e2fa` every time. It is
self-consistent; it simply does not compute the same thing as the Rust reference.

**D's bit-exactness is robust**, not luck: the guest was rebuilt at `opt-level=0` and at
`opt-level="s"` with LTO, and produced the same hash both times. WebAssembly specifies `f32`
operations exactly, and LLVM does not reassociate floats without fast-math, which Rust never enables.

### State survival across a reload

| | | |
|---|---|---|
| **A** | — | no mechanism; the process restarts |
| **B** | ✅ | hash unchanged across the swap; final hash still `4ce822099ee1fa4a` |
| **C** | ✅ | hash unchanged across the swap; final hash still `8b4e0eb7c9d2e2fa` |
| **D** | ✅ | hash unchanged across the swap; final hash still `4ce822099ee1fa4a` |

### Runtime cost — release build, 64 enemies

| | µs/tick | vs native |
|---|---|---|
| **A** pure Rust | 4.6 | — |
| **B** cdylib | 4.6 | ~1× |
| **D** WASM | 5.7 | 1.24× |
| **C** Luau | 109.7 | **24×** |

**Where Luau's cost actually is**, measured by running the benchmark with a do-nothing script:

| | µs/tick |
|---|---|
| full script | 106.9 |
| do-nothing script — marshalling alone | 84.0 |
| **Luau executing the AI** | **22.9** |

So ~78% of the overhead is the table-marshalling binding, not the language. Luau itself runs the AI
in 22.9 µs — the same order as native Rust's *entire* 4.6 µs tick, which for an interpreted language
is genuinely fast. A userdata-based binding would recover most of the difference; building one well
is the "every engine API must be bound and kept in sync" tarpit that `docs/04-subsystems.md` warns
about.

### What candidate A pays instead of a reload

A cannot preserve state, so its real cost grows with how far into a session the interesting moment
is:

| restart + re-simulate | wall time |
|---|---|
| 1 800 ticks (30 s simulated) | 47 ms |
| 7 200 ticks (2 min simulated) | 158 ms |
| 18 000 ticks (5 min simulated) | 382 ms |

Linear, at ~21 µs/tick. **This — not compile time — is A's structural weakness**, and it is fixed by
snapshot/restore (already planned in M1's `amadeo-agent`), not by a scripting language.

### Cost of adoption

| | added dependencies | cold build | notes |
|---|---|---|---|
| **A** | none | — | |
| **B** | `libloading` | ~1 s | needs `unsafe`; conflicts with `unsafe_code = "forbid"` |
| **C** | `mlua` + vendored Luau C++ | 85 s | needs a working C++ toolchain |
| **D** | `wasmtime` (~200 crates) | 145 s | plus the `wasm32-unknown-unknown` rustup target |

---

## Findings that were not on the question list

Discovered while building the prototypes. All four are real and outlive this spike.

1. **`Service: Send + Sync` excludes every scripting runtime.** Neither `mlua::Lua` nor
   `wasmtime::Store` is `Sync`, so neither can live in the world. Both candidates had to keep the
   runtime in an `Rc<RefCell<..>>` captured by the system closure — which works, because `system()`
   requires only `FnMut(&mut World) + 'static`, but it means the runtime is **invisible to world
   introspection**. That is a direct cost against `docs/03-ai-native-design.md`. The same bound will
   block an audio mixer and an asset loader later.

2. **ECS queries max out at two components, and the benchmark needed three.** `Enemy` (write),
   `Transform2d` (read), `Velocity` (write) does not fit `for_each_pair_mut`, so every candidate
   collects into a `Vec`, decides, and writes back by entity handle. This levelled the comparison —
   a script or WASM boundary would have to marshal anyway — but it is a real ergonomic limit, and it
   is on A's and B's critical path where it need not be.

3. **ADR 0008 is what makes candidate B work at all.** `ComponentId` hashes the type *name* rather
   than using `TypeId`, so the host and the dynamic library agree on component identity despite
   being separate compilation units. A `TypeId`-based design would have failed here, silently — the
   downcast would return `None` and the AI would do nothing.

4. **B's dangerous failure mode is not the one its ABI check catches.** The exported
   `amadeo_abi_version` catches a stale library. It cannot catch a *changed component layout*:
   edit a component's fields, rebuild only the library, and the host reinterprets old memory as the
   new shape with no error at all.

## Reproducing

```bash
cargo build -p a-rust -p b-cdylib-logic -p b-cdylib-host -p c-luau -p d-wasm-host
```

```bash
cd d-wasm-logic && cargo build --release --target wasm32-unknown-unknown
```

Then any of:

```bash
cargo run -p a-rust
```

```bash
cargo run -p b-cdylib-host -- --reload-at 900
```

```bash
cargo run -p c-luau -- --script null --ticks 1800
```

```bash
cargo run -p d-wasm-host -- --reload-samples 20 --ticks 60
```
