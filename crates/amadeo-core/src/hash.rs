//! Stable hashing, used to fingerprint simulation state.
//!
//! # Why this is hand-written rather than a dependency
//!
//! Same reasoning as [`crate::rng`], but stronger. This hash is the assertion in every golden
//! replay test (ADR 0005): "run 600 ticks, hash the world, compare against the recorded value." If
//! the hash algorithm ever changes, every recorded value in the repository becomes wrong
//! simultaneously — and it would look like the engine broke.
//!
//! So it is pinned here. The algorithm is FNV-1a (64-bit): about six lines, fully specified, and
//! entirely adequate for change detection. It is **not** cryptographic and must never be used for
//! anything security-related.
//!
//! # Why not `std::hash::Hash`?
//!
//! Two reasons, both fatal for our purposes:
//!
//! 1. The standard library makes no stability guarantee about its `Hash` implementations across
//!    compiler versions.
//! 2. `f32` and `f64` do not implement `Hash` at all, because `NaN != NaN` breaks the contract.
//!    Floats are most of what a game's state consists of, so we need an answer rather than an
//!    exclusion — see [`StableHasher::write_f32`].

/// An FNV-1a 64-bit hasher, used to fingerprint simulation state deterministically.
///
/// Feed values in with the `write_*` methods, then read the result with [`StableHasher::finish`].
/// Order matters: the same values fed in a different order produce a different hash. That is
/// desirable here, because entity iteration order is part of what we want to pin down.
#[derive(Debug, Clone)]
pub struct StableHasher {
    state: u64,
}

/// FNV-1a 64-bit offset basis, from the algorithm's specification.
const FNV_OFFSET_BASIS: u64 = 0xcbf2_9ce4_8422_2325;
/// FNV-1a 64-bit prime, from the algorithm's specification.
const FNV_PRIME: u64 = 0x0000_0100_0000_01b3;

impl Default for StableHasher {
    fn default() -> Self {
        Self::new()
    }
}

impl StableHasher {
    /// Creates a fresh hasher.
    #[must_use]
    pub fn new() -> Self {
        Self {
            state: FNV_OFFSET_BASIS,
        }
    }

    /// Absorbs a single byte. Every other `write_*` method reduces to this.
    pub fn write_u8(&mut self, value: u8) {
        self.state ^= u64::from(value);
        self.state = self.state.wrapping_mul(FNV_PRIME);
    }

    /// Absorbs a byte slice.
    pub fn write_bytes(&mut self, bytes: &[u8]) {
        for &byte in bytes {
            self.write_u8(byte);
        }
    }

    /// Absorbs a `u32`, little-endian.
    pub fn write_u32(&mut self, value: u32) {
        self.write_bytes(&value.to_le_bytes());
    }

    /// Absorbs a `u64`, little-endian.
    pub fn write_u64(&mut self, value: u64) {
        self.write_bytes(&value.to_le_bytes());
    }

    /// Absorbs an `i32`, little-endian.
    pub fn write_i32(&mut self, value: i32) {
        self.write_bytes(&value.to_le_bytes());
    }

    /// Absorbs an `i64`, little-endian.
    pub fn write_i64(&mut self, value: i64) {
        self.write_bytes(&value.to_le_bytes());
    }

    /// Absorbs a `bool`.
    pub fn write_bool(&mut self, value: bool) {
        self.write_u8(u8::from(value));
    }

    /// Absorbs a string's bytes.
    pub fn write_str(&mut self, value: &str) {
        self.write_bytes(value.as_bytes());
    }

    /// Hashes a string at **compile time**.
    ///
    /// Exactly equivalent to `StableHasher::new()`, `write_str(name)`, `finish()` — pinned by
    /// `const_hash_agrees_with_the_hasher` below, because the two drifting apart would silently
    /// change every [`crate::StableHash`]-derived id in the engine.
    ///
    /// # Why this exists
    ///
    /// `ComponentId` is the FNV-1a hash of a component's canonical name (ADR 0017). That name is
    /// fixed at compile time, so the id is a constant — but computing it through the hasher happens
    /// at runtime, on the hot path of every component lookup. Measured at 20,000 sprites, that cost
    /// dominated the sprite batcher (Q16).
    ///
    /// Written as a `while` loop over bytes rather than an iterator because `for` and `Iterator` are
    /// not available in a `const fn`. That is the only reason; it is otherwise the same three lines
    /// as [`StableHasher::write_u8`].
    #[must_use]
    pub const fn hash_str(name: &str) -> u64 {
        let bytes = name.as_bytes();
        let mut state = FNV_OFFSET_BASIS;
        let mut index = 0;

        while index < bytes.len() {
            state ^= bytes[index] as u64;
            state = state.wrapping_mul(FNV_PRIME);
            index += 1;
        }

        state
    }

