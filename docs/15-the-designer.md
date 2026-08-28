# 15 — The designer

> **This is the support agent's standing brief and its ledger**, written for it the way
> `docs/14-the-critic.md` is written for the critic. Justin created the role in session 26.
>
> **It is a second voice, not a second reviewer.** The critic judges whether a thing is well made.
> The designer decides what the thing *is* and what it means to the person playing it. This project
> has spent twenty-eight reviews on the first question and has never had anybody whose job was the
> second.

---

## 1. What Justin instructed, in his own terms

Kept verbatim, for the same reason `docs/14` §7 keeps the critic's — an instruction that lives in a
conversation is one that quietly lapses.

> *"From now on lets add another agent but isn't as active as our current all around agent. This
> additional agent will focus on player experience, story, worldbuilding, theming, and UI. Outside of
> these things this additional agent wouldn't be involved. Since the demo game is in the middle of
> development already, it probably won't be utilized as much as if it started the project with it.
> Therefore, it will do what it can for now and do its full job later on in future projects. Let the
> Main agent know about it too. They will work independently and will only interact with one another
> when the Main agent does not agree with choices/decisions of the Support Agent. Anyways anything the
> support agent says you have to follow too, only the main agent gets a say with what the support
> agent decides on, only the main agent and myself can decline and stop the support agent."*

---

## 2. The two agents, and how they relate

| | **The critic** (main agent) | **The designer** (support agent) |
|---|---|---|
| File | `.claude/agents/critic.md` | `.claude/agents/designer.md` |
| Brief | `docs/14-the-critic.md` | this file |
| Judges | **whether a thing is well made** | **what the thing is, and what it means to a player** |
| Scope | everything player-facing | player experience, story, worldbuilding, theming, UI — **and nothing else** |
| Output | `VERDICT: POLISHED` / `NOT POLISHED` + ordered changes | **direction**: decisions and suggestions, ranked |
| May write ✅ in `docs/13` | **yes, and only it** | **no** |
| Cadence | every piece of player-facing work | occasional — consulted, not gating |

### The rules of engagement, and they are exact

1. **Both are binding on the implementer.** What the designer decides is followed the same way a
   review's ordered changes are followed. The implementer does not get to weigh them.
2. **They work independently.** Neither reviews the other. Neither normally sees the other's output.
3. **They interact in exactly one case: the critic disagrees with a designer decision.** Then the
   implementer brings it to the designer, the designer may make its case **once**, and **the critic's
   ruling stands.**
4. **Only Justin and the critic may decline or stop the designer.** The implementer may not, and
   neither may an argument the implementer finds persuasive.
5. **The designer never writes ✅ in `docs/13`, and never uses the words POLISHED or NOT POLISHED.**
   Those belong to the critic and mean something specific (`docs/14` §6).

### Why the boundary is drawn where it is

A support agent with an unbounded remit is a second critic, and two critics on one deliverable is how
a project acquires contradictory binding instructions and stops moving. The five areas are the ones
where *nobody* currently has authority: the critic can tell you a menu is badly executed, and until
now nothing could tell you the menu was saying the wrong thing.

**The engineering side is out of scope even for the designer's own five areas.** It decides that a
prompt should read *"Locked"* rather than *"Press E"*; it does not decide which component carries the
string, which crate that component lives in, or when the system that writes it runs.

---

## 3. Where the designer is in the project, and why it will be quiet for now

**`games/warren` is nearly finished and its design already passed the critic** — `docs/11-the-warren.md`
took six critiques to get there, and its premise, world, palette, loop and level generator are all
built. `docs/13` §1b's budget for finishing it is two or three sessions and seven rows.

So the designer's remit on *this* game is narrow and Justin said so directly: *"it will do what it can
for now and do its full job later on in future projects."* Concretely:

- **Do not redesign the Warren.** Its design is settled.
- **Do make what exists mean more** — the words on screen, the first thirty seconds, the last ten,
  the framing of the three landmarks, the prompts, the title and the two endings. All of that lands
  inside rows **F1**, **F3** and **F5**, which are already in the plan.
- **Do not reopen a cut row** without saying what it displaces. The cut list is `docs/13` §1b.

