//! The engine's derive macros: `#[derive(Reflect)]` and `#[derive(StableHash)]`.
//!
//! **Not used directly.** Each macro is re-exported next to the trait it implements, so a caller
//! writes `use amadeo_core::StableHash;` or `use amadeo_reflect::Reflect;` and gets the trait and
//! its derive together — the way `Debug` works, and the least surprising arrangement.
//!
//! # Where this sits in the crate graph
//!
//! Below everything, because it depends on no engine crate at all — only `syn`, `quote`, and
//! `proc-macro2`. That is what lets `amadeo-core`, the bottom of the runtime graph, re-export
//! `StableHash` from here without creating a cycle (invariant I6). A proc-macro crate is a
//! compile-time tool, not a runtime dependency, and nothing it emits references it.
//!
//! # Why a derive macro at all
//!
//! Invariant I8 says an unreflected type does not exist as far as the editor and the agent are
//! concerned, and trap 5 in `CLAUDE.md` section 7 is "skipping reflection registration". Both of
//! those are really predictions about human behaviour: if registering a component means hand-writing
//! three functions, people will skip it, and the cost surfaces three milestones later.
//!
//! So the macro exists to make the correct thing the lazy thing. It generates exactly the code you
//! would write by hand — there is no runtime machinery hiding behind it, and `cargo expand` on any
//! derived type shows plain, readable Rust.
//!
//! # What it supports
//!
//! - structs with named fields
//! - newtype structs (one unnamed field), which are **transparent**: `Health(f32)` serialises as a
//!   bare number, not as a wrapper, because that is what a human writing a scene file expects
//! - unit structs
//! - enums whose variants are unit or named-field
//!
//! Deliberately unsupported, with a clear compile error rather than surprising output: tuple structs
//! of two or more fields, tuple *variants*, and generic types. Each would need a positional
//! representation in the text format, and that is a format design question (Q2), not something a
//! macro should answer by default.

use proc_macro::TokenStream;
use proc_macro2::TokenStream as TokenStream2;
use quote::{format_ident, quote};
use syn::{
    Attribute, Data, DeriveInput, Expr, ExprLit, Fields, Lit, LitStr, Meta, parse_macro_input,
};

/// Derives `amadeo_reflect::Reflect`.
///
/// See the crate docs for what is supported, and the `Reflect` trait docs in `amadeo-reflect` for
/// the attribute vocabulary.
#[proc_macro_derive(Reflect, attributes(reflect))]
pub fn derive_reflect(input: TokenStream) -> TokenStream {
    let input = parse_macro_input!(input as DeriveInput);
    match expand(input) {
        Ok(tokens) => tokens.into(),
        Err(error) => error.to_compile_error().into(),
    }
}

/// Derives `amadeo_core::StableHash` by hashing each field.
///
/// # Why this exists
///
/// A hand-written `stable_hash` that forgets a field still compiles, still runs, and still produces
/// a plausible number — while silently excluding part of the simulation from every golden replay
/// assertion. That is the worst possible failure shape for invariant I3: the tests keep passing and
/// stop testing. Deriving it makes the omission impossible.
///
/// # Field order is sorted by name, not declaration order
///
/// So that reordering fields — a pure refactor with no behavioural meaning — does not change every
/// state hash in the project and invalidate every committed replay. This matches how the archetype
/// hashes components (sorted by id) and how `amadeo_reflect::Value` hashes structs. (Not a rustdoc
/// link: this crate cannot depend on `amadeo-reflect`, since `amadeo-reflect` depends on it.)
///
/// **Converting an existing hand-written impl to this derive will change that type's hash** if its
/// fields were not already in alphabetical order. That is a deliberate golden-replay regeneration,
/// not a bug — see `docs/07-working-with-the-code.md` on golden replays.
///
/// # `#[reflect(skip)]` is honoured
///
/// A skipped field is excluded here too. It is not serialised, so including it would make a
/// round-tripped value hash differently from the original — which would break save/load and
/// snapshot comparison. A field that genuinely is authoritative state should not be skipped.
#[proc_macro_derive(StableHash, attributes(reflect))]
pub fn derive_stable_hash(input: TokenStream) -> TokenStream {
    let input = parse_macro_input!(input as DeriveInput);
    match expand_stable_hash(input) {
        Ok(tokens) => tokens.into(),
        Err(error) => error.to_compile_error().into(),
    }
}

