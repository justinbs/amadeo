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
//!   Transform2d
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
//! and narrowing numbers to their declared widths. That is `amadeo check` and scene loading, and it
//! is not built yet.
//!
//! Keeping them apart means a syntax error and a schema error are different things with different
//! messages, rather than one confusing pile.
//!
//! # What is deliberately not here
//!
//! **Prefab override *semantics*** (open question Q7) — nesting, propagation, and what happens when
//! a prefab changes under an instance that overrode some of its fields. This crate records overrides
//! faithfully and visibly, which is the requirement from `docs/04-subsystems.md` §9; deciding what
//! they *mean* is a separate and harder design problem.
//!
//! **Instantiating into a `World`.** That needs to construct components by name from the registry,
//! which is type-erased work that belongs above `amadeo-ecs`, not here.

mod document;
mod parse;
mod write;

pub use document::{SceneDocument, SceneEntity};
pub use parse::{INDENT, ParseError, ParseErrorKind, parse};
pub use write::to_text;