    /// Absorbs an `f32`, canonicalising the awkward cases first.
    ///
    /// Two float values need special handling before hashing, or identical-looking states would
    /// hash differently:
    ///
    /// - **NaN** has many bit patterns, and `NaN != NaN`. All NaNs collapse to one canonical
    ///   pattern, so two states that both contain "not a number" agree.
    /// - **Negative zero** has a different bit pattern from positive zero even though
    ///   `-0.0 == 0.0`. It is normalised to `+0.0`.
    ///
    /// Note this means the hash cannot distinguish a NaN that arrived one way from a NaN that
    /// arrived another. That is the right trade: NaN in simulation state is a bug to be caught by
    /// validation, not something to fingerprint precisely.
    pub fn write_f32(&mut self, value: f32) {
        let canonical = if value.is_nan() {
            f32::NAN
        } else if value == 0.0 {
            0.0
        } else {
            value
        };
        self.write_bytes(&canonical.to_bits().to_le_bytes());
    }

    /// Absorbs an `f64`. Same canonicalisation as [`StableHasher::write_f32`].
    pub fn write_f64(&mut self, value: f64) {
        let canonical = if value.is_nan() {
            f64::NAN
        } else if value == 0.0 {
            0.0
        } else {
            value
        };
        self.write_bytes(&canonical.to_bits().to_le_bytes());
    }

    /// Returns the hash accumulated so far.
    ///
    /// Does not consume the hasher, so intermediate fingerprints can be read mid-stream.
    #[must_use]
    pub fn finish(&self) -> u64 {
        self.state
    }
}

/// Types that can contribute to a stable state fingerprint.
///
/// Implement this for any component whose value should participate in golden replay assertions.
///
/// # Derive it
///
/// ```
/// use amadeo_core::StableHash;
///
/// #[derive(StableHash)]
/// struct Velocity {
///     x: f32,
///     y: f32,
/// }
/// ```
///
/// **Prefer the derive over writing it by hand.** A hand-written impl that forgets a field still
/// compiles, still runs, and still produces a plausible number — while silently excluding part of
/// the simulation from every golden replay assertion. That is the worst failure shape available
/// under invariant I3: the tests keep passing and quietly stop testing.
///
/// The derive hashes fields **sorted by name**, so reordering fields does not change the
/// fingerprint. Converting an existing hand-written impl whose fields were not already in
/// alphabetical order will change that type's hash — a deliberate golden-replay regeneration, not a
/// bug. See `docs/07-working-with-the-code.md` on golden replays.
pub trait StableHash {
    /// Feeds this value into the hasher.
    fn stable_hash(&self, hasher: &mut StableHasher);
}

/// Convenience: hash one value on its own and return the fingerprint.
pub fn stable_hash_of<T: StableHash + ?Sized>(value: &T) -> u64 {
    let mut hasher = StableHasher::new();
    value.stable_hash(&mut hasher);
    hasher.finish()
}

// Implementations for primitives. Written out rather than macro-generated: a macro here would save
// perhaps thirty lines and cost readability, which is the wrong trade in this project
// (CLAUDE.md section 6).

impl StableHash for u8 {
    fn stable_hash(&self, hasher: &mut StableHasher) {
        hasher.write_u8(*self);
    }
}

impl StableHash for u32 {
    fn stable_hash(&self, hasher: &mut StableHasher) {
        hasher.write_u32(*self);
    }
}

impl StableHash for u64 {
    fn stable_hash(&self, hasher: &mut StableHasher) {
        hasher.write_u64(*self);
    }
}

impl StableHash for i32 {
    fn stable_hash(&self, hasher: &mut StableHasher) {
        hasher.write_i32(*self);
    }
}

impl StableHash for i64 {
    fn stable_hash(&self, hasher: &mut StableHasher) {
        hasher.write_i64(*self);
    }
}

impl StableHash for f32 {
    fn stable_hash(&self, hasher: &mut StableHasher) {
        hasher.write_f32(*self);
    }
}

impl StableHash for f64 {
    fn stable_hash(&self, hasher: &mut StableHasher) {
        hasher.write_f64(*self);
    }
}

