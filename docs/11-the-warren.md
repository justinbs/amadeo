# 11 — The Warren: premise, world, and how it drives every system

> Read this before touching anything a player sees in `games/warren`.
> **PASSED the critic on its sixth review.** `05-roadmap.md` says *what* the milestone requires.
> This says *what the game is*, which until
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
(`docs/04` §14 and ADR 0066 §5, blocked on a rigged model; **Q41**). A design that required a
convincing walk cycle
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

**No sight model at all** — not a cone, not a raycast. This is absolute, because the moment sight
exists the light-versus-dark trade in §4 becomes a second, competing system and neither reads
clearly.

**At range, sound and nothing else:**

| The player is | Speed | Heard at |
|---|---|---|
| standing still | — | never |
| walking | **1.8 m/s** | 8 m |
| running | **2.6 m/s** | 22 m |
| **charging at a panel** | — | **the section, continuously, for as long as it runs** |
| pulling an isolator | — | the whole section |
| starting the generator | — | everywhere |

**The panel draws, and that is what makes the charge a decision rather than a wait.** It was
introduced as a noise loud enough to deafen the *player* and then left out of this table — so a sound
beside your head, loud enough to mask a footfall, was inaudible to a thing that hunts by nothing but
sound. That is not a small inconsistency: it is the difference between the design's self-described
best moment being a gamble and being a pause.

It also lands this document on the same side as the prior art it already cites. **The Bunker's crank
is noisy because winding it calls the Beast**, not because it inconveniences you. Half of that
mechanic was written here and the half that bites was not.

So "you may leave early and keep what it gave you" binds every single time: you are choosing how much
charge is worth how much attention, while deaf, listening to nothing.

**The speeds, all four, because two of them were doing one number's work.** Patrol 1.5 < **walk
1.8** < **run 2.6** < pursue 2.9. The player's authored `speed` today is 2.6, which the rest of this
document had been using as the *travel* speed while §4 was distinguishing walking from running — so
the pacing and the charge economy were both computed at the speed you are not supposed to move at.
Travel is a walk. Running is the loud, expensive thing you do when it has already gone wrong, and it
does not save you: pursuit is faster.

Surfaces modify it: standing water and steel plate carry further than dust and carpet, by roughly
half again. That is one number per floor material and it is what makes the flooded section in §4 a
real cost rather than a description.

### And at arm's length, it checks — which is what stops standing still being invincibility

The table above, alone, is a **hard-off switch on the entire threat**. Standing still is heard
"never" and there is no sight, so the warden cannot detect a motionless player at *any* range,
including zero, including in the middle of a lit corridor while it walks through the room. The
optimal play would be: walk in bursts, freeze the instant the tread stops, repeat. That is not a
decision, it is a metronome — and it is a strictly better exploit than the "hold walk" one this
document already fixed once.

The fix is in the fiction rather than bolted on. **It is counting.** An institution counting bodies
does not scan a room from the doorway; it walks the route and puts its hand on each bunk.

> **The warden checks fixed points on its route, and a player within arm's length of one is found,
> silent or not.**

**What is checked is a visible class of thing, not a list of authored coordinates:**

> **Every made-up bunk, every tally board, every live charging panel, every isolator — and, per
> condition, whatever that section's own equipment is: pump housings and duckboards where it is
> flooded, numbered racking bays and the renumbering ledger where it has been stripped.**

**The per-condition members are not decoration, and leaving them out nearly voided the rule.** The
class as first written was populated only in the section that is *still made up*: a stripped section
has no bunks by definition and a flooded one has neither bunks nor racking, so "4–6 drawn per
circuit" had to take everything that existed — two or three points, every round, for ever. A fixed
set is learnable in one pass, which is exactly the six-percent-of-floor collapse this rule was
written to prevent, and it landed hardest on the **flooded** section, whose entire question is sound.

The added members are props §5.3 wants anyway for breaking sightlines, so they cost nothing new.

| Section | Bunks | Boards | Panels | Isolators | Per-condition | Candidates |
|---|---|---|---|---|---|---|
| Still made up | dozens | 1 | ≤1 | 1 | — | plenty |
| Stripped / re-racked | none | 1 | ≤1 | 1 | racking bays × 8–12, the ledger | ~12 |
| Flooded / abandoned | none | 1 | ≤1 | 1 | pump housings × 3–4, duckboard runs × 4–6 | ~10 |

And the draw degrades honestly rather than silently: **half the class, minimum three, maximum six.**
So a thin section is still unpredictable rather than exhaustive, and the rule cannot quietly become a
fixed set again if a later condition turns out to be sparse.

That wording is load-bearing and the first version of it was wrong. "Authored points on the piece"
made the checked area about **6% of a section's floor** — six circles of 1.2 m in a 450 m² tunnel —
sitting on props a player has no reason to stand beside. So walk-and-freeze survived almost intact,
and the only spatial knowledge on offer was "do not stand in the 6%", whose answer is "stand
anywhere". Worse, the **flooded** section has no bunks, no racking and no boards, so the one section
whose entire question is *sound* would have had no checks at all and freezing there would have been
unconditionally free.

