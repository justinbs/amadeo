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
| `q2-scene-format` | Which concrete syntax for scene files? | `docs/adr/0014` | ✅ resolved, frozen |

## Not every spike is code

`q2-scene-format` is four hand-written files and a comparison script — no cargo workspace. A spike is
**evidence for a decision**; usually that means a prototype, sometimes it means artefacts and a
measurement harness. The rules above still apply: committed, reproducible, and frozen once its ADR
is written.

Note also that a spike can end *without* settling its question. Q1's numbers were decisive. Q2's
were not — they ruled out two candidates and showed that the criterion everyone expected to be the
discriminator (diff quality) does not discriminate at all. That is a real result, and it hands the
remaining judgement to a human rather than pretending the measurement made it.
