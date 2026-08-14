//! ADR 0069: the same file, read leniently, survives the game being patched.
//!
//! # How two builds fit in one process
//!
//! Every test here pairs two Rust types under one canonical name with `#[reflect(name = "…")]`.
//! ADR 0017 makes the canonical name the identity, so as far as the file, the registry and the
//! state hash are concerned these *are* one component before and after a patch — which is the only
//! way to test this without shipping two binaries.
//!
//! Its companion, `a_patch_invalidates_every_save.rs`, pins what the strict path does with the same
//! situation. Read them together: that one is the problem, this one is the answer.

use amadeo_core::StableHash;
use amadeo_ecs::{Component, ComponentRegistry, World};
use amadeo_reflect::Reflect;
use amadeo_snapshot::{Redirects, Snapshot, capture, restore_save};

// --- The shipped build -------------------------------------------------------------------------

/// A component as it shipped.
#[derive(Debug, Clone, PartialEq, StableHash, Reflect)]
#[reflect(name = "Lantern")]
struct LanternV1 {
    /// How bright.
    intensity: f32,
}
impl Component for LanternV1 {}

/// A second component, so that "one component changed" does not mean "everything changed".
#[derive(Debug, Clone, PartialEq, StableHash, Reflect)]
#[reflect(name = "Marker")]
struct Marker {
    /// Which one.
    id: u32,
}
impl Component for Marker {}

// --- The patched build -------------------------------------------------------------------------

/// The same component after a patch added a field.
#[derive(Debug, Clone, PartialEq, StableHash, Reflect)]
#[reflect(name = "Lantern")]
struct LanternWithBattery {
    /// How bright.
    intensity: f32,
    /// Added by the patch, and the thing a save cannot know about.
    battery: f32,
}
impl Component for LanternWithBattery {}

/// The same component after a patch removed a field.
#[derive(Debug, Clone, PartialEq, StableHash, Reflect)]
#[reflect(name = "Lantern")]
struct LanternWithoutIntensity {
    /// The only field left.
    battery: f32,
}
impl Component for LanternWithoutIntensity {}

/// The same component after a patch renamed its field.
#[derive(Debug, Clone, PartialEq, StableHash, Reflect)]
#[reflect(name = "Lantern")]
struct LanternRenamedField {
    /// `intensity`, renamed.
    brightness: f32,
}
impl Component for LanternRenamedField {}

/// The same component under a new name.
#[derive(Debug, Clone, PartialEq, StableHash, Reflect)]
#[reflect(name = "Torch")]
struct Torch {
    /// How bright.
    intensity: f32,
}
impl Component for Torch {}

/// What a lamp is doing. An enum, so it has no default that is not a guess.
#[derive(Debug, Clone, PartialEq, StableHash, Reflect)]
enum LampMode {
    /// Steady.
    Steady,
    /// Flickering.
    Flicker,
}

/// The same component after a patch added an enum field.
#[derive(Debug, Clone, PartialEq, StableHash, Reflect)]
#[reflect(name = "Lantern")]
struct LanternWithMode {
    /// How bright.
    intensity: f32,
    /// Added by the patch, and undefaultable on purpose.
    mode: LampMode,
}
impl Component for LanternWithMode {}

// --- Scaffolding -------------------------------------------------------------------------------

/// A save written by the shipped build: one lantern, one marker.
fn shipped_save() -> Snapshot {
    let mut registry = ComponentRegistry::new();
    registry.register::<LanternV1>().expect("registers");
    registry.register::<Marker>().expect("registers");

    let mut world = World::new();
    let lamp = world.spawn();
    world.insert(lamp, LanternV1 { intensity: 22.0 });
    let other = world.spawn();
    world.insert(other, Marker { id: 7 });

    capture(&world, &registry)
}

/// A registry holding one patched component plus the unchanged `Marker`.
fn build_with<T: Component>() -> ComponentRegistry {
    let mut registry = ComponentRegistry::new();
    registry.register::<T>().expect("registers");
    registry.register::<Marker>().expect("registers");
    registry
}

// --- The good case, which is most loads ---------------------------------------------------------

#[test]
fn the_same_build_still_gets_the_full_exact_check() {
    // The consequence ADR 0069 names explicitly: a conditional check that silently stops applying
    // is worse than no check. This is the test that keeps the strict path alive.
    let save = shipped_save();
    let mut registry = ComponentRegistry::new();
    registry.register::<LanternV1>().expect("registers");
    registry.register::<Marker>().expect("registers");

    let mut world = World::new();
    let report = restore_save(&mut world, &registry, &save, &Redirects::new()).expect("restores");

    assert!(report.exact, "the layout is unchanged");
    assert!(
        report.state_hash_checked,
        "so the recorded hash still means something and must be enforced"
    );
    assert!(report.is_clean(), "and nothing had to be papered over");
    assert_eq!(world.state_hash(), save.state_hash);
}