**Its full job begins with `docs/05` M4b** — the first *published* game, a survival game in the
Project Zomboid line, isometric. That game has no design document, no fiction, no theme and no name.
`docs/12` §4 requires the design to pass before any code, which is exactly where this role is worth
the most: `docs/11` was written by the implementer and took six critiques to become good. The next
one should be written by somebody whose job it is.

---

## 4. What a useful piece of direction looks like

The failure mode is direction so general it cannot be built, and the second failure mode is direction
that is really an aesthetic preference wearing a reason.

- **Specific enough to build.** *"The ending should feel earned"* is worth nothing. *"The escape
  screen says YOU GOT OUT over a black fill; hold the last frame of the tunnel behind it, because the
  player's final image should be the place they escaped, not a card"* is worth a session.
- **The meaning first, then the instruction.** The reason is the durable half. An implementer who
  understands why gets the *next* decision right without asking.
- **Referenced.** *"Amnesia's notes are written by people who did not know they were about to die,
  which is why they are mundane"* beats *"the notes should feel real."*
- **Ranked, with a drop-first.** There is usually no time.
- **Marked as a decision or a suggestion.** A decision is binding. A suggestion is offered. Disguising
  either as the other is the thing that makes a second voice expensive.
- **Willing to say a thing is fine.** A support agent that finds a problem every time it is asked is
  one the project learns to stop asking.

---

## 5. The ledger

Append-only, like `docs/14` §8. Every consultation is recorded with what was asked, what was decided,
what was merely suggested, and — if it ever happens — how a disagreement with the critic resolved.


### Direction 1 — `games/warren`, session 26 — the first consultation

**Asked:** what the designer can do inside the seven FINISH rows, given the Warren's design is settled
and the budget is two or three sessions. Specifically the two endings, the section naming scheme, the
prompts, the title screen, and what the warden *is*.

**Its one-sentence diagnosis, and it is the reason the role exists:** *"This game has a fiction and an
interface, and they are not speaking to each other yet. The walls are an institution's; the words on
screen are a video game's."* Nothing it asked for reopens a cut row, needs a new asset type, or needs a
new font. Most of it is edited strings and paints.

#### The decisions, ranked as it ranked them

1. **The ending line comes off `Accent`, and so does the title heading.** `docs/11` §5a's one colour
   rule is that safety orange marks what you can act on and nothing else may have it. The ending line
   is the largest, loudest orange object in the game and it is **not actionable** — it unteaches
   twenty minutes of that rule at the moment the lesson ends. Both become `Ink`; `Accent` on that
   screen belongs to the focused button alone.
2. **`IT FOUND YOU` becomes `ACCOUNTED FOR`.** The old line is a narrator stating a fact you just
   watched, and its pronoun frames the warden as a monster that hunts. `docs/11` §3 is emphatic that
   it is not: it is **an institution still performing its function, and the function is counting.**
   `ACCOUNTED FOR` is that institution's own word for what has happened to you.
   **`YOU GOT OUT` stays and must not be "fixed" to match** — once its partner speaks in the
   institution's voice, the asymmetry is the point: *when you escape you get the last word; when you
   are caught, the shelter does.* It explicitly rejected `UNACCOUNTED FOR` for the escape as one
   prefix away from its partner.
3. **The two endings use the game's two typographic registers**, which `docs/11` §8 already names —
   the interface is signage, the pause menu is a form. **Escaped: the sign speaks** — `Title`,
   upper-left, over a light ~35% scrim so the frame you escaped through is readable behind it, held
   2 s. **Caught: the record speaks** — `Body` scale, low and left where a reference number goes on a
   form, over a deep ~75% scrim, held 3 s. *"A player knows which ending they got before they read a
   word."*
4. **The prompts become the label the shelter would have put on the object** — not the verb, never a
   sentence, and **uppercase**, since every other string in the interface already is.
   `Torch` → **`HAND LAMP`** (the mesh is already `hand_lamp`; only the word stayed a video-game
   torch). `Brass key` → **`KEY · T`** ("brass" is a material adjective and says nothing; the tag on a
   key board says what it opens — which turns the fetch into navigation). `Locked` →
   **`WAY OUT · SECURED`**. And **`Way out` is already right**: it is London Underground's own phrase
   for an exit, which is this game's exact typology.
