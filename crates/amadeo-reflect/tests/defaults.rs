//! `default_value` — the thing that lets a save survive a component gaining a field (ADR 0069).
//!
//! Out here rather than inline because these need `#[derive(Reflect)]`, and the derive emits
//! `amadeo_reflect::` paths that do not resolve from inside the crate itself. `tests/derive.rs` is
//! here for the same reason.

use amadeo_reflect::{Reflect, TypeRegistry, Value, default_value_for};

#[derive(Debug, PartialEq, Reflect)]
struct Leaf {
    /// A number.
    size: f32,
    /// A flag.
    on: bool,
}

#[derive(Debug, PartialEq, Reflect)]
struct Nested {
    /// One level down, to prove the recursion works.
    leaf: Leaf,
    /// A fixed-length array, which must not default to empty.
    pair: [f32; 2],
    /// A growable list, which must.
    many: Vec<f32>,
    /// An absent value.
    maybe: Option<f32>,
    /// A name.
    label: String,
    /// An unsigned count, since zero has a different spelling from a signed zero.
    count: u32,
}

#[derive(Debug, PartialEq, Reflect)]
enum Mode {
    /// Doing nothing.
    Idle,
    /// Doing something.
    Busy,
}

#[derive(Debug, PartialEq, Reflect)]
struct HasEnum {
    /// The field with no honest default.
    mode: Mode,
}

fn registry_with<T: Reflect>() -> TypeRegistry {
    let mut types = TypeRegistry::new();
    types.register::<T>().expect("registers");
    types
}

#[test]
fn a_default_value_builds_the_type_it_describes() {
    let types = registry_with::<Nested>();
    let value = default_value_for("Nested", &types).expect("has a default");

    // The only claim worth asserting: a default is something `from_value` accepts. Comparing the
    // rendered text instead would be testing `Value`'s formatter, which has its own tests.
    let built = Nested::from_value(&value).expect("a default value builds the type");

    assert_eq!(built.pair, [0.0, 0.0]);
    assert!(built.many.is_empty());
    assert_eq!(built.maybe, None);
    assert_eq!(built.label, "");
    assert_eq!(built.count, 0);
    assert_eq!(
        built.leaf,
        Leaf {
            size: 0.0,
            on: false
        }
    );
}

#[test]
fn a_fixed_array_defaults_to_the_right_length_and_a_vec_to_empty() {
    let types = registry_with::<Nested>();
    let value = default_value_for("Nested", &types).expect("has a default");
    let Value::Struct(fields) = &value else {
        panic!("expected a struct, got {value}");
    };

    // The trap. An empty list for `[f32; 2]` is rejected by `from_value` for the wrong length,
    // which surfaces as a corrupt save rather than as a missing default — a much worse message
    // than the one the reader needs.
    assert_eq!(fields["pair"], Value::List(vec![Value::F32(0.0); 2]));
    assert_eq!(fields["many"], Value::List(Vec::new()));
}

#[test]
fn an_enum_is_refused_and_the_reason_names_the_path_to_it() {
    let types = registry_with::<HasEnum>();
    let why = default_value_for("HasEnum", &types).expect_err("an enum has no honest default");

    // A reason that named only `Mode` would leave the reader hunting for which field held one.
    assert!(
        why.contains("HasEnum") && why.contains("mode"),
        "the reason should name the path to the field, got: {why}"
    );
    assert!(
        why.contains("guess"),
        "the reason should say why refusing is deliberate rather than unfinished, got: {why}"
    );
}

#[test]
fn an_unregistered_type_says_so_rather_than_inventing_a_value() {
    let types = TypeRegistry::new();
    let why = default_value_for("Nothing", &types).expect_err("nothing is registered");
    assert!(why.contains("Nothing"), "got: {why}");
}
