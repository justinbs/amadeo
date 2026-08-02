//! The JSON-RPC envelope, and the methods that need only a world and a registry.
//!
//! ADR 0016 decided the process model: the game binary hosts the agent, because it is the only
//! process that knows a game's components. This module is the protocol half of that — parsing a
//! request, routing it, and shaping a reply. The *hosting* half (owning an `App`, running ticks,
//! reading stdin) lives in `amadeo-app`, because `App` is defined there and invariant I6 forbids
//! this crate from reaching down for it.
//!
//! # Why JSON-RPC 2.0 rather than something bespoke
//!
//! `docs/03-ai-native-design.md` picked it for being boring: inspectable by eye, writable by hand at
//! a shell prompt, and already understood by every client anyone might write. This is a subset — one
//! request, one response, no batching and no notifications — but it is a *conforming* subset, so a
//! stock client works against it.
//!
//! # One-shot, and read-only
//!
//! Per ADR 0016 the first transport is batch: one invocation is one fresh deterministic run. How far
//! to run is a launch argument, not a method, which is what keeps every method here read-only —
//! asking a world a question cannot change the answer to the next question. `sim.step`,
//! `world.spawn`, and `world.set_component` arrive with the persistent session, when M4's editor
//! needs a connection that outlives a single question.

use crate::inspect::{entity, query};
use crate::json::Json;
use amadeo_ecs::{ComponentRegistry, World};
use std::collections::BTreeMap;

/// The protocol version this build speaks.
///
/// Bumped when a method changes shape in a way an existing client would notice. `describe` reports
/// it, so a client can check before assuming.
pub const PROTOCOL_VERSION: u32 = 1;

/// A parsed request.
#[derive(Debug, Clone, PartialEq)]
pub struct Request {
    /// The method name, such as `world.query`.
    pub method: String,
    /// The method's named arguments.
    ///
    /// A map rather than a [`Json`] because this server takes named arguments only, so "params is
    /// an object" is a fact worth having in the type instead of re-checking at every use. A request
    /// with no `params` gets an empty map.
    pub params: BTreeMap<String, Json>,
    /// The client's correlation id, echoed back untouched.
    ///
    /// JSON-RPC allows a string, a number, or null, so it is kept as a [`Json`] rather than
    /// narrowed — reshaping a client's id is a good way to break it.
    pub id: Json,
}

/// Why a request could not be answered.
///
/// The codes below -32000 are JSON-RPC's own reserved range and mean what the spec says they mean.
/// Using them rather than inventing codes is the point of picking a standard protocol.
#[derive(Debug, Clone, PartialEq, Eq, thiserror::Error)]
pub enum RpcError {
    /// The request was not valid JSON.
    #[error("could not parse the request: {message}")]
    ParseFailed {
        /// The parser's message, which carries a line and column.
        message: String,
    },

    /// Valid JSON, but not a valid JSON-RPC request.
    #[error("not a valid request: {message}")]
    InvalidRequest {
        /// What was wrong with it.
        message: String,
    },

    /// No such method.
    #[error("no method named `{method}`. Known methods: {known}")]
    UnknownMethod {
        /// What was asked for.
        method: String,
        /// A comma-separated list, so the reply is enough to recover from without documentation.
        known: String,
    },

    /// The method exists but its arguments were wrong.
    #[error("`{method}`: {message}")]
    BadParams {
        /// The method that was called.
        method: String,
        /// What was wrong, and what was expected.
        message: String,
    },
}

impl RpcError {
    /// The JSON-RPC error code for this failure.
    #[must_use]
    pub fn code(&self) -> i64 {
        match self {
            RpcError::ParseFailed { .. } => -32700,
            RpcError::InvalidRequest { .. } => -32600,
            RpcError::UnknownMethod { .. } => -32601,
            RpcError::BadParams { .. } => -32602,
        }
    }
}

