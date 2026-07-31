//! The registry: canonical name to schema, for everything the engine knows about.

use crate::Reflect;
use crate::info::TypeInfo;
use std::collections::BTreeMap;

/// What can go wrong registering a type.
#[derive(Debug, Clone, PartialEq, Eq, thiserror::Error)]
pub enum RegistryError {
    /// Two different types claimed the same canonical name.
    #[error(
        "two different types are both registered as `{name}`; rename one with \
         #[reflect(name = \"...\")], because a scene file naming `{name}` would be ambiguous"
    )]
    NameCollision {
        /// The contested name.
        name: String,
    },
}

/// Every reflected type, by canonical name.
///
/// # Why `BTreeMap`
///
/// Iteration order is reproducible across builds and machines. Anything generated from this — a
/// schema dump, a documentation page, a canonical file listing — is therefore reproducible too,
/// which is invariant I3 applied to tooling rather than to simulation. A `HashMap` would make
/// `amadeo describe` emit a differently ordered document on every run, and that document is meant to
/// be diffable.
#[derive(Debug, Default, Clone)]
pub struct TypeRegistry {
    types: BTreeMap<String, TypeInfo>,
}

impl TypeRegistry {
    /// An empty registry.
    #[must_use]
    pub fn new() -> Self {
        Self {
            types: BTreeMap::new(),
        }
    }

    /// Registers a type under its canonical name.
    ///
    /// Registering the same type twice is fine and does nothing — plugins and modules will register
    /// overlapping sets, and making that an error would push the deduplication burden onto every
    /// caller.
    ///
    /// # Errors
    ///
    /// Returns [`RegistryError::NameCollision`] if a *different* type already holds this name.
    /// Silently letting the second win would mean a scene file's `Health` loading as somebody else's
    /// `Health`, which is precisely the kind of failure that surfaces three milestones later
    /// (`CLAUDE.md` section 7, trap 5).
    pub fn register<T: Reflect>(&mut self) -> Result<(), RegistryError> {
        let info = T::type_info();
        match self.types.get(&info.name) {
            Some(existing) if *existing == info => Ok(()),
            Some(_) => Err(RegistryError::NameCollision { name: info.name }),
            None => {
                self.types.insert(info.name.clone(), info);
                Ok(())
            }
        }
    }

    /// Looks up a type's schema by canonical name.
    #[must_use]
    pub fn get(&self, name: &str) -> Option<&TypeInfo> {
        self.types.get(name)
    }

    /// Whether a name is registered.
    #[must_use]
    pub fn contains(&self, name: &str) -> bool {
        self.types.contains_key(name)
    }

    /// How many types are registered.
    #[must_use]
    pub fn len(&self) -> usize {
        self.types.len()
    }

    /// Whether anything is registered.
    #[must_use]
    pub fn is_empty(&self) -> bool {
        self.types.is_empty()
    }

    /// Every registered schema, in sorted name order.
    ///
    /// The order is part of the contract, not an accident — see the note on this type.
    pub fn iter(&self) -> impl Iterator<Item = &TypeInfo> {
        self.types.values()
    }

    /// Every registered name, in sorted order.
    pub fn names(&self) -> impl Iterator<Item = &str> {
        self.types.keys().map(String::as_str)
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::info::{ScalarKind, TypeKind};
    use crate::value::Value;
    use crate::{Reflect, ReflectError};

    /// A type whose canonical name can be varied, so a collision can be provoked.
    macro_rules! named_type {
        ($rust_name:ident, $canonical:literal) => {
            #[derive(Debug, PartialEq)]
            struct $rust_name(u32);

            impl Reflect for $rust_name {
                fn type_name() -> String {
                    $canonical.to_string()
                }
                fn type_info() -> TypeInfo {
                    TypeInfo {
                        name: $canonical.to_string(),
                        docs: String::new(),
                        version: 1,
                        kind: TypeKind::Scalar(ScalarKind::UnsignedInt),
                    }
                }
                fn to_value(&self) -> Value {
                    Value::U64(u64::from(self.0))
                }
                fn from_value(value: &Value) -> Result<Self, ReflectError> {
                    match value {
                        Value::U64(inner) => Ok($rust_name(*inner as u32)),
                        other => Err(ReflectError::mismatch($canonical, "uint", other)),
                    }
                }
            }
        };
    }

    named_type!(Health, "Health");
    named_type!(Armour, "Armour");

    /// Same canonical name as `Health`, different shape. The collision case.
    #[derive(Debug)]
    struct RivalHealth;

    impl Reflect for RivalHealth {
        fn type_name() -> String {
            "Health".to_string()
        }
        fn type_info() -> TypeInfo {
            TypeInfo {
                name: "Health".to_string(),
                docs: "a different type wanting the same name".to_string(),
                version: 1,
                kind: TypeKind::Scalar(ScalarKind::Bool),
            }
        }
        fn to_value(&self) -> Value {
            Value::Bool(true)
        }
        fn from_value(_value: &Value) -> Result<Self, ReflectError> {
            Ok(RivalHealth)
        }
    }

    #[test]
    fn registering_makes_a_type_discoverable_by_name() {
        let mut registry = TypeRegistry::new();
        assert!(registry.is_empty());

        registry.register::<Health>().expect("first registration");
        assert_eq!(registry.len(), 1);
        assert!(registry.contains("Health"));
        assert_eq!(
            registry.get("Health").map(|info| info.name.as_str()),
            Some("Health")
        );
        assert_eq!(registry.get("Nothing"), None);
    }

    #[test]
    fn registering_the_same_type_twice_is_harmless() {
        // Modules will register overlapping sets; making that an error would push deduplication
        // onto every caller.
        let mut registry = TypeRegistry::new();
        registry.register::<Health>().expect("first");
        registry.register::<Health>().expect("second is a no-op");
        assert_eq!(registry.len(), 1);
    }

    #[test]
    fn two_different_types_cannot_share_a_name() {
        let mut registry = TypeRegistry::new();
        registry.register::<Health>().expect("first");

        let error = registry
            .register::<RivalHealth>()
            .expect_err("a different type claiming `Health` must be refused");
        assert_eq!(
            error,
            RegistryError::NameCollision {
                name: "Health".to_string()
            }
        );
        // And the original survives intact.
        assert_eq!(
            registry.get("Health").map(|info| &info.kind),
            Some(&TypeKind::Scalar(ScalarKind::UnsignedInt))
        );
    }

    #[test]
    fn the_collision_message_says_how_to_fix_it() {
        let error = RegistryError::NameCollision {
            name: "Health".to_string(),
        };
        let message = error.to_string();
        assert!(
            message.contains("#[reflect(name"),
            "the message must name the fix, not just the problem: {message}"
        );
    }

    #[test]
    fn iteration_is_sorted_not_insertion_ordered() {
        // The property that makes anything generated from the registry diffable.
        let mut registry = TypeRegistry::new();
        registry.register::<Health>().expect("registers");
        registry.register::<Armour>().expect("registers");

        let names: Vec<&str> = registry.names().collect();
        assert_eq!(names, vec!["Armour", "Health"]);

        // And building it the other way round agrees.
        let mut reversed = TypeRegistry::new();
        reversed.register::<Armour>().expect("registers");
        reversed.register::<Health>().expect("registers");
        assert_eq!(reversed.names().collect::<Vec<_>>(), names);
    }
}
