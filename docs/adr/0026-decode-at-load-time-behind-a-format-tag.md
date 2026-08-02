# ADR 0026 — Images decode at load time, behind an explicit format tag

**Status:** Accepted · **Date:** 2026-08-03 · **Amends:** `docs/04-subsystems.md` §5

## Context

`amadeo-assets` reads files and stops there, deliberately: an asset is an id, a path, and a
`Vec<u8>`. A GPU cannot use any of that. A `.png` is compressed, a `.ppm` is text, and a texture
upload wants a flat grid of bytes. Something has to sit in between, and where that something lives
turned out to be a larger question than "which crate holds a PNG parser".

**`docs/04-subsystems.md` §5 already carried an answer, marked decided:**

> ✅ Import pipeline: source assets (`.png`, `.gltf`, `.wav`) are compiled once into internal
> formats. **The runtime never parses source formats.**

It was never written up as an ADR, and the code contradicts it — the asset layer hands over source
bytes and quad-demo loads a `.ppm` at startup. So the decision existed on paper, disagreed with
reality, and had never been tested against real work. That made it worth re-deriving rather than
obeying, which is what this ADR does.

### The two things that are actually being decided

They are usually conflated and they have very different costs to undo:

1. **When decoding happens** — in the game at load time, or in a build step that emits an
   engine-internal file.
2. **What the runtime holds afterwards** — a bare pixel grid, or a pixel grid that knows what
   format it is in.

### Why an import pipeline is eventually mandatory, not merely tidier

The reason is concrete and is not about purity. GPUs can sample **GPU-compressed** texture formats
(BC7, ASTC) directly, and those formats are the reason a shipped game's textures fit in video
memory: a 4096×4096 image is 64 MB as plain RGBA and 16 MB as BC7, and it stays 16 MB on the GPU.

Those formats are deliberately asymmetric — near-free to decode, brutally expensive to encode.
Reference BC7 encoders take on the order of an hour for a 4K texture single-threaded, and even fast
production encoders are seconds per texture. **Compression can therefore only ever happen offline.**
Godot, Unity and Unreal all import for exactly this reason. Bevy is the outlier that decodes `.png`
at runtime, and this project has already declined Bevy's answer twice on its merits (ADR 0021,
ADR 0022).

So decision 1 has a known destination. The live question was only *when* to get there.

### Why building the pipeline now would be machinery with no payload

The full import path is a compiled file format, a writer in `amadeo import`, a reader in the loader,
a cache directory, and cache invalidation — which `docs/04-subsystems.md` §5 itself still lists as
an unsolved ⚠️. Built today it would carry nothing: the only thing it could emit is the same RGBA
the decoder would have produced anyway, since there is no compressor to run. Designing that pipeline
against zero real content is how the wrong shape gets locked in.

## Decision

**Three parts.**

### 1. The runtime carries an explicit pixel format from the first line of code

```rust
pub struct TextureData { width: u32, height: u32, format: PixelFormat, pixels: Vec<u8> }
pub enum PixelFormat { Rgba8UnormSrgb }
```

One variant today. The *tag* is the point, not the variety. This is the genuinely
expensive-to-retrofit part of the decision and it costs nothing now: with it, adding BC7 later is a
new variant plus a new producer; without it, it means changing the loader, the cache, the backend,
and every test that asserts on pixels.

### 2. Decoding happens at load time, for now

The game decodes PPM and PNG into `TextureData` the first time a sprite asks for a texture. The
import pipeline is a **later addition**, not a later rewrite, because everything above the decoder
already speaks `TextureData` and will not notice where one came from.

### 3. PNG uses the `png` crate; PPM is hand-written

PPM stays hand-written (~200 lines) because it is the format that keeps `placeholder.ppm` a file a
human can read, edit, and check against the screen — the same trade invariant I1 makes for `.scene`,
`.replay`, and `.ama-meta`.

PNG takes a dependency. **This breaks a pattern the project has held to deliberately** — PCG32,
FNV-1a, a JSON reader and writer, and two text formats are all hand-rolled, and `thiserror` was the
only non-optional external dependency in the workspace. The distinction that justifies breaking it:
PNG's image data is zlib/DEFLATE-compressed, so hand-writing a decoder means hand-writing **inflate**
— Huffman table construction, fixed and dynamic blocks, LZ77 back-references — at roughly 800 lines
all told. PCG32 and FNV-1a are ~100 lines each with published test vectors, and a mistake in one
shows up immediately as a wrong known answer. **A mistake in inflate shows up as slightly corrupt
pixels**, which is precisely the failure a Rust-learning maintainer should never have to chase.

