//! The canonical value tree that every reflected type converts to and from.

use amadeo_core::{StableHash, StableHasher};
use std::collections::BTreeMap;
use std::fmt;

/// A reflected value: the common currency between a Rust type, a text file, and the agent.
///
/// # Why a value tree rather than dynamic field access
///
/// The alternative — `fn field(&self, name: &str) -> Option<&dyn Reflect>` — is more powerful and
/// avoids allocating. It also requires `dyn Reflect`, trait-object-safe accessors, and downcasts at
/// every level, which is exactly the kind of Rust that makes a codebase unreadable to someone still
/// learning it (`CLAUDE.md` section 6).
///
/// This engine's three consumers — canonical text serialisation, the editor inspector, and agent
/// introspection — all want a *whole tree* at once, not a cursor into a live value. So the boring
/// option is also the fitting one, and the allocation happens when a scene is saved or an entity is
/// inspected, never in a simulation tick.
///
/// # Canonical by construction
///
/// [`Value::Struct`] holds a `BTreeMap`, so fields come out **sorted by name, always**. That is
/// invariant I2 — byte-stable serialisation — falling out of the data structure rather than
/// depending on every writer remembering to sort. There is deliberately no way to represent a
/// struct whose fields are in some other order.
#[derive(Debug, Clone, PartialEq)]
pub enum Value {
    /// No data. The payload of a fieldless enum variant, and the value of a unit struct.
    Unit,
    /// A boolean.
    Bool(bool),
    /// A signed integer. All signed widths widen to this.
    I64(i64),
    /// An unsigned integer. All unsigned widths widen to this.
    U64(u64),
    /// A 32-bit float.
    ///
    /// Kept distinct from [`Value::F64`] rather than widening everything to `f64`. The round trip
    /// would be lossless, but the *text* would not be: formatting an `f32` that had been through
    /// `f64` can produce a different decimal string, which would break byte-stability (I2).
    F32(f32),
    /// A 64-bit float.
    F64(f64),
    /// A UTF-8 string.
    String(String),
    /// An ordered sequence. Order is data here, so it is preserved exactly.
    List(Vec<Value>),
    /// Named fields, always sorted by name. See the type-level note on canonical form.
    Struct(BTreeMap<String, Value>),
    /// Keys chosen by the author, always sorted. Values are all the same type.
    ///
    /// # Why this is a separate variant when it holds the same map as [`Value::Struct`]
    ///
    /// They are structurally identical and semantically opposite. A struct has a **fixed, known**
    /// set of field names, so an unrecognised one is a typo worth reporting (`ReflectError::
    /// UnknownField`). A map has **arbitrary** keys, so an unrecognised one is the entire point and
    /// rejecting it would be a bug.
    ///
    /// Keeping them apart is what lets `from_value` be strict about one and permissive about the
    /// other, and what lets the editor render a fixed inspector for a struct and an add-and-remove
    /// list for a map. Merging them would mean choosing one behaviour for both.
    ///
    /// # Why the key is a string — ADR 0027
    ///
    /// A key type renders to a string and parses back, via [`ReflectKey`](crate::ReflectKey). That
    /// keeps a map readable and hand-writable in the scene format — indented `strength 10`, exactly
    /// like a struct's fields, which is what ADR 0014 chose that format for — and it sidesteps the
    /// fact that `Value` contains floats and therefore has no total order to sort arbitrary keys by.
    Map(BTreeMap<String, Value>),
    /// One variant of an enum, with its payload.
    Enum(EnumValue),
}

/// One variant of an enum, plus whatever it carries.
#[derive(Debug, Clone, PartialEq)]
pub struct EnumValue {
    /// The variant's name, as written in the source and in text files.
    pub variant: String,
    /// The variant's data. [`Value::Unit`] for a fieldless variant.
    pub payload: Box<Value>,
}

impl Value {
    /// Builds a struct value from name/value pairs.
    ///
    /// Convenience for tests and for hand-constructing values; the derive emits the map directly.
    #[must_use]
    pub fn structure<I, K>(fields: I) -> Self
    where
        I: IntoIterator<Item = (K, Value)>,
        K: Into<String>,
    {
        Value::Struct(
            fields
                .into_iter()
                .map(|(name, value)| (name.into(), value))
                .collect(),
        )
    }

    /// Builds a fieldless enum value.
    #[must_use]
    pub fn unit_variant(variant: impl Into<String>) -> Self {
        Value::Enum(EnumValue {
            variant: variant.into(),
            payload: Box::new(Value::Unit),
        })
    }

