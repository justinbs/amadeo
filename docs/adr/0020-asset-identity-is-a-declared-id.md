# ADR 0020 — An asset is identified by a declared id, not by its path

**Status:** Accepted · **Date:** 2026-08-01 · **Resolves:** Q4

## Context

`amadeo-assets` is the last unbuilt thing blocking two others: sprite textures (so the 2D renderer
can draw something that is not a coloured rectangle) and prefab instancing (`instantiate` still
refuses a `from` line, saying exactly why). Before any of it, one question: what names an asset?

`docs/04-subsystems.md` §5 recorded two options and leant toward paths with a rename-tracking tool.
Looking at it against decisions this project has since made changes the picture.

**This is Q13 again, one layer up.** Q13 asked whether a component's identity should be its
*location* (the Rust path) or its *name*. ADR 0017 chose the name, because coupling identity to
location made a pure refactor — moving a file — silently invalidate every state hash that mentioned
it, with no warning. A filesystem path is a location. "Asset identity is the path" has the same
shape of flaw and the same failure mode: reorganising a folder quietly breaks references, and
nothing tells you until something is missing at runtime.

**And the project already answered it for entities.** ADR 0014's scene format writes
`entity a1 "Player"` — a stable authoring id that survives reordering — precisely so that moving an
entity within a file does not churn everything referring to it. `a1` is the identity; the position
in the file is not. Assets resting on a different principle than entities would be an inconsistency
every reader has to learn and remember.

**The real argument for paths, which is specific to this engine.** A path is *self-describing to an
agent*. With paths I can list a directory and reference what I see; `amadeo check` verifies a
reference by asking whether a file exists. With a declared id I have to read sidecars to learn what
ids even exist — an extra indirection, and a new place to be wrong. That is a genuine cost against
the AI-native thesis (`docs/03`), not a hypothetical one, and it is what made this a real decision
rather than a formality.

## Decision

**An asset is identified by an `id` declared in its sidecar.** Not its path, not a GUID.

```text
# textures/wall_concrete.png.ama-meta
id = "wall_concrete"
filter = "nearest"
```

A scene refers to it by that id alone:

```text
entity a1 "Wall" from wall_concrete
```

### The id defaults to the filename stem

This is what resolves the tension above rather than accepting it.

On import, an asset with no sidecar gets one generated, with `id` set to the file's stem. So
`textures/wall_concrete.png` becomes `id = "wall_concrete"` with nobody typing anything.

The consequence is that **it reads exactly like paths on day one** — an agent listing a directory
guesses the id correctly nearly every time — while the id is nonetheless *recorded*, so moving the
file to `textures/interior/wall_concrete.png` changes nothing. The ergonomics of paths, without the
location coupling.

The default is a starting value, not a rule. Renaming the file afterwards does not change the id,
which is the entire point.

### Ground truth is a protocol method, not a convention

`assets.list` (already planned in `docs/03-ai-native-design.md`) reports every id, its source path,
and its load state. `amadeo assets` exposes the same thing.

This is the mitigation for the one real cost, and it must exist **before** the id becomes the
reference syntax — otherwise the first agent to author a scene has to guess. Guessing is exactly the
"plausible but wrong" failure Pillar 2 exists to eliminate.

### Duplicate ids are an error, named at both ends

Two assets claiming one id is refused at scan time, with a message naming **both files**. The
registry already does this for component names (ADR 0017's accepted cost), and the reasoning
transfers: a collision resolved arbitrarily is a bug that surfaces somewhere else entirely.

### Scanning is ordered

The asset scan walks directories in sorted order and stores into a `BTreeMap`, for the same reason
every other registry in this engine does. Filesystem enumeration order is not reproducible across
machines, and anything derived from it would not be either (I3).

## Consequences

- **Moving or reorganising asset files is free**, which is the point. So is renaming them.
- **Renaming an *id* is a breaking change**, and a visible one: it appears in the diff of the sidecar
  and of every scene file that referenced it. Same asymmetry ADR 0017 chose for components, for the
  same reason — a rename is a decision someone made, a move is a refactor nobody expects to have
  consequences.
- **Every asset needs a sidecar before it can be referenced.** Generated on import, so the common
  path is zero-friction, but an asset dropped into the tree with no import step is invisible. That
  is a real papercut and the error must say so plainly rather than "asset not found".
- **`amadeo check` gains a job**: verifying that every `from` in a scene resolves to a known id, and
  listing near-misses when it does not. It already validates component names against the registry;
  this is the same move for assets.
- **A rename-tracking tool is no longer needed.** The prior's `amadeo mv` existed to repair
  path references after a move; there is nothing to repair.
- Sidecars are text and hand-editable, satisfying I1 for asset metadata as well as for scenes.

## Rejected alternatives

**Stable paths, plus an `amadeo mv` that rewrites references.** The recorded prior, and genuinely
attractive: self-describing, no sidecar needed to reference anything, and trivially verifiable.
Rejected because it makes every file move a repository-wide edit that only holds if everyone routes
through the tool — and nothing routes an editor's drag-and-drop, a `git mv`, or a file manager
through it. The failure is silent and arrives later. This is the same judgement ADR 0017 made about
component paths, and it would be strange to reach the opposite conclusion one layer up.

**Opaque GUIDs.** Survives everything, needs no tooling, and is what Unity does. Rejected because it
is precisely what makes Unity's scene files unreadable to humans and agents alike — `from
f3a9c2e14b...` tells a reader nothing, cannot be written by hand, and makes a diff meaningless. That
is a direct hit on I1, which is invariant number one.

**Paths now, ids later behind a trigger.** The shape ADR 0011 used to reserve WASM, and it works
there because the escape hatch is a *code* change. Here it is not: by the time it hurt, asset
references would be embedded in committed scene files, so migrating would be a rewrite of authored
content rather than of engine code. The cheap moment to choose is now, while the number of
references is zero.
