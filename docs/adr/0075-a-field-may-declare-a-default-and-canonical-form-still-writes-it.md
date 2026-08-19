# ADR 0075 — A field may declare a default, and canonical form still writes it

**Status:** Accepted · **Date:** 2026-08-17 · **Builds on:** ADR 0012, ADR 0014, ADR 0029, ADR 0069 ·
**Closes:** **Q32**

## Context

Reflection required **every** field to be present when reading a value, so adding one to a component
invalidated every text file that spelled that component out. Q32 recorded this in session 14 after
`Material` gained `normal_texture` and `normal_strength` and five files had to change, and it
predicted the problem would recur: "PBR will do it again; so will triplanar; so will every texture
slot."

It did. The first engine review (session 20) measured the consequence from the other end — every
texture slot of every material in the repository is empty, and 23 of 23 meshes are `BoxMesh` — and
the ordered plan that came out of it has **two** consecutive items that need new `Material` fields:
alpha cutout, then the texture path. Q32 stopped being a P2 annoyance and became the thing in front
of the work.

### Q32's stated tension was half wrong, and that is what changed the answer

Q32 argued that `MissingField` is load-bearing for two reasons, and both need correcting:

- *"`MissingField` is what catches a typo'd field name."* It is not. `from_value` checks for
  **unknown** fields before it reads any field at all, so `roughnes 0.5` already fails with
  `UnknownField` naming the typo and listing the real fields. A typo produces a wrong field name, and
  a wrong field name is caught by the check that exists for it.