#[test]
fn a_corrupt_file_is_still_refused_when_the_layout_matches() {
    // Leniency must not become a way for a genuinely broken file to load quietly. With the layout
    // unchanged, there is no version gap to blame, so this behaves exactly like `restore`.
    let mut save = shipped_save();
    save.state_hash ^= 0xdead_beef;

    let mut registry = ComponentRegistry::new();
    registry.register::<LanternV1>().expect("registers");
    registry.register::<Marker>().expect("registers");

    let error = restore_save(&mut World::new(), &registry, &save, &Redirects::new())
        .expect_err("the hash does not match and nothing explains it away");
    assert!(error.to_string().contains("does not match"), "{error}");
}

// --- Adding a field, which is the case Q37 was raised for ----------------------------------------

#[test]
fn a_component_that_gained_a_field_loads_and_says_what_it_filled_in() {
    let save = shipped_save();
    let registry = build_with::<LanternWithBattery>();

    let mut world = World::new();
    let report = restore_save(&mut world, &registry, &save, &Redirects::new())
        .expect("a save older than the patch still loads");

    assert!(!report.exact, "the layout changed");
    assert!(
        !report.state_hash_checked,
        "the recorded hash describes a layout that no longer exists"
    );

    // The field that was there came back; the one that was not is at its default.
    let lamp = world.entities()[0];
    assert_eq!(
        world.get::<LanternWithBattery>(lamp),
        Some(&LanternWithBattery {
            intensity: 22.0,
            battery: 0.0
        })
    );

    // And it is reported, because a defaulted field is a silent gameplay change.
    assert_eq!(report.defaulted.len(), 1, "{:?}", report.defaulted);
    assert_eq!(report.defaulted[0].owner, "Lantern");
    assert_eq!(report.defaulted[0].field, "battery");
    assert!(report.dropped.is_empty(), "{:?}", report.dropped);

    let summary = report.lines().join("\n");
    assert!(summary.contains("battery"), "{summary}");
    assert!(
        summary.contains("resume differently"),
        "the summary should say why it matters, got: {summary}"
    );
}

#[test]
fn the_component_that_did_not_change_is_untouched() {
    // A patch to one component must not disturb the rest of the save.
    let save = shipped_save();
    let registry = build_with::<LanternWithBattery>();

    let mut world = World::new();
    restore_save(&mut world, &registry, &save, &Redirects::new()).expect("loads");

    let other = world.entities()[1];
    assert_eq!(world.get::<Marker>(other), Some(&Marker { id: 7 }));
}

// --- Removing things -----------------------------------------------------------------------------

#[test]
fn a_field_the_component_no_longer_has_is_dropped_and_named() {
    let save = shipped_save();
    let registry = build_with::<LanternWithoutIntensity>();

    let mut world = World::new();
    let report = restore_save(&mut world, &registry, &save, &Redirects::new()).expect("loads");

    let dropped: Vec<_> = report
        .dropped
        .iter()
        .filter(|d| d.field.as_deref() == Some("intensity"))
        .collect();
    assert_eq!(dropped.len(), 1, "{:?}", report.dropped);
    assert!(
        dropped[0].reason.contains("no longer has that field"),
        "{:?}",
        dropped[0]
    );

    // `battery` is new, so it is defaulted, and the component still builds.
    let lamp = world.entities()[0];
    assert_eq!(
        world.get::<LanternWithoutIntensity>(lamp),
        Some(&LanternWithoutIntensity { battery: 0.0 })
    );
}

#[test]
fn a_component_this_build_no_longer_has_is_dropped_rather_than_refused() {
    // `restore` errors here. A save must not be destroyed by a component being deleted.
    let save = shipped_save();
    let mut registry = ComponentRegistry::new();
    registry.register::<Marker>().expect("registers");

    let mut world = World::new();
    let report =
        restore_save(&mut world, &registry, &save, &Redirects::new()).expect("still loads");

    assert!(
        report.dropped.iter().any(|d| d.owner == "Lantern"),
        "{:?}",
        report.dropped
    );
    // The rest of the world is intact, which is the whole point of not refusing.
    let other = world.entities()[1];
    assert_eq!(world.get::<Marker>(other), Some(&Marker { id: 7 }));
}

// --- Renames, which defaults alone cannot fix ------------------------------------------------------

#[test]
fn a_renamed_component_is_recovered_by_a_redirect_file() {
    let save = shipped_save();
    let mut registry = ComponentRegistry::new();
    registry.register::<Torch>().expect("registers");
    registry.register::<Marker>().expect("registers");

    let redirects = Redirects::parse("amadeo-redirects 1\ncomponent Lantern Torch\n").expect("ok");

    let mut world = World::new();
    let report = restore_save(&mut world, &registry, &save, &redirects).expect("loads");

    let lamp = world.entities()[0];
    assert_eq!(
        world.get::<Torch>(lamp),
        Some(&Torch { intensity: 22.0 }),
        "the value survived the rename"
    );
    assert_eq!(report.redirected.len(), 1);
    assert_eq!(report.redirected[0].from, "Lantern");
    assert_eq!(report.redirected[0].to, "Torch");
    assert!(report.is_clean(), "{:?}", report);
}

