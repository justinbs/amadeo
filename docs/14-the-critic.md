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

12. **A byte-identical frame across several authored positions means the object is behind something,
    not that the field is unread.** Moving a fully *occluded* object in x or y changes nothing at all,
    so the capture cannot tell you the edit landed. The Warren's exit lamp hid for two sessions this
    way: three positions, three identical frames, and the implementer concluded the component was not
    being applied. Review 23 found it by making the material emissive and diffing — **917 px changed
    in a 90 × 22 box**, which was its two bolt heads and nothing else — then projected the fixture
    from the snapshot's own camera and found every part of it behind the bulkhead plate. **If an A/B
    is byte-identical, change the variable to something unmissable before concluding anything.**

13. **A light at the eye casts no shadow the eye can see.** Three pieces of relief were built on the
    Warren's door in one pass — a 100 mm proud leaf, an 85 mm handwheel, six dogs — and every one read
    as an engraving, because the fixture over the door was aimed at the floor 2.3 m in front of it and
    the player's head-mounted torch was doing all the lighting. **Before judging that modelled relief
    is flat, find out what is lighting it**; the cone term in `mesh.wgsl` will tell you in one
    evaluation whether the fixture in the frame is contributing anything at all.

14. **The commonest failure in this gate is measuring a thing that resembles the thing the clause
    names.** Five times now: a histogram helper that read the percentage from the wrong field for
    seven of sixteen buckets; a probe on the lit bunk *beside* the warden; a crate's silhouette edge
    counted as a shadow; a lining-versus-deck reading counted as a same-material colour split; and a
    frame-to-wall band counted as a leaf-to-frame joint whose real depth was zero. **Name the row, the
    x-range and the object.** A close condition that can be satisfied by measuring the wrong object is
    not falsifiable, and rewriting one to name pixels is a legitimate review output (§6).

15. **Repetition is only a tell on objects that should have individual histories.** A cast-iron tunnel
    lining genuinely *is* twenty identical segments and conduit clips genuinely are at a fixed pitch.
    R1's first form flagged thirteen `repeat` blocks of which twelve were manufactured. Judge a
    duckboard, a crate or a bunk by its variants; do not judge a casting by them.

16. **A texture measured from a render is a texture nobody measured.** §4 #5 says do not diagnose a
    texture from a render, and it was written about a *lattice* being wrong. The harder case is the
    reverse: `games/warren`'s floor was flagged by reviews 20, 22 and 25 as detail that failed to
    survive to render scale, and each time the conclusion was that the texture was right and
    something downstream flattened it. **Opening the PNG settled it in one command:** 0.82 mean
    adjacent |ΔL| at native resolution over a range of 89–107, which is a wash. The texture was never
    right.

    The cause is worth knowing because the comments in the file claimed the opposite. **Gradient
    noise is exactly zero at every lattice point** — that is what makes it smooth — so a lattice
    approaching the texel size returns almost nothing. The call was `tiling(.., 704)` on a 1024 map:
    1.45 texels per cell. Per-texel grain needs a *value* hash, not gradient noise.

    **Measure the source before measuring the render, and publish both numbers.**

17. **A control is not a control until you have shown it lands on the object.** Review 28 demanded a
    control run on F2; the control was then taken at a column that was **62% off the old model**, so
    what it measured was the tunnel wall's plate joints behind it — the highest-contrast thing in the
    frame — and the conclusion drawn was that the clause scored the *worse* model higher. On a column
    verified on-figure for both, the rebuilt model beat the control by 74%, in the right direction.
    **Print the on-object sample count beside every figure**, for the subject *and* for the control.

    **The instrument that settles it is a matte, and it costs three commands.** Set the subject's
    material to `base_colour 0 0 0` and `emissive 12 0 12`, capture, restore the material with
    `git checkout`, and re-capture to prove byte-identity. A material is an asset, so §4 #11 permits
    it through `--from`. The subject's pixels are then exactly `R ≥ 200 ∧ G ≤ 60` — no lighting, no
    background, no contrast direction, no shadow trough — and **a broken run on a row is occlusion,
    detected for free.** Matte *every* material the subject wears, not just the main one: a figure
    whose head, lantern and glass are separate materials shows holes in a single-material matte, and
    those holes are indistinguishable from something standing in front of it.

    This is the seventh instance of #14 and the first committed *inside* a control demanded to prevent
    exactly it.

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

**There are four of them, and reviews 16 and 17 both had to ask.** `playing.snapshot` is the start
line, and it was the only one — so the key, the way out, the warden and two thirds of the section
conditions could not be photographed at all, and review 17 said the gate was being *"decided on one
fourteenth of the game"*. The others are `at_key.snapshot`, `at_exit.snapshot` and
`at_warden.snapshot`, each standing at the landmark of that name and facing along the bore:

```bash
cargo run -p amadeo-cli -- capture -p warren --from games/warren/snapshots/at_key.snapshot --ticks 5 out.png
```

`cargo run -p warren --bin moment` writes all four; `--bin moment -- key` writes just one.

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

### Review 15 — engine gate — `af98989` — item 24, geometry half — **NOT POLISHED**

Seven frames at 1920 × 1080 plus all four other games, six crops, ~40 probes, four row/column
profiles, the measurements recounted, the fog term re-derived from `mesh.wgsl`, and one controlled
A/B on the `Environment` — which §4 #11 permits through `--from` because an environment is an asset.

**What it credited.** *"It is a tunnel"* — answering the question it was asked directly, and calling
the pitch-up frame the best this project has produced. It confirmed both measurements (0 / 18 and
2 / 7), re-derived the crown geometry independently (radius 3.0, headroom 2.0 m at the wall, crown
3.2 m) and noted that the fog is now doing real work: `1 − exp(−((d − 1.5) · 0.055)²)` gives 5.9% at
6 m, 28.4% at 12 m and 78.4% at 24 m, against the 7%-across-a-room review 12 rejected.

**Three of the close condition's five clauses failed on measurement.**

- **The pool was a hole.** 27,659 clipped pixels; row 500 of the yaw-90 frame ran 242–254
  *continuously* for 252 px. Cause: a `PointLight` 0.3 m off 0.6-albedo plate — irradiance ∝ 3.2/0.09
  — and halving exposure still clipped 10,385 px, so it was the lamp rather than the grade. It also
  inverted the read: an emissive tube cannot out-brighten a wall already at 254, so the fixture
  rendered as a shadow.
- **The grade cancelled the palette the design asks for.** Hand lamp R−B = +12 at luma 164; fitting
  pool R−B = −10. Both read white. At `saturation 1.0` the fitting comes back at G−R = +20. The same
  grade put safety orange at **(107, 0, 0)** with G and B clamped to exactly zero, against
  (199, 81, 34) for the identical orange in the UI — so the walls and the interface, which `docs/11`
  §2 calls the luckiest coincidence in the project, were two different colours.
- **The props were not there.** Bunk frames at literally `RGB(0,0,0)` — 222 consecutive zero pixels —
  because `metallic 1.0` has no diffuse term and §5a specifies this surface by its *base colour*.
  Both mattresses wore `screed`, the floor material. So the section conditions were built in
  `lay_out` and invisible on screen, *"which for my purposes is not built"*.

**And two more:** black fog can only subtract, so nothing can emerge from it; and the lining stopped
at the springing, meeting the arch on a dead-straight horizontal seam — *"the residue of corridor
with a lid"*.

**On the sign** it measured a crisp black `⊢` where the texture holds a clean `H`, and ruled out
filtering by finding single-pixel edges. The three diagnostics that followed — a ten-band ruler, a
ramp, then a bold `F` — showed the plate sampling about half of `u`. The letter is geometry now
rather than a picture; see `finishes.rs`'s `sign_colour` for why that is a retreat and not a fix.

Item 24 stays 🟡. Its eight ordered changes are what session 23's second half built.

### Review 16 — engine gate — `3c77a69` — item 24, third pass — **NOT POLISHED**

Seven frames at 1920 × 1080 plus the other four games, seven crops, ~55 probes, six row/column
profiles, three controlled A/Bs, both measurements recounted, and one number re-derived from the
collection pass. It called the remainder *"a near miss with a short, concrete remainder"* and
credited ten things on measurement, of which the ones worth carrying forward are:

- **"It is a tunnel"**, and the pitch-up frame *"the best frame this project has produced"*.
- **Real pools with real dark between them.** It had formed the opposite impression from the
  histogram and the profiles refuted it: row 470 is bimodal with genuine zeros between the pools.
  *"Withdrawn."*
- **The warm/cold split works**: hand-lamp pool R−B = +22, fitting pool G−R = +13, same frame.
- **The measurements are honest** — no drift in either direction, which §3 #6 exists because of.

**What it failed the item on**, and all of it landed in the same session:

- **Fourteen signs, every one saying H.** The largest defect in the game, and one the submission had
  claimed as answered — moving the letter into geometry made a per-section letter *possible* and did
  not deliver it. `docs/11` §2 makes the alphabetical naval scheme the thing that lets a player
  orient with no map; fourteen identical letters do not fail to solve that, they invert it.
- **Nothing had a fill light.** Three authored objects rendering at exactly `RGB(0,0,0)`: a 20 cm
  skirting kerb as a hard black band across every frame, the fitting housing, the sign surround. It
  suspected a metallic material, A/B'd it, **withdrew**, and A/B'd the ambient instead.