- *"...and what makes a prefab that lost a component refuse to load rather than silently reverting
  (ADR 0029's deliberate opposite of Unity)."* That is a missing **component**, resolved by
  `ComponentRegistry`, and nothing here touches it.

What `MissingField` genuinely protects is a field that is *actually absent from the file*, which for
a hand-authored asset is precisely the case worth allowing. The tension Q32 described was real when
written and does not survive checking.

### The trap that decides the shape

The engine already has machinery for this: ADR 0069's `default_value` builds a default from a schema,
for a lenient save restore. Reusing it here — every missing field takes its type's zero or empty —
would be the smallest possible change and it would be **silently wrong**. `Material::default`'s own
doc comment says why:

> a derived default would be black, fully smooth and fully transparent, which is not a material so
> much as an absence of one.

A `.material` omitting `base_colour` would load as transparent black. A `BoxMesh` omitting `size`
would be a zero-size box that draws nothing and reports no fault. That is the failure mode this
project has been burned by most often — a plausible substituted value hiding a defect — and it ruled
the cheap option out.

## Decision

### 1. A field may declare a default, as a Rust expression, and one that does not stays required

```rust
pub struct Material {
    #[reflect(default = [1.0, 1.0, 1.0, 1.0])]
    pub base_colour: [f32; 4],
    #[reflect(default = 1.0)]
    pub normal_strength: f32,
    #[reflect(default = AlphaMode::Opaque)]
    pub alpha_mode: AlphaMode,
}
```

**Opt-in per field, not blanket**, which is what keeps the guarantee where it is worth having: a
field with no `default` behaves exactly as it always has, so no existing type changes meaning and no
existing file changes validity. `BoxMesh::size` stays required because a box with no size is not a
box.

**A bare expression rather than serde's `default = "path::to::fn"` string.** The value is what a
reader wants to see, and `#[reflect(default = 1.0)]` shows it where `default = "one_point_oh"` hides
it behind a function nobody will look up. It also means the compiler type-checks the default against
the field: the expression is passed through `<FieldType as Reflect>::to_value`, so a default of the
wrong type is a build error rather than a surprise at load.

The cost is that the engine author has to remember the attribute. That cost is paid **loudly** — a
forgotten default means files must name the field, which is the behaviour of today and fails with
`MissingField` naming it — and it is the reason this is the option chosen over a blanket rule.

### 2. The default is in the schema, so an agent can see it

`FieldInfo::default` carries the value, which means `describe` reports which fields may be omitted
and what omitting them means. This is not decoration: `docs/12-the-bar.md` §3 requires that Claude
can author a game's assets, and a rule that only exists in a Rust attribute is one an agent authoring
a `.material` cannot discover. It is deliberately **not** part of ADR 0069's layout fingerprint, for
the reason docs and ranges are not — a default cannot move a state hash, so including it would
reject good saves for a change that provably does not matter.

### 3. Canonical form still writes every field

`amadeo fmt` output is unchanged: every field, alphabetically, including ones sitting at their
default. Two things follow, and the second is the reason:

- **A file's meaning never depends on the engine version's defaults.** A file that omits a field is
  valid input; it is not valid *canonical* output. So an archived asset says what it is, and a diff
  between two materials shows their real values rather than requiring the reader to know what was
  left out.
- **`amadeo fmt` is the migration tool, with no new flag.** An old file that predates a new field
  reads (the field defaults), and writing it back adds the field at its default. Q32's third option
  proposed `fmt --migrate` for exactly this; declaring defaults gets it for free.

The alternative — omitting fields at their default — was considered and rejected. It makes the churn
vanish entirely rather than becoming a command, which is a real advantage, and it pays for it by
making every old file's behaviour hostage to a default nobody remembers changing.

## Consequences

- **Hand-authoring an asset gets substantially shorter**, which is the half of this that serves I1 and
  `docs/12` §3 rather than the engine's own convenience. A material is eight lines today, of which
  five are values the author does not care about; the required set is `base_colour`, `metallic` and
  `roughness`.
- **`Material` can grow.** The two items behind Q32 in the plan — an alpha mode, and the texture path
  — no longer rewrite twelve files to add a field, and neither will the ones after them.
- **A default and a hand-written `Default` impl are two places that can disagree.** `Material`,
  `Environment` and `Theme` all have hand-written `Default`s for the reason quoted above. A test
  asserts that a component built from an empty value equals its `Default`, so the two cannot drift
  without CI saying so.
- **The `--example` output is unchanged**, and stays a complete instance. A minimal example is
  arguably now the required fields only; that is a separate call about what an example is *for*, and
  a complete one remains valid input.
- **Nothing about the state hash changes.** A default is applied while building a value, so a
  component built from a file with an omitted field is byte-identical to one built from a file that
  spells it out. This is what makes the change safe to apply to existing types: no replay, golden or
  pinned hash moves.

---

## Amendment, session 21 — the title of this ADR is wrong about `amadeo fmt`

**"Canonical form still writes every field, so `amadeo fmt` is the migration tool with no new flag"
is false, and it was false when it was written rather than having gone stale.** Found while adding
`Environment::sky_ambient` (ADR 0079): running `amadeo fmt` over the four `.environment` files in this
repository did **not** add the new field to the three that omit it.

`format_scenes` in `crates/amadeo-cli/src/main.rs` is `amadeo_scene::parse` followed by
`amadeo_scene::to_text`. Both operate on the **document** — the text as parsed — and neither has a
`TypeRegistry`. It cannot add a field it has never heard of, and by ADR 0016 it never will: `fmt` is
the one command that is deliberately standalone and does not launch the game, which is exactly what
makes it usable on a project whose game will not compile.

### What is actually true

"Canonical form writes every field" is a property of the **engine's** writer — the path
`snapshot.take` and any future editor save take, where the text is generated from a `Value` built out
of a live component, and a `Value` necessarily has every field in it. It has never been a property of
reformatting a file that was hand-written.

### What this costs, which is less than it sounds

- **Nothing is broken and nothing is at risk.** A file omitting a field loads on the declared default,
  produces a byte-identical component, and hashes identically. That is the whole point of ADR 0075
  and it works.
- **But this ADR's claim that "no file's meaning depends on the engine's defaults" does not hold for
  a hand-written file.** If a default is ever changed, every file that omits that field changes
  meaning silently. The mitigation that does exist is real: `describe --example` publishes the
  declared default as authoring advice (`crates/amadeo-agent/src/example.rs`), so the value is
  discoverable rather than buried in an attribute — which was the other half of why the default rides
  in the schema.
- **So the operative rule is: treat a declared default as part of the format's contract, not as a
  tuning knob.** Changing one is a change to the meaning of every file that omits it, and there is no
  tool that will tell you which files those are.

### What was deliberately not built

A `fmt --migrate` that launches the game and rewrites every file with a complete field list. It is a
real option and it is the honest fix, but it is a new command shape rather than a flag — it would make
`fmt` sometimes-standalone and sometimes-not, which is the property ADR 0016 chose it for. Recording
the correct behaviour is worth more right now than a tool nothing yet needs; the moment a declared
default has to *change*, this is the thing to build first.
