# 11 — The Warren: premise, world, and how it drives every system

> Read this before touching anything a player sees in `games/warren`.
> `05-roadmap.md` says *what* the milestone requires. This says *what the game is*, which until
> session 20 nothing did — and that absence is the whole reason the game read as an engine test.

---

## 0. The correction this document exists to make

`00-vision.md` has always said the M3 exit gate is "a small but genuinely **complete** game", and
that "short horror games are a respected format, so small does not mean unfinished". It is not an
engine demo. Everything built so far was built as though it were.

Justin's audit, verbatim in substance:

- the levels are *"too linear like a kid made them, just rooms literally straightly connected"*
- *"no life, colour, creativity and plan, just plain dead rooms laid out next to each other"*
- *"the menu sucks, the layout and everything"*
- *"you love sticking to a locked door and there's a key for it you need, this is a simpleton's idea
  of a game"*

All four are correct, and they share one cause: **there was no fiction, so every decision was made
on engineering grounds.** Rooms are 12×12 because a grid is easy to reason about. The objective is a
key and a door because those are the two smallest things `mod-interaction` and `mod-inventory` could
demonstrate. The menu is three rectangles because three rectangles proved that focus navigation
works. Each of those is a defensible *engineering* answer to a question nobody had asked as a
*design* question.

This document asks them as design questions.

---

## 1. What the research says, and what it indicts

Sources are listed at the end. Five findings shaped everything below, and four of them condemn what
exists today.

**Frictional Games' own nine-year retrospective** — the studio that made Amnesia and SOMA — is the
most useful single source, because several lessons are stated *against their own later work*:

| Their lesson | What the Warren does today |
|---|---|
| **Hubs beat linear.** Amnesia's hub maps "increased anxiety"; SOMA's streamlined maps produced "a considerable drop in scariness" | A single chain of rooms with one route through it |
| **Narrative is core** — "a big part of horror takes place inside a player's head" | No narrative whatsoever |
| **Keep it vague** — "silhouettes frighten more than close-up views" | The HUD says "Take the brass key" and "The door is locked" |
| **Players need a role** — a generic protagonist distances the player | The player is nobody, from nowhere, for no reason |
| **Agency is crucial** — voluntary risk is scarier than forced progression | One objective, one order, no choice |
| **Space the scares apart** | The warden is present and hunting from the first second |

**On light**, Frictional's earlier post is precise and contradicts the obvious reading of "make it
dark": pitch black is *not* effective, because "watching a pitch black image is not all that
exciting". What works is a player-carried source, a little ambient, and **fog that thickens with
distance** so that things emerge from it. Two of those three now exist in the engine; the third is
the mechanic.

**On level design**, the recurring technique is alternation: tight corridors for claustrophobia,
wide rooms for exposure, switching between them to keep the player off balance — and deliberately
breaking sightlines so a straight run is never visible end to end.

**On procedural levels feeling handcrafted**, the technique that matters is Dormans' separation of
**mission** from **space**: generate *what the player must do and in what order* as a graph first,
then realise it as geometry. Spelunky's reputation for hand-made-feeling levels comes from strict
assembly rules over authored chunks, not from randomness. A main chain plus branches, rather than a
single walk.

**On environmental storytelling**, the usable rule is that every prop should carry story, carry
atmosphere, or be there because it looks right — and that a prop implies "a character, an event, a
habit". A crate implies nothing. A made-up bunk implies someone slept in it.

---

## 2. The premise

> **You are a records clerk sent into a decommissioned deep-level shelter to retrieve one box before
> the site is sealed. The lift dies behind you. Something in here is still doing its job.**

### The setting is a real typology, used honestly

Between 1940 and 1942, eight **deep-level shelters** were dug beneath London Underground stations —
Clapham South, Stockwell, Camden Town and others. Each is a pair of parallel tunnels over a
thousand feet long, more than a hundred feet down, reached by a spiral staircase and a small lift.
Clapham South held 8,000 bunks.

Three facts from that history are worth more than any invented setting:

