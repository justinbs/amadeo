# assets/

Everything a game loads by id lives here. The directory is named by the game —
`games/quad-demo/src/main.rs` says `ASSET_DIRECTORY`, and the path is resolved against the nearest
`amadeo.toml` rather than against the working directory, so it means the same thing however the game
was started.

## An asset is named, not located

Each asset file has a `.ama-meta` sidecar next to it declaring its `id`. That id — not the path — is
what a scene refers to:

```text
entity a1 "Wall" from wall_concrete
```

The id defaults to the filename stem on import, so it reads exactly like a path on day one. But it
is *recorded*, so moving `textures/wall_concrete.png` into `textures/interior/` changes nothing.
ADR 0020 has the reasoning.

## Adding one

Drop the file in and run:

```
amadeo import
```

That writes a sidecar for every asset that has none, with the id taken from the filename. Until an
asset has a sidecar it is invisible — `amadeo assets` lists it under NOT IMPORTED rather than
pretending it does not exist.

To see what exists:

```
amadeo assets
```

## Editing a sidecar by hand

It is text and it is meant to be edited:

```text
id = "wall_concrete"
filter = "nearest"
```

`id` is the identity; everything else is import settings, which are passed through to whichever
importer handles that kind of file. Renaming the id is a real change and shows up in the diff of
every scene that referred to it — that asymmetry against a free file move is the whole point.

Note that `filter` is **recorded but not yet honoured** — every texture is sampled
nearest-neighbour today. See `docs/04-subsystems.md` §4.

## Image formats

Textures may be **PNG** or **PPM**, and the format is decided by what is inside the file rather than
by its extension — so a `.png` that is secretly something else gives you a useful message instead of
a confusing one.

- **PNG** is what you want for real art. Every variety decodes: 8- and 16-bit, greyscale, palette,
  with or without transparency.
- **PPM** is a real image format that is also a plain text file. `assets/textures/placeholder.ppm` is
  one, and you can open it and read the pixel values:

  ```text
  P3
  # comments are allowed anywhere
  2 2
  255
  92 90 86    74 72 69
  74 72 69    92 90 86
  ```

  Nothing exports PPM, and that is fine — it exists so a fixture can be checked by eye and edited
  without a paint program, the same trade every other text format in this project makes.

Decoding happens when the game runs, not when you import. That will change once textures need GPU
compression — the reasoning, and what would trigger it, is in `docs/adr/0026`.

## When a texture does not appear

A sprite whose texture is missing or broken draws a **magenta and near-black check** rather than
disappearing or crashing. That is deliberate (ADR 0021): you can see something is wrong, and the game
keeps running.

To find out what:

```
amadeo assets
```

If you would rather see your own stand-in, ship an asset with the id `placeholder` and it is used
instead.

## Sound formats

Sounds are **uncompressed `.wav`**, decoded into samples the first time something plays them. 16-bit
PCM, 24-bit PCM and 32-bit float all work, mono or stereo, along with the `WAVE_FORMAT_EXTENSIBLE`
variant a Windows tool writes above 16 bits.

**A compressed file is refused by name** rather than silently ignored, so an `.mp3` or an `.ogg`
dropped in gives you a message saying what it found. Adding those formats means adding `symphonia`,
and that is a deliberate decision nobody has needed to make yet.

**A placed sound should be mono.** This surprises people: a stereo recording already has its own left
and right, so a position has nothing left to decide, and a backend given a stereo sound to put
somewhere can only pick one of a few wrong answers. Music and narration are the opposite — those are
not placed at all, and stereo is exactly right for them.

## When a sound is not heard

**There is no placeholder sound, and that is a decision rather than an omission** (ADR 0060). A
missing texture draws magenta because nobody ships magenta — it is unmistakably not content. Nothing
audible has that property: a beep, a tone, a click are each indistinguishable from something a game
might legitimately play, and unlike magenta it would repeat, at the volume and in the position the
missing asset would have had.

So a sound that will not load is **silent**, and the report is the whole diagnosis. Same command:

```
amadeo assets
```

If the file is there and catalogued, the next suspects are, in order:

1. **The scene's `assets` block does not declare the id.** Nothing loads bytes it was not asked for.
2. **Nothing in the world has an `AudioListener`.** A world with no ears submits no voices at all —
   it does not guess where to hear from, because guessing is what puts a sound on the wrong side.
3. **`AudioSource::playing` is false, or its `gain` is zero.** Either one removes the voice before it
   reaches a backend.
4. **The game installed `NullAudio`.** Every headless build does, deliberately, and it makes no
   sound by design. Only a windowed build swaps in the real one.

## Generating an asset instead of committing one

Two games do this, and it is a pattern rather than a one-off:

```
cargo run -p vault  --bin pix     # .pix text  -> PNG sprites
cargo run -p atrium --bin tone    # a table of frequencies -> .wav
```

The reasoning is invariant I1's. A PNG and a `.wav` are both undiffable binaries, so where the
content is simple enough to describe in text, the **text is the source** and the binary is derived.
Both tools are idempotent: run them twice and the files are byte-identical, so a diff shows only what
actually changed.

Neither is a substitute for real art or real sound design. They exist so a demo is self-contained and
so the thing being tested is the engine rather than somebody's asset pipeline. Drop a real file in
with the same id and it is used instead, with nothing else to change.
