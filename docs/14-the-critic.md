# 14 — The critic: standing brief, procedure, and ledger

> **This document is for two readers and nobody else: the critic agent, and Justin.**
>
> The critic reads **§§1–7** at the start of every review, cold, before it looks at anything —
> §7 is Justin's standing instruction in his own words, and it is the standard the rest of the file
> serves. Justin reads **§7 and §8** to see what has been ruled and what it cost. Whoever is
> implementing reads **§6** to find out what it may write in a status column, and otherwise stays
> out of this file.
>
> It exists because `.claude/agents/critic.md` states the critic's *taste* and nothing states its
> *procedure*, so eleven reviews were each as good as one cold agent's improvisation on the day.
> Review 12 asked for this file by name and specified most of what is in it.

---

## 1. Why there is a critic at all

In session 20 Justin looked at `games/warren` — five sessions of green tests, complete
documentation, and a milestone marked done — and called it a bland engine test. He was specific:
rooms "literally straightly connected", "no life, colour, creativity", a menu that "sucks", and an
objective that was "a simpleton's idea of a game". All four were correct.

Nothing in the project caught any of it. The tests passed, the invariants held, the ADRs were
sound. **The engine's own quality machinery cannot see a picture**, and the author of a system is
the worst available judge of whether it is worth showing to anyone.

So the critic is not a linter and not a second opinion. It is the only mechanism in this project
that looks at the thing a *player* meets and is allowed to say no.

**Its verdict is binding** (`docs/12` §4). Where it disagrees, its changes are followed. Where it is
factually wrong about the repository it is corrected with evidence — which has happened twice, and
both times it verified and withdrew. Both halves of that are load-bearing: take it seriously, and
check it.

---

## 2. The bar, in one paragraph

`docs/12-the-bar.md` is the full statement and it is short; read it. In one line: **AA indie, Hello
Games as the reference, and the question is whether Justin would put this frame on a screen in front
of a thousand people at a developers' conference.** "It works", "it is tested" and "it is
documented" are answered by other reviews and are not what is being asked. Passable is explicitly
not the bar.

---

## 3. Required evidence, before any verdict

**A verdict given without these is not a verdict, it is an impression.** Every one of these exists
because a review that skipped it reached a wrong or unactionable conclusion.

| # | Required | Why it is here |
|---|---|---|
| 1 | **Seven frames minimum per game**: the authored camera, yaw 0 / 90 / 180 / 270, pitch +30 and −30 | Item 12g — three of four quadrants of the Atrium were empty, and eight reviews of the authored view alone never saw it. `--yaw` and `--pitch` exist for this |
| 2 | **At least one capture at 1920 × 1080** | Half of review 12's findings — normal-map mip aliasing, the shadow staircase, the absence of contact darkening — are invisible at the 1280 × 720 default |
| 3 | **Three magnified crops before ruling on any surface claim**, with the crop rectangle stated | So the next reviewer can retake the identical crop and compare. A surface claim from a full frame is a guess |
| 4 | **A pixel probe for every quantitative claim** | "The lamp does not light the floor" is arguable. "Row y=600 reads 198/200/202/211 monotonically past the lamp base, with no local peak" is not. Use `amadeo image` (§5) |
| 5 | **Capture every game, not only the one under discussion** | Review 12's most consequential finding — that `games/warren` had not changed one pixel in nine reviews — came from capturing a game nobody had mentioned |
| 6 | **Recount the tracked measurements in `docs/13` §1 yourself** | The file that exists to stop the bar drifting had itself drifted: it claimed 23 of 31 box meshes when the true count was 24 of 37 |
| 7 | **Re-derive at least one authored number from the shader that consumes it** | Review 12 disproved "depth reads across the room" by evaluating the fog term: `1 − exp(−(0.016 × 17)²) = 0.071`, a 7% haze. No amount of squinting settles that |

---

## 4. Warnings — things this repository has already fooled a reviewer with

Read these before forming any theory. Each one cost a real review a real mistake.

1. **A green test suite says nothing about a picture.** Every quad the voxel mesher emitted was wound
   against its own normal for two sessions — every surface inside out — while the suite stayed green,
   because the tests checked normals (always correct) and nothing had ever drawn one.

2. **A knob feeding two consumers cannot be diagnosed by moving it.** Session 21 "proved" a dark band
   was the sky by recolouring the sky map magenta and watching the band go magenta. The map feeds the
   backdrop *and* the ambient fill, so the experiment could not distinguish them and returned a
   confident wrong answer. **Split the variable, then move one half.**