1. **The sub-shelters were named alphabetically** so that people could remember where they slept —
   at Clapham South: Anson, Beatty, Collingwood, Drake, Evans, Freemantle, Grenville, Hardy,
   Inglefield, Jellicoe, Keppel, Ley, Madden, Nelson, Oldham, Parry. Naval names, in order.
   **This is a wayfinding system that already exists**, and it solves the problem that a player
   cannot orient themselves in a generated maze — without a map, a minimap or a compass.
2. **After the war they became secure archives.** The bunk frames were *converted into racking* by
   raising the top bunk. One piece of furniture, two eras of use, visible at a glance. That is
   environmental storytelling that costs one mesh.
3. **They are pairs of long tubes joined by cross-passages.** That is not a grid. It is inherently a
   loop with branches — the shape the research says horror wants — and it is architecturally true
   rather than invented.

The Warren is **fictional and explicitly so**: "Shelter Four", under a London that is never named,
sealed in 1987. It borrows the typology, not the place. Nothing in it claims to depict a real site
or real people.

### Why this setting and not another

It earns its keep on four counts, which is the test any setting should pass:

- **It explains the darkness.** A decommissioned site on standby power is dark for a reason. The
  player is not in an arbitrarily unlit building.
- **It explains the light you carry.** A clerk sent down with a hand lamp is not a horror-game
  protagonist who happens to own a torch.
- **It gives the game its typography.** The Signage theme Justin chose from four mock-ups — bone on
  near-black, safety orange, zero rounding — *is institutional wayfinding*. Making the world a place
  with signage makes the interface **diegetic**: the HUD and the walls speak the same language
  because they are the same system. This was an accident and it is the single luckiest thing about
  the project so far.
- **It plays to what the engine can do.** See §7.

---

## 3. The threat

**It is the warden.** Not a monster with a name invented for it — an air-raid warden was a real
role, and the shelters had them: the person who walked the tunnels, kept the numbers, and made sure
everyone was where they were supposed to be.

That is what is still down there, and it is still doing that job. It is counting.

**It is never clearly seen.** This is Frictional's Lesson 7 — silhouettes frighten more than
close-ups — and it is also the honest truth about the engine, which has no skeletal animation
(`06-open-questions.md`, blocked on a rigged model). A design that required a convincing walk cycle
would be a design that shipped badly. A design built on *not showing it* is better horror **and**
better engineering, and those agreeing is the sign the design fits.

So it is known by: a sound that moves, a shadow that crosses a lit doorway, racking that has been
disturbed since you passed, and a tally that has gone up by one.

**It hunts by sound.** Not by sight. This is the decision everything else hangs from — see §4.

---

## 4. The loop, and why it is not a key and a door

The current loop is: find torch → find key → open door. Three fetches in a line, no decisions, and
the only interaction between them is ordering.

The replacement is built from **one mechanic with a real cost**:

> **Your lamp is safe. Your footsteps are not. It hunts by sound.**

That single rule turns *every second of movement* into a decision — walk or run, cross the open
tunnel or go the long way, wait or move — which is what the current game has none of. It also uses
the two systems the engine is genuinely good at (spatial audio, dynamic light) as the *core*, rather
than as decoration.

### The three beats

**1. Descent.** The lift takes you down and dies. You have a hand lamp and a docket with a box
number on it. You learn the alphabetical sections from the signs, because you have to find one.

**2. The Warren.** The lift needs power. The standby set needs **three isolators** thrown, in three
different sections — and *in any order you like*. That is Lesson 9: agency, and the choice of where
to go first is a real one because the sections differ in what they cost you.

**Throwing an isolator brings that section's emergency lighting up, permanently.** This is the
decision that makes the game a game:

> **Light is orientation, and darkness is concealment. You cannot have both, and you choose which
> to spend.**

A lit section is navigable and memorable — you can find your way back through it. It is also a
section in which you can no longer stand still in the dark and let something walk past. You are
spending safety to buy a map.

**3. The run.** Starting the generator is the loudest thing that has happened down here in forty
years. The way back to the lift is the level you have already lit — which is exactly why you lit it,
and exactly why it can see you coming.

### What this fixes, point by point

