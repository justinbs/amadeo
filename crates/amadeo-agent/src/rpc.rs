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
use amadeo_reflect::TypeRegistry;
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

    /// The engine's own schema does not hold together, so no honest answer exists.
    ///
    /// In practice: a resource and a component share a canonical name but have different shapes, so
    /// `describe` cannot say what that name refers to. An authoring bug in the *game*, not in the
    /// request — which is why it is reported as an internal error rather than as bad params.
    #[error("the engine's schema is inconsistent: {source}")]
    Schema {
        /// The registry's complaint, which names the contested type and how to fix it.
        #[source]
        source: amadeo_reflect::RegistryError,
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
            // -32603 is JSON-RPC's "internal error": the request was fine and the server is not.
            RpcError::Schema { .. } => -32603,
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

    /// An optional whole-number argument.
    ///
    /// # Errors
    ///
    /// [`RpcError::BadParams`] if present but not a whole number. Absent is not an error — that is
    /// what "optional" means, and the caller supplies its own default.
    pub fn optional_int_param(&self, name: &str) -> Result<Option<i64>, RpcError> {
        match self.params.get(name) {
            None | Some(Json::Null) => Ok(None),
            Some(Json::Int(value)) => Ok(Some(*value)),
            Some(other) => Err(self.bad_params(format!(
                "`{name}` must be a whole number, found {}",
                other.to_compact()
            ))),
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
    "describe.example",
    "render.describe",
    "world.entity",
    "world.list",
    "world.query",
    "world.resources",
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
                    let mut document = crate::describe(world, registry)
                        .map_err(|source| RpcError::Schema { source })?;
                    if let Json::Object(members) = &mut document {
                        members.insert(
                            "protocol_version".to_string(),
                            Json::Int(i64::from(PROTOCOL_VERSION)),
                        );
                    }
                    Ok(Some(document))
                }
                Some(name) => {
                    // Looked up in the *whole* schema, not just the components. Since ADR 0030 that
                    // also holds resources and every nested type, so `describe Run` and
                    // `describe Phase` both answer — before, both were "no such component", which
                    // was true and useless.
                    let types = full_schema(world, registry)?;
                    let info = types.get(name).ok_or_else(|| {
                        request.bad_params(unknown_type_message(name, &types, registry))
                    })?;
                    Ok(Some(crate::describe_type(info)))
                }
            }
        }

        // The gap between "here is the schema" and "here is something that loads". Bevy's users hit
        // the same wall from the other side and wrote `discover_format` to close it, after
        // reverse-engineering formats out of error messages; ADR 0030 chose to have the engine
        // simply say it.
        "describe.example" => {
            let name = request.string_param("type")?;
            let types = full_schema(world, registry)?;
            let info = types
                .get(name)
                .ok_or_else(|| request.bad_params(unknown_type_message(name, &types, registry)))?;
            crate::example::describe_example(info, &types)
                .map_err(|message| request.bad_params(message))
                .map(Some)
        }

        // ADR 0020 requires this to exist *before* ids become the reference syntax, so that the
        // first agent to author a scene can look the ids up rather than guess at them.
        "assets.list" => Ok(Some(crate::assets::list(world))),

        // The other half of "what is in this world": entities carry components, and everything
        // else is a resource. Blocked until ADR 0027, because a resource behind a trait object had
        // thrown away everything about its type except a hash — the world could *hash* one but not
        // show one.
        //
        // An object keyed by name rather than an array of `{name, value}` pairs: a caller almost
        // always wants one specific resource, and `resources.SimRng` beats scanning a list for it.
        // Names are unique by construction — a `ResourceId` is the hash of one — so nothing is lost.
        // The agent's cheap eyes, and M1's exit gate requires them: verification of that milestone's
        // game is to be done "purely through `inspect`, headless runs, and `render.describe`, with
        // screenshots used only for final confirmation".
        //
        // Reads the world rather than the last frame, so it costs nothing when nobody asks and works
        // headlessly — see `amadeo_render::describe_frame` for why a `SpriteInstance` deliberately
        // has no entity id to read back.
        "render.describe" => {
            let description = amadeo_render::describe_frame(world);

            let drawn: Vec<Json> = description
                .drawn
                .iter()
                .map(|entry| {
                    let bounds = entry.bounds();
                    let mut members = vec![
                        ("entity", Json::Int(i64::from(entry.entity.index()))),
                        (
                            "generation",
                            Json::Int(i64::from(entry.entity.generation())),
                        ),
                        ("order", Json::Int(i64::from(entry.order))),
                        ("visible", Json::Bool(entry.visible)),
                        (
                            "center",
                            Json::Array(vec![
                                Json::Float(f64::from(entry.center[0])),
                                Json::Float(f64::from(entry.center[1])),
                            ]),
                        ),
                        (
                            "size",
                            Json::Array(vec![
                                Json::Float(f64::from(entry.size[0])),
                                Json::Float(f64::from(entry.size[1])),
                            ]),
                        ),
                        // `[left, top, right, bottom]`, so a client can answer "do these overlap"
                        // without redoing the projection.
                        (
                            "bounds",
                            Json::Array(
                                bounds.iter().map(|v| Json::Float(f64::from(*v))).collect(),
                            ),
                        ),
                    ];

                    match &entry.kind {
                        amadeo_render::DrawnKind::Quad => {
                            members.push(("kind", Json::string("quad")));
                        }
                        amadeo_render::DrawnKind::Sprite { texture } => {
                            members.push(("kind", Json::string("sprite")));
                            members.push(("texture", Json::string(texture)));
                        }
                        // Q26. Before this, a 3D world reported zero drawn entities through a
                        // default orthographic camera nobody authored — plausible and wrong, which
                        // is worse than an error. `center` and `size` are the screen rectangle the
                        // mesh's bounds project to, so "is it visible" means the same thing for a
                        // mesh as it already did for a sprite.
                        amadeo_render::DrawnKind::Mesh { mesh, material } => {
                            members.push(("kind", Json::string("mesh")));
                            members.push(("mesh", Json::string(mesh)));
                            members.push(("material", Json::string(material)));
                        }
                    }

                    Json::object(members)
                })
                .collect();

            Ok(Some(Json::object([
                (
                    "viewport",
                    Json::Array(vec![
                        Json::Int(i64::from(description.viewport[0])),
                        Json::Int(i64::from(description.viewport[1])),
                    ]),
                ),
                (
                    "camera",
                    Json::object([
                        // Named `center` rather than `eye` because that is what it means to a
                        // reader of a 2D description — the world point in the middle of the view.
                        // It comes from the camera entity's `Transform` rather than from the
                        // camera itself (ADR 0031), which is an internal move a client should not
                        // have to care about.
                        //
                        // **Three components since Q26.** A 2D camera's z is zero, so nothing that
                        // read the first two changes meaning; a 3D camera's height is most of what
                        // decides its view, and dropping it silently was the same confidently-wrong
                        // answer this whole method used to give a 3D world. Widened now rather than
                        // later because nothing outside this repository consumes the protocol yet,
                        // which is exactly when a shape change is cheap.
                        (
                            "center",
                            Json::Array(vec![
                                Json::Float(f64::from(description.eye[0])),
                                Json::Float(f64::from(description.eye[1])),
                                Json::Float(f64::from(description.eye[2])),
                            ]),
                        ),
                        // Reported only for an orthographic camera, because since ADR 0032 that is
                        // the only kind that has one — the projection carries its own parameters,
                        // so a perspective camera has no height to report rather than a meaningless
                        // one. Omitted rather than null, like every other optional field here.
                        (
                            "projection",
                            match description.camera.projection {
                                amadeo_render::Projection::Orthographic { height } => {
                                    Json::object([
                                        ("kind", Json::string("orthographic")),
                                        ("height", Json::Float(f64::from(height))),
                                    ])
                                }
                                amadeo_render::Projection::Perspective { fov, near, far } => {
                                    Json::object([
                                        ("kind", Json::string("perspective")),
                                        ("fov", Json::Float(f64::from(fov))),
                                        ("near", Json::Float(f64::from(near))),
                                        ("far", Json::Float(f64::from(far))),
                                    ])
                                }
                            },
                        ),
                        ("order", Json::Int(i64::from(description.camera.order))),
                    ]),
                ),
                ("drawn", Json::Int(description.drawn.len() as i64)),
                ("visible", Json::Int(description.visible_count() as i64)),
                (
                    "off_screen",
                    Json::Int(description.off_screen_count() as i64),
                ),
                ("entities", Json::Array(drawn)),
            ])))
        }

        "world.resources" => {
            let resources: Vec<(String, Json)> = world
                .resources()
                .into_iter()
                .map(|(name, value)| (name, crate::value_to_json(&value)))
                .collect();

            Ok(Some(Json::object([
                ("count", Json::Int(resources.len() as i64)),
                ("resources", Json::Object(resources.into_iter().collect())),
            ])))
        }

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

