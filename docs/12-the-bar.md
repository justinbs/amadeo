# 12 — The bar the engine is held to

> Set by Justin in session 20, in response to the Warren reading as a bland engine test.
> **This is the standard every engine gate is judged against from here on.** It is written down
> because the critic agent starts cold each time and because a bar that lives in one conversation is
> a bar that quietly slips.

---

## 1. The standard, in Justin's terms

> *"The engine is something that should be on that level before even making a game… it should be
> polished not just passable… something you wouldn't be afraid to showcase in front of an audience of
> a thousand people."*

The reference point is **Hello Games** — the studio behind No Man's Sky — as the model of a small
team shipping something of genuine scale and ambition. Not a AAA studio, not a hobby engine: the
AA-indie tier, where a handful of people ship a product that stands next to commercial work.

**"Passable" is explicitly not the bar.** The failure this document exists to prevent is the one that
already happened once: a system that works, is tested, is documented, and would embarrass anyone who
showed it.

---

## 2. What it must be capable of

Named by Justin as the class of game the engine should be able to make. Each is here because it
demands something different, and together they define the gaps.

| Game | What it demands that the others do not |
|---|---|
| **Minecraft** | Voxel worlds, infinite streaming, player-modified terrain |
| **Terraria** | 2D at scale — tilemaps, sprites, a world of tiles rather than meshes |
| **Project Zomboid** | Isometric 2.5D, large simulated populations, systemic AI |
| **RimWorld** | Isometric, deep simulation, emergent narrative, enormous UI |
| **Stellaris** | Grand-strategy UI, thousands of entities, data-driven content, no character at all |
| **Kenshi** | Huge open world, squad AI, no level loading |
| **No Man's Sky** | Procedural generation of *everything* — terrain, flora, fauna, texture — at planet scale |
| **Palworld** | Creatures, skeletal animation, taming and combat systems, open world |
| **Schedule I** | First-person simulation, economy, interiors, systemic NPCs |

### The three that are first-class, chosen in session 20

The first engine review made the case that **the nine are effectively nine different engines** — no
commercial engine is good at all of them, and Unity has a thousand engineers and is bad at
grand-strategy UI and bad at voxels. Holding Amadeo to all nine at once is holding it to a standard
nothing meets. Justin chose:

| Tier | Games | What they demand together |
|---|---|---|
| **First-class now** | **Project Zomboid · No Man's Sky · Schedule I** | Tilemaps and isometric sorting; pathfinding; entity throughput in the thousands; procedural worlds at planet scale; transparency and vegetation; skeletal animation for populated worlds; first-person interiors |
| **Next, and not precluded** | **RimWorld · Stellaris** | A real UI framework — dense, scrollable, sortable, tabbed — and simulation throughput at 10k+ entities |
| Reachable, not driving | Minecraft, Terraria, Kenshi, Palworld | Each shares most of its needs with a first-class target |

**This is not the pick the review recommended, and the difference matters.** It proposed the
Minecraft/No Man's Sky/Terraria family, on the grounds that voxels and determinism are where the
engine is already unusual. Justin's pick keeps No Man's Sky and swaps in **Project Zomboid** and
**Schedule I** — which pulls `mod-tilemap`, isometric y-sorting, pathfinding and entity throughput
*up* the plan rather than deferring them, and makes populated worlds a requirement rather than a
later nicety.

The union of the three is a wider engine than the review's pick, and it is coherent: Zomboid and
Schedule I both need many simulated NPCs, so throughput and pathfinding serve both; No Man's Sky
needs the world scale and the vegetation, which nothing else does.

Three things fall out of the full list:

- **2D and isometric are not optional.** Three of the nine are 2D or isometric, which matches trap 9
  in `CLAUDE.md` and the target list in `00-vision.md`. `mod-tilemap` currently sits in M7.
- **Skeletal animation is required.** Palworld is creatures; Zomboid and Kenshi are people. The
  engine has none, and `docs/04` §14 and ADR 0066 §5 both record it as blocked on a rigged model the
  repository does not have. **Now `Q41`**, because a thing this document raises to a requirement is
  no longer a note in a subsystem list.
- **Procedural generation is the through-line.** Five of the nine are built on it, and it is the one
  area where this engine is already genuinely strong.

---

## 3. The requirement that is new, and the hardest

**Amadeo is an agent-native engine (`CLAUDE.md` I5). That has to extend to making the content, not
just to editing it.**

> *"As an agent centric engine, the engine should be capable of being utilized fully by Claude to
> create games, from all parts, there shouldn't be a part where it asks me to 'create textures for
> it', 'create sounds for it', 'create models for it', outside of choices, it shouldn't ask me to do
> all the manual labor for it."*

This is stronger than invariant I5 and it is the thing most likely to be quietly dodged. I5 says the
agent can do anything the editor can. **This says the agent can produce the game's assets.** An
engine where Claude can author a level but has to ask a human for a model is an engine that has
offloaded the expensive half.

**And it cuts both ways.** Justin must be able to work on the same game by hand, mixing his work with
Claude's — which is what M4's editor is for, and is why this has to be settled *before* the editor
rather than after.

### Where that stands today, honestly