| The complaint | What answers it |
|---|---|
| "a locked door and a key" | Three objectives, any order, each with a cost |
| "no plan for how interactions affect the rest" | Every isolator permanently changes the level's lighting *and* its danger |
| levels too linear | A hub with three spurs, entered in the player's chosen order |
| no life or creativity | A place with a history, a job the player is doing, and a threat with a reason |

### The box

The docket in your pocket has a box number on it. Finding it is optional and it is the whole story:
it is a shelter register, and the last page is a list of names with a mark beside each. Yours is on
it. Nothing says so out loud.

That is Lesson 7 and Lesson 5 in one prop, and it costs a mesh, a piece of text and an interaction
the engine already has.

---

## 5. Level design rules

These replace the grid walk. The generator's job changes from "place rooms" to "realise a mission in
architecture".

### 5.1 Mission first, space second

Generate the mission graph — descend, three isolators reachable independently, generator, return —
then realise it. Today's `lay_out` does the opposite and it is why the level is a chain.

### 5.2 The architecture is two tubes and cross-passages, not a grid

- **Tunnels**: long (60–120 m), 5 m wide, curved ceiling, running parallel.
- **Cross-passages**: short, low, blind — you cannot see what is in the other tunnel until you are
  in the passage. This is the sightline break the research asks for, and it comes free with the
  architecture.
- **Chambers**: the plant room, the lift landing, the medical bay. Different height, different
  material, different sound.
- **The shaft**: vertical. The engine has never drawn a vertical space and a shelter is defined by
  being deep.

**No two spaces the same size.** The single strongest tell that a level is generated is uniform
dimensions, and today every room is exactly 12×12×3.

### 5.3 Sightlines are the unit of pacing

Alternate tight and open, and never let a straight run be visible end to end. In a tunnel that means
racking, collapses and bulkheads breaking the length — which are props that also carry story.

### 5.4 Wayfinding without a map

Every section carries its name on the wall, in the game's own typeface, at eye height at every
junction: **NELSON**, **KEPPEL**, **DRAKE**. Alphabetical order tells you which way you are going.
The player builds a mental map out of the fiction rather than out of a UI element.

---

## 6. Lighting, which is the medium

Justin: *"light plays a huge role... the player shouldn't just be able to see everything, just a
small light source kind of like how you'd see in real life."* This is also Frictional's position,
with one important correction: **not pitch black.**

The layers, and what each is for:

1. **The hand lamp** — narrow, warm, casts real shadows (the engine does this now). It is what you
   see with. Everything else is context.
2. **A very low ambient** — the environment map. Enough that a wall is not a void. `gloom.rs`
   already does this and its level is now a design number, not a fudge.
3. **Fog that thickens with distance** — landed in ADR 0073. This is the thing Frictional names
   explicitly, and it is what lets something *emerge* rather than appear.
4. **Emergency lighting, off until you switch it on** — the mechanic from §4. Sparse, cold, and
   pooled: light *from fittings that exist*, with dark between them. Never uniform.
5. **One practical per chamber that means something** — a lit inspection lamp on the generator, a
   bulkhead light over a door.

**The failure mode to avoid is uniform gloom**, which is what the game has now: everything equally
and mildly visible, no pools, no contrast, nothing to walk towards. Contrast is the point. A corridor
with one working fitting halfway down is frightening; a corridor at 15% brightness everywhere is
grey.

---

## 7. What the engine can and cannot do, and designing to it

This is not a limitation section. It is why this design and not another.

**Strong:** dynamic light and real-time shadows, fog, spatial audio, a first-person body, physics,
interaction, a level defined in text, PBR materials with normal maps.

**Absent:** skeletal animation, particles/VFX, decals, any hand-made art, and no artist.

A design leaning on a visible animated creature, on debris and gore, or on detailed props would fail
on all four absences at once. **This design leans entirely on architecture, light, sound and
implication** — every one of which is a strength, and three of which are exactly what the genre says
matter most. That the constraints and the genre agree is the strongest argument that the premise is
right.

Particles are the one gap worth closing for atmosphere: dust in a lamp beam is most of what makes a
volumetric look real. It is already on the M3 build list and unbuilt.

