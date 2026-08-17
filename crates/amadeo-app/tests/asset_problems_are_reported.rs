//! An asset that names a component it cannot build says so — **Q32**'s actual cost.
//!
//! # The failure this closes
//!
//! An asset file that spells out a component the engine then cannot build used to be skipped in
//! silence.
//!
//! **Two sentences here were wrong and ADR 0075 corrected them.** They said reflection requires all
//! fields to be present, that this is what catches a typo'd field name, and that it is what makes a
//! prefab that lost a component refuse to load. A typo is caught by the **unknown**-field check,
//! which runs before any field is read; the prefab guarantee is `ComponentRegistry`'s and is about a
//! missing *component*. So a field may now declare a default and a file may omit it — which is why
//! the first test below misspells a field rather than deleting one.
//!
//! The churn was never the problem. **The reporting was.** An unparseable asset was skipped in
//! silence, so whatever depended on it failed later somewhere unrelated — when `Environment` gained
//! a `sky` field, every `.environment` file stopped parsing and the symptom was a test complaining
//! that a *service* had not been installed. Nothing in that message mentioned a field, a file, or a
//! schema, and it cost a debugging cycle to trace back.
//!
//! # Why this is not simply "make it an error"
//!
//! ADR 0021's posture holds: a game with one broken asset should start and be visibly wrong rather
//! than refuse to run. So the load still succeeds, the cache still falls back, and the *complaint*
//! is what is new.

use amadeo_app::App;
use amadeo_render::{Camera, Environment, Material, Mesh};
use amadeo_transform::Transform;
use std::path::{Path, PathBuf};

/// Writes an asset directory holding one material file with the given body.
fn directory_with(name: &str, material: &str) -> PathBuf {
    let directory = PathBuf::from(env!("CARGO_TARGET_TMPDIR")).join(name);
    let _ = std::fs::remove_dir_all(&directory);
    std::fs::create_dir_all(&directory).expect("a writable directory");

    std::fs::write(directory.join("paint.material"), material).expect("writes");
    std::fs::write(
        directory.join("paint.material.ama-meta"),
        b"id = \"paint\"\n",
    )
    .expect("writes");
    // The marker that makes this an asset root (ADR 0022).
    std::fs::write(directory.join("amadeo-assets.toml"), b"").expect("writes");
    directory
}

/// An app with one entity whose `Mesh` names the material, so something asks for it.
fn app_wanting_the_material(directory: &Path) -> App {
    let mut app = App::new();
    app.register_component::<Transform>().expect("fresh");
    app.register_component::<Mesh>().expect("fresh");
    app.register_component::<Camera>().expect("fresh");
    app.register_component::<Material>().expect("fresh");
    app.register_component::<Environment>().expect("fresh");
    app.scan_assets(directory.to_str().expect("utf-8"))
        .expect("scans");

    let entity = app.world.spawn();
    app.world.insert(entity, Transform::at(0.0, 0.0));
    app.world.insert(entity, Mesh::new("cube", "paint"));
    app
}

/// A material file that is complete and valid.
const GOOD: &str = "scene paint\nversion 1\n\nentity material \"Paint\"\n  Material\n    \
                    base_colour 1.0 1.0 1.0 1.0\n    base_colour_texture \"\"\n    \
                    emissive 0.0 0.0 0.0\n    metallic 0.0\n    \
                    metallic_roughness_texture \"\"\n    normal_strength 1.0\n    \
                    normal_texture \"\"\n    roughness 0.5\n";

#[test]
fn a_material_with_a_misspelled_field_is_reported_rather_than_skipped_in_silence() {
    // This used to delete `roughness` outright, on the grounds that omitting a field is exactly what
    // adding one does to every file that predates it. **ADR 0075 made that case legal** — every
    // `Material` field declares a default now, so a file may leave any of them out.
    //
    // The mechanism under test is unaffected and still worth testing, so the vehicle changed to the
    // case that is still a fault and always was one: a typo. `roughnes` is not a field, so the
    // `Material` is present and cannot be built. That is a better vehicle anyway — a misspelling is
    // what actually happens to a hand-authored file, where a deliberately omitted field is not.
    let broken = GOOD.replace("    roughness 0.5\n", "    roughnes 0.5\n");
    let directory = directory_with("asset_problem_misspelled_field", &broken);

    let mut app = app_wanting_the_material(&directory);
    app.load_materials();

    assert!(
        app.has_asset_problems(),
        "a material missing a field must be complained about, not skipped"
    );

    let problems: Vec<(String, String)> = app
        .asset_problems()
        .map(|(id, reason)| (id.to_string(), reason.to_string()))
        .collect();
    assert_eq!(problems.len(), 1, "one broken asset, one complaint");

    let (id, reason) = &problems[0];
    assert_eq!(id, "paint");
    // The three things that make it actionable, and whose absence is what cost a debugging cycle:
    // which file, which type, and which field.
    assert!(
        reason.contains("paint"),
        "the message must name the asset, got: {reason}"
    );
    assert!(
        reason.contains("Material"),
        "and the component it holds, got: {reason}"
    );
    assert!(
        reason.contains("roughnes"),
        "and the field that is wrong, got: {reason}"
    );
}

#[test]
fn a_material_that_omits_a_field_loads_at_its_default() {
    // ADR 0075, at the level a game meets it: the file a session-14 author wrote is still valid after
    // two fields were added, and the reverse is what matters here — a file may name only what it
    // cares about. Q32 sat at P2 for six sessions because this was not possible.
    let terse = "scene paint\nversion 1\n\nentity material \"Paint\"\n  Material\n    \
                 base_colour 0.2 0.4 0.8 1.0\n";
    let directory = directory_with("asset_problem_terse_material", terse);

    let mut app = app_wanting_the_material(&directory);
    app.load_materials();

    assert!(
        !app.has_asset_problems(),
        "omitting a defaulted field is not a fault, got: {:?}",
        app.asset_problems()
            .map(|(id, reason)| format!("{id}: {reason}"))
            .collect::<Vec<_>>()
    );
}

#[test]
fn a_valid_material_produces_no_complaint() {
    // The control. A report that fires on healthy assets is one nobody reads.
    let directory = directory_with("asset_problem_none", GOOD);

    let mut app = app_wanting_the_material(&directory);
    app.load_materials();

    let problems: Vec<&str> = app.asset_problems().map(|(id, _)| id).collect();
    assert!(
        problems.is_empty(),
        "a valid material must produce no complaint, got {problems:?}"
    );
}

#[test]
fn an_asset_that_simply_is_not_a_material_is_not_a_problem() {
    // **The distinction that makes this report worth having.** A mesh asset is read as a `BoxMesh`
    // and then as a `PlaneMesh`, and one of those attempts always finds nothing — that is the
    // mechanism ADR 0035 uses, not a fault. Only a document that *has* the component and cannot
    // build it is worth a word.
    let not_a_material = "scene paint\nversion 1\n\nentity thing \"Thing\"\n  Transform\n    \
                          rotation 0.0 0.0 0.0\n    scale 1.0 1.0 1.0\n    \
                          translation 0.0 0.0 0.0\n";
    let directory = directory_with("asset_problem_wrong_kind", not_a_material);

    let mut app = app_wanting_the_material(&directory);
    app.load_materials();

    assert!(
        !app.has_asset_problems(),
        "an asset holding no `Material` at all is ordinary, not a problem: {:?}",
        app.asset_problems().collect::<Vec<_>>()
    );
}