5. **The warden's lamp must leave the fittings' colour family.** `warden_post.scene` had
   `colour 0.5 0.72 0.62` against every emergency fitting's `0.58 0.78 0.68` — **the same green, eight
   percent down.** `docs/11` §4 claims the lamp as the warden's ranged tell, *"you track it through a
   level by the light crossing a doorway two rooms away"*, and as written it was indistinguishable
   from a wall fitting at any distance. **Three light sources must mean three things: yours is warm,
   the building's is green, its is blue-white.** Called non-negotiable; the numbers left to the critic.
6. **The title screen carries both names**, closing a problem `docs/11` §10 leaves open — *"a title the
   world never uses is a title that belongs to the box art"*: `THE WARREN` over
   `SHELTER FOUR · SUB-SURFACE ARCHIVE`. One is what the staff called it, one is what the paperwork
   did, **and the gap between them is the premise, stated before the player has moved.** Options
   become a stencilled list with an orange caret rather than filled panels — *"a filled bar behind a
   word is what makes a menu look like a dialog box; a marked line in a list is what a sign looks
   like"* — which also satisfies F1(b) by construction.
7. **BEGIN drops you exactly where the title camera stood**, same position and yaw. One transform, and
   the opening stops being a cut from a card to a corridor.
8. **A section is a lettered *stretch*, not a cell.** `manhattan % 5` is a ring — it rises whichever
   way you walk and repeats every five cells, so it *cannot* say "further in". Fourteen cells becomes
   five stretches of about three, letters ascending along the spine, no letter twice.
9. **The spawn's four quarter-turns are a design rule, not a brightness target**: one working fitting
   ahead and only one thing to walk towards; a closed bulkhead behind, so the first decision is never
   a false choice; **and no light source placed in either lateral view.** F1(d)'s 36.7% reading is
   what breaks when that last rule is violated.
10. **The warden's constant channel is a tread, not a breath.** `warden_post.scene` loops
    `warden_breath` spatially, forever, in every state — *"a thing that breathes continuously reads as
    an animal, which is the one thing §3 says it is not"*, and a permanent sound from the antagonist
    is what makes §9's near-silence unhearable when §3a's most important tell is an **absence** of
    sound. Breath belongs to pursuit alone. Lands inside **F6**.
11. **The section names**, four of five taken from the real Clapham South deep shelter, which named its
    sub-shelters A–P after senior naval officers: **H HARDY · I INGLEFIELD · M MADDEN · O OLDHAM ·
    T TORRINGTON**. The five existing letter meshes are already alphabetical, so the letters cost
    nothing; nine new stencil glyphs do. **The gaps are deliberate and must not be explained** — a
    player who walks H → I → M concludes correctly that this place is far larger than the part they
    are in, which is the cheapest scale-building available.
12. **When it reaches you, it stops.** No lunge, no scream. It arrives, it stops, the ending follows —
    because it is not hunting you, it is counting, *"and the correct beat at the catch is an arrival."*
    Also the only catch this engine can stage convincingly, which it named as the sign the design fits.

**Its one suggestion rather than a decision, and its own drop-first:** waking on the deck — two seconds
of the camera rising from floor height before input is accepted, *"which explains why your first input
is the mouse"*. It flagged that this needs `amadeo-anim` wired into this game and that it could not
price that from where it sat.

#### What it ruled already right, and told us not to touch

`WAY OUT` as the exit label. `YOU GOT OUT`, **conditional on its partner changing**. The reticle's
colour logic — dim dot, `Accent` ticks that open on reach — *"the orange rule used exactly as
intended"*. The warm/cold split between the hand lamp and the building, *"the warden is the only thing
sitting in the wrong half of it"*. And `docs/11` §8's title-screen layout, which it confirmed rather
than replaced.

#### Its drop order, since the budget is fixed

14 (waking on the deck), then per-outcome button labels, then `KEY · T` falls back to `KEY`, then the
five names descope to three (**H HARDY · M MADDEN · O OLDHAM**, six glyphs). It was explicit that
**the names must not be dropped in favour of letters alone** — *"a letter alone is the thing §5.4
calls set dressing"* — and that items 1–7 are cheap enough that no version of the budget excludes them.

