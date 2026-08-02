//! Type registry, schema, and the canonical value model.
//!
//! One registry, three consumers — canonical text serialisation, the editor inspector, and agent
//! introspection. That is invariant I8 and Pillar 2 of `docs/03-ai-native-design.md`: **if a type is
//! not reflected, it does not exist** as far as the editor and the agent are concerned.
//!
//! ```
//! use amadeo_reflect::{Reflect, TypeRegistry, Value};
//!
//! /// How much damage something can take.
//! #[derive(Debug, PartialEq, Reflect)]
//! struct Health {
//!     /// Current hit points.
//!     #[reflect(min = 0.0, max = 100.0, unit = "hp", sync = "on_change")]
//!     current: f32,
//!     /// Whether this entity ignores damage entirely.
//!     invulnerable: bool,
//! }
//!
//! // The schema an agent or an inspector reads.
//! let info = Health::type_info();
//! assert_eq!(info.name, "Health");
//! assert_eq!(info.field("current").unwrap().unit.as_deref(), Some("hp"));
//! assert_eq!(info.docs, "How much damage something can take.");
//!
//! // The value tree the serialiser reads. Fields come out sorted, always.
//! let health = Health { current: 75.0, invulnerable: false };
//! let value = health.to_value();
//! assert_eq!(value.to_string(), "{current: 75, invulnerable: false}");
//!
//! // And it round-trips.
//! assert_eq!(Health::from_value(&value).unwrap(), health);
//!
//! // Registration is what makes it discoverable by name.
//! let mut registry = TypeRegistry::new();
//! registry.register::<Health>().unwrap();
//! assert!(registry.get("Health").is_some());
//! ```
//!
//! # The three pieces
//!
//! - [`Value`] — the canonical data tree. Struct fields are sorted by construction, which is how
//!   byte-stable serialisation (I2) stops depending on every writer remembering to sort.
//! - [`TypeInfo`] — the schema. What fields exist, what they mean, valid ranges, units, and the
//!   replication annotations ADR 0006 reserved.
//! - [`TypeRegistry`] — the name-to-schema map, iterated in sorted order so anything derived from
//!   it is reproducible (I3).
//!
//! # What this crate deliberately does not do
//!
//! No text syntax, and no JSON. The concrete scene syntax is an undecided question (Q2) and a
//! designed artefact in its own right — letting a serialiser's default output become the format is
//! trap 4 in `CLAUDE.md` section 7. This crate produces the *model*; `amadeo-scene` and
//! `amadeo-cli` render it.
//!
//! It also knows nothing about entities or worlds. `amadeo-reflect` sits below `amadeo-ecs` in the
//! crate order (invariant I6), so the glue that inserts a reflected component onto an entity lives
//! there, not here.

#![doc(html_no_source)]

mod impls;
mod info;
mod registry;
mod value;

pub use amadeo_derive::Reflect;
pub use impls::ReflectKey;
pub use info::{
    FieldInfo, Interpolation, Range, Replication, ScalarKind, SyncPolicy, TypeInfo, TypeKind,
    VariantInfo,
};
pub use registry::{RegistryError, TypeRegistry};
pub use value::{EnumValue, Value};

/// What can go wrong converting a [`Value`] back into a Rust type.
///
/// Every variant names the type, what was wrong, and what would have been right. That is not
/// politeness — an error message is the agent's only feedback channel here, and a message that does
/// not say what to do next is a defect (`docs/03-ai-native-design.md` Pillar 5).
#[derive(Debug, Clone, PartialEq, Eq, thiserror::Error)]
pub enum ReflectError {
    /// The value had the wrong shape entirely.
    #[error("{type_name}: expected {expected}, found {found}")]
    TypeMismatch {
        /// The type being built.
        type_name: String,
        /// What was needed, in [`Value::shape`] terms.
        expected: String,
        /// What was there instead.
        found: String,
    },

    /// A required field was absent.
    #[error("{type_name}: missing field `{field}`; required fields are {required}")]
    MissingField {
        /// The type being built.
        type_name: String,
        /// The field that was not supplied.
        field: String,
        /// Every field the type needs, comma separated.
        required: String,
    },

    /// The value carried a field the type does not have.
    ///
    /// Reported rather than ignored, because silently dropping an unknown field turns a typo into a
    /// value that mysteriously never takes effect.
    #[error("{type_name}: unknown field `{field}`; {type_name} has {known}")]
    UnknownField {
        /// The type being built.
        type_name: String,
        /// The field that does not belong.
        field: String,
        /// Every field the type does have, comma separated.
        known: String,
    },

    /// The value named a variant this enum does not have.
    #[error("{type_name}: `{variant}` is not a variant; valid variants are {known}")]
    UnknownVariant {
        /// The enum being built.
        type_name: String,
        /// The variant that was named.
        variant: String,
        /// Every valid variant, comma separated.
        known: String,
    },

