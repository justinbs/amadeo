# 09 — M1 exit gate 4, and what it found

> `amadeo describe` output is sufficient to write a new component and system without reading engine
> source. **Tested by actually doing it.**
>
> — `docs/05-roadmap.md`, M1 exit gate 4

**Result: the claim is false as written, and usefully so.** `describe` is excellent at what it
actually does and does not do the thing the gate asked for. This records the experiment, the
finding, and what would close the gap — the last of which is a decision rather than a task.

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

## What would close it

Three options, in increasing order of ambition. **Not decided** — this is a design question about
what the protocol is *for*, and it belongs to whoever picks up M2.

1. **Leave the gate failed, and say so.** `describe` is a schema; `docs/07-working-with-the-code.md`
   is the manual. Cheapest, and it accepts that an agent reads repo documentation — which sits
   awkwardly with `docs/03-ai-native-design.md`'s premise that the engine should be usable through
   the protocol.
2. **Add an `authoring` block to `describe`.** A static section stating the recipe once: the derives
   a component needs, the `Component` bound, the registration call, a system's signature, and the
   query forms. The engine knows all of it about itself. Small, and it makes the gate true — at the
   cost of putting documentation inside a protocol reply, where it will drift unless it is generated.
3. **Extend the protocol to describe resources and the query surface as first-class schema**, so
   `describe` covers everything the reflection registry knows, and add `describe --example <Type>`
   emitting a compilable skeleton. Most complete, most work, and the only option that would survive
   the stronger test described above.

## What was built along the way

`Trap` and `spring_traps` are not throwaway. The traps sit at `(-3, 0)` and `(3, 0)`, in the
corridors between each pair of pillars — which is the apparent express lane between the two middle
sigils, and is now the dangerous one. The arena gained a real decision, and the exercise gained
something worth keeping.