Putting the checks on **the places the player must occupy** fixes all of that at once, and it makes
stillness something you *purchase* rather than something you get.

| | |
|---|---|
| Check radius | 1.2 m — arm's length, not a room |
| Pause at a check | ~2 s |
| **Checked per pass** | **4–6 of the class, chosen from the seed and re-chosen every circuit** |
| What it does | **Starts a pursuit. It does not end the run** — see below |

**"Every made-up bunk" and "four to six per section" are the same rule stated two ways**, and which
one binds decides whether the mechanic works. A made-up section has dozens of bunks and the warden
does not stop at every one. A *fixed* subset would be learnable in one circuit and would collapse
straight back to the six-percent-of-floor problem this rule was written to fix.

So the class says **what may be checked** — a thing the player can read off the world at a glance —
and the seed says **which of them this circuit**, redrawn each time round. You can see that a bunk is
made up, and you cannot know whether it is on this pass. That is the whole of the tension in standing
still, and it costs one `SimRng` draw per circuit.

**"Found" starts a chase, and that is not softness.** An instant catch at an invisible 1.2 m boundary
would be a fail state learnable only by dying, and this engine cannot show the thing that makes it
fair elsewhere — Alien: Isolation opens the locker, The Bunker comes out of a hole you can watch.
With no skeletal animation there is nothing to show, so the compensation is a **reaction window**:
the check has a sound, you get the moment it takes, and then you are being chased. A chase is the
game working. An instant death at a boundary you cannot see is the game cheating.

Ranged detection stays "sound and nothing else", so §4's trade is untouched. And a check point is
still a *place with a history*: a bunk that is still made up is a bunk it still checks, which is why
the stripped section and the made-up section are dangerous in different places.

### How it moves

- **Patrolling**: 1.5 m/s along a route whose stops are re-drawn each circuit (see the check box below) and whose *length* shortens as the shift wears on (§4). Slower than a walk, so
  you can follow it and it cannot catch you by accident.
- **Investigating**: it goes to *where the noise was*, not to where you are — the distinction that
  makes moving after making a noise the correct play, and the thing a player has to learn once.
  **And an investigation ends in a check at the place it arrives**, wherever that is, prop or open
  floor. Without that line the modal outcome of every pursuit in this game is an unanimated mesh
  standing a metre from a motionless player, doing nothing, for ever — because freezing is the only
  survival move against something faster than you, and the check class is on props while a chase ends
  wherever you happened to stop. It also teaches the rule in one beat: **freezing after you have
  moved works; freezing where the noise was does not.**
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
| Patrolling | an even, unhurried tread |
| **Checking** | the tread **slows over a step or two**, then a named sound — a hand on a frame, chalk on a board — then it moves on |
| Investigating | the tread stops **abruptly, mid-stride, and nothing follows it** — the silence is the tell |
| Pursuing | the tread breaks into something faster and the breath changes |
| Losing you | it slows, stops, waits far longer than feels comfortable, then resumes patrolling |

**The abrupt stop is the most important sound in the game** — it is the moment the player learns the
rule — and adding check points nearly destroyed it. A warden that stops four to six times a section
to check a bunk fires the "it heard me" tell constantly, and a tell that fires falsely teaches
nothing.

So the two stops are built to be **opposites**, which is one extra clip and a deceleration:

- a **check** decelerates and is *followed by a sound*;
- an **investigate** is instant and is *followed by silence*.

The distinguishing feature is not the stop. It is what happens in the second after it, and the
frightening one is the one where nothing does.

### When it catches you

**The run ends and the level does not reset.** The isolators you threw stay thrown, the sections you
lit stay lit, and what you were carrying stays where you were carrying it. You restart at the lift.

This is the single most load-bearing decision in the section and the first draft did not make it. A
horror game that takes progress away on death teaches the player to stop taking risks, which is the
opposite of what §4 wants; a game that takes nothing away has no stake. The stake here is **time and
noise**: you have to walk back through a level whose lights are on and whose warden is somewhere new,
and the lamp does not recharge on death.

**And the walk back must not be through solved, empty corridor.** As stated, the punishment for
failing routes the player through the least interesting space in the game — lit, known, cleared —
immediately after the most tense moment they have had. That is a punishment made of boredom, which
is the one kind that makes people stop playing.

> **The warden relocates onto your return route.** It heard where you were; that is where it now
> patrols.

So the way back is the one stretch of level whose danger has *increased*, it costs nothing to build
(a patrol route is data, and moving it is a write), and it is the honest behaviour of a thing that
investigates where the noise was.

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
exactly as safe as standing still in a dark one — so pulling an isolator costs nothing, and the
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

### What "lit" means, and the two things this repair broke

Fixing §4 in isolation broke two other sections, and neither was traced. Both are recorded because
the failure mode — repair one section, contradict two others — is the same one that produced the
sense-model contradiction above.

