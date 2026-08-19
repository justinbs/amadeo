//! Procedural surface textures, as engine code.
//!
//! ```text
//! use amadeo_texture::{Bond, Canvas, Courses, Space, maps};
//!
//! let wall = Courses {
//!     seed: 0x51A9_3C7E,
//!     rows: 6,
//!     across: 4,
//!     variation: 0.35,
//!     joint: 0.008,
//!     bond: Bond::Broken,
//! };
//! let mut colour = Canvas::new(512, Space::Srgb);
//! colour.fill(|u, v| {
//!     let stone = wall.at(u, v);
//!     let shade = 0.55 + (wall.tone(stone, 0) - 0.5) * 0.2;
//!     let joint = stone.joint;
//!     let value = shade * (1.0 - joint) + 0.3 * joint;
//!     [value, value, value, 1.0]
//! });
//! let png = colour.encode().expect("encodes");
//! ```
//!
//! # Why this is an engine crate rather than a binary in a game
//!
//! `docs/12-the-bar.md` §3 rates textures **"Partly"** authorable by an agent, and that rating is
//! the requirement most likely to be quietly dodged: *"there shouldn't be a part where it asks me to
//! create textures for it"*. What existed was `games/vault`'s `pix`, which writes pixel art from a
//! hand-drawn grid of characters, and two near-identical copies of a noise routine in two other
//! games. None of it reaches a 512² tiling stone with a normal map, and every game that wanted one
//! was going to write its own third copy.
//!
//! Engine gate item 13 made it a deliverable: *"the generator is engine code with its own tests"*.
//!
//! # What it does not do
//!
//! It does not write files, and it does not know about assets. [`Canvas::encode`] hands back PNG
//! bytes and stops there. Deciding where a texture lives and what its asset id is belongs to the
//! *tool* that runs the generator, which is a binary in a game — the same division `amadeo-gltf`
//! keeps, where the parser is a crate and the importer is a CLI command.
//!
//! It also has no opinion about what a material is *for*. A stone, a plaster and a rusted steel
//! plate are all courses, noise and ramps in different proportions, and which proportions make a
//! Victorian brick tunnel is genre knowledge that lives in a game (invariant I4).
//!
//! # Determinism
//!
//! Everything here is `+ - * /`, `floor` and integer hashing, which IEEE 754 specifies exactly
//! (ADR 0044). The one exception is the sRGB transfer curve in [`maps::to_srgb_byte`], which uses
//! `powf` and is safe because it runs at generation time and its output is a committed PNG rather
//! than gameplay state — the same exemption `amadeo-image`'s mip chain takes.
//!
//! So the same seed writes the same bytes on every machine, which is what makes a generated texture
//! something a repository can hold as text plus a generator rather than as a binary blob nobody can
//! diff (invariant I1).

pub mod maps;
pub mod masonry;
pub mod noise;

pub use maps::{Canvas, Space};
pub use masonry::{Bond, Courses, Stone, Wall};
pub use noise::{hash01, octaves, tiling};