impl Request {
    /// Reads a request from one line of JSON.
    ///
    /// # Errors
    ///
    /// [`RpcError::ParseFailed`] if the text is not JSON, [`RpcError::InvalidRequest`] if it is
    /// JSON but not shaped like a request.
    pub fn parse(text: &str) -> Result<Request, RpcError> {
        let document = Json::parse(text).map_err(|error| RpcError::ParseFailed {
            message: error.to_string(),
        })?;

        let Json::Object(members) = &document else {
            return Err(RpcError::InvalidRequest {
                message: "a request is a JSON object, such as \
                          {\"jsonrpc\":\"2.0\",\"method\":\"describe\",\"id\":1}"
                    .to_string(),
            });
        };

        // The spec requires the version marker. Checking it costs nothing and catches a client
        // pointed at the wrong kind of server immediately rather than three methods later.
        match members.get("jsonrpc") {
            Some(Json::String(version)) if version == "2.0" => {}
            Some(other) => {
                return Err(RpcError::InvalidRequest {
                    message: format!(
                        "\"jsonrpc\" must be the string \"2.0\", found {}",
                        other.to_compact()
                    ),
                });
            }
            None => {
                return Err(RpcError::InvalidRequest {
                    message: "missing \"jsonrpc\": \"2.0\"".to_string(),
                });
            }
        }

        let method = match members.get("method") {
            Some(Json::String(name)) => name.clone(),
            Some(other) => {
                return Err(RpcError::InvalidRequest {
                    message: format!("\"method\" must be a string, found {}", other.to_compact()),
                });
            }
            None => {
                return Err(RpcError::InvalidRequest {
                    message: "missing \"method\"".to_string(),
                });
            }
        };

        let params = match members.get("params") {
            Some(Json::Object(arguments)) => arguments.clone(),
            None => BTreeMap::new(),
            Some(other) => {
                return Err(RpcError::InvalidRequest {
                    message: format!(
                        "\"params\" must be an object, found {}. \
                         This server takes named arguments, not positional ones",
                        other.to_compact()
                    ),
                });
            }
        };

        let id = members.get("id").cloned().unwrap_or(Json::Null);

        Ok(Request { method, params, id })
    }

    /// A string argument, or an error naming what was expected.
    ///
    /// # Errors
    ///
    /// [`RpcError::BadParams`] if the argument is missing or is not a string.
    pub fn string_param(&self, name: &str) -> Result<&str, RpcError> {
        match self.params.get(name) {
            Some(Json::String(text)) => Ok(text),
            Some(other) => Err(self.bad_params(format!(
                "`{name}` must be a string, found {}",
                other.to_compact()
            ))),
            None => Err(self.bad_params(format!("missing required argument `{name}`"))),
        }
    }

    /// An optional string argument.
    ///
    /// # Errors
    ///
    /// [`RpcError::BadParams`] if present but not a string.
    pub fn optional_string_param(&self, name: &str) -> Result<Option<&str>, RpcError> {
        match self.params.get(name) {
            None | Some(Json::Null) => Ok(None),
            Some(Json::String(text)) => Ok(Some(text)),
            Some(other) => Err(self.bad_params(format!(
                "`{name}` must be a string, found {}",
                other.to_compact()
            ))),
        }
    }

    /// A whole-number argument.
    ///
    /// # Errors
    ///
    /// [`RpcError::BadParams`] if the argument is missing or is not an integer.
    pub fn int_param(&self, name: &str) -> Result<i64, RpcError> {
        match self.params.get(name) {
            Some(Json::Int(value)) => Ok(*value),
            Some(other) => Err(self.bad_params(format!(
                "`{name}` must be a whole number, found {}",
                other.to_compact()
            ))),
            None => Err(self.bad_params(format!("missing required argument `{name}`"))),
        }
    }

    /// A list-of-strings argument, defaulting to empty when absent.
    ///
    /// # Errors
    ///
    /// [`RpcError::BadParams`] if present but not an array of strings.
    pub fn string_list_param(&self, name: &str) -> Result<Vec<String>, RpcError> {
        let Some(value) = self.params.get(name) else {
            return Ok(Vec::new());
        };

        let Json::Array(items) = value else {
            return Err(self.bad_params(format!(
                "`{name}` must be an array of strings, found {}",
                value.to_compact()
            )));
        };

        let mut names = Vec::with_capacity(items.len());
        for item in items {
            match item {
                Json::String(text) => names.push(text.clone()),
                other => {
                    return Err(self.bad_params(format!(
                        "every entry in `{name}` must be a string, found {}",
                        other.to_compact()
                    )));
                }
            }
        }
        Ok(names)
    }