**It broke §3.** That section promises the warden is *never clearly seen*, justified explicitly on
the engine having no skeletal animation. But three sections became **compulsorily lit**, including
the one described as where the warden spends most of its patrol — so the design was guaranteeing a
full-light encounter with a rigid, unanimated mesh sliding at 2.9 m/s, on the critical path, where
before it was avoidable. That is the single thing most likely to collapse the illusion.

**It broke §9**, which says near-silence is the default and *"a single sound is an event"*. By the
endgame the landing and three sections would all be humming continuously, so the soundscape at the
moment of maximum tension would be a drone.

Both are repaired by being precise about one word:

> **"Lit" means a handful of pooled fittings with real dark between them — never an evenly lit
> space. The warden stays in the dark between the pools.**

- **The warden crosses a pool but never lingers in one.** The first version of this said the player
  is *never* in a lit pool at the same time as the warden — which, once charging moved to the wall
  circuits, guaranteed that nothing could happen during the 25 seconds §4 calls its best moment. The
  prose promised dread and the rules forbade it, on the same page. Crossing is the repair and it is
  also the better picture: **the silhouette appears at the pool's edge, passes through, and is gone**,
  which is exactly the shot §3 wants and the only one the engine can render convincingly.
- It is what §6 asks for anyway — contrast, not uniform gloom — so this costs nothing it was not
  already buying.
- And the hum is **per fitting, spatial, and narrow-band**: a buzz sitting on the tread's own
  frequency, loud enough to mask that specific sound within a pool and quiet enough that the section
  is not filled. So a lit section is *quiet with deaf spots*, rather than a drone, and §9's rule
  survives.

### The warden carries a lamp, and that is a feature rather than a loose end

The justification above — *it does not linger under a fitting because it has a lamp of its own* —
quietly introduced a **moving light** into a budget §6 has already spent, and quietly opened a
*visual* channel on a threat §3 says is known only by sound. It has to be either claimed or dropped.

**Claim it.** A moving pool of light, seen before its owner, with a shape occasionally in front of
it, is the strongest thing this engine can do with a threat it cannot animate. It turns "no skeletal
animation" from a constraint being worked around into the actual aesthetic — you never see the
warden, you see *its lamp*, and you track it through a level by the light crossing a doorway two
rooms away.

It also gives the player a second channel that agrees with the first: the lamp says where, the tread
says what it is doing, and losing one still leaves the other. In a deafened section, its lamp is the
only warning left — which is precisely the trade §4 is built on, seen from the other side.

**Budget** (§6): the warden's lamp is one of the eight punctual slots and is **not** a shadow caster
— the two casting slots stay with the player's lamp and one chamber practical. A cold, narrow,
downward beam, deliberately unlike the player's warm one, so a light seen at a distance is
immediately identifiable as *not yours*.

**And §3's promise survives**, because it was always about the *creature* rather than about visual
information: you see a light, a shape at a pool's edge, a silhouette crossing a doorway. You never
get a clear look at the thing, which is the promise, and now there is something to not-quite-see.

### What the player is spending

The second thing the first draft got wrong: **nothing was scarce**, so there was a dominant strategy
and it was "hold walk for twenty minutes". A resource with no scarcity is not a resource, and a
stealth system whose safe option is permanently affordable is a slow walk rather than a decision.

So the lamp is a **shelter-issue accumulator lamp with a charge**, and it dims as it drains.

**It can be switched off, and that has to be said, because everything else hangs from it.** A hand
lamp obviously can be, and pretending otherwise would be the kind of unstated rule this document
exists to stop. But if the charge drained *only while the lamp was on*, the player would simply
switch it off in every lit section — so the number of draining sections would fall as the game
progressed, and the pressure curve would run **backwards**: tightest at the start when nothing is
hunting, loosest during the climax.

> **The charge drains whenever the lamp is on, or you are moving. Standing in the dark costs you no
> charge.**

That is the same accumulator the shelter issued for a shift underground.

### But standing still cannot be free, and for two drafts it was

The line above used to end *"…is the only thing that is free"*, followed by a claim that "both cost
you the clock". **There was no clock.** The charge drains on movement-or-light, which makes it a
*distance* budget rather than a time one, so a patient player paid nothing for waiting — and since
§3a makes a motionless player undetectable off-prop, the optimal run was **creep, freeze, wait,
creep**, indefinitely, at zero cost. This document correctly diagnosed "hold walk for twenty
minutes" and twice replaced it with something strictly cheaper.

Both cited references close exactly this hole: The Bunker's fuel burns in real time, and
*Alien: Isolation*'s creature is drawn to a player who hides too long. **And the fix was already in
the premise, unused**: *"before the site is sealed"*.

**The sealing crew is working above you, and you can hear them.**

- It is **diegetic and continuously audible** — pours, drills, the concrete going in — so there is no
  timer on screen and the player always knows roughly how much shift is left. That is the same
  channel the rest of the game speaks in.
- It is **generous**: about 30 minutes against a 20-minute run. It does not punish an unhurried first
  playthrough, and it does not permit an infinitely patient one.
- When it finishes, the way out is gone. That is a real ending rather than a failure message, and it
  is the one the fiction has been pointing at from the first line.