/// Components, resources, and every type either of them names — one place to look a name up.
///
/// Built per call rather than cached: it is a few hundred small clones, it happens only when
/// something asks a question, and a cache would need invalidating whenever a resource is inserted.
fn full_schema(world: &World, registry: &ComponentRegistry) -> Result<TypeRegistry, RpcError> {
    let mut types = registry.types().clone();
    world
        .register_resource_schemas(&mut types)
        .map_err(|source| RpcError::Schema { source })?;
    Ok(types)
}

/// "No such type", with the nearest matches, and the components when there are none.
///
/// Dumping the whole schema is no longer the right fallback: since ADR 0030 it holds every field
/// type as well, so `f32` and `list<array<f32, 2>>` are in it and a full list would bury the answer.
/// Near matches first — they are what a typo needs. Failing that, the **components**, because that is
/// the list someone reaching for `describe` almost always wanted.
///
/// Pillar 5: the reply has to be enough to recover from, because an agent cannot ask a follow-up.
fn unknown_type_message(name: &str, types: &TypeRegistry, registry: &ComponentRegistry) -> String {
    let lowered = name.to_lowercase();
    let near: Vec<&str> = types
        .names()
        .filter(|known| {
            let known = known.to_lowercase();
            known.contains(&lowered) || lowered.contains(&known)
        })
        .take(8)
        .collect();

    if !near.is_empty() {
        return format!(
            "no type named `{name}` is in this game's schema ({} known). Did you mean: {}?",
            types.len(),
            near.join(", ")
        );
    }

    let components: Vec<&str> = registry.names().collect();
    if components.is_empty() {
        return format!(
            "no type named `{name}` is in this game's schema, and no components are registered \
             at all. The game registers its components with `App::register_component`"
        );
    }
    format!(
        "no type named `{name}` is in this game's schema ({} known). Registered: {}. \
         `describe` with no argument also lists resources and every field type",
        types.len(),
        components.join(", ")
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

    /// A resource with something worth reading in it.
    #[derive(Debug, Clone, Copy, PartialEq, amadeo_core::StableHash, Reflect)]
    struct Score {
        /// Points so far.
        points: u32,
    }
    impl amadeo_ecs::Resource for Score {}

    #[test]
    fn world_resources_reports_every_resource_by_name() {
        // What ADR 0027 was for. Before the `Reflect` bound this method could not exist: a resource
        // behind a trait object had thrown away everything about its type except a hash.
        let mut world = World::new();
        world.insert_resource(Score { points: 7 });

        let reply = dispatch_world(
            &request(r#"{"jsonrpc":"2.0","method":"world.resources","id":1}"#),
            &world,
            &registry(),
        )
        .expect("dispatches")
        .expect("world.resources is a world method");

        let text = reply.to_compact();
        assert!(text.contains(r#""count":1"#), "got: {text}");
        assert!(text.contains(r#""Score":{"points":7}"#), "got: {text}");
    }

    #[test]
    fn world_resources_is_keyed_by_name_and_sorted() {
        // Sorted by *name* rather than by resource id, because an id is a hash and its ordering is
        // arbitrary — reproducible, but in a sequence no reader could predict.
        #[derive(Debug, Clone, Copy, PartialEq, amadeo_core::StableHash, Reflect)]
        struct Ammo {
            /// Rounds left.
            rounds: u32,
        }
        impl amadeo_ecs::Resource for Ammo {}

        let mut world = World::new();
        world.insert_resource(Score { points: 1 });
        world.insert_resource(Ammo { rounds: 2 });

        let reply = dispatch_world(
            &request(r#"{"jsonrpc":"2.0","method":"world.resources","id":1}"#),
            &world,
            &registry(),
        )
        .expect("dispatches")
        .expect("present");

        let text = reply.to_compact();
        let ammo = text.find("Ammo").expect("Ammo present");
        let score = text.find("Score").expect("Score present");
        assert!(ammo < score, "resources must be sorted by name: {text}");
    }

    #[test]
    fn an_empty_world_reports_no_resources() {
        let reply = dispatch_world(
            &request(r#"{"jsonrpc":"2.0","method":"world.resources","id":1}"#),
            &World::new(),
            &registry(),
        )
        .expect("dispatches")
        .expect("present");

        assert!(reply.to_compact().contains(r#""count":0"#));
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
