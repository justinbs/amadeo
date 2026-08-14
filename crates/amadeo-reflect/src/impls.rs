//! [`Reflect`] for the standard types components are built from.
//!
//! # `usize` and `isize` are deliberately absent
//!
//! Their width is platform-dependent, so a value that round-trips on a 64-bit machine can overflow
//! on a 32-bit one — a divergence that would appear as a corrupt scene file rather than as an
//! obvious portability bug. Invariant I3 requires the same inputs to produce the same state on any
//! machine, and a platform-sized integer in serialised simulation state quietly breaks that.
//!
//! Use a fixed width. If a count genuinely needs to be `usize` in memory, store it as `u32` or `u64`
//! in the component and convert at the edges.

use crate::info::{ScalarKind, TypeInfo, TypeKind};
use crate::value::Value;
use crate::{Reflect, ReflectError};

/// Implements [`Reflect`] for a primitive that maps directly onto one [`Value`] variant.
///
/// A macro because there are a dozen of these and they are identical apart from three tokens.
/// It expands to exactly the code you would write by hand — no hidden behaviour — which is the bar
/// `CLAUDE.md` section 6 sets for reaching for a macro at all.
macro_rules! reflect_scalar {
    ($type:ty, $name:literal, $scalar:expr, $variant:ident) => {
        impl Reflect for $type {
            fn type_name() -> String {
                $name.to_string()
            }

            fn type_info() -> TypeInfo {
                TypeInfo {
                    name: $name.to_string(),
                    docs: String::new(),
                    version: 1,
                    kind: TypeKind::Scalar($scalar),
                }
            }

            fn to_value(&self) -> Value {
                Value::$variant(self.clone())
            }

            fn from_value(value: &Value) -> Result<Self, ReflectError> {
                match value {
                    Value::$variant(inner) => Ok(inner.clone()),
                    other => Err(ReflectError::mismatch($name, $name, other)),
                }
            }
        }
    };
}

reflect_scalar!(bool, "bool", ScalarKind::Bool, Bool);
reflect_scalar!(String, "string", ScalarKind::String, String);

/// Implements [`Reflect`] for a float, accepting any numeric value.
///
/// # Why floats are lenient about which numeric variant they arrive in
///
/// A [`Value`] does not always come from `to_value`. It also comes from a scene file, and the parser
/// there has no schema — it decides `1` is an integer and `1.0` is a float purely from how they were
/// written (ADR 0014). So a designer typing `intensity 3` for an `f32` field produces
/// [`Value::I64`], and typing `0.85` produces [`Value::F64`] whatever width the component wants.
///
/// Rejecting those would be pedantry with no upside: the number is unambiguous, and the schema — not
/// the punctuation — says what width it should end up as. So a float accepts any numeric variant.
///
/// **Precision is not checked**, deliberately. Narrowing `0.1_f64` to `f32` loses bits, and that is
/// exactly what someone writing `0.1` into an `f32` field is asking for. Integers are different: an
/// out-of-range integer is a mistake rather than an approximation, and stays an error.
macro_rules! reflect_float {
    ($type:ty, $name:literal, $scalar:expr, $variant:ident) => {
        impl Reflect for $type {
            fn type_name() -> String {
                $name.to_string()
            }

            fn type_info() -> TypeInfo {
                TypeInfo {
                    name: $name.to_string(),
                    docs: String::new(),
                    version: 1,
                    kind: TypeKind::Scalar($scalar),
                }
            }

            fn to_value(&self) -> Value {
                Value::$variant(*self)
            }

            fn from_value(value: &Value) -> Result<Self, ReflectError> {
                match value {
                    Value::F32(inner) => Ok(*inner as $type),
                    Value::F64(inner) => Ok(*inner as $type),
                    Value::I64(inner) => Ok(*inner as $type),
                    Value::U64(inner) => Ok(*inner as $type),
                    other => Err(ReflectError::mismatch($name, "a number", other)),
                }
            }
        }
    };
}

reflect_float!(f32, "f32", ScalarKind::Float32, F32);
reflect_float!(f64, "f64", ScalarKind::Float64, F64);