3. **The Atrium is a demo; it is not a game.** `docs/13` §3's POLISHED condition says *a frame from a
   real game*. `games/atrium` has no premise, no objective and no fiction, and an Atrium frame
   therefore cannot satisfy that condition however good it looks. The only game here with a passed
   design is `games/warren`.

4. **Look for the light with no fixture, and the fixture with no light.** Three of five lights in the
   Atrium had no `Mesh` and were invisible in the scene file, because the defect is a component that
   is *absent*. Grep for entities carrying a `PointLight` or `SpotLight` and no `Mesh`, and the
   inverse, rather than reading the file top to bottom.

5. **A capture at tick 1 is a weaker picture than it looks.** ADR 0072 closed the worst of this by
   composing the hierarchy at load, but `--ticks 5` is still the floor for anything about lighting or
   placement, and `--ticks 1` catches only "does it load".

6. **`--yaw` is positive-left.** Stated once here so nobody burns three captures on the sign again.

7. **When a capture disagrees with your model of the code, believe the capture.** Session 21 produced
   three confident wrong diagnoses in one week — the dark band, the ceiling seam, and the glass — and
   every one was settled by measuring an image rather than by reasoning about a shader.

8. **You have been factually wrong once, and you withdrew on evidence.** That withdrawal is what
   makes the verdict binding. Keep checking yourself; a review containing no self-correction has
   probably not checked itself. (Twice now — review 13 withdrew review 12's claim about
   `lamp_fitting` on a controlled A/B, which is the mechanism working rather than a failure.)

9. **An authored value may be dead data, because a clip is writing the same field.** `atrium.scene`
   authored a `PointLight::intensity` and `lamp_flicker.anim` drove the same field from tick 1, so
   the scene's number had no effect whatever — a whole commit's fix changed nothing and the capture
   was byte-identical. Nothing reports this: not `amadeo check`, not a test, not reading the scene
   top to bottom. **Before believing a number in a scene file, grep the `.anim` files for the
   component and field it belongs to.**

10. **A prefab-instance override silently discards a root offset.** ADR 0029 makes an override
    *replace* the root's component, so a piece that authors its height on its own root loses it the
    moment a generator places it. `wall.scene` was the one piece in `games/warren` that did — every
    generated wall was half underground and the top 1.5 m of every room was open to the sky map, in
    the shipped level, for three sessions. Invisible to `amadeo check`, to `amadeo fmt` and to the
    whole suite, because the collider sinks with the mesh and a 1.5 m wall still stops a 1.9 m
    capsule. The generator's own `PLAYER_STAND` doc comment states the rule this violated. **Check a
    piece's root translation against what the generator writes over it.** Found and verified in
    review 14; `nothing_in_the_level_is_buried_and_every_wall_reaches_the_crown` is the geometric
    test that now catches it without knowing the mechanism.

11. **A `capture --from <snapshot>` does not see an edit to a scene or a piece, so every A/B on
    `games/warren` through that route is invalid unless the snapshot is rebuilt.** A snapshot
    restores *components* (ADR 0028), so a `PointLight` in `playing.snapshot` carries the intensity
    it had when the snapshot was taken; editing `room_lamp.scene` and re-capturing changes nothing,
    and the diff comes back **byte-identical for a reason that has nothing to do with the light**.
    This is the review procedure's own §4 #2 turned on the reviewer: the experiment could not
    distinguish "contributes nothing" from "was never applied".

    Session 23 produced two confident wrong findings this way in ten minutes — that the Warren's
    fittings and its `spill` directional were both dead data — and both were false. Rebuilt properly
    (`cargo run -p warren --bin moment` between the edit and the capture), the fittings move **73% of
    pixels by up to 234 levels** and `spill` moves **12.5% by up to 52**.

    **Editing the snapshot text instead does not work either, and fails loudly**, which is the one
    piece of luck here: `restore` checks the recorded state hash and refuses. Materials, meshes and
    the `Environment` *are* re-read from disk, so an A/B on those is valid through `--from`. The rule
    is: **anything that is a component needs the snapshot rebuilt; anything that is an asset does
    not.**

---

## 5. Tools

Captures, without editing the game:

```bash
cargo run -p amadeo-cli -- capture -p atrium --ticks 5 --width 1920 --height 1080 out.png
```