**And the inner clock is the warden itself.** As the shift wears on it **walks a shorter route** —
dropping the far end of its circuit, then the next — so it comes past more often. It is counting, the
count is due, and it stops covering ground it has already covered.

**Specifically a shorter route, and not the other two readings.** "Its round shortens" could mean a
faster patrol, which would contradict §3a's guarantee that patrol is *slower than a walk so you can
follow it and it cannot catch you by accident* — a guarantee several other things lean on. Or it
could mean fewer stops, which would make the late game *less* dangerous and invert the pressure curve
this document has already rejected twice by name. Only a shorter circuit gives the stated effect
without breaking either. Patrol stays 1.5 m/s and the check count per pass is unchanged; what shrinks
is the loop.

So patience costs in the currency the game is actually about: not a number draining, but the thing in
the tunnel coming round more often.

That gives waiting a price at both scales, neither of which is a UI element.

### Where you can recharge, and why it is the best moment in the design

**Not at the isolators.** The first draft put the charging points on the objectives, which means the
critical path tops you up for free and the resource never binds on the route — it only bites when
you leave it. That makes the charge **a tax on exploration**, which is precisely the voluntary risk
§1 quotes Frictional as the source of the fear. Exactly backwards.

**A charging panel makes its own noise**, and that is what deafens you rather than the section being
lit. A shelter accumulator panel whines while it charges — right beside your head, for twenty-five
seconds, at exactly the volume that matters.

> **The only places you can recharge are the places you cannot hear.**

This is a correction of the previous draft, which put the panels "wherever the fittings are" so that
the deafness came from the section's lighting. Two things went wrong with that and both are the same
mistake:

- **It was false on the whole outward leg.** Panels are live before their section is lit (below), so
  an unlit section had a quiet, fully-hearing, zero-risk panel — and the dominant play became "top up
  before pulling each isolator", which is the safest possible moment. The deaf-pool moment only
  existed on the way back, by which point you were full.
- **It put the warden under a light.** Panels are check points (§3a) and the fittings are the pools,
  so a two-second check at a panel is the warden *lingering in a pool* — which §4 forbids three
  subsections earlier, and which would stand an unanimated mesh in full light 1.2 m from the player
  at the design's self-described best moment. Precisely the illusion collapse this section exists to
  prevent.

So **panels are on the wall, in the dark, away from the fittings.** The whine is the risk, it travels
with the mechanic rather than with the lighting, and it is true from the first minute of the game to
the last.

Standing beside a whining panel, deaf, for a stretch of time whose **rate** you cannot hurry -- you
may leave early and keep what it gave you, but you cannot make it come faster -- listening to
nothing, is the best moment this design has. One of the three is deliberately *off* the isolator
route, so a full run cannot be done without leaving the path once.

**And the panels are live before they are lit.** A wall circuit that only charged after you threw its
isolator would put every charging point *behind* the objectives, so the early game — when nothing is
hunting you — would be the tightest and the climax the loosest. That is the same backwards pressure
curve this section already rejected once for the lamp-off case, arriving through a different door.
The panels are on the standby ring and have always been live; the *lights* are what the isolator
brings up.

**The numbers, and the arithmetic that has to accompany them.** §10 derives its run length honestly
and the first version of this table did not, which is how it ended up describing an errand:

| | |
|---|---|
| Full charge | 6 minutes of moving-or-lit |
| **A 25-second top-up buys** | **90 seconds** — a *rate*, not a refill |
| Interruptible | yes, and it keeps what it took, so a partial charge is a real choice |
| Reusable | yes, unlimited. The scarcity is *time and exposure*, never a consumable |

Working it through against §10's twenty minutes, **at the walk of 1.8 m/s and not at the run** — the
error the first version of this sum made: about 11 minutes of movement across three sections, a
descent and the return, plus 3–4 minutes of lamp-on searching while standing — call it **14–15
minutes of drain**. Against a 6-minute charge at 90 seconds a stop, that is **six stops**, each 25
seconds long, each spent beside a whining panel that the warden can hear.

The rate is what makes this bind. The first version gave a *full* refill for the same 25 seconds and
made the whole run need one stop, at 2% of its length, risk-free. One free errand is not a resource.

### What a flat lamp costs, because "usable" made never charging optimal

A lamp run flat still gives a faint usable glow rather than a black screen — Frictional's rule that a
pitch-black image is not exciting, applied to the failure case.

**But "usable" alone made the whole economy skippable.** Charge drains on movement whether the lamp
is lit or not, so switching it off saves nothing, and if the dim state is merely dimmer then the
optimal run is **never to charge at all**: skip six stops, skip six seeded check points, skip six
stretches of deafness, and buy back several minutes of the seal clock.

So what a flat lamp costs is **reach**, not brightness:

- **Signage and boards stop resolving at distance.** You can walk; you cannot read. Which means the
  stripped section — whose whole question is *search* — becomes nearly impossible on a flat lamp, and
  the alphabet you navigate by stops working.
- **A silhouette at a pool's edge stops resolving.** The warden's lamp is still visible, because that
  is its own light; what you lose is the ability to tell what is in front of it.