- **Every material had flat roughness and flat occlusion** — varying by at most six levels inside any
  one texture, with occlusion a constant 255 in five of six. *"Materials differed from each other and
  no material differed from itself."*
- **The fitting's cone was a 136° flood**, so the pool had no edge: 138 → 100 over 500 px.
- **Only one place in the game can be photographed** — `moment.rs` snapshots the player start and
  nothing else, so no reviewer can see the key, the way out, or two of the three conditions.

**And one self-correction worth recording**, because it is the mechanism working: it called the
lining's rust *"the same airbrushed blob repeated"* from the render, opened the PNG, found the
staining runs downward from the bolts the way water actually does, and withdrew — while keeping the
real finding, which is that it is executed at a tenth of the contrast it needs to survive being lit.

**On the sign it split the verdict**, and correctly: the method is right and it accepted it, but
*"moving the letter into geometry is only worth the trade because it makes a different letter per
section trivial — that is the entire payoff, and it was not taken."*

### Review 17 — engine gate — `6d82f7e` — item 24, fourth pass — **NOT POLISHED**

Eight frames plus the other three games, six crops, ~30 probes, four row profiles, **six controlled
A/Bs**, both measurements recounted. It credited eight things — zero clipped pixels in all eight
frames, the signs no longer all H, the cone fixed (`113 → 0 in 25 px`), the warm/cold split holding
in frame at R−B = +20 against G−R = +12, and the measurements honest for a third review running.

**And it found an engine defect nobody had looked for, with a derivation.**

> `post.wgsl:110` — `color = (color - 0.5) * grade.x + 0.5`. Solving for an output of zero at
> `contrast = 1.05` gives an input of `0.5 − 0.5/1.05 = 0.0238`, which is byte **44** on an sRGB
> target. Everything below that was clamped to pure black.

