# 09 — M1 exit gate 4, and what it found

> `amadeo describe` output is sufficient to write a new component and system without reading engine
> source. **Tested by actually doing it.**
>
> — `docs/05-roadmap.md`, M1 exit gate 4

**Result: the claim is false as written, and usefully so.** `describe` is excellent at what it
actually does and does not do the thing the gate asked for. This records the experiment, the
finding, and how it was closed.

> **Resolved by ADR 0030**, later the same session. Three of the five gaps below turned out to be
> holes in the schema and are fixed; two are API knowledge and deliberately live in
> `docs/07-working-with-the-code.md`, which `describe` now points at. See *What closed it* at the
> bottom. **The five gaps below are left as written** — they are the finding, and rewriting them
> would destroy the record of what the experiment actually produced.

## The experiment

The component is `Trap` and the system is `spring_traps`, both in `games/vault/src/game.rs`: a floor
plate that ends the run when the player stands on it. Real code, shipped in the game, with three
tests.

The available surface was `amadeo describe --package vault` (210 lines of JSON), `amadeo schedule`,
and `docs/protocol/v1.md`. The question was which parts of writing that component and system could
be answered from those and which could not.

## What `describe` supplied, and it is a lot

For every component in the game:

```json
"Warden": {
  "docs": "A patrolling warden. Touching one ends the run.",
  "fields": [
    {
      "docs": "World units per second it travels along its route.",
      "name": "speed",
      "range": { "max": 20.0, "min": 0.0 },
      "type": "f32",
      "unit": "world units/s"
    }
  ],
  "kind": "struct", "name": "Warden", "version": 1
}
```

That is genuinely enough to:

- **know what data exists and what it means** — every field carries its own doc comment, so the
  intent survives without the source;
- **get the units right**, which is the class of plausible-but-wrong error Pillar 2 exists to
  eliminate. `speed` is world units per *second*, not per tick, and nothing else would have said so;
- **stay inside declared ranges**;
- **write a type spelling that will parse** — `f32`, `bool`, `array<f32, 2>`, `list<array<f32, 2>>`
  are all shown by example;
- **author a scene file**, completely. This is the thing `describe` is unambiguously sufficient for,
  and `scenes/vault.scene` was written that way.

## What it did not supply

Five things were needed and absent. None is a small gap.

**1. How to declare a component at all.** Nothing in `describe` mentions
`#[derive(Debug, Clone, Copy, PartialEq, StableHash, Reflect)]` or `impl Component for Trap {}`.
`describe` reports that `Warden` *is* a component; it never says what makes something one. Those two
lines are unguessable from the output.

**2. How to register it.** `app.register_component::<Trap>()?` has no counterpart in the schema. A
component that is written but not registered is invisible to `describe` and unusable from a scene —
which is precisely the failure ADR 0016 found in `quad-demo` — and nothing in the output hints that
the step exists.

**3. A system's signature.** `amadeo schedule` lists systems by *name* and in resolved order. It
does not say that a system is `fn(&mut World)`, or how one is added to a stage, or that ordering is
expressed with `.after(...)`. The stage names come from `schedule`; everything else does not.

**4. How to read the world.** `world.query::<(&Transform, &Trap)>()` cannot be derived from anything
`describe` emits. The schema describes the shape of data at rest and says nothing about how to reach
it.

**5. Resources are entirely absent.** This is the sharpest gap. `describe` has a `components` key
and no equivalent for resources, so `Run` — which holds the Vault's score and its win/lose phase —
**does not appear anywhere in the output**. `spring_traps` exists to set `Run.phase`, and an agent
working from `describe` alone could not know that `Run` exists, let alone what is in it.

`world.resources` (ADR 0027) reports resource *values* from a live world, so the information is
reachable — but the *schema* half is missing, and `describe` is where a reader would look.

## What this means

The gate's phrasing conflates two different claims:

- **"`describe` tells you the data model."** True, and well done. It is enough to author content,
  and enough to write code *once you know the API*.
- **"`describe` tells you how to extend the engine."** False. It is a schema, not a manual, and it
  was never designed to be one.

The honest reading is that gate 4 was aiming at something real — an agent should not need the engine
source to work — but named the wrong artefact. `describe` is one of three things such an agent needs,
alongside a recipe for declaring types and a description of the query surface.

**One caveat, stated plainly.** This experiment was run by an agent that had already read the engine
source in the same session. That is a real confound: I could not un-know `impl Component`, so the
five gaps above are ones I noticed *while reaching for knowledge `describe` did not supply* rather
than ones I was stopped by. The gaps are structural — they can be verified by reading the output and
checking whether the fact is present — but a stronger test would give the JSON to a reader with no
prior exposure and see what they produce. That test has not been run.

## What closed it — ADR 0030

Three options were put to Justin, from "leave it and say so" to extending the protocol. **He chose
the most complete one**, consistent with his standing preference for a complete engine over a fast
one. The decision and its reasoning are ADR 0030; the short version:

**Gaps 1–4 are API knowledge and stay in `docs/07-working-with-the-code.md`.** The argument that
settled it is **invariant I5**: anything the editor can do, the CLI and RPC can do — and the editor
will never declare a new Rust component type, because that means editing the game crate and
recompiling. So the protocol is not obliged to carry it, and this gate was asking for something the
project's own invariants do not ask of it. `describe` gained a **`manual` key naming the file**,
because a pointer cannot drift the way copied prose does, and because saying nothing would leave a
reader to conclude that the absence means impossible.

**Gap 5 was a hole rather than a scope decision — and it had company.** Fixing resources properly
turned up two more of the same kind that this write-up had missed:

- The schema was **not closed**. `Run.phase` reported `"type": "Phase"` and nothing could look
  `Phase` up, so nothing could know its legal values were `Playing`, `Won`, `Lost`.
- A fixed array's **length existed only inside its name**, so anything needing the count had to parse
  `"array<f32, 2>"` back apart.

Both are now fixed, along with resources, and `describe.example` emits a minimal valid instance in
the scene spelling and the JSON spelling — generated from one value so they cannot disagree, and
tested by pasting it into a scene file and loading it.

The clearest thing that vindicated the example generator: `Run.phase` has to be written as a **bare
word** (`phase Playing`), never `phase "Playing"`. Bare-versus-quoted is scene-format grammar rather
than type information, so no amount of schema would ever have said so, and getting it wrong produces
a file that parses and then fails to load.

Every gap this document identified is now pinned as a test in the game that found it:
`games/vault/tests/gate_four.rs`.

## What was built along the way

`Trap` and `spring_traps` are not throwaway. The traps sit at `(-3, 0)` and `(3, 0)`, in the
corridors between each pair of pillars — which is the apparent express lane between the two middle
sigils, and is now the dangerous one. The arena gained a real decision, and the exercise gained
something worth keeping.