    /// Builds a [`RpcError::BadParams`] already tagged with this method's name.
    #[must_use]
    pub fn bad_params(&self, message: impl Into<String>) -> RpcError {
        RpcError::BadParams {
            method: self.method.clone(),
            message: message.into(),
        }
    }
}

/// Wraps a successful result in a JSON-RPC response envelope.
#[must_use]
pub fn success(id: &Json, result: Json) -> Json {
    Json::object([
        ("jsonrpc", Json::string("2.0")),
        ("result", result),
        ("id", id.clone()),
    ])
}

/// Wraps a failure in a JSON-RPC response envelope.
///
/// The message is the error's `Display` text, which every error in this crate writes to be
/// actionable on its own — Pillar 5 treats that as a functional requirement, and an agent reading
/// this reply has no way to ask a follow-up question.
#[must_use]
pub fn failure(id: &Json, error: &RpcError) -> Json {
    Json::object([
        ("jsonrpc", Json::string("2.0")),
        (
            "error",
            Json::object([
                ("code", Json::Int(error.code())),
                ("message", Json::string(error.to_string())),
            ]),
        ),
        ("id", id.clone()),
    ])
}

/// The methods answerable from a world and a registry alone.
pub const WORLD_METHODS: &[&str] = &[
    "assets.list",
    "describe",
    "world.entity",
    "world.list",
    "world.query",
];

/// Answers a request that needs only the world and the registry.
///
/// Returns `Ok(None)` when the method is not one of [`WORLD_METHODS`] — the caller is expected to
/// try its own methods next and produce [`RpcError::UnknownMethod`] if it has none either. That
/// keeps the two halves of the method table from having to know about each other.
///
/// # Errors
///
/// [`RpcError::BadParams`] if a known method's arguments are wrong.
pub fn dispatch_world(
    request: &Request,
    world: &World,
    registry: &ComponentRegistry,
) -> Result<Option<Json>, RpcError> {
    match request.method.as_str() {
        "describe" => {
            // With no argument, the whole schema. With one, a single type — which is the common
            // case once an agent knows what it is looking for, and much less to read.
            match request.optional_string_param("type")? {
                None => {
                    let mut document = crate::describe(registry);
                    if let Json::Object(members) = &mut document {
                        members.insert(
                            "protocol_version".to_string(),
                            Json::Int(i64::from(PROTOCOL_VERSION)),
                        );
                    }
                    Ok(Some(document))
                }
                Some(name) => {
                    let info = registry.info(name).ok_or_else(|| {
                        request.bad_params(unknown_component_message(name, registry))
                    })?;
                    Ok(Some(crate::describe_type(info)))
                }
            }
        }

        // ADR 0020 requires this to exist *before* ids become the reference syntax, so that the
        // first agent to author a scene can look the ids up rather than guess at them.
        "assets.list" => Ok(Some(crate::assets::list(world))),

        "world.list" => {
            let entities: Vec<Json> = world
                .entities()
                .into_iter()
                .map(|handle| {
                    Json::object([
                        ("index", Json::Int(i64::from(handle.index()))),
                        ("generation", Json::Int(i64::from(handle.generation()))),
                    ])
                })
                .collect();

            Ok(Some(Json::object([
                ("count", Json::Int(entities.len() as i64)),
                ("entities", Json::Array(entities)),
                ("tick", Json::Int(world.tick().0 as i64)),
            ])))
        }

        "world.entity" => {
            let index = request.int_param("entity")?;
            let handle = resolve_entity(request, world, index)?;
            Ok(Some(entity(world, registry, handle)))
        }

        "world.query" => {
            let names = request.string_list_param("components")?;

            // A filter naming a component nobody registered would silently match nothing, which
            // reads exactly like "there are none of those" — the plausible-but-wrong answer Pillar 2
            // exists to eliminate. Say so instead.
            for name in &names {
                if !registry.contains(name) {
                    return Err(request.bad_params(unknown_component_message(name, registry)));
                }
            }

            let borrowed: Vec<&str> = names.iter().map(String::as_str).collect();
            Ok(Some(query(world, registry, &borrowed)))
        }

        _ => Ok(None),
    }
}

