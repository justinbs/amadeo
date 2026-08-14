# ADR 0069 — A save is a snapshot read leniently, and renames are authored data

**Status:** Accepted · **Date:** 2026-08-15 · **Builds on:** ADR 0017, ADR 0028, ADR 0029, ADR 0032 ·
**Resolves:** Q37

## Context

`games/atrium` saves by writing a `.snapshot`, and that works — a resumed game and one that never
stopped are proven to be the same game. But a snapshot is deliberately a **short-lived artefact**:
`amadeo-snapshot`'s own docs say it captures one moment of one run, that there is no migration path,
and that a mismatch is refused rather than guessed at. That is exactly right for "get back to the
moment I was debugging".

A **save file is the opposite kind of thing.** It belongs to a player and has to survive the game
being updated. Today it does not, and the failure is as small as it gets: adding one field to any
component invalidates every existing save.

### What was measured, because the obvious fix does not work

Q37 recorded the expected answer as "restore leniently: take the fields the file has, default the
rest", and claimed that would make a save survive an added field with no migration code at all.
**It does not.** `crates/amadeo-snapshot/tests/a_patch_invalidates_every_save.rs` runs two builds of
one component in one process, sharing a canonical name via `#[reflect(name = "…")]` — which is what
ADR 0017 makes identity mean, so as far as every file and every state hash is concerned these are
one component before and after a patch:

```
strict:   BadComponent { reason: "missing field `b`; required fields are a, b" }
lenient:  HashMismatch { expected: 6783642539998936112, actual: 13968525498961532720 }
```

Leniency gets past the first wall and into a second one. A defaulted field is still a field, it is
still hashed, so the rebuilt world cannot hash to the number the file recorded — and the world is
rebuilt **correctly** and then rejected, because the recorded hash describes a component layout that
no longer exists.

So the snapshot's integrity check and a save's survival of a patch are **structurally exclusive**,
not two strictnesses of one idea. That check is not decoration — it is what turns "the restore
silently produced a slightly different world" into an error at the moment it happens, rather than
into a run that poisons every assertion after it. A decision here has to say where it goes.

## Decision

### 1. One format, two entry points

`restore` stays exactly as it is: every field required, every name known, the hash check enforced.
That is the snapshot contract and nothing about it changes.

`restore_save` is the lenient reading of the **same file**, by the same parser and the same writer,
so `amadeo fmt`, `amadeo check` and every CLI command work on a save unchanged and a save stays a
text file a person can diff.

Two *formats* were rejected: it would duplicate the 1,600 lines in `amadeo-snapshot` so that every
future format fix has to land twice, and no benefit was identified that two entry points do not
already give.

### 2. The hash check becomes conditional on the layout, not dropped

The file records a **layout fingerprint**: a hash over every component and resource name that
appears in it, paired with its fields in declaration order and their type names.

- **Fingerprint matches** → the layouts are identical, the recorded state hash still means what it
  meant, and `restore_save` enforces it exactly like `restore` does.
- **Fingerprint differs** → the recorded hash describes a layout that no longer exists, so it is
  reported rather than enforced.

This is the point of the whole design. **The common case — a player who has not updated — keeps the
full integrity check**, and leniency costs something only in the case that actually needs it.

**The fingerprint covers names and types and deliberately nothing else.** Not docs, not ranges, not
units, not `version`. The question it answers is "could the state hash mean something different",
and that is decided by exactly the set of names and field types. Folding a doc comment into it would
force a lenient load on every documentation edit — harmless, but it would make the exact path fire
so rarely that nobody would notice when it stopped firing at all.

### 3. A missing field is filled from the FIELD's type, not the component's

`amadeo_reflect::default_value` builds a default from a `TypeInfo`: zero for a number, `false` for a
bool, empty for a string, a list or a map, `Value::Unit` for an absent `Option`, and a struct built
by recursing into its fields.

**Filling from the field's type rather than the component's is what makes this cost nothing to
adopt.** A whole-component default would need `Default` on every component — a bound on the
`Component` trait, or an opt-in that is silent when forgotten, which is the failure mode this
project spends most of its design effort avoiding. A field type's default is available for every
scalar in the engine without anybody writing anything.