`--pitch <deg>` and `--yaw <deg>` aim every camera before drawing and put them back. Review 10 called
this the most useful thing built for the critic's job since capture itself. The games are `atrium`,
`warren`, `scarp`, `vault`, `quad-demo`.

**`games/warren` boots into its title screen and a capture flag cannot dismiss a menu**, so getting
a frame of it *in play* takes one more argument:

```bash
cargo run -p amadeo-cli -- capture -p warren --from games/warren/snapshots/playing.snapshot --ticks 5 out.png
```

Review 13 found that without this **every frame any reviewer had ever taken of that game was its
main menu**, while §3 of `docs/13` defines POLISHED as *a frame from a real game*. The snapshot is
committed text and is regenerated by `cargo run -p warren --bin moment`; run that if a component has
gained a field and the restore starts reporting defaults.

Reading a capture at the pixel level:

```bash
cargo run -p amadeo-cli -- image probe out.png 512 260 470 280
cargo run -p amadeo-cli -- image row out.png 600 300 900
cargo run -p amadeo-cli -- image col out.png 1017 200 400
cargo run -p amadeo-cli -- image crop out.png 480 250 80 60 5 crop.png
cargo run -p amadeo-cli -- image stats out.png
cargo run -p amadeo-cli -- image stats out.png 400 150 200 300
cargo run -p amadeo-cli -- image diff before.png after.png
```

`probe` reads named pixels, and `row` and `col` print a scanline as `x r g b luma`.

`crop` magnifies a rectangle by an integer factor **with no filtering** — so it shows the real texels
rather than an interpolation of them, which matters because the defects worth finding are *made of*
the texel grid.

`stats` gives a luminance histogram and a **clipped-pixel count** (any channel ≥ 254), over the whole
frame or over one rectangle. The clipped count is the figure that says "this highlight has no detail
left in it": review 13 found a lamp erasing a stone pillar over a 200-row run, and said this number
would have surfaced it in the first command rather than the twelfth.

`diff` compares two captures and reports the changed-pixel count, the largest difference and where it
is, and **a bounding box of everything that moved**. §4 #2 tells you to split a variable and move one
half, which makes this the operation that follows almost every experiment — it was hand-rolled six
times in one review before it existed.

**Write captures to the scratchpad, never into the repository.**

---

## 6. How a verdict is recorded

The mechanism that failed in session 21 is visible in `docs/13` item 13b: it was marked ✅ **done**
with neither half of its close condition met. Nobody lied — the work landed, the file was updated in
good faith, and no capture was ever taken. Four rules close that hole.

1. **Only a review may write ✅.** Implementation writes 🟡 **built, awaiting verdict**. `docs/13`
   already did this correctly once, for item 10, and said why: *the file must not claim a verdict
   nobody gave.* That is now the rule rather than the exception.

2. **Every ✅ carries the review number and its evidence.** `✅ r12 — c_wall_lit.png, joints read
   +18/−22 either side` beats `✅ done (s21)`. A closed item that names no artefact cannot be
   re-checked, which means it cannot be trusted after the conversation that closed it ends.

3. **A reopened item keeps its history.** `⬜ (reopened r12 — bloom halo 3 px, fog 7% at the far
   wall)`, never a silent revert to open. An item that has failed once is not the same as an item
   that has never been tried.

4. **Verdicts are appended, never edited.** `docs/13` §4 is append-only, and §7 below is too.

**And a close condition must be falsifiable.** *"A capture shows the lamp blooming"* is satisfied by
a 15-level lift over 3 pixels and by an 80-level lift over 40, and only one of those was wanted.
**A close condition names a number, a crop, or a pair of images that must differ.** Reviews 3 and 11
both had to rewrite conditions mid-review; that is the tell, and rewriting one is a legitimate
review output.

**A fix proposed by review N is closed by review N+1 or later, on a fresh capture.** Item 12f part
one was proposed, built and closed on a mechanism the proposing review had named. It happened to be
right. It is also exactly the loop that produces a false POLISHED, and the rule costs nothing.

---

## 7. What Justin has instructed, in his own terms

Kept here because the critic starts cold and because an instruction that lives in a conversation is
one that quietly lapses — the same argument `docs/12` makes about the bar itself.

**On what the engine is being held to:**

> *"You the agent, are supposed to make sure this project is a game engine that is enough for a AA
> Indie studio the likes of Hello Games the creator of No Man's Sky as well as Project Zomboid and
> their studios. That's the level that passes, imagine you are presenting this engine to a game
> developer conference."*

**On the demo games:**