/// Implements [`Reflect`] for an integer.
///
/// Accepts either integer variant — a scene file's `-1` parses as [`Value::I64`] and `3` may parse
/// as either, and which one it happened to be says nothing about what the field wants. Both are
/// routed through `i128`, which is the only type that holds all of `i64` and all of `u64`, and then
/// narrowed with a **checked** conversion.
///
/// Unlike floats, an out-of-range integer is an error rather than an approximation. Silently
/// truncating one is the kind of failure that surfaces three subsystems from its cause.
///
/// Floats are **not** accepted: `2.7` into a `u32` field has no defensible answer — round, truncate,
/// or refuse — so it refuses and says so.
macro_rules! reflect_integer {
    ($type:ty, $name:literal, $scalar:expr, $variant:ident) => {
        impl Reflect for $type {
            fn type_name() -> String {
                $name.to_string()
            }

            fn type_info() -> TypeInfo {
                TypeInfo {
                    name: $name.to_string(),
                    docs: String::new(),
                    version: 1,
                    kind: TypeKind::Scalar($scalar),
                }
            }

            fn to_value(&self) -> Value {
                Value::$variant(<_>::from(*self))
            }

            fn from_value(value: &Value) -> Result<Self, ReflectError> {
                let wide: i128 = match value {
                    Value::I64(inner) => i128::from(*inner),
                    Value::U64(inner) => i128::from(*inner),
                    other => return Err(ReflectError::mismatch($name, "a whole number", other)),
                };
                <$type>::try_from(wide).map_err(|_| ReflectError::OutOfRange {
                    type_name: $name.to_string(),
                    value: wide.to_string(),
                    target: stringify!($type).to_string(),
                })
            }
        }
    };
}

reflect_integer!(i8, "i8", ScalarKind::SignedInt, I64);
reflect_integer!(i16, "i16", ScalarKind::SignedInt, I64);
reflect_integer!(i32, "i32", ScalarKind::SignedInt, I64);
reflect_integer!(i64, "i64", ScalarKind::SignedInt, I64);
reflect_integer!(u8, "u8", ScalarKind::UnsignedInt, U64);
reflect_integer!(u16, "u16", ScalarKind::UnsignedInt, U64);
reflect_integer!(u32, "u32", ScalarKind::UnsignedInt, U64);
reflect_integer!(u64, "u64", ScalarKind::UnsignedInt, U64);

impl<T: Reflect> Reflect for Vec<T> {
    fn type_name() -> String {
        format!("list<{}>", T::type_name())
    }

    fn type_info() -> TypeInfo {
        TypeInfo {
            name: Self::type_name(),
            docs: String::new(),
            version: 1,
            kind: TypeKind::List {
                element: T::type_name(),
                // A `Vec` has no length in its type, so the schema says so rather than inventing one.
                length: None,
            },
        }
    }

    fn register_dependencies(
        registry: &mut crate::TypeRegistry,
    ) -> Result<(), crate::RegistryError> {
        registry.register::<T>()
    }

    fn to_value(&self) -> Value {
        Value::List(self.iter().map(Reflect::to_value).collect())
    }

    fn from_value(value: &Value) -> Result<Self, ReflectError> {
        match value {
            Value::List(items) => items.iter().map(T::from_value).collect(),
            // **A single value fills a one-element list**, because the scene format has no way to
            // spell one. `value 22.0` is one token, and layer 1 has no schema to tell "a number"
            // from "a list of one number" — so it produces a scalar, every time, and a `Vec<f32>`
            // field could not be written with one element in it at all.
            //
            // The type is what resolves that, here, which is the same job `f32::from_value`
            // accepting an integer already does: the text is genuinely ambiguous and the schema is
            // the thing that knows. Anything that is not a list and cannot be an element still
            // fails, with the element's own message.
            other => T::from_value(other).map(|single| vec![single]),
        }
    }
}

/// [`Reflect`] for `amadeo-core`'s small value types.
///
/// # Why these live here rather than beside their definitions
///
/// `amadeo-core` sits **below** this crate, so it cannot implement a trait defined here (invariant
/// I6). But this crate depends on `amadeo-core`, so implementing the trait for its types is legal
/// in both directions — the impl is written where the *trait* lives instead of where the type does.
///
/// That is the standard answer for a type that has to reflect but sits below the reflection layer.
/// The alternative, exposing state and hand-writing the impl further up, is only necessary when the
/// state is private — see `Rng::state` and `SimRng` for that case.
impl Reflect for amadeo_core::Tick {
    const STATIC_NAME: &'static str = "Tick";

