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
//! use amadeo_transform::{Parent, Transform};
//!
//! let mut registry = ComponentRegistry::new();
//! registry.register::<Transform>().expect("registers");
//! registry.register::<Parent>().expect("registers");
//!
//! let mut world = World::new();
//!
//! // "What can I do?" -- generated from the code, so never stale and never a guess.
//! // The world is here because resources are part of the schema too (ADR 0030).
//! let schema = describe(&world, &registry).expect("no name collisions").to_pretty();
//! assert!(schema.contains("\"unit\": \"deg\""));
//!
//! // "What did I just do?"
//! let entity = world.spawn();
//! world.insert(entity, Transform::at(1.0, 2.0));
//!
//! let found = query(&world, &registry, &["Transform"]).to_compact();
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
//! # Where the transport lives
//!
//! This crate owns the *protocol*: [`Request`] parsing, the JSON-RPC envelope, and [`dispatch_world`]
//! for the methods that need only a world and a registry. It does not own the *hosting* — the stdin
//! loop and the methods needing an `App` are in `amadeo-app`, because `App` is defined there and
//! invariant I6 forbids reaching down for it. ADR 0016 explains the split; a client never sees it.
//!
//! Both halves of the JSON codec are here: [`Json`] writes, [`Json::parse`] reads, hand-written on
//! the same legibility grounds that kept PCG32 and FNV-1a hand-written.
//!
//! # What is not here yet
//!
//! **The mutating methods.** `world.spawn`, `world.set_component`, and `sim.step` wait for the
//! persistent session, which M4's editor is the first thing to actually need. Under the one-shot
//! batch model each invocation is a fresh deterministic run, so there is nothing to mutate *into*.
//!
//! **`render.capture` and `render.describe`.** The agent's eyes. They need the 2D renderer, and
//! specifically the sprite batcher that Q3's remaining third gets decided against.

mod anim;
mod assets;
mod audio;
mod describe;
mod example;
mod inspect;
mod json;
mod parse;
mod rpc;

pub use anim::describe as describe_animation;
pub use assets::list as list_assets;
pub use audio::describe as describe_audio;
pub use describe::{DESCRIBE_FORMAT_VERSION, MANUAL_PATH, describe, describe_type};
pub use example::describe_example;
pub use inspect::{entity, query, value_to_json};
pub use json::Json;
pub use parse::{JsonError, JsonErrorKind, MAX_DEPTH};
pub use rpc::{
    PROTOCOL_VERSION, Request, RpcError, WORLD_METHODS, dispatch_world, failure, success,
};
