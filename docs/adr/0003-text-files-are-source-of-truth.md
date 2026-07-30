# ADR 0003 — Text files are the only source of truth; the editor is a client

**Status:** Accepted · **Date:** 2026-07-30

> This is the ADR that makes human/AI collaboration possible. If only one decision in this repository
> survives, it should be this one.

## Context

Justin asked whether Amadeo could have a real graphical editor *and* full code/text/headless authoring,
such that either author can work on a shared project without locking the other out.

The answer is yes — but only if designed in from the start. It cannot be retrofitted, because the
failure is structural, not featural.

**Why existing engines fail at this.** Three specific, diagnosable failures:

1. **The editor owns the truth.** Scene files are serialization output, not a designed format. Unity's
   files are GUID-laden and effectively unreadable. Godot's `.tscn` is far better but still reorders
   and rewrites on unrelated edits.
2. **Editor-only capabilities.** Some things can only be done by clicking. Any such capability is a
   permanent hole in the agent's reach.
3. **No introspection of a running game.** State is only observable through a human-oriented debugger,
   so an agent can never verify that its change did what it intended.

Each of these has a corresponding decision below.

## Decision

**Text files are the only source of truth. The editor is one client among several, with no privileged
access.**

Concretely, four commitments:

### 1. All authored data is hand-writable text
Scenes, prefabs, asset import settings, input action maps, and project config. Designed formats with
specs, not serializer output. A human or an agent can create any of them in a text editor with no tool
involvement.

### 2. Serialization is canonical and byte-stable
Saving an unchanged file produces a byte-identical file. Sorted keys, stable IDs, fixed number
formatting, deterministic collection order, one property per line where practical.

`amadeo fmt` is the single formatting authority. The editor calls it. The CLI calls it. Same bytes out.

Enforced by a CI round-trip test: parse → serialize → assert byte-identical.

### 3. Stable, content-independent identity
Authoring IDs are assigned once and never change on reorder, reparent, or unrelated edits. Note this
means **two ID spaces** — stable authoring identity in files, generational handles at runtime, with a
mapping between them. Both must be designed together.

### 4. The editor is an RPC client
`amadeo-editor` sits *above* `amadeo-agent` in the crate graph and communicates only through its
protocol — the same protocol the CLI and the agent use. Structurally enforced by the dependency DAG
(I6), not by discipline.

Therefore: **anything the editor can do, the CLI can do** (I5). If the editor needs a capability the
protocol lacks, that is a bug in the protocol, filed and fixed. M4's exit gate includes an explicit
protocol completeness audit for exactly this reason.

## How the two authors actually share a project

```
Justin drags a node in the editor
   → editor issues an RPC mutation
   → scene graph changes
   → canonical write to the .ama file
   → minimal, reviewable git diff

Claude edits the .ama file directly (or issues the same RPC)
   → same parse → instantiate path
   → same canonical write
   → same shape of diff
```

One code path. Neither author has a special route. Diffs stay small enough to review and merge, which
is what makes concurrent work on one project realistic rather than theoretical.

## Consequences

**Costs, accepted:**
- Format design is real work, and canonical serialization is more effort than "derive Serialize."
- Some editor conveniences are unavailable if they'd require hidden state. Undo/redo in particular must
  be modeled as protocol operations rather than an in-memory editor stack.
- The RPC protocol becomes load-bearing and must be versioned and specified.
- A separate-process editor (likely, per Q6) costs latency and complexity.

**Gains:**
- Both authors are permanently first-class. This property is preserved by structure, not vigilance.
- Scenes are code-reviewable. Game content enters normal software practice: diffs, review, blame, CI.
- The protocol gets exercised constantly by the editor, so it stays complete and correct — the editor
  becomes the protocol's best test.
- Merge conflicts in scene files become tractable.

**Testable, in CI:**
1. Round-trip byte-stability on every scene file.
2. Reorder/reparent operations produce diffs proportional to the change, not to the file size.
3. Protocol completeness: no editor capability absent from the CLI (audited at M4).

## Rejected alternatives

**Binary scene format with a text export.** Faster to load, and the export path sounds sufficient.
Rejected because export is lossy in practice and the binary inevitably becomes authoritative; the text
form decays into a debugging aid.

**Editor as the primary authoring tool with text as a fallback.** The conventional approach. Rejected
because it makes the agent structurally second-class, which defeats the project's purpose.

**Text-only, no editor.** Cheapest and fully preserves agent parity. Rejected because Justin wants an
editor, and it's the right call — hand-placing 200 tiles in a text file is miserable, and a GUI is
genuinely better for spatial and visual work. The point is parity, not asceticism.