    /// The name of this value's shape, for error messages.
    ///
    /// Deliberately the *shape* rather than the Rust type: a mismatch report is more useful saying
    /// "expected a struct, found a list" than naming a type the reader may not recognise.
    #[must_use]
    pub fn shape(&self) -> &'static str {
        match self {
            Value::Unit => "unit",
            Value::Bool(_) => "bool",
            Value::I64(_) => "signed integer",
            Value::U64(_) => "unsigned integer",
            Value::F32(_) => "32-bit float",
            Value::F64(_) => "64-bit float",
            Value::String(_) => "string",
            Value::List(_) => "list",
            Value::Struct(_) => "struct",
            Value::Map(_) => "map",
            Value::Enum(_) => "enum",
        }
    }

    /// Builds a map value from key/value pairs.
    ///
    /// The counterpart to [`Value::structure`], and the same convenience: for tests and for
    /// hand-construction. The generic impl emits the map directly.
    #[must_use]
    pub fn map<K: Into<String>>(entries: impl IntoIterator<Item = (K, Value)>) -> Value {
        Value::Map(
            entries
                .into_iter()
                .map(|(key, value)| (key.into(), value))
                .collect(),
        )
    }

    /// Looks up a field, if this is a struct that has one by that name.
    ///
    /// Deliberately does **not** look inside a [`Value::Map`]. A field is a schema-known name and a
    /// map key is author-supplied data; conflating them would let a caller reach into a map by
    /// accident and get a value the type system said could not be there.
    #[must_use]
    pub fn field(&self, name: &str) -> Option<&Value> {
        match self {
            Value::Struct(fields) => fields.get(name),
            _ => None,
        }
    }

    /// Looks up an entry, if this is a map that has one under that key.
    #[must_use]
    pub fn entry(&self, key: &str) -> Option<&Value> {
        match self {
            Value::Map(entries) => entries.get(key),
            _ => None,
        }
    }
}

impl fmt::Display for Value {
    /// A compact single-line rendering, for diagnostics.
    ///
    /// **Not** the canonical text format — that is `amadeo-scene`'s job and it is a designed
    /// artefact with its own spec (`CLAUDE.md` section 7, trap 4). This exists so an error message
    /// can quote the value it choked on.
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Value::Unit => write!(f, "()"),
            Value::Bool(value) => write!(f, "{value}"),
            Value::I64(value) => write!(f, "{value}"),
            Value::U64(value) => write!(f, "{value}"),
            Value::F32(value) => write!(f, "{value}"),
            Value::F64(value) => write!(f, "{value}"),
            Value::String(value) => write!(f, "\"{value}\""),
            Value::List(items) => {
                write!(f, "[")?;
                for (index, item) in items.iter().enumerate() {
                    if index > 0 {
                        write!(f, ", ")?;
                    }
                    write!(f, "{item}")?;
                }
                write!(f, "]")
            }
            Value::Struct(fields) => {
                write!(f, "{{")?;
                for (index, (name, value)) in fields.iter().enumerate() {
                    if index > 0 {
                        write!(f, ", ")?;
                    }
                    write!(f, "{name}: {value}")?;
                }
                write!(f, "}}")
            }
            // Rendered with `=>` rather than `:` so a map and a struct are distinguishable at a
            // glance in an error message. They hold the same shape, and telling a reader "expected
            // a struct, found {a: 1}" when the value was a map would be actively misleading.
            Value::Map(entries) => {
                write!(f, "{{")?;
                for (index, (key, value)) in entries.iter().enumerate() {
                    if index > 0 {
                        write!(f, ", ")?;
                    }
                    write!(f, "{key} => {value}")?;
                }
                write!(f, "}}")
            }
            Value::Enum(value) => match value.payload.as_ref() {
                Value::Unit => write!(f, "{}", value.variant),
                payload => write!(f, "{}({payload})", value.variant),
            },
        }
    }
}

