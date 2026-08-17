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
/// - `Value::F32` is widened to the `f64` that **spells the same**, rather than by `f64::from`. That
///   is not a formality: `f64::from(0.18_f32)` is `0.18000000715255737`, and the shortest text that
///   round-trips *that* `f64` really is all seventeen digits — so every JSON reply carrying a
///   component read like that while the same component's scene spelling read `0.18`. See
///   `widen_for_json` below.
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
        Value::F32(inner) => Json::Float(widen_for_json(*inner)),
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

/// Widens an `f32` to the `f64` that *spells the same*, rather than to the one with the same bits.
///
/// # Why this is not `f64::from`
///
/// It was, and the JSON half of every reply read like this:
///
/// ```text
/// "scene": "  StairMesh\n    rise 0.18\n    run 0.28\n"                the scene spelling
/// "json":  {"rise": 0.18000000715255737, "run": 0.2800000011920929}    the same numbers
/// ```
///
/// `f64::from(0.18_f32)` is exactly `0.18000000715255737`, and the shortest text that round-trips
/// *that* `f64` really is all seventeen digits of it. `amadeo-scene::format_float_32` fixed the same
/// defect for the scene spelling; **this is the half an agent parses**, and it reaches
/// `describe --example`, `world.entity`, and every other JSON reply carrying a component.
///
/// The round trip through the `f32`'s own decimal is the operation rather than a trick: it asks for
/// the shortest decimal that identifies this `f32` — which is what `{}` on an `f32` gives — and then
/// for the `f64` nearest to that. Parsing cannot fail on a string Rust has just produced, and the
/// fallback is the old behaviour rather than a panic.
///
/// **It allocates a `String` per float, so it belongs on a diagnostic path and nowhere else.** That
/// is free here — `world.entity` and `describe` run once per request — and would not be in a
/// collection pass that touches every drawable every frame.
fn widen_for_json(value: f32) -> f64 {
    if !value.is_finite() {
        // Nothing `f32`-specific to gain, and an infinity has no decimal spelling to parse.
        return f64::from(value);
    }
    format!("{value}")
        .parse()
        .unwrap_or_else(|_| f64::from(value))
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

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn an_f32_reaches_json_at_f32_precision() {
        // The JSON half of the defect `amadeo-scene::format_float_32` fixed for the scene half. This
        // is the spelling an agent *parses*, so it was the more consequential of the two: a component
        // dumped over RPC read `"rise": 0.18000000715255737` while the same component's scene form
        // read `rise 0.18`, and the two are the same number.
        assert_eq!(value_to_json(&Value::F32(0.18)).to_compact(), "0.18");
        assert_eq!(value_to_json(&Value::F32(0.28)).to_compact(), "0.28");
        assert_eq!(value_to_json(&Value::F32(1.2)).to_compact(), "1.2");

        // Still visibly a float, and still exact for values that were never the problem.
        assert_eq!(value_to_json(&Value::F32(1.0)).to_compact(), "1.0");
        assert_eq!(value_to_json(&Value::F32(0.5)).to_compact(), "0.5");
        assert_eq!(value_to_json(&Value::F32(-2.5)).to_compact(), "-2.5");

        // An `f64` is untouched: its own shortest spelling was always right.
        assert_eq!(
            value_to_json(&Value::F64(0.180_000_007_152_557_37)).to_compact(),
            "0.18000000715255737"
        );
    }

    #[test]
    fn a_non_finite_f32_still_widens_the_plain_way() {
        // The early return exists because "nan".parse::<f64>() would work but there is nothing
        // f32-specific to gain, and an infinity's decimal spelling is not a number at all.
        assert!(widen_for_json(f32::NAN).is_nan());
        assert_eq!(widen_for_json(f32::INFINITY), f64::INFINITY);
        assert_eq!(widen_for_json(f32::NEG_INFINITY), f64::NEG_INFINITY);
    }
}