#### What was applied in session 26

**Decisions 5 and 10.**

**Decision 5** bears on **F2**, built in the same session: the warden's lamp left the fittings' colour
family. Engine gate review 29 ruled that this *"was never the designer's to grant"* — `docs/11` §4
already specifies *"a cold, narrow, downward beam, deliberately unlike the player's warm one"*, and the
shipped green flood met **neither word**, so what the designer identified was **a violation of a passed
design rather than a new direction**. It also corrected the implementation: the change was described as
a reduction and was not — peak channel went `11.0 × 0.72 = 7.92` **up** to `9.0 × 1.0`, which is why the
coat blew out. Renormalised on peak channel, and taken to a cyan-leaning white rather than blue on the
review's non-blocking note that `0.62 0.76 1.0` read as a modern LED work light.

**Decision 10** landed with **F6**: the warden **treads** rather than breathing until it sees you.
`warden_breath` had looped in every state forever, which reads as an animal where `docs/11` §3 says
institution, and which makes §9's near-silence unhearable when §3a's most important tell is an
*absence* of sound. A new `warden_tread` clip loops at range; the breath is pursuit only — so the
change of sound **is** the moment of being noticed, with nothing on screen having to say so.

Everything else is recorded here and lands in F1, F3 and F5, which are later in the critic's order.

**No disagreement with the critic arose.** Nothing above contradicts a review's ordered change, and
decision 1 in particular runs the same way as the critic's own F5(d), which requires that no
non-interactive surface out-saturate `accent`.


#### What direction 1 had produced by the end of session 26

Seven of its fourteen items are built. **Decision 5** (the warden's lamp out of the fittings' colour
family) landed with F2 and review 29 ruled it *"was never the designer's to grant"* — `docs/11` §4
already specified a cold narrow beam and the shipped green flood met neither word, so what the
designer identified was **a violation of a passed design rather than a new direction**.

**Decisions 1, 2 and 4** landed with F1: the ending line and the title heading off `Accent`,
`IT FOUND YOU` → **`ACCOUNTED FOR`**, and the prompts in the shelter's own voice — `HAND LAMP`,
`WAY OUT · SECURED`, and `WAY OUT` kept exactly as it was.

**Decision 3** landed as the two registers, and it is the one that changed the game most. Escaped
speaks as a sign — `Title`, high and left, over a 0.35 scrim, the tunnel measuring 4.44 mean adjacent
|ΔL| over a range of 65 through it. Caught speaks as a record — `Body`, low and left where a
reference number goes on a form, over a 0.78 scrim reading 2.07 over 31. **A player knows which
ending they got before they read a word.**

**Decisions 6 and 10** landed too: both names on the title screen (`THE WARREN` over `SHELTER FOUR ·
SUB-SURFACE ARCHIVE`), which closes `docs/11` §10's *"a title the world never uses belongs to the box
art"*; and the warden **treads** rather than breathing until it sees you, so the change of sound is
the moment of being noticed.

**Still to build:** decision 8's stretch rule is done but **decision 11's names are not** — nine
stencil glyphs, and the drop-first descope to three sections needs five. Decision 7 (BEGIN dropping
the player where the title camera stood), decision 12 (`KEY · T`), decision 13 (per-outcome button
labels) and the suggestion (waking on the deck) are all open.

**No disagreement with the critic has arisen yet.** Review 29 corrected the *implementation* of
decision 5 and endorsed the decision itself.

---

### Direction 2 — `games/warren`, session 27 — the warden's form, in silhouette

**Asked:** what the warden *is*, as a shape seen for one second in a dark tunnel at eight metres —
because F2 had failed twice on **form** rather than on craft, and both passes had been attempts to
satisfy a measurement rather than to build a thing. The brief offered it the exit that the honest
answer might be *less* rather than more: smaller, darker, seen only in fragments, on the reading that
`docs/11` §3's "never clearly seen" meant chasing a legible silhouette at all was the mistake.

**It refused that framing, and the refusal is the most useful thing in the direction:**

> *"A silhouette is by definition a legible outline you cannot resolve the surface of. What that
> sentence forbids is the surface — a face, a texture, a material read, a detail you could describe if
> you froze the frame. It does not forbid the outline; it **requires** one, because a shape you cannot
> resolve at all is not a silhouette, it is a smudge, and a smudge is what the game has now."*

