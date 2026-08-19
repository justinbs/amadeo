# 14 — The critic: standing brief, procedure, and ledger

> **This document is for two readers and nobody else: the critic agent, and Justin.**
>
> The critic reads §§1–5 at the start of every review, cold, before it looks at anything. Justin
> reads §6 to see what has been ruled and what it cost. Whoever is implementing reads §4 to find out
> what it is allowed to write in a status column, and otherwise stays out of this file.
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
factually wrong about the repository it is corrected with evidence — which has happened once, and it
verified and withdrew. Both halves of that are load-bearing: take it seriously, and check it.

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

---

## 5. Tools

Captures, without editing the game:

```bash
cargo run -p amadeo-cli -- capture -p atrium --ticks 5 --width 1920 --height 1080 out.png
```

`--pitch <deg>` and `--yaw <deg>` aim every camera before drawing and put them back. Review 10 called
this the most useful thing built for the critic's job since capture itself. The games are `atrium`,
`warren`, `scarp`, `vault`, `quad-demo`.

Reading a capture at the pixel level:

```bash
cargo run -p amadeo-cli -- image probe out.png 512 260 470 280
cargo run -p amadeo-cli -- image row out.png 600 300 900
cargo run -p amadeo-cli -- image col out.png 1017 200 400
cargo run -p amadeo-cli -- image crop out.png 480 250 80 60 5 crop.png
cargo run -p amadeo-cli -- image stats out.png
```

`probe` reads named pixels, `row` and `col` print a scanline as `x r g b`, `crop` magnifies a
rectangle by an integer factor with no filtering — so a magnified crop shows the real texels rather
than an interpolation of them — and `stats` gives a luminance histogram, which is how "65% of this
frame is one gradient" gets said with a number.

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

## 7. The ledger

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
