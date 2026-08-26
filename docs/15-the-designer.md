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

**Decision 5 only**, because it bears on **F2**, which was being built in the same session: the
warden's lamp is `0.62 0.76 1.0` at a 21° outer cone against the fittings' green. Everything else is
recorded here and lands in F1, F3, F5 and F6, which are later in the critic's order.

**No disagreement with the critic arose.** Nothing above contradicts a review's ordered change, and
decision 1 in particular runs the same way as the critic's own F5(d), which requires that no
non-interactive surface out-saturate `accent`.