impl StableHash for Value {
    /// Fingerprints a value reproducibly.
    ///
    /// Each variant writes a distinct discriminant first, so `U64(1)` and `I64(1)` cannot collide —
    /// without it, two different values could hash identically and a replay assertion would miss a
    /// real divergence.
    fn stable_hash(&self, hasher: &mut StableHasher) {
        match self {
            Value::Unit => hasher.write_u8(0),
            Value::Bool(value) => {
                hasher.write_u8(1);
                hasher.write_bool(*value);
            }
            Value::I64(value) => {
                hasher.write_u8(2);
                hasher.write_i64(*value);
            }
            Value::U64(value) => {
                hasher.write_u8(3);
                hasher.write_u64(*value);
            }
            Value::F32(value) => {
                hasher.write_u8(4);
                hasher.write_f32(*value);
            }
            Value::F64(value) => {
                hasher.write_u8(5);
                hasher.write_f64(*value);
            }
            Value::String(value) => {
                hasher.write_u8(6);
                hasher.write_str(value);
            }
            Value::List(items) => {
                hasher.write_u8(7);
                hasher.write_u64(items.len() as u64);
                for item in items {
                    item.stable_hash(hasher);
                }
            }
            Value::Struct(fields) => {
                hasher.write_u8(8);
                hasher.write_u64(fields.len() as u64);
                // BTreeMap iterates sorted, so this order is reproducible with no extra work.
                for (name, value) in fields {
                    hasher.write_str(name);
                    value.stable_hash(hasher);
                }
            }
            Value::Enum(value) => {
                hasher.write_u8(9);
                hasher.write_str(&value.variant);
                value.payload.stable_hash(hasher);
            }
            // Discriminant 10, distinct from `Struct`'s 8, so a map and a struct holding identical
            // entries do not hash alike. Without that a replay assertion could miss a type genuinely
            // changing from one to the other.
            Value::Map(entries) => {
                hasher.write_u8(10);
                hasher.write_u64(entries.len() as u64);
                // BTreeMap iterates sorted, so this order is reproducible with no extra work.
                for (key, value) in entries {
                    hasher.write_str(key);
                    value.stable_hash(hasher);
                }
            }
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use amadeo_core::stable_hash_of;

    #[test]
    fn struct_fields_are_sorted_regardless_of_insertion_order() {
        // The property invariant I2 rests on: canonical form is not something a writer has to
        // remember to produce.
        let forwards = Value::structure([
            ("alpha", Value::I64(1)),
            ("beta", Value::I64(2)),
            ("gamma", Value::I64(3)),
        ]);
        let backwards = Value::structure([
            ("gamma", Value::I64(3)),
            ("beta", Value::I64(2)),
            ("alpha", Value::I64(1)),
        ]);

        assert_eq!(forwards, backwards);
        assert_eq!(forwards.to_string(), "{alpha: 1, beta: 2, gamma: 3}");
        assert_eq!(stable_hash_of(&forwards), stable_hash_of(&backwards));
    }

    #[test]
    fn different_shapes_holding_the_same_number_hash_differently() {
        // Without a per-variant discriminant these would collide, and a replay assertion would
        // silently stop noticing a real change.
        assert_ne!(
            stable_hash_of(&Value::I64(1)),
            stable_hash_of(&Value::U64(1))
        );
        assert_ne!(
            stable_hash_of(&Value::F32(1.0)),
            stable_hash_of(&Value::F64(1.0))
        );
        assert_ne!(
            stable_hash_of(&Value::Unit),
            stable_hash_of(&Value::Bool(false))
        );
    }

    #[test]
    fn list_order_is_significant() {
        let ascending = Value::List(vec![Value::I64(1), Value::I64(2)]);
        let descending = Value::List(vec![Value::I64(2), Value::I64(1)]);
        assert_ne!(ascending, descending);
        assert_ne!(stable_hash_of(&ascending), stable_hash_of(&descending));
    }

    #[test]
    fn field_lookup_returns_none_off_a_non_struct() {
        assert_eq!(Value::I64(1).field("anything"), None);
        let value = Value::structure([("present", Value::Bool(true))]);
        assert_eq!(value.field("present"), Some(&Value::Bool(true)));
        assert_eq!(value.field("absent"), None);
    }

    #[test]
    fn display_is_readable_enough_to_quote_in_an_error() {
        let value = Value::structure([
            ("name", Value::String("goblin".into())),
            ("state", Value::unit_variant("Patrol")),
            ("path", Value::List(vec![Value::F32(1.5), Value::F32(2.0)])),
        ]);
        assert_eq!(
            value.to_string(),
            "{name: \"goblin\", path: [1.5, 2], state: Patrol}"
        );
    }

    #[test]
    fn shape_names_are_human_readable() {
        assert_eq!(Value::List(Vec::new()).shape(), "list");
        assert_eq!(Value::unit_variant("A").shape(), "enum");
    }
}
