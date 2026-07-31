//! The agent interface layer: the engine's answers to the three questions an agent has to ask.
//!
//! `docs/03-ai-native-design.md` frames them as *"what can I do?"*, *"what did I just do?"*, and
//! *"is it still right?"*. This crate answers the first two mechanically:
//!
//! | Question | Here |
//! |---|---|
//! | what can I do? | [`describe`] — every registered component, its fields, units, ranges, and docs |
//! | what did I just do? | [`entity`] and [`query`] — the live world, by handle or by component filter |
//! | is it still right? | determinism and golden replays, which live in `amadeo-app` |
//!
//! ```
//! use amadeo_agent::{describe, query};
//! use amadeo_ecs::{ComponentRegistry, World};
//! use amadeo_transform::{Parent, Transform2d};
//!
//! let mut registry = ComponentRegistry::new();
//! registry.register::<Transform2d>().expect("registers");
//! registry.register::<Parent>().expect("registers");
//!
//! // "What can I do?" -- generated from the code, so never stale and never a guess.
//! let schema = describe(&registry).to_pretty();
//! assert!(schema.contains("\"unit\": \"rad\""));
//!
//! // "What did I just do?"
//! let mut world = World::new();
//! let entity = world.spawn();
//! world.insert(entity, Transform2d::at(1.0, 2.0));
//!
//! let found = query(&world, &registry, &["Transform2d"]).to_compact();
//! assert!(found.starts_with(r#"{"count":1,"#));
//! ```
//!
//! # Everything here is read-only, deliberately
//!
//! Describing and inspecting cannot change simulation state, so none of it can perturb what it is
//! measuring. The mutating half of the protocol — `world.spawn`, `world.set_component`, `sim.step` —
//! is a separate piece of work, and keeping the read side independent means an agent can always look
//! at a world without wondering whether looking changed it.
//!
//! # What is not here yet
//!
//! **The transport.** There is no JSON-RPC server yet — this is the library one is built on. The
//! process model was the open part, and **ADR 0016 settled it**: because ADR 0011 compiles game logic
//! into the game binary, that binary is the only process that knows a game's components, so it hosts
//! the agent and `amadeo-cli` launches it and talks over stdio. The first transport is one-shot
//! batch: one invocation is one fresh deterministic run.
//!
//! **JSON parsing.** [`Json`] writes; nothing reads. The RPC server needs a parser, and that is a
//! larger piece than the writer. ADR 0016 keeps it hand-written and in this crate, next to the
//! writer, rather than taking a dependency.
//!
//! **`render.capture` and `render.describe`.** The agent's eyes. They need the 2D renderer, which
//! needs Q3 settled.

mod describe;
mod inspect;
mod json;
mod parse;
mod rpc;

pub use describe::{DESCRIBE_FORMAT_VERSION, describe, describe_type};
pub use inspect::{entity, query, value_to_json};
pub use json::Json;
pub use parse::{JsonError, JsonErrorKind, MAX_DEPTH};
pub use rpc::{
    PROTOCOL_VERSION, Request, RpcError, WORLD_METHODS, dispatch_world, failure, success,
};