Enough to keep walking. Not enough to keep playing well. That is what makes six stops worth making.

### How the three sections differ

The first draft claimed agency and then made the three spurs interchangeable, so the choice of where
to go first was cosmetic. Each has one concrete, mechanical difference:

The first version of this table gave all three the same cost at different settings — two were
loudness and the third was a probability — so it was one axis pretending to be three. Each now poses
a **different kind of question**:

| Section | The question it asks | What it costs you |
|---|---|---|
| **The flooded one** (lower cross-passages) | *Sound* | Standing water: every step carries half again as far, and you cannot move quietly at all. The only silent state is standing still — which §3a has just made a choice about *where* |
| **The stripped one** (re-racked as archive) | *Search* | Its isolator is unmarked. The racking was renumbered when the archive moved in, so you have to read the boards to work out which bay it is in — with a lamp, in the dark, while something patrols |
| **The warden's own** (still made up, bunks and all) | *Timing* | Its isolator is behind a bulkhead, and opening a bulkhead is heard **everywhere**. The question is not where it is or how quietly you can reach it. It is *when you are willing to announce yourself* |

There is no correct order. There is an order that suits how much charge you have left and where you
last heard the tread, which is the decision the loop exists to produce.

### The three beats

**1. Descent.** The lift takes you down and dies. You have a lamp and a docket with a letter on it.
You learn the alphabetical sections from the signs, because you have to find one.

**2. The Warren.** The lift needs power. The standby set needs **three isolators** thrown, in three
different sections, *in any order you like*. Pulling one brings that section's emergency lighting
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

> ### ✅ Built in session 23, and the numbers moved from the ones written below
>
> A cell is no longer a room. It is **12 m of bore, 4.8 m wide, with a segmental cast-iron crown
> 3.2 m above the deck**, and every bore runs north–south. An east or west door is a **cross-passage**
> through the 3.6 m of ground between two tubes, and a north or south end without a door is closed by
> a **bulkhead**. The conditions below are a `Condition` drawn per room in `lay_out`.
>
> **Three numbers differ from this section as first written and each was decided rather than
> drifted.** The tunnel is 4.8 m rather than 5 — half of it either side of a centreline, so the
> cross-passage arithmetic is exact. It is 12 m per cell rather than 60–120 m, because a run is
> however many cells share a line and the graph decides that. And the crown is 3.2 m rather than the
> plan's 4.7: engine gate review 14 ruled that a 4.8 m tube with 4.7 m of headroom is a *running
> tunnel*, where the real deep-level typology is a 5.03 m bore **split into two decks** — 2.2–2.5 m
> of real headroom — and that claustrophobia is not served by a space you could drive a lorry
> through.
>
> **What is not built** is §5.1's mission-first generator: the room graph is still `lay_out`'s walk,
> realised as tubes rather than replaced. §10's plan item 3.

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
| Floor, flooded | near-black | 0.35, metallic 0.2 | see the note below |
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

**Safety orange**, and nothing else in the world is allowed it. It marks the isolators, **the
charging panels**, the lift call plate, and the interface's focus highlight — which is the same
orange, because §8 says the interface is signage. So the only orange things in the Warren are the
things you can act on. That is a wayfinding system and a UI convention in one colour, for free.

**The panels were missing from that list and it mattered.** They did not need marking while they
lived under the fittings; they now sit on a dark wall (§4) and are the most frequently used
interactive object in the game. Without the orange there is a small, nasty spiral — low charge, need
a panel, sweep a dark wall with the lamp to find one, spend charge finding it. The principle above
already covered them; only the enumeration was stale, which is the same section-goes-out-of-date
failure this document has now recorded four times.

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

### The light budget, which is a hard engine limit and has to be designed to

`MAX_PUNCTUAL_LIGHTS = 8` and `MAX_SHADOW_SPOTS = 2`, and **the nearest lights win the cut**. Nothing
warns you: the ninth light is silently dropped by distance.

Walk down a 100 m tunnel with a fitting every 10 m and you will watch lights **pop in and out at the
budget boundary** — which is the most machine-made artefact available, in the one system this
document calls the medium. So:

- **No more than five fittings within a tunnel's visible run.** Five, not six, and the difference is
  the whole cap: five fittings plus the hand lamp plus one chamber practical plus **the warden's
  lamp** is eight exactly. Six would be nine, and the ninth is dropped silently by distance — so the
  light that vanished would be the far fitting, at the far end of the tunnel, *at the moment the
  warden entered a lit section*. The machine-made artefact this section names, triggered by the one
  shot the design exists for.
- **The fittings do not cast shadows.** The two casting slots are the hand lamp and one practical per
  chamber, and that is the whole budget. This is also correct artistically: a pooled fitting overhead
  wants to read as a wash, and the shadows that matter are the ones your own lamp throws.
- Sparse fittings are what §4's "pools with real dark between" needs anyway, so the engine limit and
  the design want the same thing. Where a limit and a design agree, take it as confirmation.

