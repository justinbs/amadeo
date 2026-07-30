# 07 — Working With the Code

> Justin's map into the codebase. Written for someone comfortable programming but still learning Rust.
> **Claude: keep this current.** Every new architectural pattern gets a short entry with a worked
> example. It's a stated project requirement (`CLAUDE.md` §6), not documentation garnish.

Right now there's no code yet, so this file is mostly setup and orientation. It grows with the engine.

---

## Environment setup

### 1. Rust toolchain

Already installed (verified 2026-07-30): rustup, rustc 1.97.1, cargo 1.97.1, target
`stable-x86_64-pc-windows-msvc`. Lives in `%USERPROFILE%\.cargo\bin`.

### 2. MSVC build tools — required

Rust's `msvc` target uses Microsoft's linker, which does not ship with Rust:

```
winget install Microsoft.VisualStudio.2022.BuildTools --override "--quiet --wait --add Microsoft.VisualStudio.Workload.VCTools --includeRecommended"
```

Without it, compilation succeeds and *linking* fails with `linking with link.exe failed`.

*Why the msvc target and not `gnu`?* The `x86_64-pc-windows-gnu` target bundles its own linker and
skips this download, but msvc has better support for Windows graphics APIs — which matters because
wgpu uses DX12 here. Not worth trading away for one install.

### 3. VS Code setup

Install the **rust-analyzer** extension. It gives inline types, jump-to-definition, inline errors, and
hover docs — the single biggest quality-of-life difference when reading unfamiliar Rust.

```
code --install-extension rust-analyzer
```

Also useful: **Even Better TOML** (for `Cargo.toml`), **CodeLLDB** (for debugging).

### 4. Known gotcha: stale PATH

Installers update the persistent PATH, but already-running processes keep the environment they started
with. VS Code's integrated terminal inherits from the VS Code process — so after installing a tool,
**restart VS Code itself**, not just the terminal tab. A `command not recognized` error right after a
successful install is almost always this.

### 5. Known gotcha: Smart App Control blocks binaries run from temp directories

**Smart App Control is enabled and enforced on this machine**
(`VerifiedAndReputablePolicyState = 1`). Freshly compiled binaries are unsigned and have no
reputation, so SAC blocks them when they are run from `AppData\Local\Temp` and similar locations:

```
Program 'foo.exe' failed to run: An Application Control policy has blocked this file
```

**Verified 2026-07-30:** the same binary built and run from a normal user directory
(`C:\Users\justi\Desktop\...`) executes fine. The block is location-sensitive, not a blanket ban on
unsigned code.

**Therefore: always build and run inside the project directory. Never build to a temp path.** That is
the normal workflow anyway, so this costs nothing.

**Do not "fix" this by disabling Smart App Control.** On Windows 11, turning SAC off is a **one-way
change** — re-enabling it requires reinstalling Windows. It is not needed here, and it is Justin's
decision to make regardless, not something a session should reach for. If a binary won't run, check
*where* it was built before touching any security setting.

---

## Everyday commands

None of these work until M0 scaffolds the workspace. Recorded here so they're in one place.

| Command | What it does |
|---|---|
| `cargo check` | Type-checks without producing a binary. **Fastest feedback — use this while iterating.** |
| `cargo build` | Compiles a debug binary. Slower. |
| `cargo build --release` | Optimized. Much slower to compile, much faster to run. Use for perf testing. |
| `cargo run -p <crate>` | Builds and runs a specific crate in the workspace. |
| `cargo test` | Runs all tests, including determinism and golden-replay tests. |
| `cargo clippy` | Lints. Catches real bugs, not just style. Worth running before committing. |
| `cargo fmt` | Auto-formats. Never argue about formatting. |
| `cargo doc --open` | Builds and opens the API docs generated from doc comments. |

**Reading a Rust error message.** rustc errors are unusually good — the useful part is usually below
the first line. Look for `help:` and `note:`; they frequently contain the exact fix. `cargo build`
also often ends with a suggested command like `rustc --explain E0502`, which gives a full explanation
of that error class.

---

## How to read this codebase

**Start with the crate graph** in `CLAUDE.md` §4. Crates are listed in dependency order, and a crate
may only depend on ones above it. That ordering is also a reading order: `amadeo-math` and
`amadeo-core` are small and self-contained; `amadeo-ecs` is where the interesting parts begin.

**Find behavior by looking for systems, not methods.** This engine has no `Player` class with an
`update()` method. Instead, there are components (plain data — `Transform`, `Velocity`) and systems
(functions that query and mutate them — `integrate_velocity`). To answer "what makes the player move,"
search for systems that read the relevant components. This is the biggest mental shift if you're
coming from Unity or Godot, and it's explained in `adr/0004`.

**Naming tells you what a thing is:**
- Components are nouns: `Transform`, `Velocity`, `Sprite`
- Systems are verb phrases: `integrate_velocity`, `resolve_collisions`
- Events are past tense: `EntitySpawned`, `DamageDealt`

---

## Rust patterns used in this engine

*Grows as the engine is built. Each entry: what it is, why it's here, minimal example.*

### `Result` and `?` — how errors travel
Rust has no exceptions. A function that can fail returns `Result<T, E>`, and `?` means "if this
failed, return that error to my caller." Engine crates return typed errors (`thiserror`); the CLI and
games use `anyhow`, which is more convenient but less specific.

```rust
fn load_config(path: &Path) -> Result<Config, ConfigError> {
    let text = std::fs::read_to_string(path)?;   // ? propagates a read failure upward
    let config = parse(&text)?;
    Ok(config)
}
```

You will not see `unwrap()` or `expect()` in engine crates outside tests — those crash on failure, and
`CLAUDE.md` §6 forbids them here. If you see one in engine code, it's a bug.

*(More entries land as the engine takes shape: ECS queries, the borrow checker in system parameters,
handles vs. references, the reflection derive macro.)*

---

## If you get stuck or disagree with something Claude did

- **`git log` and the commit messages** are written to explain *why*. Start there.
- **`docs/adr/`** holds the decision records. If something looks arbitrary, there's likely an ADR
  explaining the trade-off and what was rejected.
- **`docs/06-open-questions.md`** lists what's deliberately undecided — so if something feels
  half-finished, check whether it's intentionally pending.
- **The eight invariants in `CLAUDE.md` §2** are the rules Claude is bound by. If code appears to
  violate one, that's a real bug worth raising — those are load-bearing.
- **Nothing here is sacred except the invariants.** If a design feels wrong to you, say so. The
  plan → build → re-plan cycle exists precisely for that, and an ADR can be superseded.
