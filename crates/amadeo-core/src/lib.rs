//! Foundational types shared by every Amadeo crate.
//!
//! This is the bottom of the dependency graph (see `CLAUDE.md` section 4) and depends on nothing
//! except `thiserror`. Everything here is determinism-critical, which is why several things that
//! look like they should be third-party dependencies are implemented here instead — see
//! [`rng`] and [`hash`] for the reasoning.

pub mod hash;
pub mod id;
pub mod rng;
pub mod time;

// The trait and its derive share a name, and that is deliberate: `use amadeo_core::StableHash;`
// brings in both, exactly as it does for `Debug`. Rust keeps macros and types in separate
// namespaces, so the two cannot collide.
pub use amadeo_derive::StableHash;
pub use hash::{StableHash, StableHasher, stable_hash_of};
pub use id::{NetId, StableId};
pub use rng::Rng;
pub use time::{FIXED_DT, FIXED_DT_NANOS, TICK_RATE_HZ, Tick};
