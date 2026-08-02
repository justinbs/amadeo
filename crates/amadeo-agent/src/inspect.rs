//! "What did I just do?" — the live world, as JSON.
//!
//! Pillar 3 of `docs/03-ai-native-design.md`. After a change, what is the state of the running game,
//! and did it change the way I intended. A human answers that by looking at the screen; this answers
//! it mechanically, which is what makes unattended progress possible.

use crate::json::Json;
use amadeo_ecs::{ComponentRegistry, Entity, World};
use amadeo_reflect::Value;

/// Renders a reflected [`Value`] as JSON.
///
/// The two models line up almost exactly — both have sorted-key structs, ordered lists, and typed
/// scalars — so this is mostly a rename. The two places they differ:
///
/// - `Value::Unit` becomes `null`. It means "no data", which is the closest JSON has.
/// - `Value::Enum` becomes `"VariantName"` when it carries nothing, and
///   `{"variant": ..., "payload": ...}` when it does. A bare string is what a reader expects for a
///   plain enum, and dressing it up as an object would make the common case unreadable.
#[must_use]
pub fn value_to_json(value: &Value) -> Json {
    match value {
        Value::Unit => Json::Null,
        Value::Bool(inner) => Json::Bool(*inner),
        Value::I64(inner) => Json::Int(*inner),
        // A u64 past i64::MAX cannot be an i64. Falling back to a float loses precision but keeps
        // the document valid and the magnitude right; the alternative is emitting a wrong integer.
        Value::U64(inner) => {
            i64::try_from(*inner).map_or_else(|_| Json::Float(*inner as f64), Json::Int)
        }
        Value::F32(inner) => Json::Float(f64::from(*inner)),
        Value::F64(inner) => Json::Float(*inner),
        Value::String(inner) => Json::string(inner),
        Value::List(items) => Json::Array(items.iter().map(value_to_json).collect()),
        Value::Struct(fields) => Json::Object(
            fields
                .iter()
                .map(|(name, inner)| (name.clone(), value_to_json(inner)))
                .collect(),
        ),
        // A map becomes a JSON object too, which is exactly right for a reader: `{"jump": {...}}`
        // is what anyone expects. The struct/map distinction is real in `Value` and is preserved
        // where it matters — `describe` reports the *kind*, so a client that needs to tell them
        // apart asks the schema rather than guessing from the data.
        Value::Map(entries) => Json::Object(
            entries
                .iter()
                .map(|(key, inner)| (key.clone(), value_to_json(inner)))
                .collect(),
        ),
        Value::Enum(inner) => match inner.payload.as_ref() {
            Value::Unit => Json::string(&inner.variant),
            payload => Json::object([
                ("variant", Json::string(&inner.variant)),
                ("payload", value_to_json(payload)),
            ]),
        },
    }
}

/// Everything on one entity.
///
/// Returns `null` for an entity that does not exist, rather than an error: "what is on this entity"
/// has a perfectly good answer for a dead handle, and it is "nothing".
#[must_use]
pub fn entity(world: &World, registry: &ComponentRegistry, entity: Entity) -> Json {
    if !world.contains(entity) {
        return Json::Null;
    }

    let components = registry
        .components_of(world, entity)
        .into_iter()
        .map(|(name, value)| (name, value_to_json(&value)));

    Json::object([
        ("id", entity_id(entity)),
        ("components", Json::Object(components.collect())),
    ])
}

/// Every entity that has **all** of the named components.
///
/// An empty filter matches everything, which is how you ask "what is in this world at all".
///
/// Component names that are not registered simply match nothing, so a typo yields an empty result
/// rather than an error. That is the wrong trade for *writing* — a misspelled component in a scene
/// file is reported loudly — and the right one for a query, where narrowing to nothing is a normal
/// answer.
#[must_use]
pub fn query(world: &World, registry: &ComponentRegistry, filter: &[&str]) -> Json {
    let mut matched = Vec::new();

    for handle in world.entities() {
        let components = registry.components_of(world, handle);
        if filter.iter().all(|name| components.contains_key(*name)) {
            matched.push(Json::object([
                ("id", entity_id(handle)),
                (
                    "components",
                    Json::Object(
                        components
                            .into_iter()
                            .map(|(name, value)| (name, value_to_json(&value)))
                            .collect(),
                    ),
                ),
            ]));
        }
    }

    Json::object([
        ("count", Json::Int(matched.len() as i64)),
        ("entities", Json::Array(matched)),
    ])
}

/// An entity handle, rendered so both halves are visible.
///
/// Index and generation separately rather than an opaque number, because "is this the same entity or
/// a reused slot" is a question worth being able to answer by eye when reading a dump.
fn entity_id(entity: Entity) -> Json {
    Json::object([
        ("index", Json::Int(i64::from(entity.index()))),
        ("generation", Json::Int(i64::from(entity.generation()))),
    ])
}
