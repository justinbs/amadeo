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

### The prior art this document originally failed to name

**Amnesia: The Bunker (2023)** is this premise, already shipped. A First World War bunker; a
semi-open hub rather than a corridor; **a generator you must keep fuelled to keep the lights on**; a
creature that comes out of the walls and **responds to loud sound**; and a **hand-cranked flashlight
whose winding is itself noisy**. The first draft of this document researched *The Dark Descent* and
*SOMA* and missed the one game that had already built the mechanic it was proposing. That is a
research failure and it is recorded here rather than quietly corrected.

It is not fatal — "hunted by sound in a wartime shelter" is a form, not a property, in the way that
"a haunted house" is. But two things follow and both are load-bearing:

**The Bunker chose the opposite polarity, and it was right to.** There, **light makes you safer** —
the Beast avoids lit rooms — and the scarce thing is **fuel, which is to say time**. The first draft
of §4 made light make you *less* safe and attached scarcity to nothing at all. The prior art solved
the same problem and reached the opposite answer, so this document has to say why it deviates. §4 now
does, and the honest summary is that it deviates *less* than it used to: light is still not a
punishment, and there is now a clock.

**What this is that The Bunker is not.** Not "smaller with a clerk instead of a soldier", which was
the only available answer before. The difference is the **warden**: The Bunker's Beast is an animal
in the walls, and this is an institution still performing its function. It is not hunting you. It is
*counting*, and you are not on the list. That difference has to show up in behaviour rather than in
prose — see §3a, where it does: the warden patrols a route, stops at fixed points, and only pursues
when something is where nothing should be.

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

## 3a. The warden, as a specification

The first draft of this document described the warden in adjectives and left every number and every
rule to be invented during implementation, which is how a threat ends up as `distance <= 9 → pursue`.
This section is the contract.

### What it senses

**Sound, and nothing else.** It has no sight model at all — not a cone, not a raycast. This is
absolute, because the moment sight exists the light-versus-dark trade in §4 becomes a second,
competing system and neither reads clearly.

| The player is | Heard at |
|---|---|
| standing still | never |
| walking | 8 m |
| running | 22 m |
| throwing an isolator | the whole section |
| starting the generator | everywhere |

Surfaces modify it: standing water and steel plate carry further than dust and carpet, by roughly
half again. That is one number per floor material and it is what makes the flooded section in §4 a
real cost rather than a description.

### How it moves

- **Patrolling**: 1.5 m/s along a fixed route between fixed stopping points. Slower than a walk, so
  you can follow it and it cannot catch you by accident.
- **Investigating**: it goes to *where the noise was*, not to where you are — the distinction that
  makes moving after making a noise the correct play, and the thing a player has to learn once.
- **Pursuing**: 2.9 m/s, faster than the player's 2.6. **A chase is not survivable by running in a
  straight line**, which is deliberate and is the inverse of what ships today: the current warden is
  slower than the player, in an empty corridor, so a chase resolves to walking away. You survive by
  breaking the sound trail — going still, or getting to a section whose noise floor covers you.

### Its tells, which are the whole of its legibility

**A sound-based threat with no tells reads as random**, and a player who cannot tell "it heard me"
from "it is wandering" learns nothing and blames the game. Each state has one unmistakable audible
signature, and they are distinguishable at a distance:

| State | What you hear |
|---|---|
| Patrolling | an even, unhurried tread, and a pause at each stop |
| Investigating | the tread stops mid-stride — **the silence is the tell** — then resumes towards you |
| Pursuing | the tread breaks into something faster and the breath changes |
| Losing you | it slows, stops, waits far longer than feels comfortable, then resumes patrolling |

The stop-mid-stride is the most important sound in the game. It is the moment the player learns the
rule, and it costs nothing to build: it is a clip change on a state transition, which
`modules/amadeo-behaviour` already drives through `BehaviourChanged`.

### When it catches you

**The run ends and the level does not reset.** The isolators you threw stay thrown, the sections you
lit stay lit, and what you were carrying stays where you were carrying it. You restart at the lift.