**The flooded floor needs its own note.** The first draft made it roughness 0.25 and called it "the
one reflective surface in the game" — but it would be reflecting `gloom.rs`'s deliberately near-black
ambient, so it would read as a hole rather than as water. It is roughness 0.35 with a little metallic
instead, and **its read comes from the hand lamp's specular highlight**: the one surface that shows
you where you are by throwing your own light back at you. That is a better idea than reflectivity for
its own sake, and it makes the flooded section legible in the dark, which is the section where you
most need to know where the floor is.

**The failure mode to avoid is uniform gloom**, which is what the game has now: everything equally
and mildly visible, no pools, no contrast, nothing to walk towards. Contrast is the point. A corridor
with one working fitting halfway down is frightening; a corridor at 15% brightness everywhere is
grey.

---

## 7. What the engine can and cannot do, and designing to it

This is not a limitation section. It is why this design and not another.

**Strong:** dynamic light and real-time shadows, fog, spatial audio, a first-person body, physics,
interaction, a level defined in text, PBR materials with normal maps, **ambient occlusion** (ADR
0083, session 22), **parametric geometry authored as text** (ADR 0074) and **a texture generator that
is engine code** (`amadeo-texture`, session 22) — the last two are what make §5a's *"a palette is a
table of numbers rather than an art commission"* true rather than hopeful.

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

- **The title screen needs a camera of its own, and does not have one.** Today it simply shows the
  play camera, wherever the player happens to have spawned — which is why the title plate currently
  floats over an arbitrary bit of room. It wants a **second `Camera` entity in the lift car** with a
  lower `order`, active only while `Screen::Title`, and `active false` once the run begins.
- **It is fixed and does not turn.** No player input reaches it.
  *(The mouse did turn the view behind the menu until commit `581aa0f`. That was a genuine bug and
  not, as it appears from the code today, impossible: `look_with_mouse` is registered in
  `Stage::Simulation` without `.while_paused()`, so it should have been skipped — but this game never
  inserted the `Paused` resource, and `resource_mut` on a resource that does not exist is silent, so
  nothing was ever skipped. Worth keeping because the shape recurs: reading the code in its fixed
  state makes the bug look like it could never have happened.)*
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
  *Where it lives mechanically*: it adjusts exposure on the loaded `Environment` **inside
  `EnvironmentCache`, which is the Service** — outside the state hash (ADR 0009) — so two players on
  different brightnesses still simulate identically and a replay is unaffected.
  *(An earlier draft said `Environment` itself is a Service. It is not: `impl Component for
  Environment` and it derives `StableHash`. The conclusion survives for a different reason — a
  `Camera` carries only an environment **id**, so the cache's copy is the only thing render reads, and
  nothing reads an `Environment` component at draw time.)*
  **Where the file lives is Q38's question**, the same one the save file has, and it should be
  answered once for both rather than twice differently.

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

### The two continuous sounds, and why near-silence survives both

§4 introduced a permanent per-fitting hum and §4's seal clock introduced a continuous industrial bed,
and **this section was not amended for either** — which matters more here than anywhere, because
§3a's most important tell is *an absence of sound*, and a continuous bed is precisely what makes an
absence hard to hear. Both are specified to the same standard the hum already got:

**The fitting hum** — per fitting, spatial, narrow-band, sitting on the tread's own frequency. Loud
enough to mask that specific sound inside a pool, quiet enough that a lit section is *quiet with deaf
spots* rather than a drone.

**The sealing crew** — the clock, and it must not fill the room:

- **Structure-borne, not airborne.** It arrives through the concrete rather than down the tunnel: low,
  dull, no high end at all. It sits **well below the tread's band**, so the two never compete and
  silence is still audible.
- **Directional, from above and towards the lift.** So it doubles as a wayfinding cue — when you are
  lost, the work tells you which way the way out is. A clock and a compass in one sound.
- **Staged, not continuous.** Distinct phases across the run — cutting, then drilling, then the pours,
  then the long quiet of it setting — so the player reads *how much shift is left* from what it is
  doing rather than from a bar. The final stage is the quietest, which is the correct and most
  frightening shape.
- **Attenuated by depth**: loudest at the lift landing and at the top of the spurs, nearly gone at the
  far ends. Going deep is going out of earshot of the only friendly sound in the game.

- **An investigation travels in silence, deliberately.** The tells table gives investigating an abrupt
  stop *followed by nothing*, and the honest consequence is that the warden then crosses the section
  without a tread — so the next thing you hear is the check at arm's length. That is the most
  frightening reading and it is chosen rather than fallen into: during that interval **its lamp is the
  only channel**, which is exactly what the lamp was claimed for, and it is the one stretch of the
  game where looking matters more than listening.

---

## 10. Scope, budget and plan

### How long a run is

**Twenty minutes, unhurried, first time through.** Roughly: three minutes of descent and
orientation, twelve across the three sections, five for the generator and the run back.

**And that does not set the tunnel lengths**, which the first draft claimed it did without doing the
arithmetic. At the walk of **1.8 m/s** (§3a) a 120 m tunnel is **67 seconds** one way, so four minutes in a
section is not four minutes of walking — it is about two minutes of movement and two of **standing
still, listening, searching and waiting**.

