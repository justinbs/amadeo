# ADR 0087 — Scattering is its own `Environment` field, not the fog's density

**Status:** accepted (session 26) — chosen by Justin, **approved with changes by engine gate review 29**; the changes are the amendment at the end
**Extends:** ADR 0073's fog. Supersedes nothing.
**Closes:** Q45.

## Context

`docs/05` M3 exit gate 5 names volumetric light shafts as the biggest remaining visual step, and
ADR 0073 records that they raymarch through exactly the fog it added. It is row **F4** of `docs/13`
§1b — and the row already named as the stretch, the one allowed to slip.

**The shipped medium cannot do the job, and review 28 derived it rather than asserting it.**
`games/warren/assets/looks/warren.environment` carries `density 0.055, start 1.5`, exponential-squared:

| distance | fog factor |
|---|---|
| 3 m | **0.68%** |
| 5 m | **3.6%** |
| 10 m | 21% |

A torch cone lives in the first 4–6 m, so **the air a torch beam crosses is essentially clear.** And
`fog.colour` is `0.02 0.028 0.024` — near black. **The same medium cannot both absorb to black and
glow.** A beam driven off these numbers is either invisible or requires raising the density until the
whole game hazes over.

## Decision

**`Environment` gains a `scattering` block with its own strength and colour**, independent of `fog`.

- `strength` — how much light the air returns towards the camera. **Defaults to `0.0`**, so a beam is
  off unless asked for, and every existing `.environment` file keeps loading and drawing identically.
- `colour` — what the air returns. Multiplied by the light's own colour, so a warm hand lamp and the
  warden's blue-white lamp each scatter in their own hue without either being authored twice.

ADR 0075/0076 make the field free to add: a file that omits it gets the declared default, and
`describe --example` publishes it.

## Why not reuse `fog.density`

It is the tidier answer on paper — one number meaning one thing, no new field, no risk of two knobs
for one quantity. It fails on physics and on authoring at once:

- **Absorption and scattering are different coefficients of the same medium**, and this engine's fog
  models only the first. Real participating media carry both; conflating them is what forces the
  choice between "invisible beam" and "hazed game".
- **`fog.colour` is what the air *absorbs to*.** A beam needs what the air *returns*, which is the
  light's colour, not the fog's. One field cannot answer both without meaning two things.
- **The Warren's fog is authored where it is deliberately.** Raising it to make a beam read changes
  the look of every frame in the game, which trades the row against the thing the row is for.

## The risk, stated rather than discovered later

**Two knobs describing the same air is the shape `docs/14` §4 #2 warns about** — the failure where one
knob is moved to compensate for a symptom belonging to another, which this repository has committed at
least twice (the ambient raised to 8.0 to compensate for the colour grade's clipping, and left there
after ADR 0084 fixed the real cause).

Three things bound it:

1. **`scattering` and `fog` are both in one asset**, side by side, so a person tuning one sees the
   other. They are not a shader constant and a scene field a hundred lines apart.
2. **`strength 0.0` is exactly off**, with an early return, so the default costs nothing and cannot be
   compensated for by accident.
3. **F4's close condition already forbids the compensation**: the delta must fall monotonically along
   the beam axis and move a named crop containing no cone by **≤ 3 levels**. A beam faked by raising
   the ambient or the fog lifts the whole frame and fails both clauses.

## Consequences

- **An `Environment` that does not ask for it produces a byte-identical capture**, which is F4's own
  clause (d) and is what lets this land without touching `games/atrium`, `games/vault` or
  `games/scarp`.
- **It raymarches; it is not a post-process.** ADR 0073's reasoning applies unchanged — how much air a
  fragment is behind depends on how far away it is, and the shader already knows its own world
  position and the camera's.
- **Every spot scatters, not only the torch.** Review 28 pointed out that a correct implementation
  gives the ceiling fittings cones too, and called that the best image this effect can give a tunnel.
  F4's original *"< 3 levels outside the cone"* clause would have failed a correct implementation;
  the rewritten clause names a crop containing **no light's** cone.
- **Cost is a per-fragment loop**, and it is the reason this row is the one allowed to slip. It is
  measured against `docs/10`'s budget before F4 closes, not after.

---

## Amendment — review 29's two required changes (session 26)

Approved with changes, and the review's own withdrawn objection is the reason for the first one. It
began by requiring `scattering.strength` be bounded as an albedo multiplying the fog's extinction —
mechanically preventing two knobs from describing different air, which is a stronger bound than
"they are side by side in one asset". It then checked that against the shipped numbers, found the
Warren's fog returns **0.68% at 3 m** so a beam bounded by it is **invisible by construction**, and
withdrew.

### 1. `fog` and `scattering` do not describe the same medium, and that is deliberate

Say it plainly rather than defending against the two-knobs charge, because the charge is *correct* on
physical grounds and the answer is that these are not physics.

**`fog` is a depth cue. `scattering` is a beam.** Both are authored for a look, and the numbers that
make each read are not the numbers a single medium would have. Review 27 recorded the identical hazard
about `source_radius 1.85`: *"anybody who later corrects it to a physical value puts 11,142 pixels
back at paper white and will read the result as a regression in the lamp."*

**The next person to unify these two numbers on physical grounds deletes the beam and will read it as
a shader bug.** This paragraph exists so that person finds this first.

### 2. `colour` defaults to white

Unstated, a file omitting it gets zeros and scatters **black** — which is Q32's defect shape a sixth
time, authorable and authored and silently ignored, in the very field this ADR exists to add. The
default is `1.0 1.0 1.0`, so scattering takes the light's own colour and a warm hand lamp and a cold
lantern each glow in their own hue without either being authored twice.

### What review 29 recorded about deferring this row

F4 is the row `docs/13` §1b names as the one allowed to slip, and review 29 explicitly declined to
reorder it — *"the budget is real and a half-built volumetric pass is worse than none."* But it
recorded the cost: **`docs/11` §4's headline mechanic is that you track the warden by its lamp crossing
a doorway two rooms away, and a carried lamp with no visible beam cannot be tracked.** F2's remaining
defects are partly this row's absence, since an object seen in a beam is *never clearly seen* — which
is what §3 requires and what a model alone cannot deliver.