This is the single most load-bearing decision in the section and the first draft did not make it. A
horror game that takes progress away on death teaches the player to stop taking risks, which is the
opposite of what §4 wants; a game that takes nothing away has no stake. The stake here is **time and
noise**: you have to walk back through a level whose lights are on and whose warden is somewhere new,
and the lamp does not recharge on death.

---

---

## 4. The loop, and why it is not a key and a door

The current loop is: find torch → find key → open door. Three fetches in a line, no decisions, and
the only interaction between them is ordering.

The replacement is built from **one mechanic with a real cost**:

> **Light is orientation. Darkness is warning. You cannot have both.**

### The contradiction this replaces, because it is instructive

The first draft said *"your lamp is safe, your footsteps are not"* and, four paragraphs later, that a
lit section is *"one in which you can no longer stand still in the dark and let something walk
past"*. **Those cannot both be true.** If the warden cannot see, standing still in a lit room is
exactly as safe as standing still in a dark one — so throwing an isolator costs nothing, and the
trade the document called "the decision that makes the game a game" did not exist. The whole of beat
3 rested on it.

The repair keeps the sense model absolute and moves the cost to the player's *own* senses:

**Emergency fittings hum.** A lit section has a noise floor, and that noise floor **masks the
warden**. In the dark you cannot see it coming and you can hear it perfectly; in the light you can
see where you are going and the first you know of it is when it is in the room.

That is symmetrical, it never contradicts "it hunts by sound", and it costs one looping spatial
source per fitting and a reduction in how far the warden's own sound carries — both of which the
engine does today. It also makes darkness genuinely *desirable* rather than merely tolerable, which
the first draft never achieved.

### What the player is spending

The second thing the first draft got wrong: **nothing was scarce**, so there was a dominant strategy
and it was "hold walk for twenty minutes". A resource with no scarcity is not a resource, and a
stealth system whose safe option is permanently affordable is a slow walk rather than a decision.

So the lamp is a **shelter-issue accumulator lamp with a charge**, and it dims as it drains. There
are charging points, and they are at the isolators.

That single object ties the whole loop together: light, route and time become one budget. It is also
how The Bunker solves the same problem — the scarcity is on the light, not on the darkness — with the
difference that a charge cannot be hoarded or spent early, so there is no inventory management and no
counting of jerrycans.

**Design targets, provisional and to be tuned by play**: a full charge lasts about a third of a
first run, so the route is planned around three visits to a charging point, and a lamp run flat still
gives a faint usable glow rather than a black screen — Frictional's rule that a pitch-black image is
not exciting, applied to the failure case.

### How the three sections differ

The first draft claimed agency and then made the three spurs interchangeable, so the choice of where
to go first was cosmetic. Each has one concrete, mechanical difference:

| Section | What it costs you |
|---|---|
| **The flooded one** (lower cross-passages) | Standing water. **You cannot move quietly at all** — every step carries half again as far. The only safe state is standing still |
| **The half-lit one** (nearest the substation) | Some fittings still have residual power, so it arrives *already* partly humming. You can see, and you were never able to hear |
| **The warden's own** (intact, re-racked as archive) | Undamaged, quiet, well ordered — and it is where the warden spends most of its patrol. The easiest section to move through and the likeliest place to meet it |

There is no correct order. There is an order that suits how much charge you have left, which is the
decision the loop exists to produce.

### The three beats

**1. Descent.** The lift takes you down and dies. You have a lamp and a docket with a letter on it.
You learn the alphabetical sections from the signs, because you have to find one.

**2. The Warren.** The lift needs power. The standby set needs **three isolators** thrown, in three
different sections, *in any order you like*. Throwing one brings that section's emergency lighting
up permanently — and permanently deafens you in it.

**3. The run.** Starting the generator is the loudest thing that has happened down here in forty
years, and it draws the warden to the plant room. The way back is through sections that are lit,
humming, and no longer able to warn you — which is exactly what you did to them.

### Teaching it

The rule is inferred, under threat, with no tutorial and no HUD. That does not work unless the first
section demonstrates it **at no cost**:

- The lift landing is lit and humming when you arrive. You cannot hear anything. That is the baseline.
- The first cross-passage is dark, and the warden passes through it on patrol while you are watching
  from the landing. **You hear it before you see it**, and it does not come towards you.