> *"The demo games are supposed to be demo games the likes of UE5's Demo Game that isn't just a
> showcase of a character, it feels like a game, and is actually a game. Obviously this won't be the
> next UE5 but AA capability and level at the VERY LEAST."*

**On the critic's authority, stated to the implementer:**

> *"Only and only if the agent passes different portions of the engine that you're working on will
> you move past that. You're not in charge of passing things, its the agent's… The Agent will
> dictate whether progress is sufficient, whether planning is enough, whether implementation is
> successful, not you."*

> *"Do not pass unless it reaches AA level."*

**And on this document:**

> *"From now on there should be an agent documentation that's JUST FOR THE AGENT and MYSELF."*

That is what this file is.

**One thing the implementer owes the critic, which review 13 had to point out:** do not edit game
content while a review is in flight. It opened its report by noting the working tree was not clean
and declining to credit the work it found there. That is correct, and it is the implementer's fault
when it happens.

---

## 8. The ledger

Engine-gate reviews 1–11 are recorded in `docs/13-the-engine-gate.md` §4, where they were written;
they stay there rather than being moved, because §6 rule 4 says verdicts are not edited. Reviews
from 12 on are recorded here, newest last.

### Review 12 — engine gate — `f1d7e28` — **NOT POLISHED**

The first review after the project changed hands between sessions. Seventeen captures across three
games, probed at the pixel level.

**What it passed:** item 13a (the stone and slate maps — joints measurably recess, per-slab tone is
flat across a slab and discontinuous at the joint) and the shadow front-face-culling fix of `c5697e7`
(the ceiling junction falls 103 → 40 in three pixels with no band, closing the leak that occupied
reviews 9–11). It also closed item 12h's original condition.

**What it found that no test could have:**

- **The engine has no ambient occlusion of any kind**, and `mesh.wgsl` samples the ORM texture's
  `packed.g` and `packed.b` while discarding `packed.r`, which is glTF's occlusion channel. Every
  contact in every frame — pillar to floor, table leg to floor, wall to wall — has zero darkening,
  and that is the largest single reason the output reads as composited rather than rendered.
- **Three of five lights in the showcase scene have no fixture**, one of which blows a pillar to
  clipped white from empty air 1.4 m away. Item 14's second clause, failing after a whole session of
  polish aimed at it.
- **`games/warren` had not changed by one pixel in nine reviews** — 13 of 13 box meshes, 6 of 6
  untextured materials, exactly as review 1 found it in session 20. Every hour of Phase C landed in
  `games/atrium`, a demo, because Phase C's preamble routed it there while the Warren's design was
  unpassed. **That design passed three sessions ago and nothing re-routed the work.**
- **Bloom and fog are authored at values that do nothing** — a 3-pixel halo, and a maximum 12% haze
  in a 20 m room — while `docs/13` recorded item 13b as done.
- `c5697e7`, a commit about shadow-pass culling, silently carried `exposure 1.0 → 0.8` and
  `sky_ambient 0.68 → 0.25`, voiding the evidence `docs/13` records for item 12c.

**Its ruling on the gate order** (asked because `STATUS.md` had it open): keep the order — engine
before game is right and Phase A proved it by finding five defects no game would have surfaced — but
**change the vehicle**. Every remaining Phase C item lands in `games/warren`, and `docs/13` §2 gains a
**"lands in"** column so nothing can drift back to the demo.

Full report and its thirteen ranked findings are the basis of `docs/13`'s current Phase C.

### Review 13 — engine gate — `f68f19a` — **NOT POLISHED**

Nine captures of `games/atrium`, five of `games/warren`, one of `games/scarp`; six magnified crops
with stated rectangles; eleven probes; **five controlled A/B captures**; the tracked measurements
recounted; the occlusion shader read and its authored numbers re-derived.

