# ADR 0067 — A list item may have named fields

**Status:** Accepted · **Date:** 2026-08-14 · **Builds on:** ADR 0014, ADR 0027, ADR 0032 ·
**Extends:** the `.scene` format

## Context

ADR 0032 gave the scene format value nesting: an indented block is a **list** if its lines start
with `- ` and **named fields** otherwise, which is YAML's rule and needs no schema. It delivered
nested structs, maps and enum payloads, and it left exactly one shape out — a **list whose items
have named fields**:

```text
tracks
  - component "Transform"
    field "rotation"
```

Nothing wanted one until ADR 0066, and then the first `.anim` file wanted two levels of it. The
writer already knew this was a gap: `write_field` had a branch emitting the `Debug` form of such an
item with a comment saying "deeper nesting than the format expresses today", so that nothing was
silently dropped.

It is not an animation-shaped gap. **Any repeated compound entry has it** — a dialogue tree's lines,
a particle emitter's stages, a state machine's transitions, a loot table's entries.

## Decision

**Fields indented beneath a `- ` line belong to that item.**

```text
tracks
  - component "Transform"
    field "rotation"
    keys
      - time 0.0
        value 0.0 1.0 0.0
      - time 1.5
        value 0.0 2.5 0.0
```

### The field on the dash's own line is a field, not a header

`component "Transform"` is one of the item's fields. That is YAML's rule and it is what makes the
block beneath line up with it: `- ` is two characters, exactly one indent level, so the continuation
sits one level deeper than the dash and visually in the same column as the first field.

A **bare `-`** with the block beneath means the same thing and parses to the same value. It exists
for the item whose alphabetically first field is itself a block, where there is nothing to put on the
line. `amadeo fmt` writes the compact form whenever it can, so the bare spelling is something a
person may write rather than something the tool produces.

### Canonical form is the alphabetically first field

`Value::Struct` is a `BTreeMap`, so "first" is the same on every machine and every run. If the writer
and the parser disagreed about which field goes on the dash line, the bytes would move every time
somebody ran `amadeo fmt`, which is invariant I2 broken — so the round trip is what
`a_list_of_structs_round_trips` asserts rather than the parse.

### A single value fills a one-element list

Not a format change, but found by the same file and fixed in the same place it belongs.

`value 22.0` is one token, and layer 1 of the format has no schema — so it produces a scalar, always.
A `Vec<f32>` field could therefore not be authored with one element in it at all, which is a
one-number animation track: exactly the common case. `Vec<T>::from_value` now accepts a single value
as a one-element list.

That is the **type** resolving an ambiguity the text genuinely has, which is the same job
`f32::from_value` accepting an integer already does. Anything that is not a list and cannot be an
element still fails, with the element type's own message.

Found by `amadeo check`, which reported `list<f32>: expected list, found 64-bit float` against the
real schema. Worth recording as the validator earning its keep: nothing about the symptom — a lamp
that did not flicker — pointed at a list of one.

## Consequences

**Additive. Nothing written before this moves**, and `a_flat_list_still_parses_the_way_it_always_did`
is what says so. A list of scalars and a list of lists both parse and write exactly as before.

**The `Debug`-form escape hatch in the writer is still there**, and now covers less: an item that is
neither inline-able nor a struct or map. Deeper shapes than that remain unwritable, and remain
visible rather than dropped.

**A schema-less format now expresses everything the value tree does except `Value::Unit`.** That
exception is unchanged and still deliberate: `Unit` is `Option::None`, and every spelling for it
either collides with an enum variant or invents punctuation this format does not have.

## Alternatives rejected

**Parallel arrays** — `times 0.0 1.5` beside `values -90.0 0.0 0.0 -64.0 0.0 0.0`. It is what glTF
does and it needs no format change at all. Rejected because it puts fifteen numbers on a line with
their grouping implied by a width nobody can see, which is trap 4 exactly: a format that is whatever
a serializer finds convenient is one humans stop being able to write.

**A map keyed by time** — `keys` as `Map<String, …>` with `"0.0"` as the key. Expressible today, and
wrong: map keys sort as **strings**, so `"10.0"` comes before `"3.0"` and a clip longer than ten
seconds plays its keys out of order. Silent, and it would look like an authoring mistake.

**One entity per item**, using the scene format's own nesting. Works for a handful of tracks and is
absurd for keyframes, and it would make every repeated value in the engine an entity in a file that
is not describing entities.