**Decision 0, which governs every later judgement about this object: the player resolves the warden's
OUTLINE and never its SURFACE.** Review 29 measured the figure at 207–231 against lining at 30–90 and
called it clearly seen — that is a *surface* being seen, and the cause is that the figure is floodlit
by the lamp it carries. Anything that makes the outline more legible is correct; anything that makes
the surface more legible is wrong, **including a light you gave it yourself**.

#### What it is, in one sentence

> *"A shelter marshal on its round: too tall, in a caped oilskin that reaches the deck, under a tin hat
> with nothing under it, carrying a tally board in the crook of one arm and a masked lamp on a strap
> over the other shoulder."*

#### The eight decisions

1. **D1 — it is a plate, not a post.** Width : depth **>= 1.9 : 1** at the shoulders and **>= 1.6 : 1**
   at the hem, and **no part of the body may be a solid of revolution** — `Cylinder` is allowed for the
   hat and for carried objects and nowhere else. This is the half of review 30's verdict nobody had
   named: *"a solid of revolution is as wide as it is deep, which means it presents the same shape from
   every angle — that is what bollard means, and it is a stronger machine-made tell than the missing
   shoulder edge."* **And it told us to design the patrol to use the consequence:** a figure 0.74 m wide
   and 0.30 m deep **narrows to almost nothing when it turns to face you**, so F2b's turn-to-face
   becomes a dramatic beat rather than a correctness fix.
2. **D2 — 2.15 m, hem on the deck, no legs and no feet.** Two problems, one answer: this engine has no
   walk cycle, and **a garment that pools on the floor has no feet to fail to move**. Height is the
   cheapest "this is not a person" signal there is and cannot read as a modelling error.
3. **D3 — a caped, belted oilskin, not a greatcoat**, because a greatcoat's outline is a tube. The
   police/ARP/railway pattern's *un-designed* silhouette is flat shoulder top -> cape hem -> belted
   waist -> skirt flaring to the deck, which is (a3)'s two bands **arriving from a real garment rather
   than applied to one**. Shoulder **0.74**, waist **0.40**, hem **0.88**, depth <= 0.38 — **1.85x and
   2.20x** against a 1.35 bar, over-built on purpose because the last two passes cleared it on the model
   and lost it to projection.
4. **D4 — three hard steps and no more: the brim, the cape hem, the top of the lamp.** Everything else
   slopes; *"cloth does not have shelves in it."*
5. **D5 — a tin hat**, ruling out the hood by name as *"the single most exhausted silhouette in
   horror"* and because it would throw away the one thing this design has, which is that the threat is
   **administrative**. Brim 0.58–0.62 m, on the same `shadow` material as the void, so helmet and
   absence are one unbroken dark mass.
6. **D6 — the landmark feature is the tally board, and counting must be visible.** Its argument is a
   story beat rather than a shape: the player has already seen chalked tally boards on the walls, so a
   figure carrying *the same object* lands the premise with no words — **the boards are not scenery,
   something is still filling them in.** It also noted that a figure with a clipboard is memorable
   precisely because it is bureaucratic, where a hooded figure with a lantern is every game ever made.
   *"The sentence a player says to a friend is: the tall one in the tin hat that carries a board."*
7. **D7 — it has one arm, and that is where the asymmetry lives.** One arm out holding the board, the
   other under the cape, *"which is what a cape is for"* — so the two sides are not one object mirrored.
   The enclosed negative space Playdead's rule wants comes free: a **0.30 x 0.22 m** hole between
   forearm, board and coat, ~28 x 21 px at eight metres. Plus the lamp shoulder dropped 9 cm and a 6–8°
   roll, so the figure carries its own weight rather than standing to attention.
8. **D8 — the lamp stops lighting its carrier.** `bulb.range` 2.0 -> ~0.35 and `glow` pitched to ~-30°.
   Blackout regulations required a warden's torch to be **masked**; the period rule and the design rule
   are the same rule, which is the sign it is right.

#### Two things worth keeping beyond this row