- The first isolator is in sight of the landing, so the change from "I could hear that" to "I cannot"
  happens in a place where nothing is hunting yet.

The player is never told the rule. They are shown it three times in ninety seconds.

### How loud am I

§8 forbids a UI element for this, so it has to be diegetic and unmistakable:

- **Footfall by surface** — dust, carpet, standing water, steel plate — audibly different, and the
  loud ones sound loud.
- **The lamp rattles at a run** and does not when you walk. This is the primary feedback and it is
  attached to the object the player is already looking at.
- **Breath**, which returns to normal only after several seconds of standing still.

### What this fixes, point by point

| The complaint | What answers it |
|---|---|
| "a locked door and a key" | Three objectives, any order, each with a stated mechanical cost, on a charge clock |
| "no plan for how interactions affect the rest" | Every isolator permanently lights a section **and** permanently deafens you in it |
| levels too linear | A spine with three spurs, entered in the player's chosen order |
| no life or creativity | A place with a history, a job, a palette (§5a) and a threat that is doing something other than hunting |

### The story, and why the register is gone

The first draft's ending was: the box holds a shelter register, your name is on the list, nothing says
so out loud. That is the stock twist of short horror — *you were counted all along* — and it was
**optional**, which in a twenty-minute game means most players would never see the story at all.

It is replaced by the idea this document already had and undersold: **the warden is still doing its
job.** It is not hunting; it is counting, and the count is wrong.

That is carried on three surfaces a player cannot miss rather than one they can:

1. **The tally.** Each section has a board with a chalked number on it — the count of people bedded
   down that night. The numbers are still being kept up to date. They go up.
2. **The state of the sections.** One is still made up, bunks and all, as though the occupants are
   expected back. One has been stripped and re-racked as an archive. One is flooded and abandoned.
   Three eras of the same building, readable at a glance, told with no words.
3. **The docket.** Your own paperwork, which is the only thing in the game that says who you are and
   why you came, and which you are carrying before the game starts.

The box still exists and still holds the register. Finding it is still optional. It is now the
*detail* rather than the whole story — which is what optional content should be.

That is Lesson 7 and Lesson 5 in one prop, and it costs a mesh, a piece of text and an interaction
the engine already has.

---

## 5. Level design rules

These replace the grid walk. The generator's job changes from "place rooms" to "realise a mission in
architecture".

### 5.1 Mission first, space second

Generate the mission graph — descend, three isolators reachable independently, generator, return —
then realise it. Today's `lay_out` does the opposite and it is why the level is a chain.

### 5.2 The architecture is two tubes and cross-passages — which is still a grid, so that is not the rule

- **Tunnels**: long (60–120 m), 5 m wide, arched, running parallel. `ArchMesh` exists for exactly
  this and is the engine's first curved primitive.
- **Cross-passages**: short, low, blind — you cannot see what is in the other tunnel until you are in
  the passage. The sightline break comes free with the architecture.
- **Chambers**: the plant room, the lift landing, the medical bay. Different height, different
  material, different sound.

**A 2×N ladder is the most regular graph there is**, and built naively it would read *more*
machine-made than the fourteen-cell maze it replaces, because every junction would look like every
other junction. Saying "it is not a grid" does not make it one. What actually breaks it, as generator
rules rather than as prose:

- cross-passages at **irregular** intervals, never a fixed stride;
- **one tunnel blocked** by a collapse partway along, so the ladder becomes a one-way for that stretch
  and the loop has a direction;
- chambers punched off the spine at **different sizes**, so the spine is not the whole level;
- bulkhead doors that **close a run** you could otherwise see down.

**"No two spaces the same size" was wrong and is withdrawn.** It contradicts the fiction this document
spends a page establishing: a shelter is sixteen *identical* sub-shelters, and institutional
architecture is repetitive by nature. Spelunky's rooms are all one size and nobody calls its levels
machine-made. What reads as generated is uniform **topology** and undifferentiated **state**.

> **Rooms may repeat. No two may be in the same condition.**

The conditions the generator draws from — and this is a generator rule, not an intention:
**flooded**, **collapsed**, **stripped**, **still made up**, **re-racked as archive**, **burnt out**,
**still lit**. It is also far cheaper than varying dimensions, because a condition is dressing and a
material swap rather than a new mesh.

