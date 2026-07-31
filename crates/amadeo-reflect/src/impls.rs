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
reflect_scalar!(f32, "f32", ScalarKind::Float32, F32);
reflect_scalar!(f64, "f64", ScalarKind::Float64, F64);
reflect_scalar!(String, "string", ScalarKind::String, String);

/// Implements [`Reflect`] for an integer narrower than the `Value` variant that carries it.
///
/// Widening on the way out is lossless; narrowing on the way back can overflow, so it is checked and
/// reported rather than truncated. A silently truncated integer is precisely the kind of failure
/// that shows up three subsystems away from its cause.
macro_rules! reflect_integer {
    ($type:ty, $name:literal, $scalar:expr, $variant:ident, $wide:ty) => {
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
                Value::$variant(<$wide>::from(*self))
            }

            fn from_value(value: &Value) -> Result<Self, ReflectError> {
                match value {
                    Value::$variant(inner) => {
                        <$type>::try_from(*inner).map_err(|_| ReflectError::OutOfRange {
                            type_name: $name.to_string(),
                            value: inner.to_string(),
                            target: stringify!($type).to_string(),
                        })
                    }
                    other => Err(ReflectError::mismatch($name, $name, other)),
                }
            }
        }
    };
}

reflect_integer!(i8, "i8", ScalarKind::SignedInt, I64, i64);
reflect_integer!(i16, "i16", ScalarKind::SignedInt, I64, i64);
reflect_integer!(i32, "i32", ScalarKind::SignedInt, I64, i64);
reflect_integer!(u8, "u8", ScalarKind::UnsignedInt, U64, u64);
reflect_integer!(u16, "u16", ScalarKind::UnsignedInt, U64, u64);
reflect_integer!(u32, "u32", ScalarKind::UnsignedInt, U64, u64);

// The widest two need no conversion, so they use the plain scalar form.
reflect_scalar!(i64, "i64", ScalarKind::SignedInt, I64);
reflect_scalar!(u64, "u64", ScalarKind::UnsignedInt, U64);

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
        assert_eq!(error.to_string(), "f32: expected f32, found string");
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
        assert_eq!(error.to_string(), "u32: expected u32, found string");
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