    fn type_name() -> String {
        Self::STATIC_NAME.to_string()
    }

    fn type_info() -> TypeInfo {
        TypeInfo {
            name: Self::type_name(),
            docs: "A simulation tick number, counting from zero at world creation.".to_string(),
            version: 1,
            // A scalar rather than a one-field struct: a tick *is* a number, and rendering it as
            // `{0: 42}` in a dump would be noise around the only thing anyone wants to read.
            kind: TypeKind::Scalar(ScalarKind::UnsignedInt),
        }
    }

    fn to_value(&self) -> Value {
        Value::U64(self.0)
    }

    fn from_value(value: &Value) -> Result<Self, ReflectError> {
        u64::from_value(value).map(amadeo_core::Tick)
    }
}

/// A type that can be a map key in the reflection tree.
///
/// ADR 0027 makes map keys strings, so any key type has to say how it renders and how it parses
/// back. Most implementations are one line each.
///
/// # The contract, and what breaks if it is not met
///
/// [`ReflectKey::to_key`] **must be injective**: two keys that are different under `Ord` must
/// produce different strings. If they do not, two entries collapse into one when the map is
/// converted to a [`Value`] and the data is silently lost.
///
/// That cannot be reported as an error, because [`Reflect::to_value`] does not return a `Result` —
/// so [`BTreeMap`](std::collections::BTreeMap)'s impl carries a `debug_assert` that the entry count
/// survived the conversion.
/// A collision therefore fails loudly in tests and in a debug build, which is where a key type gets
/// written.
///
/// # Why not just require `Display` and `FromStr`
///
/// Both are general-purpose formatting traits that a type may already implement for a *human*
/// audience — `ActionId`'s `Display`, for instance, renders `action#1a2b` for a diagnostic. Reusing
/// it as an identity would tie the on-disk key to how a message happens to read, and changing a log
/// line would rewrite every saved file. A separate trait keeps the two free to differ.
pub trait ReflectKey: Sized + Ord + 'static {
    /// The key type's name, for [`TypeKind::Map`].
    fn key_type_name() -> String;

    /// Renders this key. Must be injective — see the trait docs.
    fn to_key(&self) -> String;

    /// Parses a key back.
    ///
    /// # Errors
    ///
    /// A [`ReflectError`] naming what the key should have looked like.
    fn from_key(text: &str) -> Result<Self, ReflectError>;
}

impl ReflectKey for String {
    fn key_type_name() -> String {
        "string".to_string()
    }

    fn to_key(&self) -> String {
        self.clone()
    }

    fn from_key(text: &str) -> Result<Self, ReflectError> {
        Ok(text.to_string())
    }
}

/// Implements [`ReflectKey`] for an integer, rendering it as plain decimal.
///
/// Injective for every integer type: distinct integers have distinct decimal spellings.
macro_rules! reflect_integer_key {
    ($type:ty, $name:literal) => {
        impl ReflectKey for $type {
            fn key_type_name() -> String {
                $name.to_string()
            }

            fn to_key(&self) -> String {
                self.to_string()
            }

            fn from_key(text: &str) -> Result<Self, ReflectError> {
                text.parse::<$type>()
                    .map_err(|_| ReflectError::TypeMismatch {
                        type_name: format!("map key <{}>", $name),
                        expected: concat!("a ", $name, " written in decimal").to_string(),
                        found: format!("`{text}`"),
                    })
            }
        }
    };
}

reflect_integer_key!(i32, "i32");
reflect_integer_key!(i64, "i64");
reflect_integer_key!(u32, "u32");
reflect_integer_key!(u64, "u64");

impl<K: ReflectKey, V: Reflect> Reflect for std::collections::BTreeMap<K, V> {
    fn type_name() -> String {
        format!("map<{}, {}>", K::key_type_name(), V::type_name())
    }