**It pre-empted the pathology that produced both previous failures.** It expects clause (d)'s "at most
three abrupt width changes" to fail on exactly the brim, the shoulder line, the cape hem and the lamp,
and instructed that the figure be **submitted with those four steps named** rather than having its
legible features deleted to satisfy a legibility measurement — *"the critic's clause to relax or hold,
not yours and not mine."* That is the correct division, and it is `docs/14` §6's rule read from the
other side.

**And it named what not to touch:** the lamp on a strap, the void under the brim, `coat_wool` itself —
*"the brightness problem was never the material; it was D8's light. Do not darken the coat to chase a
number the lamp is causing"* — and the lamp's cyan-leaning white, whose *reach* is all that changes.

#### Its drop order

D1, D2, D5 and D3 never. Then the **arm** falls back to the board slung flat on a neck cord standing
0.16 m proud (*"keeps the landmark, loses the hole — do not drop the board"*); then D7's lean; then D8;
and a helmet tilted 8° askew is offered as a suggestion rather than a decision and is dropped first.

#### Its one suggestion the implementer could not price

**S1: the frame submitted for F2 should be the frame the design exists to produce** — `docs/11` §4's
*"the silhouette appears at the pool's edge, passes through, and is gone"* — with a lit fitting behind
the warden and the figure between camera and pool, rather than the figure photographed against unlit
lining with its own lamp on its coat.

#### What the implementer verified before building on it

Two load-bearing repository claims, checked rather than taken. **`shadow` is used by nothing outside
`warden_post.scene`** — confirmed, so the helmet can share it without contaminating F2's matte
instrument. **The lamp floods the coat** — confirmed and *stronger* than stated: the direction
estimated 0.5 m, and `bulb` sits at local `0.44` against a coat radius of ~0.28, so it is **~0.16 m off
the coat surface** at range 2.0. No correction was needed; the conclusion was understated.

**No disagreement with the critic has arisen.** Review 32, running concurrently on other rows, **agreed
with this office** on the caret: it refused the implementer's request to amend F1's clause (b) and
recorded that it was *"agreeing with the designer here, not overruling it"* — direction 1's decision 6
had given the answer and the implementer had shipped the filled bar anyway. That decision is built now.

---

### Direction 3 — `games/warren`, session 27 — what lights a wall

**Asked:** engine gate review 34 failed the warden's silhouette because *"there is no fixture in the
frame at all"*, and four measured placements of the existing fitting could not reach its 35-level
separation bar. The cause is structural: **`room_lamp`'s spot is pitched −52°, so every fixture in
this game pools on the deck** and nothing lights a vertical surface at standing height. I asked what
a wall light is *in this place*, rather than invent one and have it read as a spotlight in a bunker.

**Its answer, and it is research rather than invention:**

> **The thing that lights a wall in this place is the fitting that was never switched off — and it is
> not a new invention, it is the fitting the real building had, on the circuit the real building ran
> it on.**

Clapham South's tunnels were lit by cast-iron bulkhead fittings **bolted into the tunnelling rings**,
and the operational fact that decides everything: **every fourth fitting stayed lit through the night
so shelterers could find the lavatories.** So the shelter had **two** lighting systems, and this game
had only one.

| | what it is | what it lights | circuit |
|---|---|---|---|
| `room_lamp` | the **working** light | the **deck**, so you can do a job on it | emergency, off until an isolator is thrown |
| **the night light** | the light left on **for people asleep** | the **lining**, so you can find your way | the standby ring, always live, never switched |

`docs/11` §4 had already established the standby ring — *"the panels are on the standby ring and have
always been live"* — so this is not a second system needing a justification. **It is the first one,
and the emergency circuit is the other.**

#### The decisions

1. **D1 — it is on the standby ring, so `light_the_sections` does not touch it.** A section that
   flooded or collapsed still has its night lights. *"This is not a workaround for the warden's dead
   section — it fixes a real design fault"*: `docs/11` §6 forbids pitch black, and a section with no
   light source cannot be navigated, cannot show a silhouette, and cannot show the player the dark
   fittings an isolator would bring up.
