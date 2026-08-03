# art/

The sprites, as text.

Each `.pix` file is a picture you can read and edit in any editor. `!` lines are the palette; every
other non-comment line is one row of pixels, one character per pixel.

```text
# A two-pixel-wide example.
! . = 00000000     transparent
! # = 1a1a24ff     outline
.#
#.
```

A palette entry is `! <char> = <rrggbbaa>`, and `<char>` may be anything except `#` or `!` at the
start of a line. Eight hex digits, so alpha is explicit rather than implied — the engine's PPM
decoder has no alpha at all, which is why these become PNG.

## Turning them into assets

```
cargo run -p vault --bin pix
```

That writes a PNG next to each `.pix` file, into `games/vault/assets/textures/`, and it is
idempotent: running it twice produces identical bytes.

## Why the source is text and the asset is not

Invariant I1 wants a human to be able to author and diff everything. A PNG is neither, so the
**source** is this, and the PNG is derived — which is a miniature of the import pipeline ADR 0026
says is coming, with the same shape: hand-authorable input, machine-readable output, one command
between them.

If you would rather draw these in a paint program, delete the `.pix` file and drop a PNG straight
into `assets/textures/`. Nothing in the game refers to the `.pix` files; the game only ever sees
asset ids.