/// Finds a live entity by slot index.
///
/// The client sends an index rather than an `index:generation` pair because that is what `world.list`
/// and every diagnostic message show first. Looking it up in the live set rather than minting a
/// handle means a stale index is an error here instead of an empty component dump later.
fn resolve_entity(
    request: &Request,
    world: &World,
    index: i64,
) -> Result<amadeo_ecs::Entity, RpcError> {
    let live = world.entities();

    let wanted = u32::try_from(index)
        .map_err(|_| request.bad_params(format!("`entity` must be a slot index, found {index}")))?;

    live.into_iter()
        .find(|handle| handle.index() == wanted)
        .ok_or_else(|| {
            request.bad_params(format!(
                "no live entity at index {wanted}. \
                 Call `world.list` for the entities that do exist"
            ))
        })
}

/// "No such component" plus the ones that do exist, so the reply is enough to recover from.
fn unknown_component_message(name: &str, registry: &ComponentRegistry) -> String {
    let known: Vec<&str> = registry.names().collect();
    if known.is_empty() {
        return format!(
            "no component named `{name}` is registered, and neither is anything else. \
             The game registers its components with `App::register_component`"
        );
    }
    format!(
        "no component named `{name}` is registered. Registered: {}",
        known.join(", ")
    )
}

#[cfg(test)]
mod tests {
    use super::*;
    use amadeo_ecs::Component;
    use amadeo_reflect::Reflect;

    #[derive(Debug, Clone, Copy, PartialEq, amadeo_core::StableHash, Reflect)]
    struct Position {
        /// Where it is, across.
        x: f32,
        /// Where it is, up.
        y: f32,
    }
    impl Component for Position {}

    fn registry() -> ComponentRegistry {
        let mut registry = ComponentRegistry::new();
        registry.register::<Position>().expect("registers");
        registry
    }

    fn request(text: &str) -> Request {
        Request::parse(text).unwrap_or_else(|error| panic!("{text} should parse, but: {error}"))
    }