That is the correct answer and it is worth stating rather than hiding, because it says what the
sections are actually made of: the pacing comes from the §4 costs — reading boards in the dark,
waiting out a patrol, holding still at a check point, charging for twenty-five seconds you cannot
shorten — and not from distance. A design that tried to fill twelve minutes with walking would need
1.8 km of tunnel and would be a corridor simulator.

So the tunnels are **60–120 m because that is what a deep-level shelter is**, and the time comes from
everything else.

### The asset budget

With no artist and every asset generated by a Rust binary, a count is the real feasibility check on
this whole document.

> ### ⚠ The constraint this table was built against **expired in session 21**, and the table has not
> caught up
>
> What this section said, and what shaped four of its decisions, was: *"the engine's only mesh
> components are `BoxMesh`, `PlaneMesh`, `ArchMesh` and `GltfPart`. There is no raw-geometry path in
> the scene format — you cannot author vertices in text — and `amadeo-gltf` is a reader with no
> writer, so a Rust binary cannot emit a prop the way `sounds.rs` emits a clip."*
>
> **Every clause of that is now false.** ADR 0074 added `CylinderMesh` (with a `top_radius`, so a
> cone is one too), `SphereMesh`, `WedgeMesh` and `StairMesh`; `CompoundMesh` assembles any number of
> those into **one** mesh, one asset and one draw call, with per-part rotation, a `repeat` and a
> `mirror`; and §4's `VertexMesh` **is** the raw-geometry path — a `.mesh` may carry vertices and
> indices directly, and `amadeo fmt` round-trips it. The paragraph below even names that engine work
> as a future line item with a fallback. It landed.
>
> This is the fifth instance of the failure this document names on itself — *a section going out of
> date while the sections around it move*. It is worth more than the others because it did not merely
> mislead: **it cut content.** The trolley below was struck out on "wheels are the one thing boxes
> cannot fake", and a wheel is a cylinder.
>
> The table's counts are still about right and the *reasoning* below is kept as written, because the
> arguments that survive the correction are the interesting ones — a machine genuinely is
> rectilinear, and a failed tunnel ring reads better than a pile of debris whatever the engine can
> draw. What changes is which of them were forced.

So "19 meshes" means **19 assemblies of primitives**, where a primitive is now a box, a plane, an
arch, a cylinder, a cone, a sphere, a wedge, a stair, or vertex data written out by hand. Every entry
below was checked against the *old* set, and four from the first draft failed it:

