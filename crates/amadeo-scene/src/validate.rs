//! Checking a scene against the registry without loading it — what `amadeo check` runs.
//!
//! [`crate::instantiate`] answers the same question by doing the thing: it needs a `World`, it
//! mutates it, and it stops at the first mistake. That is right for loading and wrong for checking.
//!
//! # Every problem, not the first one
//!
//! The difference that matters. An agent fixing a scene file cannot ask a follow-up question, so
//! reporting one error, being fixed, and then reporting the next is a slow loop with a round trip
//! per mistake. `docs/03-ai-native-design.md` Pillar 5 treats that as a functional defect, not a
//! papercut. So validation collects.
//!
//! It reports the same problems `instantiate` would raise, in document order.

use crate::document::{SceneDocument, SceneEntity};
use amadeo_ecs::ComponentRegistry;
use std::collections::BTreeSet;

/// One thing wrong with a scene.
///
/// Carries the **authoring id** rather than a line number, because a [`SceneDocument`] does not
/// record where in the file it came from. The id is what the reader searches for, and a caller that
/// still has the source text can turn `entity <id>` into a line — which is what `amadeo check` does.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Diagnostic {
    /// The authoring id of the entity involved.
    pub entity: String,
    /// The component involved, when the problem is about one.
    pub component: Option<String>,
    /// What is wrong, written to be actionable on its own.
    pub message: String,
}

/// Checks every entity and component in a document against the registry.
///
/// Returns an empty vector when the scene would load. Otherwise one [`Diagnostic`] per problem, in
/// document order — parents before children, components in name order.
#[must_use]
pub fn validate(document: &SceneDocument, registry: &ComponentRegistry) -> Vec<Diagnostic> {
    let mut diagnostics = Vec::new();
    let mut seen: BTreeSet<String> = BTreeSet::new();

    for entity in &document.entities {
        check_entity(entity, registry, &mut seen, &mut diagnostics);
    }

    diagnostics
}

/// Checks one entity and everything beneath it.
fn check_entity(
    source: &SceneEntity,
    registry: &ComponentRegistry,
    seen: &mut BTreeSet<String>,
    diagnostics: &mut Vec<Diagnostic>,
) {
    if !seen.insert(source.id.clone()) {
        diagnostics.push(Diagnostic {
            entity: source.id.clone(),
            component: None,
            message: format!(
                "entity id `{}` is used more than once; ids have to be unique because they are \
                 how everything else refers to an entity",
                source.id
            ),
        });
    }

    if let Some(prefab) = &source.prefab {
        diagnostics.push(Diagnostic {
            entity: source.id.clone(),
            component: None,
            message: format!(
                "instances the prefab `{prefab}`, which cannot be loaded yet: resolving a prefab \
                 path needs the asset layer (`amadeo-assets`, M1). The scene is otherwise valid"
            ),
        });
    }

    // Overrides are checked the same way as components -- they are values for the same types, and a
    // typo in an override block is exactly as wrong as one anywhere else.
    for (name, value) in source.components.iter().chain(source.overrides.iter()) {
        if let Err(error) = registry.validate(name, value) {
            diagnostics.push(Diagnostic {
                entity: source.id.clone(),
                component: Some(name.clone()),
                message: error.to_string(),
            });
        }
    }

    for child in &source.children {
        check_entity(child, registry, seen, diagnostics);
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::parse;
    use amadeo_core::StableHash;
    use amadeo_ecs::Component;
    use amadeo_reflect::Reflect;

    /// Where something is.
    #[derive(Debug, Clone, Copy, PartialEq, StableHash, Reflect)]
    struct Position {
        /// Horizontal.
        x: f32,
        /// Vertical.
        y: f32,
    }
    impl Component for Position {}

    /// Marks the player.
    #[derive(Debug, Clone, Copy, PartialEq, StableHash, Reflect)]
    struct Player;
    impl Component for Player {}

    fn registry() -> ComponentRegistry {
        let mut registry = ComponentRegistry::new();
        registry.register::<Position>().expect("registers");
        registry.register::<Player>().expect("registers");
        registry
    }

    fn check(text: &str) -> Vec<Diagnostic> {
        let document = parse(text).expect("the fixture should parse");
        validate(&document, &registry())
    }

    #[test]
    fn a_valid_scene_reports_nothing() {
        let diagnostics = check(
            "scene demo\nversion 1\n\nentity a1 \"Player\"\n  Position\n    x 1\n    y 2\n  Player\n",
        );
        assert_eq!(diagnostics, Vec::new());
    }

    #[test]
    fn an_unknown_component_lists_the_ones_that_exist() {
        let diagnostics = check("scene demo\nversion 1\n\nentity a1 \"P\"\n  Velocity\n    x 1\n");

        assert_eq!(diagnostics.len(), 1);
        assert_eq!(diagnostics[0].entity, "a1");
        assert_eq!(diagnostics[0].component.as_deref(), Some("Velocity"));
        // Sorted, because the registry is a BTreeMap -- so the suggestion list is stable across
        // runs and diffable, like everything else the agent reads.
        assert!(
            diagnostics[0].message.contains("Player, Position"),
            "got: {}",
            diagnostics[0].message
        );
    }

    #[test]
    fn an_unknown_field_is_caught() {
        let diagnostics = check("scene demo\nversion 1\n\nentity a1 \"P\"\n  Position\n    z 1\n");
        assert_eq!(diagnostics.len(), 1);
        assert_eq!(diagnostics[0].component.as_deref(), Some("Position"));
    }

    #[test]
    fn every_problem_is_reported_not_just_the_first() {
        // The whole reason this exists rather than reusing `instantiate`. Three mistakes across two
        // entities and a child, all reported in one pass.
        let diagnostics = check(
            "scene demo\nversion 1\n\n\
             entity a1 \"One\"\n  Velocity\n    x 1\n  Position\n    z 9\n\
             entity a2 \"Two\"\n  Health\n    hp 3\n",
        );

        assert_eq!(diagnostics.len(), 3, "got: {diagnostics:#?}");
        assert_eq!(diagnostics[0].entity, "a1");
        assert_eq!(diagnostics[1].entity, "a1");
        assert_eq!(diagnostics[2].entity, "a2");
    }

    #[test]
    fn children_are_checked_too() {
        let diagnostics = check(
            "scene demo\nversion 1\n\nentity a1 \"Parent\"\n  entity a2 \"Child\"\n    Nope\n      x 1\n",
        );
        assert_eq!(diagnostics.len(), 1);
        assert_eq!(diagnostics[0].entity, "a2");
    }

    #[test]
    fn a_duplicate_id_is_reported_once_against_the_repeat() {
        let diagnostics = check(
            "scene demo\nversion 1\n\nentity a1 \"One\"\n  Player\nentity a1 \"Two\"\n  Player\n",
        );
        assert_eq!(diagnostics.len(), 1);
        assert_eq!(diagnostics[0].entity, "a1");
        assert!(diagnostics[0].message.contains("more than once"));
    }

    #[test]
    fn a_prefab_says_what_is_missing_rather_than_failing_obscurely() {
        let diagnostics =
            check("scene demo\nversion 1\n\nentity a1 \"Door\" from prefabs/door\n  Player\n");
        assert_eq!(diagnostics.len(), 1);
        assert!(diagnostics[0].message.contains("amadeo-assets"));
    }
}
