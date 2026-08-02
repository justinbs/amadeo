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