impl StableHash for bool {
    fn stable_hash(&self, hasher: &mut StableHasher) {
        hasher.write_bool(*self);
    }
}

// The length goes first, for exactly the reason `[T]` below writes its own: without it, two string
// fields sitting next to each other can be confused by moving a character between them. `("ab", "")`
// and `("a", "b")` would hash identically, and that is reachable from content rather than
// theoretical -- `Camera { target: "", environment: "x" }` and
// `Camera { target: "x", environment: "" }` are different worlds that must not share a state hash.
//
// Deliberately here rather than inside `StableHasher::write_str`, which stays a raw primitive: it is
// what `hash_str` mirrors for component *names*, where a name is hashed alone and never adjacent to
// another field, and changing it would move every `ComponentId` in the engine for no gain.
impl StableHash for str {
    fn stable_hash(&self, hasher: &mut StableHasher) {
        hasher.write_u64(self.len() as u64);
        hasher.write_str(self);
    }
}

impl StableHash for String {
    fn stable_hash(&self, hasher: &mut StableHasher) {
        self.as_str().stable_hash(hasher);
    }
}

impl<T: StableHash> StableHash for [T] {
    fn stable_hash(&self, hasher: &mut StableHasher) {
        // Length is hashed first so that [a] and [a, b] cannot collide with each other, and so
        // that concatenation ambiguities ([a],[b] vs [a,b]) are impossible.
        hasher.write_u64(self.len() as u64);
        for item in self {
            item.stable_hash(hasher);
        }
    }
}

impl<T: StableHash> StableHash for Vec<T> {
    fn stable_hash(&self, hasher: &mut StableHasher) {
        self.as_slice().stable_hash(hasher);
    }
}

impl<T: StableHash, const N: usize> StableHash for [T; N] {
    fn stable_hash(&self, hasher: &mut StableHasher) {
        // Delegates to the slice impl, so `[f32; 2]` and `&[f32]` holding the same values agree.
        // The length it writes is redundant for a fixed-size array, and worth the redundancy to
        // keep those two from disagreeing.
        self.as_slice().stable_hash(hasher);
    }
}

impl<T: StableHash> StableHash for Option<T> {
    fn stable_hash(&self, hasher: &mut StableHasher) {
        match self {
            None => hasher.write_u8(0),
            Some(value) => {
                hasher.write_u8(1);
                value.stable_hash(hasher);
            }
        }
    }
}