    fn type_info() -> TypeInfo {
        TypeInfo {
            name: Self::type_name(),
            docs: String::new(),
            version: 1,
            kind: TypeKind::Map {
                key: K::key_type_name(),
                value: V::type_name(),
            },
        }
    }

    // Only the value type. A key is not a `Reflect` — `ReflectKey` renders it to a string and back
    // (ADR 0027) — so there is no schema for it to register, and `TypeKind::Map::key` is a name for
    // a reader rather than a lookup.
    fn register_dependencies(
        registry: &mut crate::TypeRegistry,
    ) -> Result<(), crate::RegistryError> {
        registry.register::<V>()
    }

    fn to_value(&self) -> Value {
        let entries: std::collections::BTreeMap<String, Value> = self
            .iter()
            .map(|(key, value)| (key.to_key(), value.to_value()))
            .collect();

        // The `ReflectKey` contract, checked where it can actually be observed. Two keys rendering
        // to the same string would silently drop an entry here, and the loss would surface much
        // later as a value that mysteriously reverted.
        debug_assert_eq!(
            entries.len(),
            self.len(),
            "two keys of {} rendered to the same string, so an entry was lost. \
             ReflectKey::to_key must be injective",
            Self::type_name()
        );

        Value::Map(entries)
    }

    fn from_value(value: &Value) -> Result<Self, ReflectError> {
        match value {
            // A `Struct` is accepted alongside a `Map`, for the same reason floats accept any
            // numeric variant: **a `Value` does not always come from `to_value`.** It also comes
            // from a text parser that has no schema, and a parser reading an indented block of
            // `name value` lines cannot know whether the type behind it declared a struct or a map —
            // they are written identically, deliberately (ADR 0027).
            //
            // Being strict here would mean the only way to author a map is to already know it is
            // one, which defeats the point of the format being hand-writable.
            Value::Map(entries) | Value::Struct(entries) => entries
                .iter()
                .map(|(key, value)| Ok((K::from_key(key)?, V::from_value(value)?)))
                .collect(),
            other => Err(ReflectError::mismatch(Self::type_name(), "map", other)),
        }
    }
}

impl<T: Reflect> Reflect for Option<T> {
    fn type_name() -> String {
        format!("option<{}>", T::type_name())
    }

    fn type_info() -> TypeInfo {
        TypeInfo {
            name: Self::type_name(),
            docs: String::new(),
            version: 1,
            kind: TypeKind::Optional {
                inner: T::type_name(),
            },
        }
    }

    fn register_dependencies(
        registry: &mut crate::TypeRegistry,
    ) -> Result<(), crate::RegistryError> {
        registry.register::<T>()
    }

    fn to_value(&self) -> Value {
        match self {
            // Absence is `Unit` rather than a missing field, so "this field is explicitly nothing"
            // and "whoever wrote this file forgot the field" stay distinguishable.
            None => Value::Unit,
            Some(inner) => inner.to_value(),
        }
    }

    fn from_value(value: &Value) -> Result<Self, ReflectError> {
        match value {
            Value::Unit => Ok(None),
            other => T::from_value(other).map(Some),
        }
    }
}

impl<T: Reflect, const N: usize> Reflect for [T; N] {
    fn type_name() -> String {
        format!("array<{}, {N}>", T::type_name())
    }

    fn type_info() -> TypeInfo {
        TypeInfo {
            name: Self::type_name(),
            docs: String::new(),
            version: 1,
            kind: TypeKind::List {
                element: T::type_name(),
                // The whole reason `length` exists: `from_value` below rejects any other count, and
                // before this the only place that number appeared was inside the name string.
                length: Some(N),
            },
        }
    }

    fn register_dependencies(
        registry: &mut crate::TypeRegistry,
    ) -> Result<(), crate::RegistryError> {
        registry.register::<T>()
    }

    fn to_value(&self) -> Value {
        Value::List(self.iter().map(Reflect::to_value).collect())
    }