Measured: `games/warren` at `contrast 1.05` had **42.5% of a frame at exactly `RGB(0,0,0)`**; at
`contrast 1.00`, **none**, with a minimum of 17. Six A/Bs to get there — vignette (no effect), the
sky map (walls only), the ambient level (worked), exposure (worked) — which is what separated "the
lighting is wrong" from "the grade is deleting the lighting". **`games/vault` authors `contrast
1.15`, which was destroying everything below byte 72.** Three reviews had been finding this one
object at a time — a skirting kerb, a fitting housing, a sign surround — and none of them were the
objects' fault.

Its other five findings: three of eight frames had no light source in them; the section letters were
`manhattan % 5`, so they are a distance *ring* that conveys no direction and repeats, and nobody ever
sees a **name**; three conditions over fourteen rooms cannot satisfy §5.2's rule by pigeonhole, and
the three that would change what a room *looks* like were the ones missing; the fitting was a box and
a capsule with no end caps, guard or conduit, interpenetrating an arch rib; and the bunks were
perfectly made, identically, everywhere.

**Its self-correction:** it called the lining's rust *"the same airbrushed blob repeated"*, opened the
source PNG, found the descending runs, and withdrew about the texture while keeping the real finding
— that what is *rendered* at 3–6 m is not those runs.

### Review 18 — engine gate — `0124db7` — item 24, fifth pass — **NO VERDICT**

**Attempted and interrupted by an API session limit before it ruled.** It had got as far as §3 #5 —
*"capture every game, not only the one under discussion"* — and produced no findings, no evidence and
no verdict.

Recorded because an attempted review that leaves no entry is indistinguishable from a review that was
never asked for, and because the next reviewer should know the state it was reaching for: `0124db7`
is pushed, CI is green on it, and the ten frames it was given are described in the handover. **Nothing
here is credited or discredited by it.** Item 24's standing verdict is review 17's NOT POLISHED, and
the four of its eight ordered changes that are still open are listed in `STATUS.md`.

### Review 19 — engine gate — `efdc90d` — item 24, fifth delivered pass — **NOT POLISHED**

The re-send of the interrupted review 18, at the same clean tree. Thirteen 1920 × 1080 captures of
`games/warren` — the authored camera, yaw 90/180/270, pitch ±30, all four committed snapshots and one
at yaw 90 from the warden post — plus all four other games, six magnified crops with stated
rectangles, eight row/column profiles, **three controlled A/Bs** on the ambient probe with a
byte-identical restore check afterwards, both measurements recounted, and two authored numbers
re-derived from the shaders that consume them.

**It verified ADR 0084 independently and credited it as the best finding of the last five reviews.**
Nine fresh frames, minima 10/14/15/15/16/16/16/16/19, not one pixel at `RGB(0,0,0)`, against review
17's measured 42.5%. Eight of nine report **0 clipped pixels**. `games/vault` at `contrast 1.15`
comes back min 20. It also credited the hand lamp without reservation — `w_pm30` row 800 lifts 139
levels through a ~120 px falloff to a plateau of 170 at **R − B = +22** — the warm/cold split holding
in one frame, the six section conditions landing as claimed, the fog at **28.3%** at the far wall of
a cell, and clause [a] of the close condition, *"a cast-iron-lined tunnel"*, outright.

**And it withdrew a finding mid-review**, which is the mechanism working for a third time: it first
wrote that the warden carried no light — the design's most-argued-for visual idea absent from the
picture — then cropped `(1090, 550, 260×220)` at 4×, found the lamp plainly lighting the racking and
silhouetting the coat, and withdrew.

#### The finding: the tunnel lining is lit by the ambient probe and by nothing else

Three reviews have argued about *"real dark between them"* by measuring three different surfaces.
This one walked the **left lining wall** of `at_key` along row 560, from 1.5 m to 14 m of depth:

```
84 84 86 89 88 87 88 95 95 95 90 92 92 91 [47] 98 84 83 99 97 87 97 94 97 92 [67] 93 93 [55] 101 88 92
```

Near quarter mean **88.4**, far quarter mean **93.7** — **the far end of a twelve-metre bore is
brighter than the near end**, and a column from crown to skirting spans 29 levels with no directional
sense at all. Three A/Bs on `warren_gloom.hdr`, which §4 #11 permits through `--from` because an
environment map is an asset:

| `gloom.rs` `LEVEL` | frame mean | wall at row 560 |
|---|---|---|
| **8.0** (shipped) | 67.8 | 84–101, flat |
| 5.0 | 47.1 | 58–69, **still flat** |
| 0.0 | 3.7 | `0 0 0 0 0 0 0 0 0 0 0 1 1 3 7` — zero for 500 px |

So essentially **100% of the illumination on the lining is the ambient probe**, which is
direction-only and distance-independent by construction and therefore paints a 12 m wall one flat
value at every depth and every height. `w_y270` and `w_at_exit` contain **no pixel above luma 130 in
two megapixels**, with 92.2% of both frames inside a single 80-level band — `docs/11` §6's named
failure mode verbatim, and the reason the frames read as a grey render although the geometry is right
and the textures are real.

**The `LEVEL 8.0` is a compensation for a defect that was fixed one commit later and never taken back
out.** `gloom.rs`'s own comment records the 5.0 → 8.0 raise as review 16's three-objects-at-black
finding; that raise is `6d82f7e` and ADR 0084 is `0124db7`, the very next commit, which proved those
objects were black because the grade was clamping below byte 44. Warning §4 #2's exact shape: a knob
moved to chase a symptom belonging to a different knob. **And the A/B rules out the obvious remedy** —
at `LEVEL 5.0` the wall is flat and merely darker, so the fix is where light comes from, not how much.

#### Two things nobody had measured

**The authored frame is mirror-symmetric to within five levels out of 255.** Mean `|L(x) − L(1919−x)|`
on row 600: **`w_auth` 4.87**, `w_y270` 6.59, `w_pm30` 9.85, `w_at_key` 13.20, `w_at_warden` 18.20,
`w_y180` 37.50 — against **96.16** for `games/atrium` as a control. A frame left–right identical to
within five levels is exactly symmetric to any viewer, and it is also `docs/11` §5.3's one explicit
prohibition, a straight run visible end to end. Bilateral symmetry is a stronger machine-made tell
than a box is, and the best-composed frames in the set are precisely the asymmetric ones.

**The way out is 31.3 m from the nearest light in the level.** A row across the door face spans
**11 levels**; the objective every system points at is invisible, unlit, unsigned, and carries none of
§5a's safety orange — which that section says exists precisely so that *the only orange things in the
Warren are the things you can act on*. Its prompt is still the literal string `"The door is locked"`,
the sentence `docs/11` §0 quotes Justin condemning and §8 forbids by name.

#### On the measurements, and it is the submitter who drifted

It recounted **0 / 27** and **2 / 11** against the submission's 0 / 25 and 2 / 10. The numerators are
honest for a fourth review running; the denominators were copied out of this file rather than
recounted, which is exactly what §3 #6 exists to prevent — **and this time the person who did it was
the implementer writing the submission, not a reviewer.** Corrected in `docs/13` §1. It also recorded,
so a later review does not "discover" it as a defect, that under a strict reading of *box-only* —
made of nothing but boxes — **14 of 27** qualify, the five letters and both sign plates among them.
That is not the metric review 1 set and it did not change it.

#### Its re-filings, which §6 makes a legitimate output

- **Item 32 is new** — the section letters name nothing and point nowhere. Off item 24: it is a
  wayfinding failure `docs/11` §5.4 owns, it is not in item 24's close condition, and a stencil
  alphabet is real work that should not sit behind a frame's verdict.
- **Item 18 absorbs the warden walking through walls.** It already names the identical defect in
  `games/atrium`. Review 15's reason for declining to credit the warden no longer applies, because it
  is now in a frame with a working lamp.
- **Item 24's close condition is rewritten into five numeric clauses**, because *"real dark between
  them"* has now been argued four ways by four reviews. It is quoted in full in `docs/13` item 24.

Its seven ordered changes are what session 24 is building.

### Review 20 — engine gate — `4693332` — item 24, sixth pass — **NOT POLISHED**

Eleven 1920 × 1080 captures of `games/warren` plus the other three games (`quad-demo` has no GPU
build and refused), eleven crops, ~70 probes, nine row/column profiles, **two controlled A/Bs** with a
byte-identical restore check afterwards, both measurements recounted, and two authored numbers
re-derived from the shaders that consume them.

**It confirmed review 19's finding is answered, and proved it rather than taking the claim.** It
regenerated `warren_gloom.hdr` at `LEVEL 0.0` itself: the `at_key` wall's near quarter goes 88.4 → 93.1
while the far goes 93.7 → 50.8, and with the probe removed the near end still reads 63 against the far
end's 1 — so ~85% of the near wall's light is punctual. It also confirmed symmetry at **70.88** against
4.87 (`games/atrium` 96.11 as control), the fog at **33.0%** at a cell's far bulkhead, all five
rewritten clauses met, and both measurements exact for a fifth review running.

**Its ruling on clause (a), which the submission asked for directly:** 65% was the right threshold and
the range was earned rather than brightened — *"if you had merely lifted the exposure the A/B would
have shown a uniform scale"*. But it added that the clause was standing in for something larger and
does not carry all of it: beyond the torch's reach the whole level is painted a uniform 34–61 by a
direction-only probe, `at_key` has **no pixel above luma 137 in two megapixels**, and `docs/11` §6 asks
for an unlit room to be *nearly black*. **The near half of the tunnel is lit and the far half is still
a grey wash.**

#### Two things it found that are defects in this repository's own beliefs

**`sky ""` supplies a BRIGHTER ambient than the authored map, not none.** It A/B'd the ambient that way
first, got a frame that was brighter (mean 91.8 against 89.1), and refused to conclude anything from
it — correctly. `ibl.rs:223` defines `DEFAULT_SKY = [0.12, 0.12, 0.12]`, the pre-ADR-0049 constant,
*deliberately* not black. **`gloom.rs`'s own doc comment says "with `sky ""` there is no indirect term
at all, so any surface a light does not reach is exactly black", and `docs/13` repeats it.** Both are
false, and the belief is the same shape as the 5.0 → 8.0 round trip: a number defended by a claim
nobody measured.

**The one emissive object in the game cannot bloom, by derivation.** `bloom.wgsl:72–81` computes
`over = max(brightness − threshold, 0)` on the brightest channel after exposure. `glow.material`
authors `emissive 0.62 0.82 0.7`, exposure is 1.0 and threshold 1.1, so **`over = 0`, always**.
Measured: the tube goes 59 → 157 → 224 in two pixels and 225 → 93 → 63 in two, flat 224/225 across its
whole face, no halo. **That is session 24's own regression** — the emissive was 1.55/2.05/1.75 and was
dropped to 0.62 during an A/B chasing a bright blob that turned out to be the torch beam, and never put
back. Q32's shape a fifth time: authorable, authored, ignored.

#### What it failed the item on

- **The key is still a key on a crate** — `docs/14` §1's founding complaint, verbatim, six reviews on,
  in a piece whose own comment says so. A **32 cm** key standing upright on its tip on a supply crate,
  in `brass` at `metallic 0.8` with no texture, so it reads as painted plastic against a near-black
  probe. **And `at_key.snapshot` does not have the key in frame** — 61.9° off the view axis against a
  ~50° half-FOV. The identical fault was found and fixed on `at_exit` this session and the other two
  were not checked.
- **Four pieces of unattached geometry float above the exit door** — `ring_fitting` at scale 0.8 seen
  head-on, reading as four fragments of debris; the orange rule as a flat slab with no bevel; the pool
  brightest at the wheel rather than falling from the lamp, because at `rotation -68` the axis clears
  the door face.
- **Yaw 270 is the worst frame in the game** — the brightest (mean 116.9) and incoherent: two
  near-white clipped slabs across the near plane, and behind them a dead-straight run of eight
  identical door frames to a vanishing point, `docs/11` §5.3's named prohibition. Turning the *spawn*
  away from it is not removing it; the player's first input is the mouse.
- **The nearest surface in every frame is featureless** — 61.7% of a 120×90 patch of near wall inside
  **one 16-level band**. It opened the source rather than judging from the render: `ring_lining_wall`
  repeats every 3.6 m and is magnified ~4.8× at two metres, and its detail is entirely low-frequency,
  so `Surface::grime` does not survive to render scale on the surface where it matters most.
- **The warden is fifteen levels from its background** — coat 14/23/27 against wall 29/30/49.
- Plus: the section letter is bone on bone where §5a specifies **black**; a bunk leg passes through a
  crate lid; the crates wear `bulkhead`, the tunnel's own bolted plate, on a 0.9 m box; the hand lamp
  has no edge because the omnidirectional spill is doing the beam's job; the three landmark frames are
  still bullseye compositions (sym600 14.8 / 17.8 / 11.1); and **twenty punctual lights against
  `MAX_PUNCTUAL_LIGHTS = 8`**, nearest wins, silent drop — which it explicitly did not claim to have
  observed, only that the arithmetic is against us and nothing warns.

**Its two self-corrections:** the `sky ""` A/B above, and it called the bunks in the aimed key shot
floating, cropped at 3×, found their legs on the deck and withdrew.

Its ten ordered changes are session 25's work; it says **items 1–4 landing would settle the item**.

### Review 21 — engine gate — `1d7784c` — item 24, seventh pass — **NO VERDICT**

**Attempted and interrupted by an API session limit before it ruled**, for the second time in this
gate's history. It had got as far as taking its first frames and produced no findings, no evidence and
no verdict.

Recorded for review 18's reason: an attempted review that leaves no entry is indistinguishable from a
review that was never asked for. **Nothing here is credited or discredited by it.** Item 24's standing
verdict is review 20's NOT POLISHED, of whose ten ordered changes **nine are built and item 5 is
open** — see `docs/13` item 24 and `STATUS.md` for which, and for why item 5 must not be re-attempted
in the door graph.

`1d7784c` is pushed and the four checks in `CLAUDE.md` §4b are green on it.

**Two of the last four reviews have now died to a session limit**, which is a fact about the process
rather than about the work: a review of this item costs roughly 90 captures and 200k tokens, and it is
worth sending as the *first* thing a session does rather than the last.

### Review 22 — engine gate — `1d7784c` — item 24, seventh delivered pass — **NOT POLISHED**

Ten 1920 × 1080 captures plus the other three games, six crops, ~70 probes, eleven row/column
profiles, both measurements recounted, two authored numbers re-derived, and one controlled experiment
using **camera yaw as the variable** because §4 #11 forbids an A/B on a component through `--from`.

**It credited more than any previous pass.** `w_at_warden` scored **98.54** on the row-600 asymmetry
test — it beats the `games/atrium` control (96.17) taken the same hour — and it called it *"the best
composed frame this project has produced"*. The three landmark frames went **14.8 / 17.8 / 11.1 →
60.6 / 57.3 / 98.5**. It re-derived the bloom (`over = max(2.5 − 1.1, 0) = 1.4` against session 24's
exact zero) and measured the halo at **248 → 63 across ~30 px** against item 13b's ≥ 12 px. It
accepted the 642 clipped pixels as the same case as review 19's 571. Clauses (a), (b), (c) and (e)
met, and it recounted the mesh denominator **against** the submission (0 / 29, not 0 / 28).

#### The finding: fifteen of the sixteen fixtures cast no shadow

`grep` says it: `player_start.scene` authors `shadows true` and `room_lamp`, `warden_post` and
`way_out` all author `shadows false`. Measured — a key board brightly lit at 1.5 m whose six hooks
stand 3 cm proud and **put not one mark on it**; duckboards that do not touch the deck; three bunks
and two crate stacks in front of a lit wall, none of them casting. **With `games/atrium` on the same
commit as the control**, whose lamp throws the table across the floor. *"Every Warren frame reads
composited rather than rendered."*

It is a flag, not a system: `MAX_SHADOW_SPOTS` is 2 and the collection pass already sorts casters by
distance from the eye, so turning the flag on hands the caster to whatever is nearest.

#### Three of the submission's numbers did not reproduce, and two of them were wrong

**This is the entry to read before writing another submission.**

- **Item 1 — the near-wall patch. The critic was right: 51.1%, not 5.0%.** The cause was a broken
  measurement helper, not a broken capture. It parsed `image stats` histogram lines with the fourth
  field after stripping the bar characters — and that field is the percentage **only for buckets whose
  label contains a space** (`0- 15` … `80- 95`, right-aligned). From `96-111` upward the label is one
  field, everything shifts, and the fourth field is empty. **Every high bucket was silently read as
  zero**, so the reported figure was the maximum over the low half of the histogram. Clause (a)'s
  64-level band survived only because its peak happened to sit low; re-measured with a correct parser
  it is unchanged at 50.5%, and the `games/atrium` control reproduces the critic's 62.1% exactly.
- **Item 7 — the warden. The critic was right: ~15 levels, not ~45.** The submission probed
  `(1120, 620)` and `(1150, 660)`. A 3× crop of `(880, 440, 380 × 340)` puts the figure at frame
  x ≈ 960–1023: **those probes were on the lit bunk frame beside the warden, not on the coat.**
- **Item 9 — the column reproduces exactly.** `image col <auth> 1000 560 1070` gives
  `57 61 56 62 97 185 189 193 172` on the shipped frame. The critic looked for it in *rows*
  (`w_p-30` row 900, `w_auth` row 1000); it is a **column**. Its judgement stands regardless — that
  gradient is a penumbra rather than an edge — but the number is real.

**The lesson is not "check your arithmetic".** It is that a measurement pipeline is a piece of
software and wants the same suspicion as a shader: **the helper agreed with the critic on one frame
and was silently wrong on another**, which is exactly the shape of §4 #2. Print the intermediate.

#### Its other findings

`letter_o` is four boxes making a rectangle and `letter_m` is a Π with a tick — **two of five section
letters are not legible as letters**, read off the files rather than the render. The warden is four
stacked cones flaring at the hem, *"a postbox or a chess pawn"*, and its own lamp does not light it.
The exit's `bulkhead_lamp` resolves to **two dark-green pixels** from the position a player arrives
at. The key is a 5 × 30 px yellow stripe. The duckboards are zero-thickness painted stripes in the
deck's own material. Yaw 270 is unchanged at symmetry 28.4. And `games/scarp` is now what review 12
said `games/warren` was — recorded so it is not discovered as a surprise.

#### On item 5 it endorsed the revert and refuted the reason

It confirmed the diagnosis (`bore_side_open` is mirrored about its own middle, so every aperture is
centred) and **endorsed reverting the door-closing fix** — *"shipping a function whose doc comment
claims a fix it does not deliver would have been worse"*. But it refuted *"both geometry"* three ways:
an off-centre opening is the same four boxes with the `mirror` flag dropped; two variants alternating
by cell parity gives a 2.8 m stagger over a 12 m pitch; and a baffle is a **placement rule** using
the `collapse` piece that already exists.

**Its third option does not survive contact and is corrected here with evidence.** `collapsed.scene`
carries a `Cuboid` collider of **3.4 × 1.2 × 2.9**, sized for a 4.8 m bore. A cross-passage is 2 m
wide and 2 m tall, so dropping a collapse into one does not break the sightline, it **walls the level
off** — and a 1.2 m heap is far above any step height. Options (i) and (ii) stand and are the route.

It also made a level-design point worth keeping: fourteen doors over fourteen rooms means the player
*can never loop, only retreat*, and that is the one topology in which a hunter cannot be beaten by
movement.

#### What session 25's second pass built

Its items 1, 3, 4, 5 and 10, plus the black letters: **shadows on** (a bunk's kerb now falls
194 → 88 in 6 px, against the new clause (f)'s ≥ 25 in 12 px); **clause (d) passes** at R − B **+17**
and G − R **+12** after the fitting came down to 8.0, the beam went to 26.0 and the hand lamp was
warmed to `1.0 0.88 0.68` — the critic's own derivation showed a 22.0 fitting at 4 m outrunning a
12.0 torch at 2.8 m; **the warden is a coat** with shoulders, a collar and a hat brim, reading 71–128
against a wall at 38–46 with 56 levels between its lamp side and its far side; **`letter_o` is an
octagon and `letter_m` has diagonals**, the M caught by derivation before it was rendered — the first
attempt put the apex at the top, which is an inverted V.

Its items 2, 6, 7, 8 and 9 are open.

### Review 23 — engine gate — `c374ceb` — item 24, eighth delivered pass — **NOT POLISHED**

**Appended late, and review 24 had to flag it.** This entry and the one below were written after review
24 pointed out that §8 ended at review 22 while the tree was three passes past it, so a reviewer was
being asked to rule under clause rewrites that existed only in a submission message. §6 rule 4 says
verdicts are appended; the reason is exactly this. **Append the verdict in the same session it is
given.**

Twelve captures plus the other three games, four crops, ~55 probes, twelve profiles, one controlled A/B
on a material, both measurements recounted, two numbers re-derived.

**It accepted all three of review 22's corrections** — the parser bug, the warden probe on the wrong
object, and the column-not-row — and re-derived the histogram itself with a parser that prints a
sanity sum. It credited `w_at_warden` at **115.95** row-600 asymmetry, beating the `games/atrium`
control, and called it the best frame the project has produced; the sightline broken (28.43 → 65.81 on
its own count, not the submission's 71.78 — **check which frame you measured**); the warden a coat at
74–130 against a wall at 39–45; the key at 141 against 56–72; `letter_o` legible as an O at 40 px.

**And it found item 7, which the submission had given up on.** It made `fittings.material` emissive
`3.0 0.2 0.2`, captured, restored, and diffed: **917 px changed in a 90 × 22 box** — the two bolt heads
at local z 0.005, and nothing else. Then it projected the fixture from the snapshot's camera and
calibrated the projection against the orange rule to five pixels. Every part at local z ≥ 0.05 was
behind the bulkhead plate. **A byte-identical frame across three authored positions is the signature
of geometry behind a wall**, because moving a fully occluded object in x or y changes nothing — which
is the diagnosis the submission had read as the field not being applied.

Its ordered list, of which nine were built in session 25: bound the torch's near field with a
sphere-light falloff (**118,040 clipped pixels, 5.69% of the yaw-270 frame**, derived from
`mesh.wgsl:679`); give the props real maps (**six of ten normal maps within ±8 of flat**); raise
`MAX_SHADOW_SPOTS` against 18 casters; negate the exit lamp's z; break the duckboard run; vary the
bunks and re-material the crates; take the right angles off the warden; adopt its clause rewrites.

**It rewrote four things and all four are adopted:** clause **(b)** to *the lit* lining wall, because
measuring falloff on the shadowed wall measures the ambient probe; clause **(d)** to a **same-material**
comparison, because the frame's extremes were duckboard timber and bunk paint — albedo, not light;
clause **(f)** as *"an object darkens a lit surface by ≥ 30 levels within 20 px at a named crop, and the
shadow's shape is recognisable as the object's"*; and **item 9's threshold retired** in favour of mean
`|Δ|` between adjacent pixels ≥ 3.0 on a 120 px run, because four reviews had argued about surfaces
using a test that measures lighting.

### Review 24 — engine gate — `2487a61` — item 24, ninth delivered pass — **NOT POLISHED**

Eleven captures plus the other three games, six crops, ~40 probes, eight profiles, one controlled A/B
on the `Environment`, both measurements recounted, two numbers re-derived, **two self-corrections**.

**All six clauses pass, for the first time in nine passes**, and it said so before anything else:
(a) 57.8% and max 250; (b) span **163**, near 129.2 against far 64.3; (c) **67.35**; (d) R−B **+17**
and G−R **+9**; (e) on inspection; (f) **30** levels at deck row 1020.

**It corrected two of the submission's numbers and both corrections stand.** The submission's clause (f)
of 40 was a **silhouette edge** — a crate's near face against the deck — not a shadow; restricted to
deck-only spans it is 30 and 62. And the submission's (d) of +38/+12 was a **lining-versus-deck**
reading, which the rewritten clause exists to forbid; same-material it is +17/+9, a two-level and a
one-level margin. It also could not reproduce the submission's 5.59/5.27 local-contrast figures because
neither the row nor the x-range was named. **Name the row and the range.**

**Clause (f) it closed on a picture rather than a number**: a bunk leg throwing a rod-shaped shadow
down-right from its foot, correctly proportioned, ending in a rounded cap that matches a round leg,
with contact darkening at the foot. Review 22's central finding is answered.

**Its own two self-corrections:** it read bright zones in `at_exit` and `at_key` as shadow-map artifacts
with straight edges, profiled them, found 150-level gradients across ~380 px with no edge anywhere, and
**withdrew** (§4 #7 again). And it nearly filed the torch as §4 #9 dead data — `player_start.scene`
authors `intensity 0.0` on both lights — then checked the snapshots, found 14.0 and 2.6 carried
correctly, and did not.

**It also tested something nobody had**: an A/B on the vignette, which moves **66.6% of pixels by up to
57 levels**. Clause (a) still passes without it at 56.6%. The histogram spread is earned by light rather
than by the lens, now established against exposure *and* vignette.

#### Why it still fails: the close condition passes and the item's title does not

*"`games/warren` stops being boxes."* Six clauses measure lighting, composition, prop presence and
shadows, and **not one of them can see uniform repetition** — the single largest machine-made tell
there is, and the game's most prominent object is now a perfect repeat. `duckboards.mesh` is one
`repeat count 30 step 0.0 0.0 0.4`: thirty slats, one width, one pitch, no board skewed, split, lifted,
missing or a different length, no wear track down the line boots walk, in honey-coloured fresh pine in a
flooded wartime shelter. It is the centre of four of nine frames. **The bearers and the cast shadows
being real is what makes it worse** — the thing is now legible enough that its regularity is the first
thing you see.

Both interactive objectives are illegible. The key is **4 × 30 px** in the snapshot named for standing
at it, from a camera aimed at it — at 4× the board is the best storytelling in the game and at
1920 × 1080 it is a black rectangle with a yellow fleck. The way out is **not a door**: the handwheel is
a flat 2D outline lying in the wall plane *with a gap in the ring*, the bulkhead's plate grid runs
continuously straight through where the leaf should be, and there is no jamb, rebate, frame, hinge or
gasket. And **nothing in the game is wired to anything** — no conduit, no tray, no junction box in ten
frames, which is §4 #4 one level along: the fixture exists and its supply does not.

#### Its replacement for the tracked measurements, which have saturated

33/33 `CompoundMesh` and 14/14 non-emissive materials textured cannot move further. It proposed two that
are countable without a human eye and would have caught what it caught:

1. **Repeat exposure** — for every `repeat` block and every prefab the generator instances, report the
   count and the number of **distinct** variants. `duckboards.mesh` scores **30 / 1**. Target: nothing
   placed more than six times has fewer than three variants.
2. **Objective legibility** — the on-screen bounding box in pixels of each interactive object from the
   snapshot named for it. The key scores **~4 × 30**; target a 40 px minimum dimension at 1920 × 1080.

Its eight ordered changes are the next session's work, and it asked for `games/scarp` to get an item of
its own rather than be carried in this one's shadow.

### Review 25 — engine gate — `9e47db5` — item 24, tenth delivered pass — **NOT POLISHED**

Nine 1920 × 1080 captures plus the other three games, seven crops, ~45 probes, eleven profiles, both
measurements recounted, two numbers re-derived, **one self-correction**. It recounted all six clauses
itself, the submission having declined to quote its own figures after being wrong four times.

**All six pass, and (b) and (c) reproduce review 24's numbers to the digit** — 163 / 129.2 / 64.3 and
67.35. It noted that as the first time in this gate's history two consecutive reviews have agreed
exactly on a clause. Nine frames with essentially no clipping (0 × 6, 38, 224, 0), against
`games/atrium` on the same commit at **9,496 clipped px** — *"the Warren is now the better-exposed of
the two games, which was not true five reviews ago."*

**Measurements saturated and confirmed: 0 / 35 box-only meshes, and 0 / 16 on all three texture
slots** — no material in the game leaves one empty. Honest for a seventh review running.

**What it credited.** The duckboards *"fixed, and fixed properly"* — **27 slats over 6 distinct
variants** against review 24's 30 / 1, and legible as broken at 1× without being told to look. The
conduit *"the best single addition in this pass… the first thing in this game that says the tunnel was
built rather than assembled from pieces"*. `w_at_warden` the best frame the project has produced and
now measurably so: asymmetry **111.07** against the Atrium control's 96.11. Surface detail past the
demo's: mean adjacent |ΔL| **4.45 / 3.18 / 3.13** on named runs against `games/atrium`'s 1.08 / 2.13.
And the key board's orange **347 × 230 px** — the approach of edging the board rather than scaling the
key *"was the right one, and it works"*.

**Its self-correction:** it wrote that the crates and a bunk interpenetrate in yaw 90, magnified the
junction at 5×, found a bunk seen edge-on that the crate ordinarily occludes, and **withdrew**.

#### Why it fails, and the answer to "should this item close"

*"The way out is still not a door, and it is the one object the whole game points at."* Measured: the
lining's plate grid runs unbroken through the leaf, and **the leaf's own edge is an 11-level dip where
plate joints inside it are 28 and 29** — the door's boundary is the weakest line on the door. The lamp
lights the wall beside the objective rather than the objective (leaf mean **80.5** against wall
**85.7**). The handwheel is a dashed ring: broken in six places, three of six spokes floating short,
a cross for a hub, and casting nothing onto the leaf.

**And it answered the question the submission put to it directly.** An item on a moving list is not an
item, so:

> **Item 24 should keep exactly one thing: the way out.** Everything else is a specific object, not a
> property of the game, and specific objects belong in specific rows.

It wrote a **clause (g)** to decide item 24 alone — the leaf/frame joint at least twice the deepest
plate joint inside the leaf, no plate joint crossing the boundary, and the leaf's mean luma exceeding
an equal-area patch of wall on either side — and re-filed the grime blur, the crate value and
markings, yaw 270's near plane, the key board's occlusion, the sign's gold and the warden's right
angles as rows of their own. Clause **(e)** either gets a real measurement or is retired; three
reviews running have closed it on inspection.

#### Its two replacement measurements, specified mechanically

The submission asked whether a prefab placed *N* times counts as *N* / 1. **No — score it by distinct
override sets.** A bore section placed fourteen times with fourteen different conditions is **14 / 14**
and not a defect; the same piece with an empty override every time is **14 / 1** and is.

1. **Intra-mesh repeat** — flag any `repeat` block with `count > 6` at a constant step. The game has
   exactly one today, `duckboards.mesh` at `count 7`.
2. **Placement repeat** — for every prefab instanced more than three times, report *N* and the number
   of distinct override sets. Target: nothing placed more than six times has fewer than three.
3. **Objective legibility** — project each `Interactable`'s collider AABB through the camera of the
   snapshot named for it, and report **two** boxes: the interactable's own and any `accent`-material
   sibling in the same instance. Target: the *marker* is ≥ 40 px on its minimum dimension **and no
   more than 25% occluded by nearer geometry.** *"The occlusion half is not optional — the key board
   passes 40 px by a factor of five today and is still eaten by a lining plate and a bunk."*

#### And `games/scarp` gets its own row

`min 60, max 188, mean 159.5` — the whole frame inside 128 levels of the upper half. One flat-shaded
heightfield, one salmon box for a player, zero textures, zero props. It is an **M2.5 exit gate** and it
is now precisely what review 12 condemned `games/warren` for being. Reviews 22, 23 and 24 each recorded
it; this one requires it to stop sitting behind item 24's verdict.

### Review 26 — engine gate — `9728f10` — item 24, eleventh delivered pass — **NOT POLISHED**

Ten captures plus the other three games, six crops, ~35 probes, nine profiles, one controlled
experiment using **camera yaw as the variable** (§4 #11 forbids an A/B on a component through `--from`),
both saturated measurements and all three of R1–R3 recounted, the spot-cone term re-derived from
`mesh.wgsl:697`.

**It credited the two halves that worked**: the plate grid is off the leaf, verified in *two* directions
— horizontally, and by column, showing that no dip on the leaf lands on any row where the wall carries a
joint — and the handwheel is *"a wheel"*, a continuous rim with a real boss and six spokes that reach it.
`w_at_warden` still the best frame the project has produced.

#### It found that the submission measured the wrong joint, and proved it three ways

The submission reported a leaf-to-frame joint of **105** at `x 930–1010`. Review 26 projected the door
from the snapshot's camera (focal `540 / tan 35° = 771.2 px`), checked the projection against four
features to within a pixel, and confirmed it by horizontal profile and by column:

> **The leaf occupies screen x 549 → 960. The stiles occupy 531–549 and 960–992. The band at 992–998 is
> the joint between the *frame* and the *bulkhead wall*.**

The submission's window contained the frame-to-wall band and **did not contain a leaf-to-frame joint at
all**. Measured properly, the real joint's prominence was **zero** — the profile ran straight through it
at rows 480, 660, 850 and 900 — against an interior maximum of 44 (the handwheel's rim reaching 99 on
row 660). *"The leaf's own outline — the line that says this part opens — does not exist in the picture
at all."* Ratio required 2; ratio measured **0**.

**And it named the substitution, which this project has now made three times**: reviews 23 and 24 each
had to correct a silhouette edge counted as a shadow and a lining-versus-deck reading counted as a
same-material colour split. *"The only way to reach 2.39 is to take the shoulder on the lit leaf side
rather than the wall side, which measures a lighting step, not a joint."*

#### The one cause under all of it: the door was lit by the player's torch

Derived from `mesh.wgsl:697` and confirmed by pixels. `head_lamp_light` sat 0.72 m out into the room at
`rotation −58`, giving an axis of `(0, −0.848, −0.530)` — **down and further away from the door**. The
cone term evaluates to **0 at the leaf's top and centre** and **0.106 at its foot**, or 2.4% of peak. The
beam's axis met the deck 2.3 m in front of the door, and that spot is visible in the frame as the only
place near the door with the fitting's cold `G > R` signature. Every lit surface *on* the door read warm
at `R − B = +18` — the player's torch at `1.0 0.88 0.68`.

Its controlled experiment: turning the camera `--yaw -30`, which moves the torch because it is a child of
the camera and moves nothing else, dropped the door region from **95.4 to 47.0**. *"The objective loses
half its light when the player turns their head."*

**And a light at the eye casts no shadow the eye can see** — which is why 100 mm of proud leaf produced a
zero-depth joint, an 85 mm handwheel read as an engraving, and six dogs read as painted dashes.
*"Three separate pieces of relief were built this pass and then lit from the one direction that cannot
show relief."*

#### Its three rulings, all adopted

- **Clause (e) retired.** Closed on inspection three reviews running, and *"a clause closed on inspection
  three times running is a clause doing no work"*. Not replaced; R3 measures what now matters.
- **Clause (g1) pinned to screen coordinates**, because the wording cost a whole pass.
- **R1 amended and its Today cell corrected.** It flagged any `repeat` with `count > 6` and claimed the
  duckboards were the only offender; **13 blocks in 9 assets** exceed it, and twelve are *manufactured*
  repetition — a cast-iron lining genuinely is twenty identical segments. **Repetition is only a tell on
  objects that should have individual histories.**
- It also caught a **false sentence the implementer had written into §1** — *"0 / 16 on all three texture
  slots, no material leaves one empty"*. True is 14/14 non-emissive textured, with `glow`, `glow_dead`
  and `flood` correctly carrying none. *"Fix the sentence, not the game."*

Session 25's answer: the lamp is aimed **at** the door (`rotation −112.5`, derived from the geometry
rather than guessed), given a **`source_radius` of 1.85** under ADR 0085 when the corrected aim blew
11,142 pixels, and the leaf is narrowed to 1.40 so its rebate is a real 50 mm gap rather than two
coplanar faces. Measured on row 480: left joint **56**, right **147**, interior maximum **25** — ratios
**2.2×** and **5.9×**. Leaf mean **141.8** against wall 83.5 and 25.9. Leaf colour `G − R = +22`, the
fitting's green rather than the torch's warm. Zero clipped. **And the handwheel now casts a crescent onto
the leaf**, which review 26 predicted would follow from the aim alone.

### Review 27 — engine gate — `c28c68a` — item 24, twelfth delivered pass — **item 24 CLOSED; gate NOT POLISHED**

Fourteen 1920 × 1080 captures, four crops, ~25 probes, **fourteen** row/column profiles, a prominence
sweep at three window widths, one controlled experiment with camera yaw as the variable, both saturated
measurements and all three of R1–R3 recounted, two numbers re-derived from `mesh.wgsl:685–700`, one
self-correction.

> **Clause (g) passes. Say it plainly: item 24 is closed.**

**(g1)** left **56**, right **147**, interior **25** — ratios **2.24×** and **5.88×**, reproducing the
submission's figures exactly, computed with a script whose intermediate was printed (review 22's lesson).
It checked the window because the left figure is the thin one: at ±6 the left collapses to 4 and at ±24
it rises to 59, **while the interior maximum stays at 25 at all three widths** — the left reveal is 19 px
wide, so a narrow window structurally cannot see it and a wide one does not inflate the interior. And it
confirmed the joint is **geometry rather than a lighting step** — a modelled 50 mm reveal, 170 mm deep,
with a dog crossing it and casting down onto the leaf — which is the substitution this gate has now made
three times.

**(g2)** verified two ways independently: the wall's plate joints at y 462/564/666/768/892 land on
**local highs** at leaf column x 880 and on a smooth monotonic decline at x 620.

**(g3)** leaf **147.1** against wall **86.6** and **27.6** on its own equal 370 × 540 boxes.

**And it tested the cause rather than taking it.** The cone term now evaluates to **0.672 / 1.000 /
1.000** at the leaf's top, centre and foot against review 26's 0 / 0 / 0.106. Its yaw experiment: the
leaf reads **147.1** head-on and **154.8** at `--yaw 30`, against review 26's 95.4 → 47.0. *"The
objective no longer loses its light when the player turns their head."* Leaf probes give `G − R` of
+22/+28/+10/+12/+14 — the fitting — while the wall beside it is −7, warm.

#### What a later reviewer must know before citing (g1) again

It profiled **ten rows, not one**, and published the table: only rows 440 and 480 pass. It ruled that
this is **structural rather than cherry-picking** and credited it on that basis — the three dogs project
to rows ≈ 480 / 660 / 845, and *"a dogged door's joint is supposed to disappear at its dogs"*; rows
560–900 pass through the handwheel, whose relief legitimately out-prominences a joint, and (g2) tests
the plate-joint question directly. It also flagged that row 480's left trough gives 56 at one pixel and
46 at the next, so the clause must say **maximum prominence within the joint band** or a later reviewer
taking the global minimum gets 1.84×.

#### Two things it measured that nobody had

**`source_radius 1.85` is an art-direction knob wearing a physics name, and it is load-bearing.** Every
point on the leaf is inside it, so the inverse-square gradient across the door is *deleted* and only the
cone and `N·L` vary. Predicted top : centre : foot **0.68 : 1.00 : 0.56** against measured **0.77 : 1.00
: 0.80** after tonemapping. Not filed as a defect — a 1.15 m tube 0.6 m off a wall really is an extended
source — but it is **1.6× the door's height**, so *"anybody who later corrects it to a physical value
puts 11,142 pixels back at paper white and will read the result as a regression in the lamp."*

**R2 has never been counted, and scored literally it is useless.** All 160 instances in
`generated.scene` carry exactly one override — `Transform` — so every prefab scores *N* / 1. Variation in
this level is expressed by **choosing a different piece**, so R2 must be scored over the piece *family*.
Scored that way: signs 14/5 pass, bore sections 14/6 pass, fittings 28/2 and cross-passages 16/1 are
manufactured — and **bunks are 12 / 2, which fails and is the one that matters**, since review 17
already said the bunks were perfectly made, identically, everywhere.

**Its self-correction:** it called the handwheel's cast shadow a formless smudge from a 4× crop, then
profiled row 790 — `111 → 51 → 103`, a 60-level core with 20–30 px edges and darker arcs inside a
broader penumbra — and **withdrew**.

#### The gate is still open, and the remainder is the fourteen rows already filed

*"Item 24 passed. The gate has not."* It measured two of the open rows itself rather than citing them:
`games/scarp` at **min 60 / max 188 / mean 159.2**, unchanged across five consecutive reviews and now
*"the single worst artefact in the repository, and nothing is in front of it any more"*; and yaw 270
from the spawn at mean 109.7, two thirds near-plane plate, one mouse movement from where the player
wakes up.

Its order: **39 (`games/scarp`)**, 35 (yaw 270), 33 (the grime blur), R2's bunks, 38 (the warden, open
six passes), then 34/36/37, the call plate at 52 × 39 px of unmodelled orange, and the two pieces of
clause housekeeping.

**The two frames it would put on a conference screen: `w_at_warden`, and `at_exit` at yaw +30** — the
second new this pass, *"and the door in it is unmistakably a door, standing proud of its frame with
three dogs bridging each reveal and its own lamp on it."*

---

### Review 28 — engine gate — `618e8d5` — **a PLANNING review of session 26's re-scope** — **NOT POLISHED (on the plan)**

The second planning review this project has taken, review 14 being the first. Nothing was submitted as
built and no capture was submitted for credit. What was submitted was `docs/13` §1b: Justin's session-26
instruction to lower the scope, cut the redundancies and finish `games/warren` in two or three sessions,
turned into thirty-four open rows becoming six.

**It reviewed the plan and it also measured, which is why its findings are worth more than an opinion
about a list**: six 1920 × 1080 captures, two magnified crops, four row profiles, four histograms, the
crate dependency graph read off `Cargo.toml`, the shipped fog re-derived from `warren.environment`, and
the warden's two systems read from source.

#### It upheld the cut it had the most standing to defend, and corrected its reason

Review 27 had ordered `games/scarp` first. Review 28 **upheld cutting it**: *"I am not going to defend
my own ordering against a deadline that did not exist when I set it… POLISHED is a frame from a real
game, and neither frozen game is one."* It also said plainly that writing the argument down and putting
it first, instead of skipping a row quietly, *"is the behaviour this process exists to produce."*

**But the stated reason was wrong and that mattered.** *"Texturing it is a second art pass"* is refuted
by `crates/amadeo-terrain/src/world.rs`'s own comment: a planar projection stretches on vertical faces
and *"triplanar mapping is the usual fix."* That is engine work an isometric outdoor game needs, so it
was filed as its own GAME 2 row (**G-tri**) rather than being allowed to leave with item 39.

#### The finding: the plan was missing its largest row, and building F6 without it would restore the lie F6 removes

`move_the_warden` writes `Transform::translation` straight at the player with no collider and no sweep.
`watch_for_you` sets `sees_you` from `distance(...) <= WARDEN_SIGHT` alone, with no line of sight.
**The antagonist of a horror game sees through cast-iron bulkheads and walks through them.**

The function's own doc comment excuses it — *"the room is open enough that it does not read as broken"* —
and **that sentence was written for `scenes/warren.scene` while the shipped level is
`scenes/generated.scene`**, fourteen sections divided by bulkheads. A comment that was true of the level
it was written against and false of the level that ships.

Why it outranks everything: **F6 exists to stop a warden being exactly as loud through a wall as through
a doorway.** Build F6 and leave this, and the player hides behind a bulkhead, hears the breath muffle
correctly, and watches the figure come through the plate — *"the lie F6 removes is put back by the
omission, in the same thirty seconds. Two systems pointing opposite ways is worse than neither."*
Filed as **F2b** and as item **41**. Not pathfinding, not item 18: `cast_shape` for sight and
`move_shape` for motion, both built and both already used in this game.

#### Five of six close conditions failed §6, and two were satisfiable with the defect fully intact

The same failure this repository has now made six times — **a condition satisfiable by measuring the
wrong object** — committed in the act of rewriting conditions to be *more* falsifiable:

- **F2 asked for ≥ 40 levels between the coat's lit and unlit sides. It measured 50 → 207 across row
  620 on the unmodified model** — a 157-level range, passing by nearly four times, on the mesh the row
  exists to replace. Item 38's clause had been *"across the coat's front face"*, which is self-shading;
  the rewrite substituted side-lighting the lamp already delivers.
- **F4's numbers were impossible.** Re-derived from the shipped `density 0.055, start 1.5`: fog is
  **0.68% at 3 m** and **3.6% at 5 m**, so the air a torch crosses is essentially clear and a 25-level
  *mean* lift cannot come from that medium. `fog.colour` is near black, so the same medium cannot both
  absorb to black and glow — an undeclared authoring decision sitting inside a row in a fixed budget.
  Its *"< 3 outside the cone"* clause would also **fail a correct implementation**, since a raymarch
  scatters every spot.
- **F5's merge was upheld and its condition rejected.** One clause — *"nothing in the shelter brighter
  than the lining"* — is **wrong art direction**: the lamp, the tube and the accent must all be
  brighter than the lining, because they are the focal points. Four sentences of universal prose had
  deleted the named rows and ranges the original rows already carried.
- **F1's scrim clause was true of an opaque black rectangle on a rendered frame** — the exact defect —
  and it had quietly relaxed item 35's *"≥ 1.5 m clear of any wall"* to 0.8 m, which with a 0.35 m
  capsule is 0.45 m of standing room in a 4.8 m bore.
- **F3 dropped the requirement `docs/11` §5.4 calls the whole scheme** — sections placed in
  alphabetical order along the spine — and substituted a pointer, which an arrow satisfies while
  fourteen signs stay decoration.
- **F6 said "measurably attenuated"**, which §6 forbids, and did not name the architecture question
  underneath it: `amadeo-audio` does **not** depend on `amadeo-physics`, so there is no way to ask
  whether a wall is in the way without a new dependency edge or a value written from above. Hard to
  reverse, so `CLAUDE.md` §5 makes it Justin's and it wants an ADR.

**Its own self-correction, recorded because §4 requires it:** it first read F1's 25-level yaw clause as
forcing uniformity and fighting item 24's clauses (a) and (c). The four cardinal means are
**62.3 / 67.8 / 50.2 / 109.7** — fix yaw 270 and the other three span 17.6 — so it **withdrew**. It
then replaced the instrument anyway, on the better ground that the mean measures brightness where the
defect is *proximity*: the fraction above luma 144 reads **3.5% / 8.5% / 0.2% / 36.7%** and separates
the defect cleanly.

#### It corrected a claim the submission made about itself

The submission said *"there is no music at all."* **`games/warren/assets/pieces/ambience.scene` plays
`warren_tone` on `Bus::Music`**, non-spatial, looping, forever. What is absent is anything *reactive*.
Its ruling was that reactive music does **not** get a row — §9's *"near-silence is the default, so that
a single sound is an event"* — but that driving the existing bed's gain from distance to the warden is
four lines and joins F6.

It also found, without ears, a design failure that needs none: **there is one `footstep` clip and the
game has three floor surfaces** — timber duckboards, concrete screed and standing water — after a whole
review cycle was spent making the duckboards legible.

**And it named what only Justin can settle**, on headphones, once: whether `warden_breath` reads as a
creature or as filtered noise, whether `warren_tone` leaves silence audible, and whether `caught` and
`escaped` read as two different endings. *"Any no is a row. Nothing else in audio is."*

#### Plan hygiene: the arithmetic did not close

`grep -c "FINISH (s26)"` returned **12** against a table naming **11** — the twelfth was item **25**,
the reticle, tagged and then absent from the six, and measured at `at_exit` as a 4 px dot at luma **122
against a door reading 200–233**, *darker* than its background at the game's final objective. Item
**31** carried no bucket and needs a verdict rather than work. Item **40** carried a GAME 2 tag while
missing from the GAME 2 list. Item **3c** was counted under EDITOR while closed on r13.

#### And it required the fallback be decided while it is a decision

*"Two-to-three sessions is not elastic and the plan does not say what ships if it runs out."*
**Mandatory: F2, F2b, F5, F1.** F6 may descope to occlusion and footstep surfaces. F3 may descope to
§5.4's requirements 1 and 3. **F4 is the stretch and is the row allowed to slip** — highest value and
highest risk, and a half-built volumetric pass is worse than none.

All ten ordered changes were applied to `docs/13` §1b before any code was written, which is what a
planning review is for.

---

## 9. The support agent — added session 26

Justin created a **second agent**: `.claude/agents/designer.md`, brief in `docs/15-the-designer.md`.
It owns **player experience, story, worldbuilding, theming and UI** — what a thing *means* to the
person playing it — and it is involved in nothing else.

**You are the main agent.** `docs/15` §2 has the full table; the four rules that concern you:

1. **You work independently.** Neither reviews the other, and you will not normally see its output.
2. **Where you disagree with a decision of its, your ruling stands.** It makes its case once, through
   the implementer, and that ends it. **Say plainly when you are overruling it** — a disagreement
   that resolves silently teaches nobody anything, and `docs/15` §5 exists to record how it went.
3. **Only you and Justin may decline or stop it.** The implementer may not.
4. **It may not write ✅ in `docs/13`, and may not use POLISHED or NOT POLISHED.** Those are yours.

**Nothing about how you review changes.** Judge what you are given on its merits. A design decision
that came from the designer gets no deference from you — a bad decision with a good provenance is
still a bad decision, and saying so is the job.

**Why the role exists, in case it is useful to you.** Twenty-eight reviews have asked whether the work
is well made. Nothing has ever had the job of asking whether it *means* anything, and `docs/11-the-
warren.md` — the one design document this project has — was written by the implementer and needed six
critiques from you to become good. The designer is meant to make that the first draft rather than the
sixth. On `games/warren` it will be quiet, because that design is settled and the budget is seven
rows; it starts properly on `docs/05` M4b, the first published game.

---

### Review 29 — engine gate — `aaa2195` — F2 built, ADRs 0086/0087 proposed — **NOT POLISHED; both ADRs APPROVED WITH CHANGES**

Twelve captures, five crops, ~30 probes, six profiles, four controlled A/Bs on assets with a
byte-identical restore check after each, and the projection re-derived from the snapshot's own
transforms and calibrated against the hat crown to 4 px.

#### It replaced the instrument, and that is the durable part

It did not try to find the silhouette photometrically. It set `greatcoat.material` to
`base_colour 0 0 0 / emissive 12 0 12`, captured, restored with `git checkout`, and re-captured to
confirm byte-identity. **A material is an asset, so §4 #11 allows it through `--from`.** The figure's
pixels are then exactly `R ≥ 200 ∧ G ≤ 60` — no lighting, no background, no contrast direction, no
shadow trough — and a broken run on a row is occlusion, detected for free.

#### The correction: clause (c) is not backwards, and the submission's control was contaminated

The submission reported the old nine-box model scoring **3.92** against the rebuilt one's **3.44** and
concluded the clause was backwards. Review 29 reproduced both figures exactly and then checked whether
the column was **on the object**:

```
x=1175, y600-950   new: on-figure 350 / off   0
                   old: on-figure 133 / off 217
