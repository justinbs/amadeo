//! "What can I do?" — the schema, as JSON.
//!
//! Pillar 2 of `docs/03-ai-native-design.md`. This is what `amadeo describe` prints, and the point
//! of it is that the answer is **generated from the code**: never a stale doc, never a guess. If the
//! schema says `Transform` has no `rotation_degrees` field, that is known before the code is
//! written rather than after a confusing debug session.

use crate::json::Json;
use amadeo_ecs::{ComponentRegistry, World};
use amadeo_reflect::{
    FieldInfo, Interpolation, RegistryError, ScalarKind, SyncPolicy, TypeInfo, TypeKind,
    TypeRegistry,
};

/// The version of the description document's own shape.
///
/// Bumped when the *layout below* changes incompatibly, so a tool reading an old dump can tell
/// "written by an older engine" from "corrupt". Separate from any component's own version.
///
/// **2** — added `resources`, `types` and `manual`, and a `length` on list kinds (ADR 0030).
pub const DESCRIBE_FORMAT_VERSION: u32 = 2;

/// Where the things this document deliberately does *not* carry are written down.
///
/// M1 exit gate 4 asked `describe` to be sufficient to write a new component and system without
/// reading engine source, and it is not: it is a schema, and how you *declare* a component, register
/// one, write a system or query the world is API rather than data. ADR 0030 decided that stays in
/// the repo's documentation, and that `describe` should say so rather than leave a reader guessing
/// that the absence means "impossible".
///
/// A pointer rather than the prose itself, on purpose: prose copied into a protocol reply is
/// documentation that nothing recompiles, so it drifts. A path cannot. Anything speaking this
/// protocol has the repo checked out, because ADR 0016 has the CLI build and launch the game.
pub const MANUAL_PATH: &str = "docs/07-working-with-the-code.md";

/// Describes everything the engine knows about itself.
///
/// ```text
/// {
///   "components": { "Transform": { ... } },   // things you can put on an entity
///   "resources":  { "Run": { ... } },         // things the world holds exactly one of
///   "types":      { "Phase": { ... } },       // every type the two above name, transitively
///   "manual": "docs/07-working-with-the-code.md",
///   "format_version": 2
/// }
/// ```
///
/// # Why `types` is separate from `components`
///
/// They answer different questions. `components` is "what can I put on an entity" — a closed list an
/// agent picks from. `types` is "what does this field's type mean" — a lookup table. `Phase` belongs
/// in the second and would be a lie in the first: you cannot spawn an entity holding a `Phase`.
///
/// # Errors
///
/// [`RegistryError`] if a resource and a component share a canonical name with different shapes.
/// There is no honest document to emit in that case, because one name would have to mean two things.
pub fn describe(world: &World, registry: &ComponentRegistry) -> Result<Json, RegistryError> {
    // Start from the component registry — which already holds every type the components name — and
    // fold the resources in on top. Cloned rather than mutated in place because `describe` answers a
    // question and must not change what it is describing.
    let mut types: TypeRegistry = registry.types().clone();
    world.register_resource_schemas(&mut types)?;

    // `registry.names()` is the components specifically, not everything in the type registry: since
    // ADR 0030 the latter also holds field types, and `f32` is not a component.
    let components = registry.names().filter_map(|name| {
        types
            .get(name)
            .map(|info| (name.to_string(), describe_type(info)))
    });

    let mut resource_types = TypeRegistry::new();
    world.register_resource_schemas(&mut resource_types)?;
    let resources = resource_types
        .iter()
        .map(|info| (info.name.clone(), describe_type(info)));

    Ok(Json::object([
        ("components", Json::Object(components.collect())),
        ("resources", Json::Object(resources.collect())),
        (
            "types",
            Json::Object(
                types
                    .iter()
                    .map(|info| (info.name.clone(), describe_type(info)))
                    .collect(),
            ),
        ),
        ("manual", Json::string(MANUAL_PATH)),
        (
            "format_version",
            Json::Int(i64::from(DESCRIBE_FORMAT_VERSION)),
        ),
    ]))
}