### 5.3 Sightlines are the unit of pacing

Alternate tight and open, and never let a straight run be visible end to end. In a tunnel that means
racking, collapses and bulkheads breaking the length — which are props that also carry story.

### 5.4 Wayfinding without a map, and what makes it actually work

Every section carries its name on the wall, in the game's own typeface, at eye height at every
junction. But **a player who sees KEPPEL learns nothing**: naval surnames are not self-evidently
ordered, and the first draft's wayfinding argument quietly assumed they were. Three things are
required and all three are constraints rather than decoration:

1. **Signs carry the letter and the name**: `K · KEPPEL`, `N · NELSON`, `D · DRAKE`.
2. **The docket names a letter**, not a room.
3. **The generator places sections in alphabetical order along the spine.** Without this the letters
   convey no direction and the whole scheme is set dressing.

### 5.5 The descent is a lift interior, not a shaft

The first draft wanted a vertical shaft on the grounds that "the engine has never drawn a vertical
space". That is a reason for caution, not for enthusiasm. A shaft means either a spiral stair — box
colliders against a character controller with a step height, which is one of the most reliable ways
to break first-person movement — or a moving platform, which is a physics problem nothing else in the
game needs.

**None of it is necessary.** A lift interior sells the descent completely: the doors close, a shudder,
a long sound, and the doors open somewhere else. Zero vertical travel, all of the effect, and it is
also the strongest possible framing for the moment the lift dies.

If a shaft is ever wanted, it is a spike with its own gate, not a bullet on a piece list.

---

## 5a. Palette and materials

The complaint included the word **colour**, and the first draft of this document did not contain it.
That was the largest gap in it: §6 was entirely about light *levels*, and a game with no palette is
grey whatever you do to its exposure.

**This is the cheapest section here to deliver.** Every material in the game is generated by a small
Rust binary, so a palette is a table of numbers rather than an art commission.

### The lining

**Cast-iron segmental rings, painted lead-white over rust.** This is what a bored tunnel is actually
made of, and it does three things at once: it reads instantly as *tunnel* rather than as *corridor*;
the ring joints and bolt holes give a normal map something to be, which is the engine's most
under-used feature; and white paint over rust gives a warm-brown bleed through a cold surface, so
even an unlit wall has two colours in it.

| Surface | Base colour | Rough | Notes |
|---|---|---|---|
| Ring lining | bone white, `0.62 0.60 0.55` | 0.9 | rust bleeding through at the joints |
| Floor, dry | dark grey-brown | 0.95 | dust over concrete |
| Floor, flooded | near-black | 0.25 | the one reflective surface in the game |
| Bunk / racking steel | cold grey-green | 0.7 | the institution's own colour |
| Doors and bulkheads | lead grey | 0.8 | |
| Signage enamel | bone, with the section letter in black | 0.4 | matches the interface exactly (§8) |

### The two lights, and the contrast that is the whole look

- **The hand lamp is warm**, around 3000 K — a tungsten filament in a shelter-issue lamp.
- **The emergency circuits are cold**, a green-tinged fluorescent, around 5000 K.

**Warm against cold is what makes both read.** Your light is the only warm thing in the Warren, and
walking into a lit section replaces it — the picture goes from amber to green as you cross the
threshold, which is a mood change the player feels without being told. It also means the lamp's pool
is legible *inside* a lit section rather than washing out.

### The one accent

**Safety orange**, and nothing else in the world is allowed it. It marks the isolators, the lift call
plate, and the interface's focus highlight — which is the same orange, because §8 says the interface
is signage. So the only orange things in the Warren are the things you can act on. That is a
wayfinding system and a UI convention in one colour, for free.

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

### The title screen, specified

The first draft said "a shelter sign, not a panel", which is a direction rather than a screen.

- **The camera is fixed and does not turn.** It sits in the lift car, looking out through the open
  cage door at the landing wall opposite. It never moves and there is no player input to it. (Today
  it turns with the mouse while the menu is up, which was a bug and is fixed.)
