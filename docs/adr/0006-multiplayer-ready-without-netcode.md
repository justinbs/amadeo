# ADR 0006 — Reserve multiplayer hooks now, build netcode at M6

**Status:** Accepted · **Date:** 2026-07-30

## Context

Session 1 listed multiplayer as a non-goal: an enormous scope multiplier touching every subsystem, with
the door left open via determinism and snapshots.

Session 2 established the target games — **Palworld, Schedule I, and Inside the Backrooms**. All three
are co-op. A non-goal that every intended game requires is not a non-goal; it is an unacknowledged
dependency.

Networking is plausibly the most painful retrofit available in engine development, worse than
determinism, because it simultaneously touches:

- **Entity identity** — the network needs stable, shared identity across processes
- **Authority** — who is allowed to mutate what
- **State replication** — which components sync, how often, and how they interpolate
- **Input** — client prediction and server reconciliation
- **Physics** — server authority over simulation
- **Every gameplay system** — each one must know whether it runs on client, server, or both

Retrofitting means revisiting all of it. But building netcode now would slow every milestone and mean
debugging networked gameplay while the engine underneath is still moving — a genuinely bad combination.

## Decision

**Split the problem by cost. Decide the cheap structural half now, during M0–M2, while the affected
systems are being written for the first time. Build the expensive half at M6.**

### Reserved now (cheap, because these systems are being built anyway)

| Hook | Where | Why now |
|---|---|---|
| **Network identity** | `amadeo-ecs`, `amadeo-scene` | Stable authoring IDs are already being designed (ADR 0003 §3). Extending that concept to a shared cross-process identity space is nearly free while designing it, and invasive afterward. |
| **Replication metadata** | `amadeo-reflect` | The reflection registry is already recording per-field metadata. Adding `#[replicate]`-style annotations (sync policy, interpolation hint, authority) costs almost nothing now and would otherwise mean revisiting every component. |
| **Authority concept** | `amadeo-ecs` | An explicit notion of "who owns this entity" — even if it's always `Local` in single-player. Systems can then be written correctly from the start rather than assuming universal write access. |
| **Client/server role in app config** | `amadeo-app` | A role enum (`Standalone`, `Client`, `Server`, `ListenServer`) threaded through configuration, even while only `Standalone` exists. |
| **Dedicated server** | `amadeo-app` | **Already free.** Invariant I7 requires every subsystem to be headless-capable with null backends. A headless authoritative simulation is what a dedicated server *is*. |
| **Deterministic simulation** | already invariant I3 | Not the networking model, but makes a reproducible server simulation dramatically easier to debug. |

### Deferred to M6 (expensive, and safe to defer)

Transport layer, connection lifecycle, snapshot delta encoding and bandwidth management, client-side
prediction, server reconciliation, lag compensation, interest management, and netcode tuning.

### The networking model, when it arrives

**Client-server with server authority and client prediction.** This is what co-op survival games use,
and it is what all three target games need.

Explicitly *not* deterministic lockstep. This corrects a framing from session 1, which implied that
determinism plus snapshots would supply rollback netcode. That is true for a different genre — fighting
games, small-player-count competitive — and not what these targets need. Determinism remains valuable
here for debuggability and testing, but it is not the architecture.

## Consequences

**Costs, accepted:**
- Small ongoing overhead in `amadeo-ecs` and `amadeo-reflect` for concepts unused in single-player.
- Some risk of designing the wrong hook. Mitigated by keeping the hooks minimal and declarative —
  metadata and identity, not machinery. A wrong annotation is cheap to change; a wrong architecture is
  not.
- Systems authors must think about authority even when there is only one player. This is mild friction
  and also just good discipline.

**Gains:**
- M6 becomes additive rather than a rewrite.
- The dedicated-server story arrives free via I7.
- Component authors annotate replication once, at the moment they have the most context about the
  component, rather than in a later sweep across the whole engine.

**What this does not authorise:** no transport code, no prediction, no reconciliation before M6. If a
milestone starts growing networking machinery, that is scope creep and should be pushed back to M6.

## Rejected alternatives

**Keep multiplayer a genuine non-goal.** Fastest to a working single-player engine. Rejected because
every named target game is co-op, so this defers a cost rather than avoiding one, and defers it to the
point of maximum expense.

**Make co-op core from M2, building networking alongside 3D.** Zero retrofit risk and no chance of a
wrong assumption baking in. Rejected on schedule and on debuggability: it pushes the first complete game
out by at least a full milestone, and debugging networked gameplay on top of an engine that is still
changing underneath is a poor way to build either.

**Design for deterministic lockstep instead.** Would let netcode reuse the existing determinism
machinery almost directly. Rejected because it is the wrong model for co-op survival games with
open worlds and asymmetric player activity — lockstep couples all clients to the slowest, which is
unacceptable for this genre.