#[test]
fn a_renamed_field_is_recovered_too() {
    let save = shipped_save();
    let registry = build_with::<LanternRenamedField>();

    let redirects =
        Redirects::parse("amadeo-redirects 1\nfield Lantern intensity brightness\n").expect("ok");

    let mut world = World::new();
    let report = restore_save(&mut world, &registry, &save, &redirects).expect("loads");

    let lamp = world.entities()[0];
    assert_eq!(
        world.get::<LanternRenamedField>(lamp),
        Some(&LanternRenamedField { brightness: 22.0 }),
        "the value moved to the new field rather than being defaulted"
    );
    assert!(report.is_clean(), "{:?}", report);
}

#[test]
fn without_the_redirect_the_same_rename_loses_the_value() {
    // The control case, and the reason redirects exist at all: defaults alone turn a rename into
    // silent data loss, which is exactly what this must not do quietly.
    let save = shipped_save();
    let registry = build_with::<LanternRenamedField>();

    let mut world = World::new();
    let report = restore_save(&mut world, &registry, &save, &Redirects::new()).expect("loads");

    let lamp = world.entities()[0];
    assert_eq!(
        world.get::<LanternRenamedField>(lamp),
        Some(&LanternRenamedField { brightness: 0.0 }),
        "the old value had nowhere to go"
    );
    // Not silent, though. Both halves of what happened are in the report.
    assert!(
        report
            .dropped
            .iter()
            .any(|d| d.field.as_deref() == Some("intensity")),
        "{:?}",
        report.dropped
    );
    assert!(
        report.defaulted.iter().any(|d| d.field == "brightness"),
        "{:?}",
        report.defaulted
    );
}

// --- The fingerprint has to see through a field's type ------------------------------------------------

/// A nested struct as it shipped.
#[derive(Debug, Clone, PartialEq, StableHash, Reflect)]
#[reflect(name = "Glow")]
struct GlowV1 {
    /// How far the light reaches.
    radius: f32,
}

/// The same nested struct after a patch added a field.
#[derive(Debug, Clone, PartialEq, StableHash, Reflect)]
#[reflect(name = "Glow")]
struct GlowV2 {
    /// How far the light reaches.
    radius: f32,
    /// Added by the patch, one level down from any component.
    falloff: f32,
}

/// A component whose own field list never changes.
#[derive(Debug, Clone, PartialEq, StableHash, Reflect)]
#[reflect(name = "Halo")]
struct HaloV1 {
    /// The nested value that changed underneath it.
    glow: GlowV1,
}
impl Component for HaloV1 {}

/// The same component, unchanged at the top level, over the patched nested struct.
#[derive(Debug, Clone, PartialEq, StableHash, Reflect)]
#[reflect(name = "Halo")]
struct HaloV2 {
    /// The nested value that changed underneath it.
    glow: GlowV2,
}
impl Component for HaloV2 {}

#[test]
fn a_change_one_level_down_still_makes_the_load_lenient() {
    // `Halo` has one field called `glow` before and after, so a fingerprint that looked only at the
    // top level would call this layout unchanged — and then enforce a state hash computed over a
    // struct that has since grown a field, rejecting a perfectly good save. This is the failure
    // mode `layout.rs` recurses to avoid, and nothing else here would notice if it stopped.
    let mut old = ComponentRegistry::new();
    old.register::<HaloV1>().expect("registers");
    let mut world = World::new();
    let entity = world.spawn();
    world.insert(
        entity,
        HaloV1 {
            glow: GlowV1 { radius: 3.0 },
        },
    );
    let save = capture(&world, &old);

    let mut new = ComponentRegistry::new();
    new.register::<HaloV2>().expect("registers");

    let mut world = World::new();
    let report = restore_save(&mut world, &new, &save, &Redirects::new()).expect("loads");

    assert!(
        !report.exact,
        "a nested struct gaining a field is a layout change, even though `Halo` still has \
         exactly one field called `glow`"
    );
    assert!(!report.state_hash_checked);
}

// --- The case the engine refuses to guess at --------------------------------------------------------

#[test]
fn a_new_enum_field_is_reported_by_name_rather_than_guessed() {
    // `default_value_for` refuses an enum because the first variant is a guess with gameplay
    // meaning. The component cannot be rebuilt, and the report has to be good enough to act on.
    let save = shipped_save();
    let registry = build_with::<LanternWithMode>();

    let mut world = World::new();
    let report = restore_save(&mut world, &registry, &save, &Redirects::new()).expect("loads");

    let named_mode: Vec<_> = report
        .dropped
        .iter()
        .filter(|d| d.field.as_deref() == Some("mode"))
        .collect();
    assert_eq!(named_mode.len(), 1, "{:?}", report.dropped);
    assert!(
        named_mode[0].reason.contains("no default"),
        "{:?}",
        named_mode[0]
    );

    // The rest of the save is unharmed, which is what makes this recoverable rather than fatal.
    let other = world.entities()[1];
    assert_eq!(world.get::<Marker>(other), Some(&Marker { id: 7 }));
}