    #[test]
    fn parses_a_well_formed_request() {
        let parsed =
            request(r#"{"jsonrpc":"2.0","method":"describe","params":{"type":"P"},"id":7}"#);
        assert_eq!(parsed.method, "describe");
        assert_eq!(parsed.id, Json::Int(7));
        assert_eq!(parsed.optional_string_param("type"), Ok(Some("P")));
    }

    #[test]
    fn params_may_be_omitted_entirely() {
        let parsed = request(r#"{"jsonrpc":"2.0","method":"describe","id":1}"#);
        assert_eq!(parsed.optional_string_param("type"), Ok(None));
    }

    #[test]
    fn a_string_id_is_echoed_unchanged() {
        // JSON-RPC allows string ids and clients use them. Renormalising to a number would break
        // the client's own correlation.
        let parsed = request(r#"{"jsonrpc":"2.0","method":"describe","id":"abc-1"}"#);
        assert_eq!(parsed.id, Json::string("abc-1"));
        assert_eq!(
            success(&parsed.id, Json::Null).to_compact(),
            r#"{"id":"abc-1","jsonrpc":"2.0","result":null}"#
        );
    }

    #[test]
    fn malformed_requests_are_rejected_with_the_spec_codes() {
        let cases = [
            ("not json at all", -32700),
            ("[1,2]", -32600),                           // not an object
            (r#"{"method":"describe","id":1}"#, -32600), // no jsonrpc
            (r#"{"jsonrpc":"1.0","method":"x","id":1}"#, -32600),
            (r#"{"jsonrpc":"2.0","id":1}"#, -32600), // no method
            (r#"{"jsonrpc":"2.0","method":5,"id":1}"#, -32600),
            (
                r#"{"jsonrpc":"2.0","method":"x","params":[1],"id":1}"#,
                -32600,
            ),
        ];
        for (text, code) in cases {
            let error = Request::parse(text).expect_err(text);
            assert_eq!(error.code(), code, "for {text}: {error}");
        }
    }

    #[test]
    fn an_unknown_method_falls_through_to_the_caller() {
        // `Ok(None)` rather than an error, so the host can try its own methods before giving up.
        let world = World::new();
        let answered = dispatch_world(
            &request(r#"{"jsonrpc":"2.0","method":"sim.step","id":1}"#),
            &world,
            &registry(),
        );
        assert_eq!(answered, Ok(None));
    }

    #[test]
    fn describe_reports_the_protocol_version() {
        let world = World::new();
        let answer = dispatch_world(
            &request(r#"{"jsonrpc":"2.0","method":"describe","id":1}"#),
            &world,
            &registry(),
        )
        .expect("dispatches")
        .expect("describe is a world method");

        assert!(
            answer.to_compact().contains(r#""protocol_version":1"#),
            "got: {}",
            answer.to_compact()
        );
        assert!(answer.to_compact().contains("Position"));
    }

    #[test]
    fn describing_an_unregistered_type_lists_what_is_registered() {
        // Pillar 5: the reply has to be enough to recover from, because an agent cannot ask a
        // follow-up question.
        let world = World::new();
        let error = dispatch_world(
            &request(r#"{"jsonrpc":"2.0","method":"describe","params":{"type":"Nope"},"id":1}"#),
            &world,
            &registry(),
        )
        .expect_err("Nope is not registered");

        assert_eq!(error.code(), -32602);
        assert!(
            error.to_string().contains("Registered: Position"),
            "got: {error}"
        );
    }

    #[test]
    fn querying_an_unregistered_component_is_an_error_not_an_empty_result() {
        // An empty result reads as "there are none", which is a different and wrong answer.
        let mut world = World::new();
        let spawned = world.spawn();
        world.insert(spawned, Position { x: 1.0, y: 2.0 });

        let error = dispatch_world(
            &request(
                r#"{"jsonrpc":"2.0","method":"world.query","params":{"components":["Velocity"]},"id":1}"#,
            ),
            &world,
            &registry(),
        )
        .expect_err("Velocity is not registered");

        assert!(
            error.to_string().contains("no component named `Velocity`"),
            "got: {error}"
        );
    }

    #[test]
    fn world_list_then_entity_round_trips() {
        // The pair an agent actually uses: find out what exists, then look at one of them.
        let mut world = World::new();
        let spawned = world.spawn();
        world.insert(spawned, Position { x: 1.5, y: -2.0 });

        let listed = dispatch_world(
            &request(r#"{"jsonrpc":"2.0","method":"world.list","id":1}"#),
            &world,
            &registry(),
        )
        .expect("dispatches")
        .expect("world.list is a world method");
        assert!(
            listed.to_compact().contains(r#""count":1"#),
            "got: {}",
            listed.to_compact()
        );

        let index = i64::from(spawned.index());
        let looked_up = dispatch_world(
            &request(&format!(
                r#"{{"jsonrpc":"2.0","method":"world.entity","params":{{"entity":{index}}},"id":2}}"#
            )),
            &world,
            &registry(),
        )
        .expect("dispatches")
        .expect("world.entity is a world method");

        assert!(
            looked_up.to_compact().contains("Position"),
            "got: {}",
            looked_up.to_compact()
        );
        assert!(
            looked_up.to_compact().contains("1.5"),
            "got: {}",
            looked_up.to_compact()
        );
    }

    #[test]
    fn a_stale_entity_index_says_so_rather_than_returning_nothing() {
        let world = World::new();
        let error = dispatch_world(
            &request(r#"{"jsonrpc":"2.0","method":"world.entity","params":{"entity":99},"id":1}"#),
            &world,
            &registry(),
        )
        .expect_err("99 does not exist");

        assert!(
            error.to_string().contains("no live entity at index 99"),
            "got: {error}"
        );
        assert!(error.to_string().contains("world.list"), "got: {error}");
    }

    #[test]
    fn bad_argument_types_name_the_method_and_the_argument() {
        let world = World::new();
        let error = dispatch_world(
            &request(
                r#"{"jsonrpc":"2.0","method":"world.entity","params":{"entity":"three"},"id":1}"#,
            ),
            &world,
            &registry(),
        )
        .expect_err("entity must be a number");

        assert_eq!(
            error.to_string(),
            "`world.entity`: `entity` must be a whole number, found \"three\""
        );
    }

    #[test]
    fn error_replies_carry_the_id_and_the_code() {
        let error = RpcError::UnknownMethod {
            method: "nope".to_string(),
            known: "describe".to_string(),
        };
        let reply = failure(&Json::Int(4), &error);
        let text = reply.to_compact();

        assert!(text.contains(r#""code":-32601"#), "got: {text}");
        assert!(text.contains(r#""id":4"#), "got: {text}");
        assert!(text.contains("no method named `nope`"), "got: {text}");
    }
}
