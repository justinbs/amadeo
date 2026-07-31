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
            },
        }
    }

    fn to_value(&self) -> Value {
        Value::List(self.iter().map(Reflect::to_value).collect())
    }

    fn from_value(value: &Value) -> Result<Self, ReflectError> {
        match value {
            Value::List(items) => items.iter().map(T::from_value).collect(),
            other => Err(ReflectError::mismatch(Self::type_name(), "list", other)),
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
            },
        }
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
                element: "f32".to_string()
            }
        );
    }
}