    fn from_value(value: &Value) -> Result<Self, ReflectError> {
        let Value::List(items) = value else {
            return Err(ReflectError::mismatch(Self::type_name(), "list", value));
        };
        if items.len() != N {
            return Err(ReflectError::WrongLength {
                type_name: Self::type_name(),
                expected: N,
                found: items.len(),
            });
        }

        let converted: Vec<T> = items
            .iter()
            .map(T::from_value)
            .collect::<Result<Vec<T>, ReflectError>>()?;

        // The length was just checked, so this cannot fail. `try_into` rather than an unchecked
        // conversion because there is no safe unchecked one, and `map_err` rather than `expect`
        // because engine crates do not panic (`CLAUDE.md` section 6).
        converted
            .try_into()
            .map_err(|converted: Vec<T>| ReflectError::WrongLength {
                type_name: Self::type_name(),
                expected: N,
                found: converted.len(),
            })
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    /// Asserts a value survives the trip out to `Value` and back.
    fn round_trips<T: Reflect + PartialEq + std::fmt::Debug>(value: T) {
        let encoded = value.to_value();
        let decoded = T::from_value(&encoded).expect("round trip");
        assert_eq!(decoded, value);
    }

    #[test]
    fn primitives_round_trip() {
        round_trips(true);
        round_trips(-7i8);
        round_trips(4242u16);
        round_trips(-100_000i32);
        round_trips(3_000_000_000u32);
        round_trips(i64::MIN);
        round_trips(u64::MAX);
        round_trips(1.5f32);
        round_trips(-0.25f64);
        round_trips("hello".to_string());
    }

    #[test]
    fn containers_round_trip() {
        round_trips(vec![1u32, 2, 3]);
        round_trips(Vec::<f32>::new());
        round_trips(Some(5u32));
        round_trips(None::<u32>);
        round_trips([1.0f32, 2.0]);
        round_trips([[1u8, 2], [3, 4]]);
    }

    #[test]
    fn none_and_some_are_distinguishable() {
        assert_eq!(None::<u32>.to_value(), Value::Unit);
        assert_eq!(Some(1u32).to_value(), Value::U64(1));
    }

    #[test]
    fn narrowing_an_out_of_range_integer_is_reported_not_truncated() {
        let too_big = Value::I64(9_000);
        let error = i8::from_value(&too_big).expect_err("9000 does not fit in an i8");
        assert_eq!(
            error.to_string(),
            "i8: 9000 does not fit in i8",
            "the message must name the value and the target"
        );
    }

    #[test]
    fn a_wrong_shape_says_what_it_found() {
        let error = f32::from_value(&Value::String("nope".into())).expect_err("wrong shape");
        // "a number" rather than "f32": any numeric variant would have been accepted, so naming the
        // exact one would describe the implementation rather than the requirement.
        assert_eq!(error.to_string(), "f32: expected a number, found string");
    }

    #[test]
    fn a_float_accepts_any_numeric_variant() {
        // A scene file's parser has no schema, so `intensity 3` arrives as an integer and `0.85`
        // arrives as an f64 whatever width the field wants. Refusing those would be pedantry.
        assert_eq!(f32::from_value(&Value::I64(3)).expect("integer"), 3.0);
        assert_eq!(f32::from_value(&Value::U64(3)).expect("unsigned"), 3.0);
        assert_eq!(f32::from_value(&Value::F64(0.5)).expect("f64"), 0.5);
        assert_eq!(f64::from_value(&Value::F32(0.5)).expect("f32"), 0.5);
    }

    #[test]
    fn narrowing_a_float_loses_precision_on_purpose() {
        // 0.1 is not representable in either width, and someone writing `0.1` into an f32 field is
        // asking for the nearest f32. Refusing would be useless; rounding silently is correct.
        let narrowed = f32::from_value(&Value::F64(0.1)).expect("narrows");
        assert_eq!(narrowed, 0.1f32);
        assert_ne!(f64::from(narrowed), 0.1f64, "precision really was lost");
    }

    #[test]
    fn an_integer_field_refuses_a_float() {
        // Unlike narrowing a float, `2.7` into a u32 has no defensible answer -- round, truncate, or
        // refuse -- so it refuses rather than picking one silently.
        let error = u32::from_value(&Value::F64(2.7)).expect_err("2.7 is not a whole number");
        assert_eq!(
            error.to_string(),
            "u32: expected a whole number, found 64-bit float"
        );
    }

    #[test]
    fn an_integer_accepts_either_integer_variant_within_range() {
        assert_eq!(i32::from_value(&Value::U64(7)).expect("unsigned"), 7);
        assert_eq!(u32::from_value(&Value::I64(7)).expect("signed"), 7);
        // ...but a negative value still cannot become unsigned.
        assert!(u32::from_value(&Value::I64(-1)).is_err());
        // ...and u64::MAX still does not fit in an i64 field.
        assert!(i64::from_value(&Value::U64(u64::MAX)).is_err());
    }

    #[test]
    fn a_wrong_length_array_says_both_lengths() {
        let three = Value::List(vec![Value::F32(1.0), Value::F32(2.0), Value::F32(3.0)]);
        let error = <[f32; 2]>::from_value(&three).expect_err("wrong length");
        assert_eq!(
            error.to_string(),
            "array<f32, 2>: expected 2 elements, found 3"
        );
    }

    #[test]
    fn a_bad_element_inside_a_list_is_reported() {
        let mixed = Value::List(vec![Value::U64(1), Value::String("two".into())]);
        let error = Vec::<u32>::from_value(&mixed).expect_err("element two is a string");
        assert_eq!(
            error.to_string(),
            "u32: expected a whole number, found string"
        );
    }

    #[test]
    fn generic_type_names_describe_their_shape() {
        assert_eq!(Vec::<f32>::type_name(), "list<f32>");
        assert_eq!(Option::<u32>::type_name(), "option<u32>");
        assert_eq!(<[f32; 2]>::type_name(), "array<f32, 2>");
        assert_eq!(Vec::<Vec<u8>>::type_name(), "list<list<u8>>");
    }

    #[test]
    fn scalar_kinds_are_reported_for_the_schema() {
        assert_eq!(f32::type_info().kind, TypeKind::Scalar(ScalarKind::Float32));
        assert_eq!(
            u16::type_info().kind,
            TypeKind::Scalar(ScalarKind::UnsignedInt)
        );
        assert_eq!(
            Vec::<f32>::type_info().kind,
            TypeKind::List {
                element: "f32".to_string(),
                length: None,
            }
        );
        assert_eq!(
            <[f32; 2]>::type_info().kind,
            TypeKind::List {
                element: "f32".to_string(),
                length: Some(2),
            }
        );
    }

    // --- Maps. ADR 0027. ---

    use std::collections::BTreeMap;

    #[test]
    fn a_string_keyed_map_round_trips() {
        let mut stats = BTreeMap::new();
        stats.insert("strength".to_string(), 10u32);
        stats.insert("agility".to_string(), 12u32);
        round_trips(stats);
    }

    #[test]
    fn an_integer_keyed_map_round_trips_through_decimal_text() {
        // The cost of string keys, stated as a test: the key goes out as `"7"` and comes back as 7.
        let mut slots = BTreeMap::new();
        slots.insert(7u32, "sword".to_string());
        slots.insert(11u32, "shield".to_string());

        let encoded = slots.to_value();
        assert_eq!(
            encoded.entry("7"),
            Some(&Value::String("sword".to_string()))
        );
        round_trips(slots);
    }

    #[test]
    fn a_map_and_a_struct_do_not_hash_alike() {
        // They hold the same shape, so without distinct discriminants a type changing from one to
        // the other would be invisible to every replay assertion.
        use amadeo_core::stable_hash_of;

        let entries = [("a".to_string(), Value::U64(1))];
        let as_map = Value::Map(entries.iter().cloned().collect());
        let as_struct = Value::Struct(entries.iter().cloned().collect());

        assert_ne!(stable_hash_of(&as_map), stable_hash_of(&as_struct));
    }

    #[test]
    fn map_entries_are_sorted_regardless_of_insertion_order() {
        // Invariant I2, falling out of the data structure rather than out of remembering to sort.
        let mut forwards = BTreeMap::new();
        forwards.insert("a".to_string(), 1u8);
        forwards.insert("z".to_string(), 2u8);

        let mut backwards = BTreeMap::new();
        backwards.insert("z".to_string(), 2u8);
        backwards.insert("a".to_string(), 1u8);

        assert_eq!(forwards.to_value(), backwards.to_value());
        assert_eq!(forwards.to_value().to_string(), "{a => 1, z => 2}");
    }

    #[test]
    fn a_map_accepts_a_struct_because_the_scene_parser_has_no_schema() {
        // A text parser reading an indented block cannot tell a struct from a map -- they are
        // written identically on purpose. Being strict here would mean the only way to author a map
        // is to already know it is one.
        let written_by_a_parser = Value::structure([("strength", Value::I64(10))]);
        let decoded = BTreeMap::<String, u32>::from_value(&written_by_a_parser).expect("lenient");

        assert_eq!(decoded.get("strength"), Some(&10));
    }

    #[test]
    fn a_map_refuses_a_shape_that_is_not_one() {
        let error = BTreeMap::<String, u32>::from_value(&Value::List(Vec::new()))
            .expect_err("a list is not a map");
        let message = error.to_string();

        assert!(message.contains("map<string, u32>"), "{message}");
        assert!(message.contains("found list"), "{message}");
    }

    #[test]
    fn a_key_that_will_not_parse_says_what_it_should_have_been() {
        let bad = Value::map([("not_a_number", Value::String("x".to_string()))]);
        let error = BTreeMap::<u32, String>::from_value(&bad).expect_err("bad key");
        let message = error.to_string();

        assert!(message.contains("u32"), "{message}");
        assert!(message.contains("decimal"), "{message}");
        assert!(message.contains("not_a_number"), "{message}");
    }

    #[test]
    fn a_map_reports_both_of_its_type_names_to_the_schema() {
        // What an editor reads to decide between a fixed inspector and an add-and-remove list.
        assert_eq!(
            BTreeMap::<String, f32>::type_info().kind,
            TypeKind::Map {
                key: "string".to_string(),
                value: "f32".to_string(),
            }
        );
        assert_eq!(
            BTreeMap::<u64, Vec<f32>>::type_name(),
            "map<u64, list<f32>>"
        );
    }

    #[test]
    fn a_map_of_compound_values_round_trips() {
        // The shape `InputState` needs: a key pointing at something that is not a scalar. A struct
        // as the value is exercised in `amadeo-input`, where `#[derive(Reflect)]` is usable -- the
        // derive emits `amadeo_reflect::` paths, which do not resolve inside this crate.
        let mut paths = BTreeMap::new();
        paths.insert("patrol".to_string(), vec![1.5f32, 2.0, -3.25]);
        paths.insert("retreat".to_string(), Vec::new());
        round_trips(paths);
    }

    #[test]
    fn a_map_nested_in_a_map_round_trips() {
        let mut inner = BTreeMap::new();
        inner.insert("strength".to_string(), 10u32);

        let mut outer = BTreeMap::new();
        outer.insert("player".to_string(), inner);
        round_trips(outer);
    }

    #[test]
    fn an_empty_map_round_trips() {
        round_trips(BTreeMap::<String, u32>::new());
    }

    #[test]
    fn a_single_value_fills_a_one_element_list() {
        // **The scene format cannot spell a one-element list.** `value 22.0` is one token, and layer
        // 1 has no schema to tell "a number" from "a list of one number", so it produces a scalar
        // every time — which means a `Vec<f32>` field could not be authored with one element in it.
        //
        // Found by `amadeo check` on the first `.anim` file with a scalar track in it, which is the
        // validator earning its keep: the message named the type, the expectation and what it got.
        assert_eq!(
            Vec::<f32>::from_value(&Value::F64(22.0)).expect("a scalar fills a one-element list"),
            vec![22.0]
        );
        assert_eq!(
            Vec::<String>::from_value(&Value::String("one".to_string())).expect("same for strings"),
            vec!["one".to_string()]
        );
    }

    #[test]
    fn something_that_is_not_an_element_still_fails() {
        // The coercion above must not turn every mistake into a one-element list. A bool is not an
        // `f32`, and the message that comes back is the *element's*, which is the useful one.
        let error = Vec::<f32>::from_value(&Value::Bool(true)).expect_err("a bool is not a number");
        assert!(error.to_string().contains("f32"), "{error}");
    }
}