**What it closed:** this document and `amadeo image` (§3 #6 caught the mesh count drifting again,
24/37 → 24/39, and §4 #2 is what made the reviewer distrust its own experiment), and item 13's
**anti-lattice clause** — measured joint positions per course, closest approach between adjacent
courses 20 px, no two sharing a line.

**What it withdrew.** Review 12's claim that `lamp_fitting` produced no pool. A controlled A/B — the
same framing with `intensity 6.0 → 0.0` — puts the lamp at +53 levels at one probe and +47 at
another, with a genuine local maximum. Review 12 probed a row that missed it. **Both halves of §4 #8
in one review: it checked itself, and it said so.**

**The finding that matters most, and it is a new defect shape.** Item 14's fix was **dead data**.
`atrium.scene` said `intensity 16.0` and `lamp_flicker.anim` drove the same field through 22.0 /
19.5 / 23.5 / 20.5 / 22.0 from tick 1. Proof: setting the *scene* value to zero produced a capture
**byte-identical** to the unmodified one; zeroing the *clip's* keys changed a hundred pixels a row.
So a whole commit changed nothing, and the pendant went on clipping 103 pixels of one row to 254 and
erasing a stone pillar's masonry 1.42 m away.

> **An authored field silently overridden by an animation clip is invisible to `amadeo check`, to the
> tests, and to reading the scene file top to bottom.** It is warning §4 #4's shape — a defect made
> of what is *not* in front of you — one field along. Added to §4 as #9.

**What it kept open:** item 21 (the occlusion pass moves 0–3 levels at every contact it names and
30–35 at silhouettes, as a banded comb the shader's own comment predicts), item 13's per-slab
variation (6% of range against a wanted 25–40%), and item 14's `gloom.rs` seam.

**What it ruled on:** the vestibule's fixtureless daylight is **legitimate**, on evidence — the
chamber reads as a place with a real light distribution, and *"the rule 'no light without a `Mesh`'
is a heuristic for the defect; the defect is light with no readable cause"*.

**What it asked for that nobody had thought of:** *there is no way to capture `games/warren` in its
playable state*. Every frame any reviewer can take of it is its title screen, while `docs/13` §3
defines POLISHED as a frame from a real game. That is now item 31 and it blocks the gate's own
condition.

### Review 14 — engine gate — `8c81dca` — **planning review** — **NOT POLISHED**

The first review of a *plan* rather than of built work, taken before any code was written and with
the working tree verified clean. Seven frames of `games/warren` through the snapshot route, three
magnified crops, eleven probes, the tracked measurements recounted from the filesystem, and one
authored number re-derived from `Mat4::perspective`.

**What it settled, so it does not have to be guessed again:**

- **Room-becomes-tunnel is the right reading of item 24**, and the cheap alternative — an arched lid
  over the existing 12 m rooms — is refuted by arithmetic rather than by taste. `ArchMesh` takes its
  segmental branch at `width 12, height 3`, giving radius 7.5 m and headroom
  `−4.5 + √(56.25 − x²)`; that falls below the player's 1.95 m beyond `|x| = 3.93`, so **34% of every
  room's floor would be unwalkable**. It is also a Nissen hut, which is a surface shelter, where
  `docs/11` §2 is about a bored deep-level tunnel.
- **The slice is judged separately**, upholding `docs/11` §10 item 1 — but cut down to four artefacts
  and three sentences rather than a full §3 pass.

**What it prescribed as a number rather than as a construction:** the crown must not exceed about
**3.4 m** above the deck. The plan's 4.7 m — forced by its own "openings only in flat wall" rule,
since a semicircular crown makes `crown = springing + width/2` — is a running tunnel rather than a
shelter deck, hangs the fittings 4.5 m up where they wash instead of pooling, and builds
claustrophobia out of a space you could drive a lorry through.

**What it verified independently and strengthened.** The half-sunk-wall defect: confirmed on the
file, on the pixels, and by re-deriving the screen offset from the projection matrix
(`360 · cot 35° · Δy/d = 514.1 · Δy/d`, giving −7.7 px for a 1.5 m wall top against +120.8 px for a
3.0 m one, measured −8). It added three things the report had not: `doorway.scene` does **not** have
the bug, so the level's wall heights were *mixed*; the sky band is **16.7% of the frame at mean
103.5** against a whole-frame 67.8; and it reaches the authored 1920 × 1080 frame item 24 is defined
on. It also rejected the proposed structural test — "no placeable piece has a non-zero root
translation" fails immediately on `player_start` and `warden_post`, both deliberate — and specified a
geometric one instead.

**Ten ordered changes**, of which the first four are: add `docs/11` §5.2's **section conditions** to
the plan, or fourteen identical bores read as *more* machine-made than the fourteen rooms they
replace; bring the crown down; put one legible institutional landmark in the frame, because item 24's
*"a prop that implies somebody was here"* is better served by a sign than by furniture; and change
the warden from a limbed humanoid — *"strictly worse than the box it replaces"*, because limbs
promise motion the engine cannot deliver — to a non-articulated greatcoat silhouette with its own
lamp.

Its full ten are the basis of what session 23 built; `docs/13` item 24 records which of them landed.
