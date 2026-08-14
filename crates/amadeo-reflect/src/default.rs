//! A default [`Value`] for a type, built from its schema.
//!
//! # Why this exists
//!
//! ADR 0069: a save file has to survive the game being patched, and the smallest patch there is —
//! adding one field to one component — makes every existing save unreadable, because `from_value`
//! requires every field. Filling the missing one with a default is what fixes that.
//!
//! # Why the FIELD's type and not the component's
//!
//! The obvious design is a default *component* — `Transform::default()` — laid under whatever the
//! file supplied. It was rejected because getting one costs something at every call site: either a
//! `Default` bound on the `Component` trait, which every component in the engine would then have to
//! satisfy whether or not it has a meaningful zero, or an opt-in registration that is **silent when
//! forgotten**, which is the failure mode this project spends most of its design effort avoiding.
//!
//! A *field* type's default needs nobody to write anything. Every scalar in the engine has an
//! obvious one, a struct is its fields defaulted, and a list is empty. So a component gains a field
//! and every old save keeps loading, with no annotation on either side.
//!
//! # What it refuses, and why refusing is the feature
//!
//! **An enum has no default.** There is no principled answer — the first variant is a guess with
//! gameplay meaning, and `ShadowMode::Off`, `Bus::Effects` and `Screen::Playing` are all plausible
//! and all wrong in some game. So a component that gains an enum field is reported as unrestorable
//! *for that field, by name*, and a person decides what old saves should get.
//!
//! That is the same call [`crate::Value`] makes about `Option`: absence is `Value::Unit` rather than
//! a missing field, so "explicitly nothing" and "nobody wrote this down" stay distinguishable.

use crate::{ScalarKind, TypeInfo, TypeKind, TypeRegistry, Value};
use std::collections::BTreeMap;

/// Why no default could be built.
///
/// A string rather than an enum: every caller puts this straight into a report a person reads, and
/// the interesting part is always the *reason*, which is different for each case.
pub type NoDefault = String;

/// The default [`Value`] for a type, by name.
///
/// The entry point most callers want, since a schema names field types as strings.
///
/// # Errors
///
/// A sentence naming why, suitable for putting straight into a report: the type is not registered,
/// or it is an enum and therefore has no principled default.
pub fn default_value_for(name: &str, types: &TypeRegistry) -> Result<Value, NoDefault> {
    let Some(info) = types.get(name) else {
        return Err(format!(
            "no type called `{name}` is registered, so there is nothing to build a default from. \
             Every type a field names should have been registered with it (ADR 0030)"
        ));
    };
    default_value(info, types)
}

/// The default [`Value`] for a type whose schema is already in hand.
///
/// # Errors
///
/// See [`default_value_for`].
pub fn default_value(info: &TypeInfo, types: &TypeRegistry) -> Result<Value, NoDefault> {
    // A depth limit rather than a visited set: a type that contains itself by value cannot exist in
    // Rust, so the only way to recurse forever is through a registry that has been built wrongly.
    // Sixteen is far past any real component and stops a bad registry from blowing the stack.
    default_with_depth(info, types, 0)
}

/// How deep the recursion may go before it is treated as a broken registry rather than a deep type.
const MAX_DEPTH: usize = 16;

fn default_with_depth(
    info: &TypeInfo,
    types: &TypeRegistry,
    depth: usize,
) -> Result<Value, NoDefault> {
    if depth > MAX_DEPTH {
        return Err(format!(
            "`{}` nests more than {MAX_DEPTH} levels deep, which a real component does not. \
             The registry probably has a type that contains itself",
            info.name
        ));
    }

    match &info.kind {
        TypeKind::Scalar(scalar) => Ok(scalar_default(*scalar)),

        TypeKind::Struct { fields } => {
            let mut members = BTreeMap::new();
            for field in fields {
                let value = named_default(&field.type_name, types, depth + 1).map_err(|why| {
                    // The field is named here rather than at the leaf, because a reader chasing
                    // "why can this save not load" needs the path, not just the bottom of it.
                    format!("`{}`.{}: {why}", info.name, field.name)
                })?;
                members.insert(field.name.clone(), value);
            }
            Ok(Value::Struct(members))
        }

        // The refusal this module exists to make. See the module docs.
        TypeKind::Enum { .. } => Err(format!(
            "`{}` is an enum, and an enum has no default that is not a guess about what the game \
             means. Give the field a value explicitly, or redirect it",
            info.name
        )),

        TypeKind::List { element, length } => match length {
            // A fixed-length array is NOT empty by default: `[f32; 3]` has to come out as three
            // zeros or `from_value` will reject it for the wrong length, which would read as a
            // corrupt save rather than as a missing default.
            Some(count) => {
                let mut items = Vec::with_capacity(*count);
                for _ in 0..*count {
                    items.push(named_default(element, types, depth + 1)?);
                }
                Ok(Value::List(items))
            }
            None => Ok(Value::List(Vec::new())),
        },

        TypeKind::Map { .. } => Ok(Value::Map(BTreeMap::new())),

        // `Option::None` is `Value::Unit` — see this crate's `impls` for why absence is spelled
        // rather than left out.
        TypeKind::Optional { .. } => Ok(Value::Unit),
    }
}

/// Looks a named type up and defaults it, keeping the depth count.
fn named_default(name: &str, types: &TypeRegistry, depth: usize) -> Result<Value, NoDefault> {
    let Some(info) = types.get(name) else {
        return Err(format!(
            "no type called `{name}` is registered, so there is nothing to build a default from"
        ));
    };
    default_with_depth(info, types, depth)
}

/// The obvious zero for each scalar. Nothing subtle here, and that is the point.
fn scalar_default(scalar: ScalarKind) -> Value {
    match scalar {
        ScalarKind::Bool => Value::Bool(false),
        ScalarKind::SignedInt => Value::I64(0),
        ScalarKind::UnsignedInt => Value::U64(0),
        ScalarKind::Float32 => Value::F32(0.0),
        ScalarKind::Float64 => Value::F64(0.0),
        ScalarKind::String => Value::String(String::new()),
    }
}