- **What it is looking at** is the landing's enamel sign, lit by the one working fitting: the letter,
  the section name, and beneath it in small institutional type `DEEP SHELTER No. 4 · SUB-SURFACE
  ARCHIVE · NO ADMITTANCE WITHOUT AUTHORITY`.
- **What is moving**: the fitting flickers, on an irregular cycle. Nothing else. One moving thing in
  a still frame is enough and two is a screensaver.
- **The options sit bottom-left**, flush, left-aligned, small, in the same bone-on-black as the sign
  — not centred, not in a panel, not over the middle of the frame. The sign is the title; the options
  are a caption to it.
- **The world behind is dimmed** — a full-screen scrim at about 65% under the interface. This applies
  to every menu, and it is the single fastest improvement available to the first thing a player sees.

### The pause menu is a form, not a clipboard

"A clipboard" was a trap and is withdrawn. With no artist and no decal system, a skeuomorphic
clipboard is either a texture nobody here can make or a flat orange rectangle that a document calls a
clipboard and a screen shows as a flat orange rectangle. That is precisely the failure mode
`CLAUDE.md` warns about.

The institutional answer is better and free: **a form.** A stamped header, rule lines between rows,
left-aligned dense text, a reference number in the corner, no centring anywhere. It is made of the
primitives that already exist, it looks like the world it is in, and it cannot be mistaken for a
default dialog.

### The rest

- **In-world prompts**: as few words as possible, and never explanatory. Not "Take the brass key" —
  the object's own name, if anything at all.
- **A reticle**, which the game currently lacks entirely. Interaction is a sphere swept along the
  camera's forward and **nothing on screen says where that points**, so a player who cannot pick
  something up has no way to tell whether they are too far away or aimed five degrees off. That is a
  usability failure at the core verb, not a polish item. It should be the smallest mark that reads —
  a single dim pixel cluster, opening slightly when something is in reach.
- **No health bar, no objective marker, no compass.** The docket is the objective; the signs are the
  compass; being caught is the health bar.
- **Winning and losing must not share a treatment.** They currently get an identical box, colour,
  size and position, so the payoff for a successful run is typographically indistinguishable from
  being killed. Different colour at minimum, and the line holds alone for a beat before any buttons
  appear, so an ending is a moment rather than a dialog.
- **A brightness setting exists and lives on the title screen**, under the options. A game whose
  entire medium is darkness ships one; Frictional's own post is mostly about players and reviewers
  seeing the wrong picture because nothing calibrated it.

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

## 10. Scope, budget and plan

### How long a run is

**Twenty minutes, unhurried, first time through.** Every dimension in §5 and the whole dressing
budget follows from this one number, and the first draft did not state it.

Roughly: three minutes of descent and orientation, twelve of the three sections, five for the
generator and the run back. Three sections at four minutes each is what sets the tunnel lengths.

### The asset budget

With no artist and every asset generated by a Rust binary, a count is the real feasibility check on
this whole document.

| | Count | Notes |
|---|---|---|
| Meshes | ~16 | arch section, cross-passage, bulkhead, door, racking, bunk frame, lamp fitting, isolator, generator, lift cage, sign plate, crate, trolley, mattress, debris, the warden |
| Materials | ~10 | §5a's table |
| Sound clips | ~14 | footfall × 4 surfaces, lamp hum, lamp rattle, breath, warden tread × 3 states, isolator, generator, lift, sting |
| Sections | 3 + landing + plant room | |
| Signs | one per junction | text, not textures |

Anything that pushes those materially past these numbers is out of scope for the slice.

### The plan

**Reordered.** The first draft built the piece kit, then rewrote the generator, then signage, and
reached lighting at item 4 — three items of work before anything was judgeable as *atmosphere*, and
it multiplied a look by a generator before the look existed. `CLAUDE.md`'s own rule is the opposite:
prefer a working vertical slice over a complete horizontal layer. The generator rewrite is also the
only item here that is expensive to undo, so it should follow the slice rather than precede it.