| | Count | Notes |
|---|---|---|
| Meshes | ~19 | All box/plane/arch assemblies. Arch section, cross-passage, bulkhead, door, racking, bunk frame, lamp fitting, isolator, lift cage, sign plate, crate, charging panel, tally board, the warden, the generator, a mattress, **pump housing, duckboard run, the renumbering ledger** (§3a's per-condition check members), **the bolt** (§5a) |
| Materials | ~10 | §5a's table |
| Sound clips | ~24 | footfall × 4 surfaces, fitting hum, **panel whine**, **the crew sealing above × 4 phases** (§9 -- cutting, drilling, pouring, setting), lamp rattle, breath, warden tread × 3 states, warden check, isolator, bulkhead, generator, lift, **the bolt landing**, **3 stings** (taken / escaped / caught — §8 requires the endings to differ, and all three already exist) |
| Sections | 3 + landing + plant room | |
| Signs | one per junction | text, not textures |

**The four that were cut or re-specified**, because a box version of each is exactly the grey-box
tell this document exists to eliminate:

- **The generator** — kept, as a bolted assembly of boxes with a pipe run of thin arch sections. A
  machine genuinely is rectilinear, so this one survives honestly.
- **The trolley** — was cut, because *"wheels are the one thing boxes cannot fake"*. **Reinstated:** a
  wheel is a cylinder, and `CompoundMesh` puts four of them under a frame from one authored part with
  a `mirror` on two axes. It is the one entry here the expired constraint actually took away.
- **The mattress** — kept but re-specified: a thin box *on a bunk frame*, which is what a stripped
  shelter mattress looks like anyway. It reads because of what it is on, not because of its shape.
- **"Debris"** — cut as a concept and replaced with a **collapse built from arch sections at
  angles**. A pile of boxes reads as a pile of boxes; a tunnel ring that has come out of true reads
  as a tunnel that has failed, and it is the same primitive already needed for the walls.

~~If a prop genuinely needs curved geometry later, that is a raw-geometry `.mesh` variant or a glTF
writer — **engine work, on the plan as its own line with a fallback**, exactly like occlusion.
Nothing in this slice requires it.~~ **Done, in session 21** — ADR 0074 §4's `VertexMesh` is that
variant, and the curved primitives arrived with it. The glTF writer is still absent and still not
needed, because a parametric prop written as text is a better artefact than a binary one anyway
(invariant I1).

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
| 5a | **A way to make noise on purpose** — see below | A hunted-by-sound game where the player cannot *make* sound has removed half the system |
| 6 | **The warden** to §3a's specification: senses, speeds, states, tells, check points | The threat stops being `distance <= 9` |
| 6a | **Audio occlusion** — `amadeo-audio` + a physics query, in a *lower crate* | See below. Own line, because it can slip for reasons the game cannot control |
| 7 | **Interface**: title screen, scrim, reticle, form-styled pause, separated endings, brightness | The menu complaint |
| 8 | **The story surfaces**: tallies, section conditions, the docket, and the chalked *WARREN* amendment on the landing sign | Carried by four things rather than one optional prop — including the one that makes the title a word the world actually uses |
| 9 | **Tuning pass**, and **music/ambience**, which `docs/05` records as open and this document had not mentioned | |

**Item 6a is the risk on this plan.** §9 promotes occlusion from polish to a gameplay requirement,
and it is not game work: it is a feature in `amadeo-audio` plus a physics query, and
`amadeo-physics` has **no raycast** today. The mechanic is a lie without it — a warden exactly as
loud through a wall as through a doorway makes the whole sound model meaningless.

**Its fallback, if it slips**: reduce a source's effective range when the direct path to the listener
is blocked, resolved with `cast_shape`, which does exist. Cruder than real occlusion and enough to
make the mechanic honest.

**Item 5a is the other half of the sound system, and the first draft did not have it at all.** A game
about being hunted by sound in which the player can only ever *avoid* making noise is playing half a
system: the player has no way to put a sound somewhere on purpose, so they can never lie to it. The
Bunker gives you bricks and bottles; Amnesia gives you objects to throw.

**One throwable** is enough: a bolt out of the racking, with a fixed audible radius where it lands.
It turns "where is it" into "where can I send it", which is the difference between hiding and
playing.

**It has a count, and the count is the whole design.** Unlimited bolts defeat the warden outright,
because it investigates *where the noise was* — so an infinite supply of noise is an infinite supply
of misdirection, and The Bunker gates its bricks and bottles through inventory for exactly this
reason.

| | |
|---|---|
| Carried | 3 |
| Recovered | yes, by picking them up where they landed — which means going *to* the place you just sent it |
| Loudness | at least a footstep, and it draws from where it lands rather than from you |

The recovery rule is what makes it a decision rather than a cooldown: spending a bolt buys you a
window and costs you either the bolt or a walk into the space you just made dangerous.

*Its dependency, and the fallback so it cannot block*: a real impact sound needs **collision events**,
which `amadeo-physics` does not have (its own docs list joints, raycasts and collision events as
still to come). The fallback needs nothing new — emit the `SoundPlayed` at the throw's predicted
landing point after a fixed flight time. Less accurate, indistinguishable at the pace this game moves,
and it means 5a cannot slip for engine reasons.

### Migration

This document implies a near-total rewrite and the first draft said nothing about what happens to
what exists. It is a slice, not a fresh repository:

| What exists | What happens to it |
|---|---|
| `assets/pieces/` — **12** prefabs | `player_start`, `hud`, `ambience`, `spill` survive. `room_shell`, `wall`, `doorway` are replaced by the arch kit. `lost_key`, `way_out` are replaced by the isolators and the lift |
| `dropped_torch.scene` | **Deleted.** The lamp is carried from the start — it is shelter issue and the clerk signed for it |
| `room_lamp.scene` | **Becomes the emergency fitting**, and is the one existing piece the new mechanic is built directly on: it grows a mesh (it currently has none), a hum, and an off state |
| `warden_post.scene` | **Becomes a patrol waypoint**, of which there are now several -- the route's shape rather than one spawn position. Not a check point: what is checked is a *class* of prop drawn per circuit (§3a), which is a different thing |
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

### What to watch while building — the critic's list at the passing review

This document **passed on its sixth review**. These are the things the reviewer said to watch during
the build rather than fix on paper, recorded so they are not lost between sessions:

1. **How the player learns "three isolators, then the generator."** §8 removes the objective marker
   and forbids explanatory prompts, and the docket names the *box*, which is optional. The orange
   rule covers it implicitly and nothing states it. Cheapest answer: **a standby-set plate in the
   plant room naming the three sections**, and the generator **refusing audibly** while a circuit is
   still open. One sign, one sound.
2. **`STRIDE = 0.95` was derived against 2.6 m/s.** At the new walk of 1.8 it gives 1.9 steps a
   second, which reads as a scurry. Re-derive it when the walk/run split lands.
3. **Charging is now the loudest thing the player does, six times, against three isolator pulls.** If
   the run turns into a chase reel, the fader to reach for is the **whine's radius**, not the charge
   rate.
4. **The check radius is a point and three of the new class members are extended objects.** Decide
   whether a duckboard run is one check or a line of them *before* building the flooded section — it
   is the difference between that section's core question working and freezing five metres along
   being free.
5. **The reach penalty on a flat lamp binds outward and barely at all on the lit return**, so the
   last one or two of the six stops are optional in practice. Watch that the endgame still has a
   reason to stop.
6. **The lamp draining while switched off and walking has no fiction**, and it is the one rule a
   player will meet and be unable to explain. Cheapest fix is to reframe the meter as **the shift**
   rather than the battery.

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
