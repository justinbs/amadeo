//! The scene file format — the shared authoring surface Justin and Claude both write to.
//!
//! The syntax is specified in **ADR 0014**, chosen by the spike in `spikes/q2-scene-format/`.
//!
//! ```
//! use amadeo_scene::{parse, to_text};
//!
//! let source = "\
//! scene corridor_a
//! version 1
//!
//! entity a1 \"Corridor\"
//!   Transform
//!     position 0.0 0.0
//!     rotation 0.0
//!
//!   entity a2 \"CeilingLight\"
//!     PointLight
//!       color 1.0 0.85 0.6
//!       intensity 3.2
//! ";
//!
//! let document = parse(source).expect("valid scene");
//! assert_eq!(document.name, "corridor_a");
//! assert_eq!(document.entities.len(), 1);
//! assert_eq!(document.entities[0].children[0].name, "CeilingLight");
//!
//! // Already canonical, so formatting changes nothing (invariant I2).
//! assert_eq!(to_text(&document), source);
//! ```
//!
//! # Two layers, and this crate is layer 1
//!
//! **Syntax** — parsing and formatting, with no reference to the reflection registry. A scene naming
//! a component from a module you have not loaded still parses and still formats, which is what makes
//! `amadeo fmt` usable on any file.
//!
//! **Schema** — binding those values to real component types, checking fields against the registry,
//! and narrowing numbers to their declared widths. That is [`validate`] (`amadeo check`) and
//! [`instantiate`] (scene loading).
//!
//! Keeping them apart means a syntax error and a schema error are different things with different
//! messages, rather than one confusing pile.
//!
//! # What is deliberately not here
//!
//! **Resolving a prefab id to a file.** [`instantiate_with`] takes an already-parsed
//! [`PrefabLibrary`]; finding the files and reading them is the caller's job, because that is asset
//! work and `amadeo-assets` sits above this crate.
//!
//! Override *semantics* used to be listed here as undecided. They are settled — ADR 0029: an
//! override is a top-level patch on the instance **root**, and no syntax can name anything inside a
//! prefab. That is what makes nesting safe rather than merely careful.

mod document;
mod instantiate;
mod parse;
mod validate;
mod write;

pub use document::{SceneDocument, SceneEntity};
pub use instantiate::{
    InstantiateError, Instantiated, PrefabLibrary, instantiate, instantiate_with,
};
pub use parse::{INDENT, ParseError, ParseErrorKind, parse};
pub use validate::{Diagnostic, validate};
pub use write::to_text;
// The scalar encoding, shared with `amadeo-snapshot`. Exposed rather than duplicated because
// byte-stability (I2) depends on these being exactly right, and two copies of `format_float` would
// be two things to keep in step. The two *formats* stay separate crates; only the spelling of a
// number and the escaping of a string are common.
pub use write::{
    MAX_PLAIN_DIGITS, component_block, escape, format_float, format_float_32, inline_value,
};