| # | Work | Why here |
|---|---|---|
| **1** | **One tunnel and one cross-passage, hand-placed** — dressed, lit, materials from §5a, walked with the lamp. **Judged before anything else is built.** | The look must be settled before a generator multiplies it. This is the vertical slice |
| 2 | **The piece kit**, dimensions taken from the slice rather than guessed | Now the numbers are known |
| 3 | **Mission-then-space generator** — spine with three spurs, irregular cross-passages, conditions per §5.2 | The linearity complaint, at its root, and the one hard thing to undo |
| 4 | **Signage and the alphabet**, with the ordering constraint from §5.4 | Wayfinding stops being decoration |
| 5 | **The loop**: isolators, the charge, the generator, light-deafens-you | Replaces key-and-door |
| 6 | **The warden** to §3a's specification: senses, speeds, states, tells | The threat stops being `distance <= 9` |
| 6a | **Audio occlusion** — `amadeo-audio` + a physics query, in a *lower crate* | See below. Own line, because it can slip for reasons the game cannot control |
| 7 | **Interface**: title screen, scrim, reticle, form-styled pause, separated endings, brightness | The menu complaint |
| 8 | **The story surfaces**: tallies, section conditions, the docket | Carried by three things rather than one optional prop |
| 9 | **Tuning pass**, and **music/ambience**, which `docs/05` records as open and this document had not mentioned | |

**Item 6a is the risk on this plan.** §9 promotes occlusion from polish to a gameplay requirement,
and it is not game work: it is a feature in `amadeo-audio` plus a physics query, and
`amadeo-physics` has **no raycast** today. The mechanic is a lie without it — a warden exactly as
loud through a wall as through a doorway makes the whole sound model meaningless.

**Its fallback, if it slips**: reduce a source's effective range when the direct path to the listener
is blocked, resolved with `cast_shape`, which does exist. Cruder than real occlusion and enough to
make the mechanic honest.

### Migration

This document implies a near-total rewrite and the first draft said nothing about what happens to
what exists. It is a slice, not a fresh repository:

| What exists | What happens to it |
|---|---|
| `assets/pieces/` — 11 prefabs | `player_start`, `hud`, `ambience`, `spill` survive. `room_shell`, `wall`, `doorway` are replaced by the arch kit. `lost_key`, `way_out` are replaced by the isolators and the lift |
| `scenes/warren.scene` — the handcrafted room | **Becomes the slice** (plan item 1). It is already the place where things are tuned by eye, and it is where the rule tests live |
| The six generated `.wav`s | `warren_tone` and `footstep` survive; `warden_breath` is replaced by §3a's three tread states; `taken`, `escaped`, `caught` survive |
| The key and the door | Removed. `WayOut` becomes the lift; `Item`/`Inventory` stay, because the lamp and the docket are items |
| `the_run_can_end.rs`, `you_find_the_torch.rs` | **These assert a game that is being deleted.** They are rewritten against the new loop as it lands, not kept limping |
| `the_level_is_a_level.rs`, `it_lays_out_an_interior.rs` | Survive; `Layout::shortcomings` grows the new rules |
| `it_makes_a_noise.rs`, `the_shell_holds_together.rs` | Survive nearly unchanged |

### Process

**Every item goes to the critic agent before it is called done** (`.claude/agents/critic.md`), and
none is left until the verdict is POLISHED. That is Justin's instruction from session 20 and it is
project process, not a one-off. **This document was itself judged and returned NOT POLISHED**; §1's
missing prior art, §4's self-contradicting sense model, the absent palette and this plan's ordering
were all found that way.

### Two names to settle

**"The Warren" and "the warden" are one letter apart** and will be confused in every conversation,
commit message and test name for the rest of the project. One of them should change, and the warden
is the cheaper one — *the registrar*, *the marshal*, or simply what the boards call it. Left open
deliberately: it is Justin's call and it is one rename.

And the title says THE WARREN while the fiction says SHELTER No. 4. If "the Warren" is what the staff
called it, **something in the world has to say so** — a chalked note, a sign someone amended. A title
that the world never uses is a title that belongs to the box art rather than to the game.

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
- [How the "Beast" Works in Amnesia: The Bunker — AI and Games](https://www.aiandgames.com/p/how-the-beast-works-in-amnesia-the)
- [Amnesia: The Bunker — the generator system — Gameranx](https://gameranx.com/features/id/467193/article/amnesia-the-bunker-generator-system-explained/)
