//! Byte-stable round-tripping, which is invariant I2 and an M1 exit-gate item.
//!
//! `docs/05-roadmap.md` § M1 gate 3: "Scene round-trip test in CI: parse → serialize →
//! byte-identical." This is that test, plus the error-quality cases that ADR 0014 justified choosing
//! a custom format for in the first place.

use amadeo_scene::{ParseErrorKind, parse, to_text};

/// The worked example from ADR 0014, in canonical form.
///
/// The ADR shows this exact text, so `the_adr_example_is_canonical` below makes the ADR executable
/// documentation rather than a thing that drifts.
const CANONICAL: &str = "\
scene corridor_a
version 1

entity a1 \"Corridor\"
  Transform
    position 0.0 0.0
    rotation 0.0
    scale 1.0 1.0

  entity a2 \"CeilingLight\"
    Flicker
      pattern Irregular
      speed 12.5
    PointLight
      color 1.0 0.85 0.6
      intensity 3.2
      range 8.0

  entity a3 \"Door\" from prefabs/door_metal
    override Door
      key_id \"rusted_key\"
      locked true
    override Transform
      position 4.0 0.0

  entity a4 \"Wanderer\"
    Enemy
      sight_range 3.5
      state Patrol
      waypoints
        - 0.0 0.0
        - 4.0 0.0
        - 4.0 3.0
";

#[test]
fn the_adr_example_is_canonical() {
    let document = parse(CANONICAL).expect("the ADR's example parses");
    assert_eq!(
        to_text(&document),
        CANONICAL,
        "\nADR 0014's worked example is no longer what the formatter produces. \
         Either the formatter changed or the ADR did; they have to agree."
    );
}

#[test]
fn formatting_is_idempotent_from_messy_input() {
    // The real I2 property: a hand-written file, formatted once, does not move again. Fields out of
    // order, extra blank lines, trailing whitespace, and a comment.
    //
    // Note what is NOT in here: wrong indentation. Indentation *is* the structure in this format,
    // so a mis-indented line is a parse error rather than something the formatter quietly repairs.
    let messy = "\
scene corridor_a
version 1


entity a1 \"Corridor\"     # the room itself
  Transform
    scale 1.0 1.0
    position 0.0 0.0
    rotation 0.0
";

    let once = to_text(&parse(messy).expect("messy input still parses"));
    let twice = to_text(&parse(&once).expect("formatted output parses"));
    assert_eq!(
        once, twice,
        "formatting must reach a fixed point in one pass"
    );

    // And the fields came out sorted, not in the order they were written.
    assert!(
        once.contains("    position 0.0 0.0\n    rotation 0.0\n    scale 1.0 1.0\n"),
        "fields should be sorted; got:\n{once}"
    );
}

#[test]
fn components_are_sorted_but_children_keep_their_order() {
    // Components are a set, so sorting them is safe and makes diffs canonical. Children are a
    // sequence — sibling order is meaningful — so sorting them would destroy information.
    let source = "\
scene ordering
version 1

entity root \"Root\"
  Zebra
    value 1
  Alpha
    value 2

  entity z \"Zulu\"

  entity a \"Alpha\"
";

    let document = parse(source).expect("parses");
    let text = to_text(&document);

    let alpha = text.find("Alpha\n").expect("Alpha component present");
    let zebra = text.find("Zebra").expect("Zebra component present");
    assert!(alpha < zebra, "components should be sorted:\n{text}");

    let children: Vec<&str> = document.entities[0]
        .children
        .iter()
        .map(|child| child.id.as_str())
        .collect();
    assert_eq!(
        children,
        vec!["z", "a"],
        "children must keep declaration order"
    );
}

#[test]
fn a_list_of_vectors_keeps_its_grouping() {
    // The subtle one. Every element inlines happily, so a naive formatter renders
    // [[0,0],[4,0]] as `waypoints 0.0 0.0 4.0 0.0`, which parses back as one flat list of four.
    // The value survives and the structure does not.
    let source = "\
scene grouping
version 1

entity a \"A\"
  Path
    points
      - 0.0 0.0
      - 4.0 0.0
";

    let document = parse(source).expect("parses");
    assert_eq!(to_text(&document), source);

    let reparsed = parse(&to_text(&document)).expect("reparses");
    assert_eq!(
        document, reparsed,
        "the document itself must survive the trip"
    );
}

#[test]
fn values_keep_their_types_across_a_round_trip() {
    // `1` is an integer, `1.0` is a float, `Patrol` is an identifier, `"Patrol"` is a string.
    // The text says which, so no schema is needed to tell them apart -- and the writer has to
    // preserve the distinction or the file stops meaning what it said.
    let source = "\
scene types
version 1

entity a \"A\"
  Mixed
    count 3
    identifier Patrol
    ratio 1.0
    text \"Patrol\"
    toggle true
";

    let document = parse(source).expect("parses");
    assert_eq!(to_text(&document), source);
}

