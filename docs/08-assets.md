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