| Asset | Can Claude author it? | How |
|---|---|---|
| Scenes, prefabs, levels | **Yes, fully** | Text, and this part is genuinely good |
| Materials | **Yes, fully** | Text |
| Environments, look, post | **Yes, fully** | Text |
| Textures | **Partly** | `games/vault`'s `pix` writes PNG from hand-written text. Procedural and pixel-art only |
| Sounds | **Partly** | `sounds.rs` synthesises `.wav`. No music, no recorded audio |
| Environment maps | **Yes** | `gloom.rs`, `sky.rs` write `.hdr` |
| **Meshes** | **Barely** | `BoxMesh`, `PlaneMesh`, `ArchMesh`. `amadeo-gltf` is a **reader with no writer**, and the scene format has **no raw-geometry path** — so a Rust binary cannot emit a model the way it emits a sound |
| Fonts | **No** | Downloaded from the web |
| Animation | **No** | No skeletal system at all |

**The mesh row is the one that matters most**, and a review of `games/warren` measured its
consequence directly: thirteen meshes, thirteen axis-aligned boxes, in a game the engine's own
documentation described as complete.

### What "Claude can author it" is allowed to mean

Justin is explicit that the answer is not "Claude must sculpt a dragon":

> *"I know that claude isn't at the pinnacle of creating high poly 3d models, so thats why low poly
> should be usable, any use case should be accounted for… Utilize what its good/great at and make the
> best possible product."*

So:

- **Low poly is a first-class art direction, not a fallback.** It is what an agent can genuinely
  author well, it is a legitimate and commercially proven style, and the engine must make it look
  *deliberate* rather than unfinished. Low, mid and high poly should all be supported paths.
- **Procedural and parametric authoring is the agent's strength** — a mesh described by numbers and
  rules, which is exactly what `ArchMesh` is and what a modular kit is.
- **The web is available**: research, and freely-licensed fonts, textures and audio, used with their
  licences carried beside them (as `games/atrium` already does for Bebas Neue).
- **External tools are available**: an MCP tool such as Blender, if one gets the engine somewhere it
  cannot otherwise go.

The test is not "did Claude model it by hand". It is: **can a game be finished without asking Justin
to do the manual labour?**

---

## 4. The gate order, and the rule

Set by Justin, and it applies to every part:

1. **Design the game** — `docs/11-the-warren.md`
2. **Improve and change the engine** to the bar above
3. **Add what the engine is missing**
4. **Build the game**

> **Nothing proceeds to the next part until the critic agent passes the current one.**

The critic is `.claude/agents/critic.md`. Its verdict is binding: where it disagrees, its changes are
followed. Where it is factually wrong about the repository, it is corrected with evidence — that has
happened several times, and it has verified and withdrawn every time.

**Session 26 added a second agent, and it does not change the sentence above.** The **designer**
(`.claude/agents/designer.md`, brief in `docs/15-the-designer.md`) owns player experience, story,
worldbuilding, theming and UI — what a thing *means* to a player — and nothing else. **Its decisions
are binding on the implementer too.** But the critic is the main agent: where the two disagree, the
critic's ruling stands, and **only Justin and the critic may decline or stop the designer.** The
designer never writes ✅ in `docs/13` and never uses the words POLISHED or NOT POLISHED.

This is deliberately slow and it is the correct trade. The alternative was demonstrated: five
sessions of systems that each passed their own tests and together produced something the owner
called a bland engine test.

### The order beyond this gate, set by Justin in session 26

The four parts above are one gate around one game. **What follows it is now fixed**, and it is
recorded here because it changes what "finished" means for the current work:

1. **Finish `games/warren`** — the M3 demo game, in **two or three sessions**, on the re-scoped plan
   in `docs/13` §1b: **seven FINISH rows**, ordered F2, F2b, F6, F5, F4, F1, F3, with a stated fallback
   for what ships if the budget runs out. It is a demo game in UE5's sense: it feels like a game and is one.
2. **The editor** — M4. `games/warren` being finished is what gives the editor something real to open.
3. **The first published game** — a full survival game in the Project Zomboid line: isometric, 2D and
   3D. **Not a demo.** `docs/13` item 40, and its design document goes to the critic before any code,
   the same way `docs/11` did.

**This narrows the current gate; it does not lower the bar.** AA indie is still the standard and the
critic is still binding. What changed is how many objects one demo is allowed to spend a review cycle
on — see `docs/13` §1b for the six rows that remain and the reason each cut is a cut.

---

## 5. How to read this against the roadmap

`docs/05-roadmap.md` is a plan for reaching M3–M7 in order. **This document raises the bar those
milestones are measured at**, and it moves work forward rather than adding a new milestone:

- **M4's editor** now depends on §3 being settled, because "Justin builds a level with only the
  editor, Claude builds one with only text" is not a real test if Claude cannot make the assets.
- **M7's 2D modules** are named in §2 as a capability the engine must have, not a deferred nicety.
- **Skeletal animation** moves from "blocked on an asset" to a requirement, which means the asset
  problem is the engine's to solve rather than to wait on.

Nothing here contradicts `00-vision.md`. It sharpens what "done enough to make games" means, which
that document deliberately left as a sentence.