/// Describes one type.
#[must_use]
pub fn describe_type(info: &TypeInfo) -> Json {
    let mut members = vec![
        ("name", Json::string(&info.name)),
        ("docs", Json::string(&info.docs)),
        ("version", Json::Int(i64::from(info.version))),
    ];

    match &info.kind {
        TypeKind::Struct { fields } => {
            members.push(("kind", Json::string("struct")));
            // An array, not an object: `TypeInfo` keeps fields in declaration order because that is
            // how the author wrote them and how the type reads best. Turning it into an object would
            // sort them and lose that.
            members.push((
                "fields",
                Json::Array(fields.iter().map(describe_field).collect()),
            ));
        }
        TypeKind::Enum { variants } => {
            members.push(("kind", Json::string("enum")));
            members.push((
                "variants",
                Json::Array(
                    variants
                        .iter()
                        .map(|variant| {
                            Json::object([
                                ("name", Json::string(&variant.name)),
                                ("docs", Json::string(&variant.docs)),
                                (
                                    "fields",
                                    Json::Array(
                                        variant.fields.iter().map(describe_field).collect(),
                                    ),
                                ),
                            ])
                        })
                        .collect(),
                ),
            ));
        }
        TypeKind::Scalar(scalar) => {
            members.push(("kind", Json::string("scalar")));
            members.push(("scalar", Json::string(scalar_name(*scalar))));
        }
        TypeKind::List { element, length } => {
            members.push(("kind", Json::string("list")));
            members.push(("element", Json::string(element)));
            // Only when the type fixes it. Omitted for a `Vec`, where any count is valid — the same
            // rule as `unit` and `range`, so `"length" in kind` is a straight answer.
            if let Some(length) = length {
                members.push((
                    "length",
                    Json::Int(i64::try_from(*length).unwrap_or(i64::MAX)),
                ));
            }
        }
        TypeKind::Optional { inner } => {
            members.push(("kind", Json::string("optional")));
            members.push(("inner", Json::string(inner)));
        }
        // Reported as its own kind rather than as a struct, because the difference is exactly what a
        // client needs in order to render it: a struct gets a fixed inspector with one row per known
        // field, a map gets an add-and-remove list. The data is indistinguishable in JSON — both are
        // objects — so the schema is the only place this can be said.
        TypeKind::Map { key, value } => {
            members.push(("kind", Json::string("map")));
            members.push(("key", Json::string(key)));
            members.push(("value", Json::string(value)));
        }
    }

    Json::object(members)
}

/// Describes one field, including the metadata that keeps an agent from guessing.
fn describe_field(field: &FieldInfo) -> Json {
    let mut members = vec![
        ("name", Json::string(&field.name)),
        ("type", Json::string(&field.type_name)),
        ("docs", Json::string(&field.docs)),
    ];

    // Omitted rather than emitted as null when absent: a reader checking `"unit" in field` should
    // get a straight answer, and a document full of nulls is harder to read.
    if let Some(unit) = &field.unit {
        members.push(("unit", Json::string(unit)));
    }
    if let Some(range) = &field.range {
        members.push((
            "range",
            Json::object([
                ("min", Json::Float(range.min)),
                ("max", Json::Float(range.max)),
            ]),
        ));
    }

    // Only when it says something. Every field defaults to not-replicated (ADR 0012), and printing
    // that on all of them would bury the ones that do replicate.
    if field.replication.is_replicated() {
        members.push((
            "replication",
            Json::object([
                ("sync", Json::string(sync_name(field.replication.sync))),
                (
                    "interpolate",
                    Json::string(interpolation_name(field.replication.interpolate)),
                ),
            ]),
        ));
    }

    Json::object(members)
}

fn scalar_name(scalar: ScalarKind) -> &'static str {
    match scalar {
        ScalarKind::Bool => "bool",
        ScalarKind::SignedInt => "int",
        ScalarKind::UnsignedInt => "uint",
        ScalarKind::Float32 => "f32",
        ScalarKind::Float64 => "f64",
        ScalarKind::String => "string",
    }
}

/// The same spellings the `#[reflect(sync = "...")]` attribute accepts, so what an agent reads is
/// what it can write back.
fn sync_name(sync: SyncPolicy) -> &'static str {
    match sync {
        SyncPolicy::Never => "never",
        SyncPolicy::OnChange => "on_change",
        SyncPolicy::Always => "always",
    }
}

/// As with [`sync_name`], these match the attribute's spellings exactly.
fn interpolation_name(interpolation: Interpolation) -> &'static str {
    match interpolation {
        Interpolation::None => "none",
        Interpolation::Linear => "linear",
        Interpolation::Angular => "angular",
    }
}