```

**62% of the old model's score was the tunnel wall behind it**, whose plate joints are the highest
contrast in the frame. On a column verified on-figure for both — x=1130, y 640–760 — the new model
reads **3.34** against the old's **1.92**: it beats the control by **74%**, in the right direction.

Filed as **§4 #16** below. It is the seventh instance of §4 #14, and it was committed *inside* a
control run that review 28 had specifically demanded.

#### Why F2 does not close

1. **It is still a stack, and part count was never the problem.** Every body part is axis-symmetric
   and every joint a hard horizontal ledge running the full circumference — six of them between brim
   and hem. *"Rotational symmetry is a stronger machine-made tell than a box is."*
2. **There are no shoulders, and the shoulder cone is upside down** — `0.29 → 0.225`, widest at its
   bottom, which is a cape rather than a shoulder. The figure's widest points are hem 164 px and
   collar 125 px with the body between them at 85–97.
3. **"No face shows under the brim" was not what rendered** — 0.095 m of skull stood proud of the
   collar, *"a bald egg, which is less frightening than darkness would be."*
4. **The lantern had no light in it and the light had no lantern, 0.48 m apart** — projected to
   **47 px** apart on screen. The lamp body read 224–239 against the coat's 207–231, *ten levels*,
   with 843 clipped pixels beside it, and the mesh carried no glass and no emissive element.
5. **It is clearly seen, and `docs/11` §3 forbids that.** Figure median 106, p90 150, against lining
   behind it at 30–90 — roughly **120 levels above its immediate background** in full detail. Review
   20 rejected this figure at *fifteen* levels. *"Fifteen was too few. One hundred and twenty is too
   many, and the row has crossed the target rather than reached it."*
6. **The arms were 2 cm proud of a coat of the same radius** — half-embedded slabs, *"present enough
   to be add-ons, absent enough not to be arms."*

**Its own self-correction:** it first called the coat *"the brightest large object in the frame"* from
a crop; the median says parity with the brightest lit lining, not dominance. It changed the sentence
to the local-contrast claim, which is what the numbers support.

#### The two unasked changes: half stands

**Anchoring the snapshot on the warden's own `Transform` stands** — *"a snapshot named for the only
landmark that moves must ask where it is."* **3.4 m and −0.9 m across do not**: row-600 asymmetry fell
**111.07 / 115.95 → 69.48** on the identical instrument, with the `games/atrium` control reproducing
at 96.16 against reviews 19 and 25, and **11 of 20 sampled rows crossed by nearer geometry**. It voids
exactly one prior reading — the composition credit on `w_at_warden` from reviews 22, 23, 25 and 27 —
and **no measurement of the warden**, because those were true of a figure at ~9 m.

**The lamp colour is approved, and it ruled that it was never the designer's to grant.** `docs/11` §4
already specifies *"a cold, narrow, downward beam, deliberately unlike the player's warm one"*, and
the shipped green flood met **neither word**. *"The designer identified a violation of a passed
design, not a new direction."* One correction to how it was described: it is **not** a reduction —
peak channel went `11.0 × 0.72 = 7.92` up to `9.0 × 1.0 = 9.00`, which is why the coat blew out.
**Renormalise on peak channel, not on the intensity scalar.**

#### The ADRs

**ADR 0086 — APPROVED WITH CHANGES.** The inversion is right and both rejections correctly argued.
Three things it must say first: **name the crate that owns the filling system** (the ADR never does,
and `modules/` would contradict its own *"not left to each game to remember"*); **one cast per voice
per tick cannot satisfy F6's own clause (a)**, since a binary cast steps 1.0 → 0.0 in one tick against
a 0.15× bound — ease the scalar toward its cast target at a bounded rate; and **record the low-pass as
the intended consumer**, because *"a wall does not make a sound quieter, it makes it dull"*, and gain
alone tells the player "further away" when the mechanic needs "behind that bulkhead".

**ADR 0087 — APPROVED WITH CHANGES.** *"Correct, and correct for the physical reason it gives."*
**Its own self-correction is the useful part:** it began to require `scattering.strength` be bounded as
an albedo multiplying the fog's extinction, then checked that against the shipped numbers — 0.68% at
3 m — found the bound would make the beam invisible by construction, and **withdrew**. Changes: say
plainly that `fog` and `scattering` **do not describe the same medium** and that this is art direction
wearing a physics name, because *"the next person to unify these two numbers on physical grounds
deletes the beam and will read it as a shader bug"*; and give `colour` a stated default of **white**,
or a file omitting it scatters black — Q32's defect shape a sixth time, in the field the ADR exists to
add.

**And one thing for Justin:** F4 is the row that cashes `docs/11` §4's headline mechanic, because
**a carried lamp with no visible beam cannot be tracked through a doorway.** It did not reorder it —
*"the budget is real and a half-built volumetric pass is worse than none"* — but recorded that F2's
remaining defects are partly F4's absence: an object seen in a beam is *never clearly seen*, which is
the promise this model cannot keep on its own.

---

### Review 30 — engine gate — `04ba327` — F2, F2b, F5, F6 — **F2b ✅ · F6 ✅ · F2 and F5 reopened · gate NOT POLISHED**

Eighteen captures, six crops, ~45 probes, ~22 profiles, five texture sources measured at **native**
resolution per its own §4 #16, both tracked measurements and all of R1–R3 recounted off the filesystem,
and three numbers re-derived from first principles.

#### The correction, and it is the eighth instance of §4 #14

**F2's (a3) ratios did not reproduce, because the submission measured width as a COUNT of figure
pixels and the clause means EXTENT.** On a row the bunk crosses, a count silently loses the 30 px the
bunk eats — so an occluded waist measures narrow and **inflates every ratio that divides by it**.
Submitted 1.80 and 1.59; measured by extent, **1.14 and 1.01**. Shoulder-over-waist was essentially
*one*.

That is why the clause says a submission below 16 usable rows **re-frames rather than re-measures**:
the widths on a broken row are not measurements of the object. Committed inside an instrument the
critic wrote to prevent exactly it.

Checked against the model instead of the pixels, the geometry agreed with the review: hem 0.36, waist
0.259, shoulder 0.355 — **1.39 and 1.37 against a bar of 1.35.** *"The figure was built to clear the
clause by three per cent, and three per cent does not survive projection, polygon flats or a 1.35 m
off-axis camera."*

#### The finding: everything added that pass is invisible by construction

All three features that exist to break the solid of revolution sit within **1.4 cm** of the surface
they are on — the closure slabs 1.35 cm (**1.8 px** at 5.7 m), the strap 1.0 cm, and the rear vent at
`z +0.3` where **no camera the game can produce ever looks**. Review 29 failed the *arms* for exactly
this; the arms were deleted and the same error was made three more times in the same file.

**The lantern is inside the coat**: radial 0.345 with a half-width of 0.060 against a coat radius of
0.280 — **5 mm of clearance, 0.7 px**. No background is visible between lamp and body, which is why it
reads as a lit slot cut into a stove. *"The figure has zero enclosed negative space anywhere."*

**And the structural verdict, which is the part worth keeping:** *"A stack of coaxial cones cannot
produce a shoulder line, because a cone's widest place is a circle and a shoulder is a horizontal
edge. That is the structural reason this keeps failing, and it is not fixed by rotating anything."*
It named Playdead's one-bit silhouettes (enclosed negative space), Tarsier's figures (mass asymmetric
about the vertical) and Frictional's (never fully lit, one landmark feature).

**Its own self-correction:** it first wrote the hem step up as a surviving *silhouette* ledge from a
4× lit crop; the matte and the model arithmetic both refuted that, and it changed the finding to a
**shading** ledge — a 7 mm exposed cap annulus reading as a full-width one-pixel line at y716.

#### The y-rotation revert — OVERRULED, and then withdrawn anyway

The arithmetic was confirmed exactly: a 9-gon's inradius is `0.9397r`, so clearance needs a **6.03%**
step — *"precisely your ~6% and precisely the ledge the overlap exists to delete."* **But nine sides
is an authored number in the same file and the argument treated it as a constraint.** At 16 sides the
step is **1.96%**, at 20 it is **1.24%**, both inside the overlap budget. *"The experiment answered
'can I do this at 9 sides' when the question was 'can I do this'."*

Then it withdrew the instruction: phase-breaking a stack of coaxial solids of revolution *"makes a
faceted bollard instead of a smooth one"*.

#### The `amadeo-app` correction — UPHELD

*"You are right about the repository and I was wrong about it in the ADR amendment."* Verified: an
ordering against an unregistered label is a hard `UnknownLabel`, and `App::new` therefore cannot
register `occlude_voices` with a correct ordering. It noted that an `install_earshot` helper is the
established shape and is worth doing when a second game wants it.

#### F5: six of seven, and the source-side diagnosis confirmed

`ring_lining` measures **12.48 / 12.67** at native against the 0.82 that produced §4 #16 — *"`speck`
was the right call and the render follows it."* It **retired** the `w_y180` crop with arithmetic: at
12 m the fog term is **28.3%** toward a near-black colour, and *"a surface behind 28.3% of near-black,
several mip levels down, cannot carry per-texel grain and should not be asked to."*

**And it held the other crop against the submission's defence, by deriving the distance.** Eye at
1.59 m, focal `540/tan 35° = 771.2`, row 600 at pitch −30 → **2.32 m**, inside the 3 m the clause
names. The cause: *"the fix went into the lining and not into the floor"* — `shelter_floor.png` was
**2.82–6.18** at native against the lining's **10.0–12.7**.

Credited without reservation: crates (44.7 against 126.1, *"the best single object change in the
submission"*), key board 3 of 4, saturation, call plate, bunks 12/3 recounted, and the reticle at
143–190 levels of separation across all four landmarks.

#### F2b and F6 passed, both with something recorded rather than smoothed over

F2b: *"the two tests discriminate against each other properly… this repository has shipped the version
without it for four milestones."* But it flagged that **no reachable capture shows the warden in
motion** — `at_warden` at tick 150 and `playing` at 400 and 900 are all byte-identical to tick 5 — so
the mechanism is proven by test only, and it rewrote the capture clause into a headless one.

F6: all three clauses verified in substance. It confirmed the downward-cast rejection independently.
**And it recorded that F6 closing does not satisfy `docs/11` §9**: gain-only at 0.30× is −10.5 dB,
*"arithmetically indistinguishable from the warden standing 1.8× further away"*, and the design needs
*far* told from *behind that bulkhead*. Its own amendment allows the descope, so it did not fail the
row — but the ledger says so.

#### The observation that produced ordered change 9

*"Fifteen seconds of game time changes not one pixel."* No `.anim` file anywhere in `games/warren`,
while `games/atrium` has had `lamp_flicker.anim` since M2. *"A horror interior in which nothing
whatever moves is a still life, and the engine has had the system for four milestones."*

---

### Review 31 — engine gate — `98488c1` — F1 and F5 — **NO VERDICT (session limit)**

The third review lost this way, after the two before review 19. It was submitted at a clean tree with
F5's clause (a) fixed and F1 built for the first time, and it stopped mid-evidence. Its last line is
the whole of what it delivered:

> *"One check before I credit F5: the same grain at close range, where the yaw-270 frame magnifies it
> most."*

**That is not a verdict and it is not recorded as one.** F1 and F5 both stay 🟡 **built, awaiting
verdict**, and the next session re-sends the same submission at the same commit. `docs/13` §1b's status
column says the same.

**Read the last line as a lead, not as a finding.** It was about to check the screed's grain *at close
range in the yaw-270 frame*, which is the one crop of the spawn where the floor is nearest the camera
and the mip chain has averaged least. The measurements this session took at 1.1 m and 2.32 m are in
the submission; whatever it was about to look at is not among them, and the honest reading is that it
had a specific magnification in mind that nobody has run.