### 4. The decoder is its own crate, at the bottom of the graph

`amadeo-image` depends on **no engine crate at all**, so it sits beside `amadeo-derive` below even
`amadeo-core` and cannot participate in a cycle (invariant I6). Keeping it separate is also what
stops `png` spreading: `amadeo-scene`, `amadeo-agent`, `amadeo-app` and the CLI never pull it in,
because they never ask for pixels. `amadeo-render` does, and it also gained a dependency on
`amadeo-assets` — an edge that already existed conceptually, since `Sprite` has named its texture by
asset id since ADR 0020.

## The measurements

Taken on the target machine rather than argued, per the standing instruction. Both are one-time
costs: dependencies rebuild only when they change, so neither touches the iteration loop ADR 0011
measured.

| | crates pulled in | clean release build |
|---|---|---|
| `png` 0.18, alone | **9** | **3.2 s** |
| `image` 0.25, `default-features = false, features = ["png"]` | **15** | **14.5 s** |
| hand-written | 0 | 0 |

`image` was the obvious alternative and is what Bevy uses. It was rejected at 6 extra crates and 4×
the build time for formats no target game needs today. If a second source format is ever wanted,
re-run that comparison rather than stacking single-format crates.

## Consequences

**Good:**

- Sprites reach the screen this milestone, which is what M1's exit gate 1 has been waiting on.
- Every consumer of a texture already speaks a format-tagged type, so the import pipeline lands
  without touching them.
- The decoder is testable with no GPU and no engine: `amadeo-image` has 37 tests of its own.
- Format is chosen by **sniffing the leading bytes, not the extension**, which matters more here
  than in most engines — assets are addressed by id (ADR 0020) and the path is bookkeeping an author
  is explicitly allowed to change.

**Bad, and accepted:**

- The engine now has a dependency that is not `thiserror`, and a bug inside it is not fixable by
  reading this repository's source.
- Every shipped game binary carries a PNG decoder (~200 KB) whether it uses one or not.
- Decoding is lazy — on first draw rather than at ADR 0021's barrier — so a texture appearing for the
  first time can cost a frame hitch. Left untuned deliberately: a decode-at-the-barrier pass is about
  ten lines, and this project adds those when a hitch is **measured** (ADR 0023, ADR 0024), not when
  one is imagined.
- `docs/04-subsystems.md` §5's ✅ is now half-true and has been annotated to say so, rather than left
  to be discovered again.

**What has to happen before this is superseded:** the moment a target game wants compressed
textures, or mip levels, or a texture atlas built offline. At that point `amadeo import` grows a
compile step, `PixelFormat` grows variants, and `TextureCache::ensure` reads a compiled file instead
of calling `decode`. Nothing above `TextureCache` changes, which is the whole reason this ADR exists.

## What was rejected

- **Build the import pipeline now.** Honours `docs/04` as written and has the BC7 path ready on day
  one. Rejected because every piece of it would be built against no real content, and because the
  vertical slice (`CLAUDE.md` §5) is a sprite on screen, not a compiled-asset format with nothing to
  compile.
- **Decode to bare RGBA with no format tag.** Least code today, and the one genuinely expensive
  reversal in the set. Rejected outright.
- **Hand-write PNG.** Consistent with every earlier call, and zero dependencies. Rejected on the
  inflate argument above: the failure mode is corrupt pixels rather than an error, and it is the one
  part of this a maintainer would not want to debug alone.
- **The `image` crate.** Many formats behind one API. Rejected on the measurement — 6 more crates and
  4× the build time for capability nothing needs yet.
- **Put the decoder in `amadeo-assets`.** No new crate, and the thing that reads bytes decodes them.
  Rejected because it contradicts that crate's own stated position, and because Cargo unifies
  features across a workspace, so everything above `amadeo-assets` — scene, agent, app, CLI, games —
  would compile `png` whether it wanted to or not.