---

## 8. Interface

The rule: **the interface is signage.** Same typeface, same palette, same right angles as the walls,
because in the fiction they are made by the same institution.

- **Title screen**: not a panel floating over the level. A shelter **sign** — the name, a number, a
  line of small institutional type. The camera is somewhere deliberate and static, and the world
  behind it is doing something (a light flickering, the lift cage). What is there now is three
  rectangles centred over whatever the player happens to be looking at, and the camera turns while
  you read it.
- **In-world prompts**: as few words as possible, and never explanatory. Not "Take the brass key" —
  the object's own name, if anything at all.
- **No health bar, no objective marker, no compass.** The docket is the objective; the signs are the
  compass; being caught is the health bar.
- **The pause menu is a clipboard**, not a dialog box.

---

## 9. Audio

Already built: spatial sources, one-shots, buses, a room tone. What the design needs from it:

- **Sound is the threat's channel**, so it has to carry information reliably: distance, direction,
  and *what it is doing* — walking, stopping, counting.
- **Your own noise is the resource you spend.** Walk, run, and stand still must sound different, and
  the player must be able to tell how loud they are being.
- **Occlusion** is the one missing engine feature that matters here (`05-roadmap.md` item 6 records
  it as open): a warden exactly as loud through a wall as through a doorway makes the whole mechanic
  a lie. This is now a *gameplay* requirement rather than a polish item.
- **Near-silence is the default.** The stings that exist are placeholders; the room tone should be
  almost nothing, so that a single sound is an event.

---

## 10. Plan

Ordered so that each step produces something judgeable, and nothing is left half-built.

| # | Work | Why here |
|---|---|---|
| 1 | **Architecture pieces**: tunnel segment, cross-passage, chamber, bulkhead, racking, the shaft | Nothing else can be judged until the level stops being boxes |
| 2 | **Mission-then-space generator** — hub with three spurs, no two spaces alike | The linearity complaint, at its root |
| 3 | **Section names and signage** on walls | Wayfinding, and it makes the world legible |
| 4 | **Lighting pass**: fittings, pools, contrast, the switchable emergency circuits | The medium |
| 5 | **The loop**: isolators, the generator, the lit-versus-hidden trade | Replaces key-and-door |
| 6 | **The warden hunts by sound**; audio occlusion | Makes the mechanic honest |
| 7 | **Title screen and prompts** as signage | The menu complaint |
| 8 | **The docket and the register** — the story, told in two props | Lesson 5, cheaply |

**Every one of these goes to the critic agent before it is called done** (`.claude/agents/critic.md`),
and none is left until the verdict is POLISHED. That is Justin's instruction from session 20 and it
is now project process, not a one-off.

---

## Sources

- [9 Years, 9 Lessons on Horror — Frictional Games](https://frictionalgames.com/2019-10-9-years-9-lessons-on-horror/)
- [The struggle between Light and Dark — Frictional Games](https://frictionalgames.com/2009-11-the-struggle-between-light-and-dark/)
- [Creating Horror through Level Design — Game Developer](https://www.gamedeveloper.com/design/creating-horror-through-level-design-tension-jump-scares-and-chase-sequences)
- [The Art of Fear: Secrets of Horror Game Level Design](https://www.algoryte.com/blogs/the-art-of-fear-secrets-of-horror-game-level-design/)
- [Clapham South: subterranean shelter — London Transport Museum](https://www.ltmuseum.co.uk/whats-on/hidden-london/clapham-south)
- [New discoveries at Clapham South's deep level shelter — London Transport Museum](https://www.ltmuseum.co.uk/blog/new-discoveries-clapham-souths-deep-level-shelter)
- [Clapham South Deep Shelter — Subterranea Britannica](https://www.subbrit.org.uk/sites/clapham-south-deep-shelter/)
- [A Hybrid Approach to Procedural Generation of Roguelike Video Game Levels — ACM](https://dl.acm.org/doi/fullHtml/10.1145/3402942.3402945)
- [Environmental Storytelling in Video Games — Game Design Skills](https://gamedesignskills.com/game-design/environmental-storytelling/)