fn expand_stable_hash(input: DeriveInput) -> syn::Result<TokenStream2> {
    if !input.generics.params.is_empty() {
        return Err(syn::Error::new_spanned(
            &input.generics,
            "#[derive(StableHash)] does not support generic types; implement it by hand",
        ));
    }

    let ident = &input.ident;
    let body = match &input.data {
        Data::Struct(data) => stable_hash_struct_body(&data.fields)?,
        Data::Enum(data) => stable_hash_enum_body(data)?,
        Data::Union(_) => {
            return Err(syn::Error::new_spanned(
                ident,
                "#[derive(StableHash)] does not support unions",
            ));
        }
    };

    Ok(quote! {
        impl ::amadeo_core::StableHash for #ident {
            fn stable_hash(&self, hasher: &mut ::amadeo_core::StableHasher) {
                #body
            }
        }
    })
}

fn stable_hash_struct_body(fields: &Fields) -> syn::Result<TokenStream2> {
    match fields {
        Fields::Named(named) => {
            // Collected then sorted by name, so declaration order cannot influence the hash.
            let mut hashed: Vec<(String, TokenStream2)> = Vec::new();
            for field in &named.named {
                let Some(ident) = field.ident.as_ref() else {
                    continue;
                };
                if parse_field_options(&field.attrs)?.skip {
                    continue;
                }
                hashed.push((
                    ident.to_string(),
                    quote! { ::amadeo_core::StableHash::stable_hash(&self.#ident, hasher); },
                ));
            }
            hashed.sort_by(|left, right| left.0.cmp(&right.0));
            let writes: Vec<TokenStream2> = hashed.into_iter().map(|(_, tokens)| tokens).collect();
            Ok(quote! { #(#writes)* })
        }

        Fields::Unnamed(unnamed) => {
            // Positional, so declaration order *is* the identity and sorting would be meaningless.
            let writes: Vec<TokenStream2> = (0..unnamed.unnamed.len())
                .map(|index| {
                    let index = syn::Index::from(index);
                    quote! { ::amadeo_core::StableHash::stable_hash(&self.#index, hasher); }
                })
                .collect();
            Ok(quote! { #(#writes)* })
        }

        // Carries no state, so it contributes nothing. Its presence is already recorded by the
        // component id the archetype writes before calling this.
        Fields::Unit => Ok(quote! {}),
    }
}

fn stable_hash_enum_body(data: &syn::DataEnum) -> syn::Result<TokenStream2> {
    let mut arms = Vec::new();

    for variant in &data.variants {
        let ident = &variant.ident;
        // The variant's *name*, not its index: inserting a variant in the middle would otherwise
        // renumber everything after it and change hashes that have nothing to do with the change.
        let name = ident.to_string();

        match &variant.fields {
            Fields::Unit => arms.push(quote! {
                Self::#ident => { hasher.write_str(#name); }
            }),

            Fields::Named(named) => {
                let mut hashed: Vec<(String, syn::Ident, TokenStream2)> = Vec::new();
                for field in &named.named {
                    let Some(field_ident) = field.ident.as_ref() else {
                        continue;
                    };
                    let binding = format_ident!("field_{}", field_ident);
                    hashed.push((
                        field_ident.to_string(),
                        field_ident.clone(),
                        quote! { ::amadeo_core::StableHash::stable_hash(#binding, hasher); },
                    ));
                }
                hashed.sort_by(|left, right| left.0.cmp(&right.0));

                let bindings: Vec<TokenStream2> = named
                    .named
                    .iter()
                    .filter_map(|field| field.ident.as_ref())
                    .map(|field_ident| {
                        let binding = format_ident!("field_{}", field_ident);
                        quote! { #field_ident: #binding }
                    })
                    .collect();
                let writes: Vec<TokenStream2> =
                    hashed.into_iter().map(|(_, _, tokens)| tokens).collect();

                arms.push(quote! {
                    Self::#ident { #(#bindings),* } => {
                        hasher.write_str(#name);
                        #(#writes)*
                    }
                });
            }

            Fields::Unnamed(unnamed) => {
                let bindings: Vec<syn::Ident> = (0..unnamed.unnamed.len())
                    .map(|index| format_ident!("field_{}", index))
                    .collect();
                let writes: Vec<TokenStream2> = bindings
                    .iter()
                    .map(|binding| {
                        quote! { ::amadeo_core::StableHash::stable_hash(#binding, hasher); }
                    })
                    .collect();
                arms.push(quote! {
                    Self::#ident( #(#bindings),* ) => {
                        hasher.write_str(#name);
                        #(#writes)*
                    }
                });
            }
        }
    }

    Ok(quote! {
        match self {
            #(#arms)*
        }
    })
}

/// Options declared on the type itself.
struct TypeOptions {
    /// `#[reflect(name = "...")]`, overriding the Rust identifier.
    name: Option<String>,
    /// `#[reflect(version = N)]`, defaulting to 1.
    version: u32,
}

/// Options declared on one field.
#[derive(Default)]
struct FieldOptions {
    /// `#[reflect(skip)]` — omitted from the schema and defaulted on load.
    skip: bool,
    /// `#[reflect(min = ..., max = ...)]`. Both must be present or neither.
    min: Option<f64>,
    /// See `min`.
    max: Option<f64>,
    /// `#[reflect(unit = "...")]`.
    unit: Option<String>,
    /// `#[reflect(sync = "...")]`, as a path into `SyncPolicy`.
    sync: TokenStream2,
    /// `#[reflect(interpolate = "...")]`, as a path into `Interpolation`.
    interpolate: TokenStream2,
}

fn expand(input: DeriveInput) -> syn::Result<TokenStream2> {
    if !input.generics.params.is_empty() {
        return Err(syn::Error::new_spanned(
            &input.generics,
            "#[derive(Reflect)] does not support generic types.\n\
             Components are plain data (ADR 0004), so a generic component is almost always a sign \
             the type wants splitting. If you genuinely need one, implement Reflect by hand.",
        ));
    }

    let options = parse_type_options(&input.attrs)?;
    let ident = &input.ident;
    let canonical = options.name.unwrap_or_else(|| ident.to_string());
    let docs = collect_docs(&input.attrs);
    let version = options.version;

    let (kind, to_value, from_value) = match &input.data {
        Data::Struct(data) => expand_struct(&canonical, &data.fields)?,
        Data::Enum(data) => expand_enum(&canonical, data)?,
        Data::Union(_) => {
            return Err(syn::Error::new_spanned(
                ident,
                "#[derive(Reflect)] does not support unions; they have no safe field-by-field \
                 representation",
            ));
        }
    };

    let dependencies = dependency_types(&input.data)?;

    Ok(quote! {
        impl ::amadeo_reflect::Reflect for #ident {
            // The same string `type_name` returns, as a constant. This is what lets `ComponentId`
            // be computed at compile time instead of allocating and hashing on every lookup (Q16).
            // `STATIC_NAME_HASH` follows from it automatically and must not be set by hand.
            const STATIC_NAME: &'static str = #canonical;

            fn type_name() -> ::std::string::String {
                #canonical.to_string()
            }

            fn type_info() -> ::amadeo_reflect::TypeInfo {
                ::amadeo_reflect::TypeInfo {
                    name: #canonical.to_string(),
                    docs: #docs.to_string(),
                    version: #version,
                    kind: #kind,
                }
            }

            fn to_value(&self) -> ::amadeo_reflect::Value {
                #to_value
            }

            fn from_value(
                value: &::amadeo_reflect::Value,
            ) -> ::std::result::Result<Self, ::amadeo_reflect::ReflectError> {
                #from_value
            }

            // One line per field type, so registering this type also registers everything it
            // names. Without it the schema could report `"type": "Phase"` with no way to look
            // `Phase` up — see `Reflect::register_dependencies` and ADR 0030.
            fn register_dependencies(
                registry: &mut ::amadeo_reflect::TypeRegistry,
            ) -> ::std::result::Result<(), ::amadeo_reflect::RegistryError> {
                #( registry.register::<#dependencies>()?; )*
                ::std::result::Result::Ok(())
            }
        }
    })
}

/// The `range:` initialiser for one field, from its `min`/`max` attributes.
///
/// Shared by struct fields and enum variant fields, which is the point: keeping one copy is what
/// stops the two drifting, as they had until session 8.
fn range_tokens(field: &syn::Field, options: &FieldOptions) -> syn::Result<TokenStream2> {
    match (options.min, options.max) {
        (Some(min), Some(max)) => Ok(
            quote! { ::std::option::Option::Some(::amadeo_reflect::Range { min: #min, max: #max }) },
        ),
        (None, None) => Ok(quote! { ::std::option::Option::None }),
        _ => Err(syn::Error::new_spanned(
            field,
            "#[reflect(...)] needs both `min` and `max`, or neither — a half-open range cannot \
             drive an editor slider",
        )),
    }
}

/// The `unit:` initialiser for one field.
fn unit_tokens(options: &FieldOptions) -> TokenStream2 {
    match &options.unit {
        Some(unit) => quote! { ::std::option::Option::Some(#unit.to_string()) },
        None => quote! { ::std::option::Option::None },
    }
}

/// Every type this one names in its schema, so `register_dependencies` can register them.
///
/// Duplicates are fine and expected — three `f32` fields produce three calls, and the second and
/// third are no-ops. Deduplicating here would mean comparing `syn::Type` values for equality, which
/// is more machinery than the saving is worth.
///
/// A `#[reflect(skip)]` field is omitted, because a skipped field produces no `FieldInfo` and so
/// nothing in the schema names its type.
fn dependency_types(data: &Data) -> syn::Result<Vec<syn::Type>> {
    let mut types = Vec::new();

    let mut collect = |fields: &Fields| -> syn::Result<()> {
        for field in fields {
            if parse_field_options(&field.attrs)?.skip {
                continue;
            }
            types.push(field.ty.clone());
        }
        Ok(())
    };

    match data {
        Data::Struct(data) => collect(&data.fields)?,
        Data::Enum(data) => {
            for variant in &data.variants {
                collect(&variant.fields)?;
            }
        }
        // Rejected earlier with a better message; nothing to collect either way.
        Data::Union(_) => {}
    }

    Ok(types)
}

/// Builds the `TypeKind`, `to_value` body, and `from_value` body for a struct.
fn expand_struct(
    canonical: &str,
    fields: &Fields,
) -> syn::Result<(TokenStream2, TokenStream2, TokenStream2)> {
    match fields {
        Fields::Named(named) => {
            let mut field_infos = Vec::new();
            let mut writes = Vec::new();
            let mut reads = Vec::new();
            let mut known: Vec<String> = Vec::new();

            for field in &named.named {
                let Some(ident) = field.ident.as_ref() else {
                    continue;
                };
                let name = ident.to_string();
                let ty = &field.ty;
                let options = parse_field_options(&field.attrs)?;

                if options.skip {
                    // Nothing restores a skipped field, so it has to be able to make itself.
                    reads.push(quote! { #ident: ::std::default::Default::default() });
                    continue;
                }

                known.push(name.clone());
                let docs = collect_docs(&field.attrs);
                let range = match (options.min, options.max) {
                    (Some(min), Some(max)) => {
                        quote! { ::std::option::Option::Some(::amadeo_reflect::Range { min: #min, max: #max }) }
                    }
                    (None, None) => quote! { ::std::option::Option::None },
                    _ => {
                        return Err(syn::Error::new_spanned(
                            field,
                            "#[reflect(...)] needs both `min` and `max`, or neither — a half-open \
                             range cannot drive an editor slider",
                        ));
                    }
                };
                let unit = match &options.unit {
                    Some(unit) => quote! { ::std::option::Option::Some(#unit.to_string()) },
                    None => quote! { ::std::option::Option::None },
                };
                let sync = &options.sync;
                let interpolate = &options.interpolate;

                field_infos.push(quote! {
                    ::amadeo_reflect::FieldInfo {
                        name: #name.to_string(),
                        type_name: <#ty as ::amadeo_reflect::Reflect>::type_name(),
                        docs: #docs.to_string(),
                        range: #range,
                        unit: #unit,
                        replication: ::amadeo_reflect::Replication {
                            sync: #sync,
                            interpolate: #interpolate,
                        },
                    }
                });

                writes.push(quote! {
                    fields.insert(
                        #name.to_string(),
                        ::amadeo_reflect::Reflect::to_value(&self.#ident),
                    );
                });

                reads.push(quote! {
                    #ident: match fields.get(#name) {
                        ::std::option::Option::Some(found) => {
                            <#ty as ::amadeo_reflect::Reflect>::from_value(found)?
                        }
                        ::std::option::Option::None => {
                            return ::std::result::Result::Err(
                                ::amadeo_reflect::ReflectError::MissingField {
                                    type_name: #canonical.to_string(),
                                    field: #name.to_string(),
                                    required: KNOWN_FIELDS.join(", "),
                                },
                            );
                        }
                    }
                });
            }

            let kind = quote! {
                ::amadeo_reflect::TypeKind::Struct {
                    fields: ::std::vec![ #(#field_infos),* ],
                }
            };

            let to_value = quote! {
                let mut fields = ::std::collections::BTreeMap::new();
                #(#writes)*
                ::amadeo_reflect::Value::Struct(fields)
            };

            // An unknown field is reported rather than ignored: silently dropping it turns a typo
            // into a setting that mysteriously never takes effect, which is far harder to diagnose.
            let from_value = quote! {
                const KNOWN_FIELDS: &[&str] = &[ #(#known),* ];

                let ::amadeo_reflect::Value::Struct(fields) = value else {
                    return ::std::result::Result::Err(
                        ::amadeo_reflect::ReflectError::mismatch(#canonical, "struct", value),
                    );
                };

                for supplied in fields.keys() {
                    if !KNOWN_FIELDS.contains(&supplied.as_str()) {
                        return ::std::result::Result::Err(
                            ::amadeo_reflect::ReflectError::UnknownField {
                                type_name: #canonical.to_string(),
                                field: supplied.clone(),
                                known: KNOWN_FIELDS.join(", "),
                            },
                        );
                    }
                }

                ::std::result::Result::Ok(Self { #(#reads),* })
            };

            Ok((kind, to_value, from_value))
        }

        // A newtype is transparent. `Health(f32)` writes `75.0`, not `{ "0": 75.0 }`, because the
        // wrapper is a Rust detail and the person editing the scene file does not care about it.
        Fields::Unnamed(unnamed) if unnamed.unnamed.len() == 1 => {
            let inner = &unnamed.unnamed[0].ty;
            let kind = quote! {
                <#inner as ::amadeo_reflect::Reflect>::type_info().kind
            };
            let to_value = quote! { ::amadeo_reflect::Reflect::to_value(&self.0) };
            let from_value = quote! {
                ::std::result::Result::Ok(Self(
                    <#inner as ::amadeo_reflect::Reflect>::from_value(value)?,
                ))
            };
            Ok((kind, to_value, from_value))
        }

        Fields::Unnamed(unnamed) => Err(syn::Error::new_spanned(
            unnamed,
            "#[derive(Reflect)] supports newtype structs (one field) but not wider tuple structs.\n\
             Positional fields have no names to put in a scene file, and inventing `0`/`1` keys \
             would make the format worse for the humans who have to read it. Use named fields.",
        )),

        Fields::Unit => {
            let kind = quote! { ::amadeo_reflect::TypeKind::Struct { fields: ::std::vec![] } };
            let to_value = quote! { ::amadeo_reflect::Value::Unit };
            // A unit struct carries nothing, so anything that arrives is acceptable. Being strict
            // here would break marker components round-tripping through an empty struct value.
            let from_value = quote! { ::std::result::Result::Ok(Self) };
            Ok((kind, to_value, from_value))
        }
    }
}

/// Builds the `TypeKind`, `to_value` body, and `from_value` body for an enum.
fn expand_enum(
    canonical: &str,
    data: &syn::DataEnum,
) -> syn::Result<(TokenStream2, TokenStream2, TokenStream2)> {
    let mut variant_infos = Vec::new();
    let mut write_arms = Vec::new();
    let mut read_arms = Vec::new();
    let mut names: Vec<String> = Vec::new();

    for variant in &data.variants {
        let ident = &variant.ident;
        let name = ident.to_string();
        names.push(name.clone());
        let docs = collect_docs(&variant.attrs);

        match &variant.fields {
            Fields::Unit => {
                variant_infos.push(quote! {
                    ::amadeo_reflect::VariantInfo {
                        name: #name.to_string(),
                        docs: #docs.to_string(),
                        fields: ::std::vec![],
                    }
                });
                write_arms.push(quote! {
                    Self::#ident => ::amadeo_reflect::Value::unit_variant(#name)
                });
                read_arms.push(quote! {
                    #name => ::std::result::Result::Ok(Self::#ident)
                });
            }

            Fields::Named(named) => {
                let mut field_infos = Vec::new();
                let mut writes = Vec::new();
                let mut reads = Vec::new();
                let mut bindings = Vec::new();
                let mut known: Vec<String> = Vec::new();

                for field in &named.named {
                    let Some(field_ident) = field.ident.as_ref() else {
                        continue;
                    };
                    let field_name = field_ident.to_string();
                    let ty = &field.ty;
                    let options = parse_field_options(&field.attrs)?;
                    if options.skip {
                        return Err(syn::Error::new_spanned(
                            field,
                            "#[reflect(skip)] is not supported inside an enum variant; a skipped \
                             variant field could not be reconstructed unambiguously",
                        ));
                    }

                    known.push(field_name.clone());
                    let field_docs = collect_docs(&field.attrs);
                    let binding = format_ident!("field_{}", field_ident);
                    // Same metadata a struct's field carries. These used to be hard-coded empty,
                    // which meant a `#[reflect(min = ..., unit = ...)]` inside a variant was
                    // silently dropped — a field lost its range simply by being moved into an enum.
                    // Found in session 8 when ADR 0032 made payload enums usable and `Camera`'s
                    // annotated fields moved into `Projection::Orthographic`.
                    let field_range = range_tokens(field, &options)?;
                    let field_unit = unit_tokens(&options);
                    let field_sync = &options.sync;
                    let field_interpolate = &options.interpolate;

                    field_infos.push(quote! {
                        ::amadeo_reflect::FieldInfo {
                            name: #field_name.to_string(),
                            type_name: <#ty as ::amadeo_reflect::Reflect>::type_name(),
                            docs: #field_docs.to_string(),
                            range: #field_range,
                            unit: #field_unit,
                            replication: ::amadeo_reflect::Replication {
                                sync: #field_sync,
                                interpolate: #field_interpolate,
                            },
                        }
                    });

                    writes.push(quote! {
                        payload.insert(
                            #field_name.to_string(),
                            ::amadeo_reflect::Reflect::to_value(#binding),
                        );
                    });

                    reads.push(quote! {
                        #field_ident: match payload.get(#field_name) {
                            ::std::option::Option::Some(found) => {
                                <#ty as ::amadeo_reflect::Reflect>::from_value(found)?
                            }
                            ::std::option::Option::None => {
                                return ::std::result::Result::Err(
                                    ::amadeo_reflect::ReflectError::MissingField {
                                        type_name: #canonical.to_string(),
                                        field: #field_name.to_string(),
                                        required: [#(#known),*].join(", "),
                                    },
                                );
                            }
                        }
                    });

                    bindings.push(quote! { #field_ident: #binding });
                }

                variant_infos.push(quote! {
                    ::amadeo_reflect::VariantInfo {
                        name: #name.to_string(),
                        docs: #docs.to_string(),
                        fields: ::std::vec![ #(#field_infos),* ],
                    }
                });

                write_arms.push(quote! {
                    Self::#ident { #(#bindings),* } => {
                        let mut payload = ::std::collections::BTreeMap::new();
                        #(#writes)*
                        ::amadeo_reflect::Value::Enum(::amadeo_reflect::EnumValue {
                            variant: #name.to_string(),
                            payload: ::std::boxed::Box::new(
                                ::amadeo_reflect::Value::Struct(payload),
                            ),
                        })
                    }
                });

                read_arms.push(quote! {
                    #name => {
                        let ::amadeo_reflect::Value::Struct(payload) = found.payload.as_ref() else {
                            return ::std::result::Result::Err(
                                ::amadeo_reflect::ReflectError::mismatch(
                                    #canonical,
                                    "struct payload",
                                    found.payload.as_ref(),
                                ),
                            );
                        };
                        ::std::result::Result::Ok(Self::#ident { #(#reads),* })
                    }
                });
            }

            Fields::Unnamed(unnamed) => {
                return Err(syn::Error::new_spanned(
                    unnamed,
                    "#[derive(Reflect)] does not support tuple variants.\n\
                     Positional fields have no names to put in a scene file. Use a named-field \
                     variant: `Chasing { target: Entity }` rather than `Chasing(Entity)`.",
                ));
            }
        }
    }

    let kind = quote! {
        ::amadeo_reflect::TypeKind::Enum {
            variants: ::std::vec![ #(#variant_infos),* ],
        }
    };

    let to_value = quote! {
        match self {
            #(#write_arms),*
        }
    };

    let from_value = quote! {
        const KNOWN_VARIANTS: &[&str] = &[ #(#names),* ];

        let ::amadeo_reflect::Value::Enum(found) = value else {
            return ::std::result::Result::Err(
                ::amadeo_reflect::ReflectError::mismatch(#canonical, "enum", value),
            );
        };

        match found.variant.as_str() {
            #(#read_arms,)*
            other => ::std::result::Result::Err(
                ::amadeo_reflect::ReflectError::UnknownVariant {
                    type_name: #canonical.to_string(),
                    variant: other.to_string(),
                    known: KNOWN_VARIANTS.join(", "),
                },
            ),
        }
    };

    Ok((kind, to_value, from_value))
}

/// Reads `#[reflect(...)]` options declared on the type.
fn parse_type_options(attrs: &[Attribute]) -> syn::Result<TypeOptions> {
    let mut options = TypeOptions {
        name: None,
        version: 1,
    };

    for attr in attrs {
        if !attr.path().is_ident("reflect") {
            continue;
        }
        attr.parse_nested_meta(|meta| {
            if meta.path.is_ident("name") {
                let literal: LitStr = meta.value()?.parse()?;
                options.name = Some(literal.value());
                return Ok(());
            }
            if meta.path.is_ident("version") {
                let literal: syn::LitInt = meta.value()?.parse()?;
                options.version = literal.base10_parse()?;
                return Ok(());
            }
            Err(meta.error(
                "unknown option; a type accepts #[reflect(name = \"...\")] and \
                 #[reflect(version = N)]",
            ))
        })?;
    }

    Ok(options)
}

/// Reads `#[reflect(...)]` options declared on a field.
fn parse_field_options(attrs: &[Attribute]) -> syn::Result<FieldOptions> {
    let mut options = FieldOptions {
        sync: quote! { ::amadeo_reflect::SyncPolicy::Never },
        interpolate: quote! { ::amadeo_reflect::Interpolation::None },
        ..FieldOptions::default()
    };

    for attr in attrs {
        if !attr.path().is_ident("reflect") {
            continue;
        }
        attr.parse_nested_meta(|meta| {
            if meta.path.is_ident("skip") {
                options.skip = true;
                return Ok(());
            }
            if meta.path.is_ident("min") {
                options.min = Some(meta.value()?.parse::<syn::LitFloat>()?.base10_parse()?);
                return Ok(());
            }
            if meta.path.is_ident("max") {
                options.max = Some(meta.value()?.parse::<syn::LitFloat>()?.base10_parse()?);
                return Ok(());
            }
            if meta.path.is_ident("unit") {
                options.unit = Some(meta.value()?.parse::<LitStr>()?.value());
                return Ok(());
            }
            if meta.path.is_ident("sync") {
                let literal: LitStr = meta.value()?.parse()?;
                options.sync = match literal.value().as_str() {
                    "never" => quote! { ::amadeo_reflect::SyncPolicy::Never },
                    "on_change" => quote! { ::amadeo_reflect::SyncPolicy::OnChange },
                    "always" => quote! { ::amadeo_reflect::SyncPolicy::Always },
                    other => {
                        return Err(syn::Error::new_spanned(
                            &literal,
                            format!(
                                "`{other}` is not a sync policy; expected \"never\", \
                                 \"on_change\", or \"always\""
                            ),
                        ));
                    }
                };
                return Ok(());
            }
            if meta.path.is_ident("interpolate") {
                let literal: LitStr = meta.value()?.parse()?;
                options.interpolate = match literal.value().as_str() {
                    "none" => quote! { ::amadeo_reflect::Interpolation::None },
                    "linear" => quote! { ::amadeo_reflect::Interpolation::Linear },
                    "angular" => quote! { ::amadeo_reflect::Interpolation::Angular },
                    other => {
                        return Err(syn::Error::new_spanned(
                            &literal,
                            format!(
                                "`{other}` is not an interpolation mode; expected \"none\", \
                                 \"linear\", or \"angular\""
                            ),
                        ));
                    }
                };
                return Ok(());
            }
            Err(meta.error(
                "unknown option; a field accepts skip, min, max, unit, sync, and interpolate",
            ))
        })?;
    }

    Ok(options)
}

/// Joins a type's or field's doc comment into one string.
///
/// Doc comments arrive as `#[doc = " line"]` attributes with a leading space from the `///`. That
/// space is stripped so the stored text reads naturally when an agent or an inspector prints it.
fn collect_docs(attrs: &[Attribute]) -> String {
    let mut lines: Vec<String> = Vec::new();

    for attr in attrs {
        if !attr.path().is_ident("doc") {
            continue;
        }
        let Meta::NameValue(name_value) = &attr.meta else {
            continue;
        };
        let Expr::Lit(ExprLit {
            lit: Lit::Str(text),
            ..
        }) = &name_value.value
        else {
            continue;
        };
        lines.push(
            text.value()
                .strip_prefix(' ')
                .unwrap_or(&text.value())
                .to_string(),
        );
    }

    lines.join("\n").trim().to_string()
}
