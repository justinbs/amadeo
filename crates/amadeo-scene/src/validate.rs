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
use amadeo_assets::AssetCatalogue;
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

/// Checks every entity, component, and declared asset in a document.
///
/// Returns an empty vector when the scene would load. Otherwise one [`Diagnostic`] per problem: the
/// declared assets first, since a scene that cannot find its textures is broken before any entity
/// matters, then entities in document order — parents before children, components in name order.
///
/// `assets` is optional because checking a component name and checking an asset id are separate
/// capabilities: a caller holding a registry but no catalogue can still do the first half. Passing
/// `None` skips asset checks entirely rather than reporting every id as missing, which would be the
/// wrong answer given confidently.
#[must_use]
pub fn validate(
    document: &SceneDocument,
    registry: &ComponentRegistry,
    assets: Option<&AssetCatalogue>,
) -> Vec<Diagnostic> {
    let mut diagnostics = Vec::new();

    if let Some(catalogue) = assets {
        check_assets(document, catalogue, &mut diagnostics);
    }

    let mut seen: BTreeSet<String> = BTreeSet::new();
    for entity in &document.entities {
        check_entity(entity, registry, &mut seen, &mut diagnostics);
    }

    diagnostics
}

/// Checks that every asset the scene declares actually exists.
///
/// ADR 0020 gave `amadeo check` this job by name: "verifying that every `from` in a scene resolves
/// to a known id, and listing near-misses when it does not". The near miss is the point — an agent
/// that guessed `wall` when the id is `wall_concrete` gets told so, rather than being told nothing
/// useful and guessing again.
fn check_assets(
    document: &SceneDocument,
    catalogue: &AssetCatalogue,
    diagnostics: &mut Vec<Diagnostic>,
) {
    for id in &document.assets {
        if catalogue.contains(id) {
            continue;
        }

        let near = catalogue.similar_to(id);
        let hint = if near.is_empty() {
            "Run `amadeo assets` for the ids that do exist".to_string()
        } else {
            format!("Did you mean {}?", near.join(", "))
        };

        diagnostics.push(Diagnostic {
            // The `assets` block belongs to the document, not to any entity, so there is no id to
            // carry. Named so a reader searching the file lands on the right line.
            entity: "assets".to_string(),
            component: None,
            message: format!("no asset is called `{id}`. {hint}"),
        });
    }
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
    use amadeo_assets::AssetCatalogue;
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

    /// A catalogue holding one asset, so the asset half of validation has something to match.
    fn catalogue() -> AssetCatalogue {
        let mut catalogue = AssetCatalogue::new();
        catalogue
            .insert(
                amadeo_assets::Sidecar::new("wall_concrete"),
                std::path::Path::new("textures/wall_concrete.ppm"),
            )
            .expect("distinct");
        catalogue
    }

    fn check(text: &str) -> Vec<Diagnostic> {
        let document = parse(text).expect("the fixture should parse");
        validate(&document, &registry(), Some(&catalogue()))
    }

    /// Validation with no catalogue at all, for the caller that only has a registry.
    fn check_without_assets(text: &str) -> Vec<Diagnostic> {
        let document = parse(text).expect("the fixture should parse");
        validate(&document, &registry(), None)
    }

    #[test]
    fn a_declared_asset_that_exists_reports_nothing() {
        let diagnostics = check("scene demo\nversion 1\n\nassets\n  wall_concrete\n");
        assert_eq!(diagnostics, Vec::new());
    }

    #[test]
    fn a_declared_asset_that_does_not_exist_is_reported_with_a_near_miss() {
        // ADR 0020 gave check this job by name, and the near miss is the point: an agent that
        // guessed the stem of a longer id gets told which one it meant.
        let diagnostics = check("scene demo\nversion 1\n\nassets\n  wall\n");

        assert_eq!(diagnostics.len(), 1, "got: {diagnostics:?}");
        assert_eq!(diagnostics[0].entity, "assets");
        assert!(
            diagnostics[0]
                .message
                .contains("Did you mean wall_concrete?"),
            "got: {}",
            diagnostics[0].message
        );
    }

    #[test]
    fn an_unknown_asset_with_no_near_miss_says_where_to_look() {
        let diagnostics = check("scene demo\nversion 1\n\nassets\n  nothing_like_it\n");

        assert_eq!(diagnostics.len(), 1, "got: {diagnostics:?}");
        assert!(
            diagnostics[0].message.contains("amadeo assets"),
            "got: {}",
            diagnostics[0].message
        );
    }

    #[test]
    fn every_missing_asset_is_reported_not_just_the_first() {
        let diagnostics = check("scene demo\nversion 1\n\nassets\n  one\n  two\n  three\n");
        assert_eq!(diagnostics.len(), 3, "got: {diagnostics:?}");
    }

    #[test]
    fn with_no_catalogue_asset_ids_are_skipped_rather_than_all_reported_missing() {
        // A caller holding a registry but no catalogue cannot judge asset ids. Reporting every one
        // as missing would be the wrong answer stated confidently, which is worse than silence.
        let diagnostics =
            check_without_assets("scene demo\nversion 1\n\nassets\n  wall\n  floor\n");
        assert_eq!(diagnostics, Vec::new());
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
