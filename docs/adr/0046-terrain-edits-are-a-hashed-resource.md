# ADR 0046 — Terrain edits are a hashed resource, and the streamer is a cache of them

**Status:** Accepted · **Date:** 2026-08-09 · **Supersedes:** ADR 0042 §4 (the placement only) ·
**Builds on:** ADR 0009, ADR 0027, ADR 0028, ADR 0036, ADR 0042, ADR 0043

## Context

ADR 0042 settled what terrain *is*: a generated base plus a sparse overlay of authored edits, so that
**a save file is a seed plus a diff**. §4 said where the diff lives:

> Edits are a reflected, hashed component on a chunk entity. Sparse — a coordinate and a value.

That was written before chunk entities existed. They exist now, and ADR 0043's streaming **despawns
them when a chunk leaves the drawn region** — so an edit stored on one would be destroyed by walking
away from the hole you had just dug.

Session 12 shipped the streamer with edits held in its own `Service` instead, which works while the
game is running and fails the moment it is saved: a `Service` is outside the state hash and untouched
by a snapshot (ADR 0009), so **a dug world reloaded undug**. That is ADR 0042's central promise
quietly unkept, and it was recorded as **Q29** rather than guessed at.

## What the options actually were

**An entity per *edited* chunk**, whose existence is driven by having been edited rather than by
being loaded. Closest to §4's intent, and it lines up with per-entity network replication, which six
of the eight target games need.

**One hashed resource** holding every edit in the world.

**Edits as their own asset file.** Rejected on inspection: an asset is authored once and loaded, and
edits change every time somebody swings a pickaxe. It also sits outside the state hash, which leaves
the actual bug — replays not reproducing a dig — unfixed.

**The locality argument for entities does not survive contact with the code.** The expectation was
that per-chunk storage would make hashing cheaper, because only edited chunks would be walked.
`World::state_hash` walks **every** entity and **every** resource regardless, so the two are the same
cost. What remained was a genuine multiplayer advantage against a second kind of chunk entity that is
*not* a streamed chunk, sitting next to one that is — a confusion this codebase would pay for
repeatedly.

## Decision

### 1. `TerrainEdits` is a reflected, hashed `Resource`

It satisfies every requirement with machinery the engine already has, which was verified rather than
assumed: `state_hash` walks resources, and `Snapshot` captures and restores them by canonical name.
It belongs to no entity, so streaming cannot take it away.

### 2. Flat, keyed by world sample — **not** grouped by chunk

This is the part that changed during implementation, and it is worth stating because the reasoning is
the opposite of what it looks like.

Grouping by chunk is the obvious shape for a network delta. It is also **wrong**, for the reason
`amadeo_voxel::Edits` is keyed by world sample and says so: a sample near a boundary is read by up to
**eight** chunks (the two-sided apron, ADR 0043 §4). Storing it under one owning chunk leaves the
other seven meshing that point differently, and the crack opens exactly where somebody has been
digging.

Per-chunk deltas remain derivable — which chunks read a sample is integer division, and
`TerrainStreamer::edit` already computes it. Structure that can be recomputed is not worth a bug that
cannot be.

### 3. Gameplay writes the resource; the streamer is a cache rebuilt from it

The same asymmetry ADR 0036 gives physics: **components are the source of truth and the solver's
world is a cache rebuilt from them.** A game's dig system writes `TerrainEdits`; `stream_terrain`
carries the change into the streamer before it updates.

Writing the streamer directly is now the mistake, because it puts the hole somewhere neither the
state hash nor the save file can see.

### 4. A revision counter is what makes a restore work

`TerrainEdits::revision` counts *changes*, not entries — digging the same sample twice still counts.
`Terrain` remembers which revision it has applied.

A snapshot restores the resource and **cannot** restore a service, so after one the streamer holds no
edits and its applied revision is zero while the resource's is whatever was saved. They disagree, the
sync runs, and the world is dug again before the next frame. That is ADR 0028's lesson — hash
equality after a restore is necessary and not sufficient — handled rather than rediscovered.

### 5. The sync is a two-way diff

`TerrainStreamer::replace_edits` compares in **both** directions and invalidates only chunks whose
samples actually changed.

Both directions, because restoring an *earlier* save means the authored set has **fewer** edits than
the streamer holds. A diff that only walked the new set would leave the extra digging in place
forever — a world that cannot be undone. `going_back_to_fewer_edits_fills_the_hole_back_in` is that
case.

## Consequences

**Good.**

- A dug world saves and reloads dug, and a replay of a dig reproduces. ADR 0042's promise is kept
  rather than stated.
- No new concepts: a resource, a counter, and a diff, all of which the engine already understood.
- Digging is authorable and inspectable like everything else — `TerrainEdits` shows up in `describe`
  and in a snapshot file as plain text.
- It closed a second gap on the way: there was **no supported way for a game to save or load at
  all**, because the registry a snapshot needs is crate-private. `App::capture_snapshot` and
  `App::restore_snapshot` exist now, which is what M3's "save/load built on snapshots" stands on.

**Bad, and accepted.**

- **One growing structure.** Every edit in the world is in the state hash every time it is computed.
  That is the same cost the entity form would have had, but it is still a cost, and a Minecraft-scale
  world would want the hash to be incremental. Not built, because no game here has that problem.
- **The sync is `O(total edits)` on a tick where somebody dug** — nothing at all on the ticks where
  nobody did, which is nearly all of them. The fix when it matters is a change list on the resource
  rather than a diff, and the place to put it is obvious.
- **Multiplayer replication is unanswered.** ADR 0006 reserves the hooks and M6 owns the problem; a
  resource has no per-entity replication path today. §2 above is what keeps that door open.
- ADR 0042 §4 is now wrong in print. It stays, because a decided ADR is never edited — this one
  supersedes it, and the *data model* it describes is untouched and still correct.