**An enum has no default and the engine refuses to invent one.** There is no principled answer —
the first variant is a guess with gameplay meaning, and `ShadowMode`, `Bus` and `Screen` all show
why picking one silently would be worse than saying so. A component that gains an enum field is
reported as unrestorable for that field, by name, and the author decides.

### 4. Renames are authored data, in a text file

A rename is the most common breaking change there is, and discovering after release that the
mechanism does not exist means a dead save file — which is unrecoverable for the player and is the
harm Q37 exists to prevent. So:

```
amadeo-redirects 1
component OldName NewName
field ComponentName.old_field new_field
```

This is **Unreal's `CoreRedirects`**, which is an ini file of `OldName -> NewName` rather than
migration code. Data rather than registered functions is the same grain as ADR 0068's facts and
ADR 0066's tracks, and it means a rename is fixed by editing a file a person can read — including
by Justin, in a session with no agent in it.

**Component redirects apply first, and field redirects are keyed by the NEW component name.** When
both a type and one of its fields are renamed in the same patch, the order is the thing that decides
whether the second redirect fires, and leaving it to be discovered would make a redirect that looks
correct and does nothing.

### 5. Everything defaulted, dropped or redirected is reported

`restore_save` returns a `SaveReport`. A defaulted field is a **silent gameplay change** — a save
that loads with a new `battery: 0.0` reads as a bug in the game rather than as a consequence of the
save predating the field — so it is named, per entity, per component, per field.

This is `asset_problems`, `SoundCache::failures` and `Animatable::missing` a fourth time, and the
rule those three established: when the engine survives something rather than refusing it, the report
*is* the diagnosis, and a report nothing can read is not one.

### 6. Per-component versions are written now and read by nothing

The file carries a `schema` block naming each component and resource it contains with its
`TypeInfo::version`, which every type already has and nothing has ever read.

Full migrations — an old version's value tree in, a new one's out — are **not built here**. They are
the only thing that survives a field changing *meaning* rather than name, and nothing in the project
needs one yet. Recording the number now is what keeps that additive: without it, a save written
today could never be migrated, because nothing would know what it was written against. That is the
argument `TypeInfo::version`'s own doc comment has been making since it was written.

## Consequences

- **`FORMAT_VERSION` goes to 2**, and snapshots written by an older build are refused. That is what
  the snapshot contract explicitly permits, and there are no committed `.snapshot` files.
- **A save can still be broken by a change of meaning** — a field that changes units, or splits in
  two. Nothing here catches that, and the fingerprint will not notice it either, because the layout
  is unchanged. This is what §6 leaves the door open for and it should be treated as the reason to
  bump a `version` rather than as a gap to be surprised by.
- **A newly-required component is still the game's problem.** If a patch adds a component that
  systems expect on every player, a restored save does not have it and no scheme here invents one —
  the entity genuinely did not have it. That is game logic, and a game that needs it should look at
  what it loaded and fill the gap.
- **The exact path must keep being exercised.** A conditional check that silently stops applying is
  worse than no check, so a test asserts that a save round-tripped through the *same* build still
  enforces the hash, not merely that a cross-build one does not.

## Rejected alternatives

**A save is not a world dump — each game writes its own versioned save struct.** What most shipped
games actually do, and the honest alternative. Rejected because it costs nothing in the engine and
everything in every game, re-solved each time, and because it gives up a save being introspectable
text that `amadeo fmt` and the CLI already work on. It stays available: a game that wants it can
write whatever it likes, and this decision does not stand in the way.

**Full versioned migrations now (option C).** Rejected as machinery ahead of a need — no field has
changed meaning yet — and §6 is what keeps it a later addition rather than a rewrite.

**Making `from_value` itself lenient.** Rejected because it would make *scene* files silently
tolerate missing fields, which is Q32's defect shape rather than its fix. Leniency is a property of
the restore path, not of the type.
