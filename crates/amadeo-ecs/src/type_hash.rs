//! Deriving stable ids from Rust types.

use amadeo_core::StableHasher;

/// Hashes a type's name into a stable 64-bit id.
///
/// # Why the name and not `TypeId`
///
/// `std::any::TypeId` is the obvious tool and the wrong one here. Its values are compiler-generated
/// and carry **no stability guarantee across builds**, so using them as map keys would make
/// iteration order — and therefore state hashes — differ between compilations of identical logic.
/// That breaks invariant I3 in the most confusing possible way: golden replay tests would fail after
/// a recompile with no source change.
///
/// A type's fully-qualified name is stable across builds, and has the additional benefit of being
/// traceable back to something a human can read when diagnosing a collision.
///
/// **Services only, now.** Components and resources both use their canonical name instead — see
/// [`hash_name`]. A service is engine machinery that no file names and no hash contains (ADR 0009),
/// so it has nothing to gain from a canonical name and is not reflected in the first place.
pub(crate) fn hash_type_name<T: 'static>() -> u64 {
    hash_name(std::any::type_name::<T>())
}

/// Hashes an already-chosen name into a stable 64-bit id.
///
/// Used for [`crate::ComponentId`] and [`crate::ResourceId`], both of which hash a **canonical** name
/// (ADR 0017) rather than a Rust path, so that moving a type between crates does not change its
/// identity and *renaming* it is the deliberate breaking change.
///
/// Resources joined components here once ADR 0027 gave them a canonical name to use — ADR 0017 had
/// deferred them for exactly that reason. Until then the two followed opposite rules, which is
/// [`hash_type_name`]'s remaining caller and the whole of Q22.
pub(crate) fn hash_name(name: &str) -> u64 {
    let mut hasher = StableHasher::new();
    hasher.write_str(name);
    hasher.finish()
}

#[cfg(test)]
mod tests {
    use super::*;

    struct First;
    struct Second;

    #[test]
    fn distinct_types_hash_differently() {
        assert_ne!(hash_type_name::<First>(), hash_type_name::<Second>());
    }

    #[test]
    fn same_type_hashes_consistently() {
        assert_eq!(hash_type_name::<First>(), hash_type_name::<First>());
    }

    #[test]
    fn hash_matches_the_type_name() {
        // Pins the derivation, so switching to TypeId would fail here rather than silently.
        let mut expected = StableHasher::new();
        expected.write_str(std::any::type_name::<First>());
        assert_eq!(hash_type_name::<First>(), expected.finish());
    }
}