2. **D2 — the sentence to keep when the numbers change:** *"The emergency circuit lights the floor so
   you can work. The night circuit lights the wall so you can find your way."* Mounted at ~2.25 m,
   raking up and along the iron, dying inside ~1.8 m so it is a **patch and not room lighting**.
   **Do not re-aim `room_lamp`** — its deck pool is load-bearing in three places, including §4's
   *"the silhouette appears at the pool's edge, passes through, and is gone"*. It also noted a payoff
   nothing to do with the warden: §5a calls the lining's normal map *"the engine's most under-used
   feature"*, and **a light at −52° on a vertical wall shows a normal map nothing.** Grazing light is
   what makes cast iron read as cast iron.
3. **D3 — it replaces a fitting position rather than adding one.** Every fourth *fitting* on the
   standby ring, which at this generator's two per cell is **one night light every second cell**, so
   a visible run holds the same number of fixtures it always did and §6's light budget is untouched.
4. **D4 — the building's green, exactly `0.58 0.78 0.68`, and no hue of its own.** Three light
   sources must still mean three things. The player separates this from the reflector by **shape** —
   a soft vertical patch against a hard ellipse on the deck — *"a stronger and cheaper distinction
   than hue, and it does not cost you the one colour rule the game runs on."*
5. **D5 — the source sits behind a cast guard and never reads as a bright blob.** Ribs break the
   glass into small pieces, *"which is why this fixture can sit in frame without producing the
   clipping reviews 33 and 34 spent two passes killing."* Put it **in** the F2 frame: a lit wall with
   no visible cause is a photographic light, not a place.
6. **D6 — the warden's post is guaranteed one, and the tally board goes with it.** A story beat
   rather than a placement: **you cannot chalk a number in the dark.** §3a has the boards still being
   kept up to date and a check sounding like chalk on a board — *"the board is beside a night light
   because the count has to be readable at night, and the warden stands there because that is where
   the board is."*

**On whether the critic's 35-level condition asks for the wrong picture, which I offered it: no.**
*"Backlighting is the only lighting that gives you an outline without a surface — every front-lit
alternative makes the surface more legible, which is the thing I forbade."* It upheld the critic and
sent nothing to arbitration. Its one refinement: **the lit thing behind the warden should be a
specific readable object with a visible fixture, not a lit wall** — *"a uniformly lit wall behind a
figure is a studio; a lit board with a shape in front of it is a scene."*

#### What the fixture says, which is why it earns its place beyond the measurement

> *"Every fourth light was left on all night so that people could find the lavatories. Nobody ever
> turned them off. They are still on."*

The building is still running its night routine forty years after the last person left — **the
warden's own sentence, stated by the architecture before the warden arrives.** And it splits the two
circuits along a cold line: the light you switch on to do a job, and the light left on for the people
who are asleep. **The player gets control of the first and never of the second.**

#### Build order and drop order

Build **D2, D1, D6, D3, D5, D4**. Drop first the junction plates (a suggestion, not a decision); then
the every-second-cell rule, falling back to one at each tally board; then **warden's post only**,
which it accepts *for one review* and says **must not ship that way**, because a fixture existing in
exactly one place is set dressing. **Never drop:** that it lights the **wall** and not the deck, and
that it is on the **always-live** circuit. *"Those two clauses are the entire answer; everything else
is placement."*

#### What session 27 built from it

**D1, D2, D3, D4, D5 and D6.** `night_light.scene` — a ribbed cast bulkhead with an emissive glass
behind the ribs and a spot grazing along the lining, in the building's green, on the standby ring —
placed **every second cell** and **guaranteed at the warden's post**, where it stands on the warden's
own wall behind the figure. Its naming note is taken: the file is `night_light` because *bulkhead* is
already spent twice in this project, on the lead-grey material and on `HEAD_PIECE`.

**Measured: separation medians 5 / −7 (r34) → 12.7 / 11.8 (post lamp) → 16.4 / 14.3 (night light),
against a bar of 35.** The direction is right and the numbers are still short; what the measurement
says is that the figure stands 0.9 m off its wall and the camera at 4.0 m sees mostly deck and far
tunnel behind it, so a near-wall patch is not yet *behind* the silhouette. **Its own refinement is
the untried lead** — a specific lit object behind the figure rather than a lit wall, which is D6's
tally board placed where the camera sees it past the warden.

**No disagreement with the critic arose; it upheld the 35-level condition explicitly.**