#[test]
fn strings_containing_awkward_characters_survive() {
    let source = "\
scene awkward
version 1

entity a \"A room \\\"quoted\\\" and \\\\ escaped\"
  Label
    text \"a # that is not a comment\"
";

    let document = parse(source).expect("parses");
    assert_eq!(
        document.entities[0].name,
        r#"A room "quoted" and \ escaped"#
    );
    assert_eq!(to_text(&document), source);
}

// --- Error quality. The reason ADR 0014 chose a format whose messages we own. ---

/// Parses and returns the error, failing the test if the input unexpectedly succeeded.
fn error(source: &str) -> amadeo_scene::ParseError {
    parse(source).expect_err("expected this to fail")
}

#[test]
fn a_tab_is_rejected_with_its_line_number() {
    let failure = error("scene s\nversion 1\n\nentity a \"A\"\n\tThing\n");
    assert_eq!(failure.line, 5);
    assert_eq!(failure.kind, ParseErrorKind::TabIndentation);
    assert!(failure.to_string().starts_with("line 5:"));
}

#[test]
fn odd_indentation_says_how_to_fix_it() {
    let failure = error("scene s\nversion 1\n\nentity a \"A\"\n   Thing\n");
    assert_eq!(failure.line, 5);
    assert_eq!(failure.kind, ParseErrorKind::OddIndentation { found: 3 });
    assert!(
        failure.to_string().contains("amadeo fmt"),
        "the message should point at the fix: {failure}"
    );
}

#[test]
fn an_unquoted_entity_name_is_explained_rather_than_guessed_at() {
    let failure = error("scene s\nversion 1\n\nentity a Corridor\n");
    assert_eq!(failure.line, 4);
    assert_eq!(
        failure.kind,
        ParseErrorKind::UnquotedEntityName {
            id: "a".to_string()
        }
    );
    assert!(failure.to_string().contains(r#"entity a "My Entity""#));
}

#[test]
fn an_override_without_a_prefab_names_the_entity_and_the_fix() {
    let failure = error("scene s\nversion 1\n\nentity a \"A\"\n  override Door\n    locked true\n");
    assert_eq!(failure.line, 5);
    assert_eq!(
        failure.kind,
        ParseErrorKind::OverrideWithoutPrefab {
            id: "a".to_string()
        }
    );
    assert!(failure.to_string().contains("from <prefab-path>"));
}

#[test]
fn a_missing_header_shows_the_shape_of_a_scene_file() {
    let failure = error("entity a \"A\"\n");
    assert_eq!(failure.line, 1);
    assert!(failure.to_string().contains("scene <name>"));
}

#[test]
fn a_duplicate_component_is_refused_rather_than_overwritten() {
    let failure =
        error("scene s\nversion 1\n\nentity a \"A\"\n  Thing\n    value 1\n  Thing\n    value 2\n");
    assert_eq!(
        failure.kind,
        ParseErrorKind::DuplicateComponent {
            entity: "a".to_string(),
            component: "Thing".to_string(),
        }
    );
}

#[test]
fn a_field_with_no_value_says_both_ways_to_give_it_one() {
    let failure = error("scene s\nversion 1\n\nentity a \"A\"\n  Thing\n    lonely\n");
    assert_eq!(failure.line, 6);
    let message = failure.to_string();
    assert!(message.contains("lonely 1.0"), "{message}");
    assert!(message.contains("- 1.0 2.0"), "{message}");
}

#[test]
fn an_unterminated_string_is_caught() {
    let failure = error("scene s\nversion 1\n\nentity a \"A\n");
    assert_eq!(failure.kind, ParseErrorKind::UnterminatedString);
}

// --- The document model ---

#[test]
fn duplicate_ids_are_reported_by_the_document_not_the_parser() {
    // Syntactically fine, semantically ambiguous. That is `amadeo check`'s call, not the parser's,
    // so parsing succeeds and the document reports it.
    let document = parse("scene s\nversion 1\n\nentity a \"A\"\n\nentity a \"Also A\"\n")
        .expect("duplicate ids still parse");
    assert_eq!(document.duplicate_ids(), vec!["a".to_string()]);
}

#[test]
fn deep_nesting_works_and_reads_back() {
    let source = "\
scene deep
version 1

entity a \"A\"

  entity b \"B\"

    entity c \"C\"

      entity d \"D\"
";
    let document = parse(source).expect("parses");
    assert_eq!(to_text(&document), source);

    let ids: Vec<&str> = document.walk().iter().map(|e| e.id.as_str()).collect();
    assert_eq!(ids, vec!["a", "b", "c", "d"]);
}

#[test]
fn the_q2_spike_candidate_still_parses() {
    // The file Justin chose from. If the format drifts away from what he picked, this fails.
    let path = concat!(
        env!("CARGO_MANIFEST_DIR"),
        "/../../spikes/q2-scene-format/candidates/scene.scene"
    );
    let source = std::fs::read_to_string(path).expect("the spike candidate is committed");
    let document = parse(&source).expect("the format Justin chose parses");

    assert_eq!(document.name, "corridor_a");
    assert_eq!(document.walk().len(), 4);
    assert!(
        document.find("a3").expect("the door exists").is_instance(),
        "a3 instances a prefab"
    );
}