impl<A: StableHash, B: StableHash> StableHash for (A, B) {
    fn stable_hash(&self, hasher: &mut StableHasher) {
        self.0.stable_hash(hasher);
        self.1.stable_hash(hasher);
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn const_hash_agrees_with_the_hasher() {
        // The safety net for `hash_str`. If these two ever disagreed, every `ComponentId` would
        // change, every state hash containing a component would change, and every committed replay
        // would fail at once — with nothing pointing at the cause.
        //
        // The empty string is included deliberately: it is the default `Reflect::STATIC_NAME`, so
        // it is the value a type that opts out of the constant path carries.
        for name in [
            "",
            "a",
            "Transform",
            "GlobalTransform",
            "SortOrder",
            "some::path::With::Colons",
            "unicode \u{2014} em dash",
        ] {
            let mut hasher = StableHasher::new();
            hasher.write_str(name);

            assert_eq!(
                StableHasher::hash_str(name),
                hasher.finish(),
                "const and runtime hashes disagree for {name:?}"
            );
        }
    }

    #[test]
    fn moving_a_character_between_two_string_fields_changes_the_hash() {
        // Found in session 9 by adding a second string field to `Camera` and noticing that the
        // golden replays did *not* move when they should have. Without a length prefix, two
        // adjacent strings hash as their concatenation, so `("ab", "")` and `("a", "b")` collide —
        // two genuinely different worlds sharing a state hash, which is invariant I3 failing
        // silently rather than loudly.
        //
        // Reachable from content, not theoretical: `Camera` carries `target` and `environment`
        // side by side, and both hold asset ids an author types.
        let pair = |left: &str, right: &str| {
            let mut hasher = StableHasher::new();
            left.stable_hash(&mut hasher);
            right.stable_hash(&mut hasher);
            hasher.finish()
        };

        assert_ne!(pair("ab", ""), pair("a", "b"));
        assert_ne!(pair("", "ab"), pair("ab", ""));
        assert_ne!(pair("a", "bc"), pair("ab", "c"));
        // And the ordinary case still behaves: the same pair hashes the same way twice.
        assert_eq!(pair("wall", "corridor"), pair("wall", "corridor"));
    }

    #[test]
    fn an_empty_string_field_is_not_invisible() {
        // The specific symptom: adding an empty-by-default string field to a component used to
        // contribute nothing at all, so a schema change that should have moved every state hash
        // moved none of them.
        let mut with = StableHasher::new();
        "".stable_hash(&mut with);
        assert_ne!(with.finish(), StableHasher::new().finish());
    }

    #[test]
    fn const_hash_really_is_const() {
        // Evaluated at compile time, not merely evaluatable. If `hash_str` stopped being a `const
        // fn` this would not compile, which is the point — the whole optimisation is that the
        // answer exists before the program runs.
        const NAME: &str = "Transform";
        const HASHED: u64 = StableHasher::hash_str(NAME);

        let mut hasher = StableHasher::new();
        hasher.write_str(NAME);
        assert_eq!(HASHED, hasher.finish());
    }

    #[test]
    fn identical_input_gives_identical_hash() {
        assert_eq!(stable_hash_of(&42u64), stable_hash_of(&42u64));
    }

    #[test]
    fn different_input_gives_different_hash() {
        assert_ne!(stable_hash_of(&42u64), stable_hash_of(&43u64));
    }

    #[test]
    fn order_matters() {
        let a = vec![1u32, 2, 3];
        let b = vec![3u32, 2, 1];
        assert_ne!(stable_hash_of(&a), stable_hash_of(&b));
    }

    #[test]
    fn length_prefix_prevents_concatenation_collisions() {
        // Without hashing the length, these two would be indistinguishable.
        let split: (Vec<u32>, Vec<u32>) = (vec![1], vec![2, 3]);
        let other: (Vec<u32>, Vec<u32>) = (vec![1, 2], vec![3]);
        assert_ne!(stable_hash_of(&split), stable_hash_of(&other));
    }

    #[test]
    fn nan_hashes_consistently() {
        // Different NaN bit patterns must agree, or a state containing NaN would hash
        // unpredictably from run to run. Negating a NaN flips its sign bit, producing a genuinely
        // different bit pattern that is still NaN -- exactly the case the canonicalisation exists
        // to collapse.
        let positive_nan = f32::NAN;
        let negative_nan = -f32::NAN;
        assert!(positive_nan.is_nan() && negative_nan.is_nan());
        assert_ne!(
            positive_nan.to_bits(),
            negative_nan.to_bits(),
            "test is pointless if the bit patterns already match"
        );
        assert_eq!(stable_hash_of(&positive_nan), stable_hash_of(&negative_nan));
    }

    #[test]
    fn negative_zero_hashes_as_positive_zero() {
        assert_eq!(stable_hash_of(&0.0f32), stable_hash_of(&-0.0f32));
        assert_eq!(stable_hash_of(&0.0f64), stable_hash_of(&-0.0f64));
    }

    #[test]
    fn ordinary_floats_still_distinguish() {
        assert_ne!(stable_hash_of(&1.0f32), stable_hash_of(&1.000_001f32));
        assert_ne!(stable_hash_of(&1.0f32), stable_hash_of(&-1.0f32));
    }

    #[test]
    fn option_none_differs_from_some_default() {
        let none: Option<u32> = None;
        let some_zero: Option<u32> = Some(0);
        assert_ne!(stable_hash_of(&none), stable_hash_of(&some_zero));
    }

    #[test]
    fn hash_is_pinned_to_known_values() {
        // Regression guards. If the algorithm is ever replaced, these fail loudly, and whoever did
        // it must confront the fact that every recorded replay in the repository just became
        // invalid. That alarm is the point -- do NOT "fix" these by updating the numbers without an
        // ADR explaining why every golden replay is being re-recorded.
        //
        // The expected value below was cross-checked against an independent FNV-1a implementation
        // written from the specification, not copied from this code. That distinction matters: a
        // constant derived from our own output would only assert that this code is
        // self-consistent, which would still pass if the implementation were subtly wrong.
        assert_eq!(StableHasher::new().finish(), FNV_OFFSET_BASIS);

        let mut hasher = StableHasher::new();
        hasher.write_str("amadeo");
        assert_eq!(hasher.finish(), 0xc3e3_fe2b_8ec1_d932);
    }
}
