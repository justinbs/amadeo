//! Gate 4, re-tested against the game that failed it.
//!
//! > `amadeo describe` output is sufficient to write a new component and system without reading
//! > engine source. **Tested by actually doing it.**
//! >
//! > — `docs/05-roadmap.md`, M1 exit gate 4
//!
//! The experiment is written up in `docs/09-gate-4-describe-is-not-enough.md`: the claim is false as
//! stated, `describe` is a schema and not a manual, and ADR 0030 decided what to do about it. Three
//! of the five gaps it found were structural and are fixed; two are API knowledge that deliberately
//! lives in `docs/07-working-with-the-code.md` instead.
//!
//! This file pins the three that were fixed, using the Vault — because the Vault is what found them,
//! and `Run` is the exact resource that was invisible.

use amadeo_agent::{Json, describe, describe_example};
use vault::build_simulation;

/// The whole schema document, as an agent receives it.
fn schema() -> std::collections::BTreeMap<String, Json> {
    let app = build_simulation().expect("the game builds");
    let document = describe(&app.world, app.components()).expect("the schema holds together");
    match document {
        Json::Object(members) => members,
        other => panic!("a schema document is an object, found {other:?}"),
    }
}

/// One section of it, by key.
fn section(document: &std::collections::BTreeMap<String, Json>, key: &str) -> Vec<String> {
    match document.get(key) {
        Some(Json::Object(members)) => members.keys().cloned().collect(),
        other => panic!("`{key}` should be an object, found {other:?}"),
    }
}

#[test]
fn the_resource_holding_the_win_condition_is_visible() {
    // Gap 5, and the sharpest one. `Run` holds the score and whether the game has been won or lost —
    // it is the entire outcome of the game — and before ADR 0030 it appeared nowhere in `describe`.
    // An agent reading the schema could not have known it existed.
    let document = schema();
    let resources = section(&document, "resources");

    assert!(
        resources.iter().any(|name| name == "Run"),
        "`Run` must be in the schema; got {resources:?}"
    );
}

#[test]
fn a_type_named_by_a_field_can_be_looked_up() {
    // `Run.phase` is a `Phase`, and `Phase` is neither a component nor a resource. The schema used
    // to name it and have nowhere to resolve it — so nothing could say the legal values are
    // `Playing`, `Won` and `Lost`, which is what an editor needs to draw a dropdown.
    let document = schema();
    let types = section(&document, "types");

    assert!(
        types.iter().any(|name| name == "Phase"),
        "`Phase` must be resolvable; got {types:?}"
    );

    let Some(Json::Object(all)) = document.get("types") else {
        unreachable!("checked above")
    };
    let rendered = all["Phase"].to_pretty();
    for variant in ["Playing", "Won", "Lost"] {
        assert!(rendered.contains(variant), "{variant} missing: {rendered}");
    }
}

#[test]
fn an_example_of_the_run_resource_spells_the_enum_correctly() {
    // The thing `describe` alone could not teach: that `phase` takes a **bare word** and not a
    // quoted string. Nothing in the schema says so — bare-vs-quoted is scene-format grammar, not
    // type information — and getting it wrong produces a file that parses and then fails to load.
    let app = build_simulation().expect("the game builds");
    let mut types = app.components().types().clone();
    app.world
        .register_resource_schemas(&mut types)
        .expect("the schema holds together");

    let info = types.get("Run").expect("Run is a resource of this game");
    let example = describe_example(info, &types).expect("an example exists");

    let Json::Object(members) = &example else {
        panic!("an example is an object")
    };
    let Some(Json::String(block)) = members.get("scene") else {
        panic!("Run is scene-expressible, so a scene form must be there")
    };

    assert!(
        block.contains("phase Playing"),
        "the example must show the bare-word spelling, got:\n{block}"
    );
    assert!(
        !block.contains("\"Playing\""),
        "a quoted variant would parse as a string and then fail to load, got:\n{block}"
    );
}

#[test]
fn the_schema_says_where_the_api_knowledge_lives() {
    // The two gaps ADR 0030 deliberately did *not* move into the protocol — how to declare a
    // component and how to write a system. `describe` points at them rather than leaving a reader to
    // conclude that their absence means impossible.
    let document = schema();

    assert_eq!(
        document.get("manual"),
        Some(&Json::string("docs/07-working-with-the-code.md")),
        "the schema must name where the manual is"
    );
}