    /// A number was well-formed but did not fit the target type.
    #[error("{type_name}: {value} does not fit in {target}")]
    OutOfRange {
        /// The type being built.
        type_name: String,
        /// The offending value.
        value: String,
        /// The Rust type it had to fit.
        target: String,
    },

    /// A fixed-size array got the wrong number of elements.
    #[error("{type_name}: expected {expected} elements, found {found}")]
    WrongLength {
        /// The type being built.
        type_name: String,
        /// How many elements were needed.
        expected: usize,
        /// How many were supplied.
        found: usize,
    },
}

impl ReflectError {
    /// Builds a [`ReflectError::TypeMismatch`] from a value, filling in what it actually was.
    ///
    /// A helper because the derive and every primitive impl construct this same error, and
    /// hand-writing it each time is how the `found` half ends up wrong.
    #[must_use]
    pub fn mismatch(type_name: impl Into<String>, expected: &str, found: &Value) -> Self {
        ReflectError::TypeMismatch {
            type_name: type_name.into(),
            expected: expected.to_string(),
            found: found.shape().to_string(),
        }
    }
}

/// A type that can describe itself and convert to and from [`Value`].
///
/// Almost always derived rather than written by hand:
///
/// ```
/// use amadeo_reflect::Reflect;
///
/// /// A two-dimensional position.
/// #[derive(Debug, Reflect)]
/// struct Position {
///     /// Horizontal, in world units.
///     #[reflect(unit = "m", sync = "on_change", interpolate = "linear")]
///     x: f32,
///     /// Vertical, in world units.
///     #[reflect(unit = "m", sync = "on_change", interpolate = "linear")]
///     y: f32,
/// }
/// ```
///
/// # Supported attributes
///
/// On the type: `#[reflect(name = "...", version = N)]`.
///
/// On a field: `#[reflect(min = F, max = F, unit = "...", sync = "...", interpolate = "...")]`, or
/// `#[reflect(skip)]` to leave a field out of the schema entirely — a skipped field must implement
/// `Default`, since nothing restores it.
///
/// `sync` is one of `never` (the default), `on_change`, `always`. `interpolate` is one of `none`
/// (the default), `linear`, `angular`. Both are ADR 0006 hooks and do nothing until M6.
///
/// # Why `Sized`, and why no `dyn Reflect`
///
/// `from_value` returns `Self`, so the trait is not object-safe. That is a deliberate trade: making
/// it object-safe would mean a separate boxed-construction path and downcasts at every level, for a
/// capability this engine does not need — the consumers all want a whole value tree, not a cursor
/// into a live one. Type-erased operations that *do* need a trait object (inserting a component onto
/// an entity by name) are built in `amadeo-ecs` from monomorphised function pointers instead.
pub trait Reflect: Sized + 'static {
    /// The canonical name as a **compile-time constant**, or `""` when it is only known at runtime.
    ///
    /// `#[derive(Reflect)]` fills this in for every struct and enum, honouring
    /// `#[reflect(name = "...")]`. It is empty for the generic impls — `[T; N]`, `Option<T>`,
    /// `Vec<T>` — whose names are built from their parameters and so cannot be a single constant.
    ///
    /// # Why a constant as well as [`Reflect::type_name`]
    ///
    /// `ComponentId` is the hash of this name (ADR 0017), and a component's name never changes while
    /// the program runs — so its id is a constant. But `type_name()` returns a fresh `String`, which
    /// means computing that id allocates *and* hashes, and it sits on the hot path of every
    /// component lookup. At 20,000 sprites that cost dominated the sprite batcher (Q16).
    ///
    /// So concrete types carry their name here, where [`Reflect::STATIC_NAME_HASH`] can turn it into
    /// an id before the program starts. Anything that cannot falls back to the old path and is no
    /// worse off — nothing that reaches `ComponentId` is generic, because a component is a struct.
    const STATIC_NAME: &'static str = "";

    /// FNV-1a of [`Reflect::STATIC_NAME`], computed at compile time.
    ///
    /// Never override this. It has a default precisely so that setting `STATIC_NAME` is the only
    /// thing a type has to do, and so the two cannot disagree.
    const STATIC_NAME_HASH: u64 = amadeo_core::StableHasher::hash_str(Self::STATIC_NAME);

    /// The canonical name, as it appears in text files and in the registry.
    ///
    /// Allocates. Prefer [`Reflect::STATIC_NAME`] on any path that runs more than once, and see the
    /// note there for why both exist.
    fn type_name() -> String;

    /// The full schema.
    ///
    /// Allocates. Called at registration and when something asks for a description — never in a
    /// simulation tick.
    fn type_info() -> TypeInfo;

    /// Converts to the canonical value tree.
    fn to_value(&self) -> Value;

    /// Rebuilds from the canonical value tree.
    ///
    /// # Errors
    ///
    /// Returns a [`ReflectError`] naming the type, the problem, and what would have been valid.
    fn from_value(value: &Value) -> Result<Self, ReflectError>;
}
