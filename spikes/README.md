# spikes/

Throwaway-by-design prototypes that exist to **answer a question with a measurement** rather than
with an argument. Each one backs an ADR.

A spike is not engine code. It is evidence.

## Rules

- **Each spike is its own cargo workspace**, excluded from the engine workspace. That keeps feature
  unification, heavy dependencies, and lint policy from leaking into the engine — and, importantly,
  keeps `cargo test --workspace` in the engine at a couple of seconds.
- **The engine crates are consumed by relative path**, so a spike measures the real engine rather
  than a mock of it.
- **A spike is frozen once its ADR is written.** It is not maintained, and it is expected to stop
  compiling eventually as the engine moves. That is fine: the ADR is the durable artefact. If a
  spike needs to be re-run years later, fix it then, deliberately.
- **`Cargo.lock` is committed.** Measurements taken against unpinned dependencies are not
  reproducible, and therefore are not evidence.
- Build directories are gitignored via `/spikes/**/target/`.

## Spikes

| Spike | Question | ADR | Status |
|---|---|---|---|
| `q1-game-logic` | How is game logic authored and hot-reloaded? | `docs/adr/0011` | ✅ resolved, frozen |
