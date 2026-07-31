//! The in-memory model of a scene file.

use amadeo_reflect::Value;
use std::collections::BTreeMap;

/// A parsed scene file.
///
/// This is the *syntactic* model — layer 1 in ADR 0014. It knows the shape of the document but
/// nothing about whether `Transform2d` is a real component or whether `position` is one of its
/// fields. That check is layer 2, against the reflection registry, and keeping the two apart is what
/// lets `amadeo fmt` work on a scene referencing a module that is not loaded.
#[derive(Debug, Clone, PartialEq)]
pub struct SceneDocument {
    /// The scene's identifier, from the `scene` header line.
    pub name: String,
    /// The document's schema version, from the `version` header line.
    ///
    /// Bumped when the *format* changes incompatibly, so a loader can tell "written before the
    /// change" from "corrupt". Distinct from a component's own version (ADR 0012).
    pub version: u32,
    /// Root entities, in the order they appear.
    pub entities: Vec<SceneEntity>,
}

/// One entity in a scene, with its components and children.
#[derive(Debug, Clone, PartialEq)]
pub struct SceneEntity {
    /// The stable authoring id. Survives reordering and is what other files reference.
    pub id: String,
    /// A human-facing label. Not an identity — two entities may share a name.
    pub name: String,
    /// The prefab this entity instances, if any. `from prefabs/door_metal` in the text.
    pub prefab: Option<String>,
    /// Components, keyed by component name.
    ///
    /// Each value is a [`Value::Struct`]. `BTreeMap` so the canonical writer emits them sorted
    /// without having to remember to sort — invariant I2 falling out of the data structure, the same
    /// trick `amadeo_reflect::Value` uses for struct fields.
    pub components: BTreeMap<String, Value>,
    /// Field-level overrides applied on top of [`SceneEntity::prefab`].
    ///
    /// Kept separate from `components` rather than merged, because
    /// `docs/04-subsystems.md` §9 requires override state to be **visible in the text** with nothing
    /// hidden. Merging them at parse time would erase which fields the instance actually overrode.
    pub overrides: BTreeMap<String, Value>,
    /// Child entities, **in declaration order**.
    ///
    /// A `Vec`, not a map: siblings are a sequence, and their order is meaningful (draw order,
    /// iteration order). Sorting them would destroy information, and reordering them is a real
    /// change that should show up as a real diff.
    pub children: Vec<SceneEntity>,
}

impl SceneEntity {
    /// A bare entity with no components, children, or prefab.
    #[must_use]
    pub fn new(id: impl Into<String>, name: impl Into<String>) -> Self {
        Self {
            id: id.into(),
            name: name.into(),
            prefab: None,
            components: BTreeMap::new(),
            overrides: BTreeMap::new(),
            children: Vec::new(),
        }
    }

    /// Whether this entity instances a prefab.
    #[must_use]
    pub fn is_instance(&self) -> bool {
        self.prefab.is_some()
    }

    /// Every entity in this subtree, depth first, starting with this one.
    ///
    /// Depth-first rather than breadth-first so the order matches how the file reads top to bottom,
    /// which is what makes a diagnostic quoting "the third entity" mean the same thing to a person
    /// scrolling the file.
    pub fn walk(&self) -> Vec<&SceneEntity> {
        let mut found = vec![self];
        for child in &self.children {
            found.extend(child.walk());
        }
        found
    }
}

impl SceneDocument {
    /// An empty scene.
    #[must_use]
    pub fn new(name: impl Into<String>) -> Self {
        Self {
            name: name.into(),
            version: 1,
            entities: Vec::new(),
        }
    }

    /// Every entity in the document, depth first.
    pub fn walk(&self) -> Vec<&SceneEntity> {
        self.entities.iter().flat_map(SceneEntity::walk).collect()
    }

    /// Finds an entity anywhere in the tree by its authoring id.
    #[must_use]
    pub fn find(&self, id: &str) -> Option<&SceneEntity> {
        self.walk().into_iter().find(|entity| entity.id == id)
    }

    /// Ids used more than once, sorted.
    ///
    /// Duplicate ids are not a parse error — the file is still syntactically valid — but they make
    /// every cross-file reference ambiguous, so `amadeo check` reports them. Returning them rather
    /// than asserting keeps this layer free of policy.
    #[must_use]
    pub fn duplicate_ids(&self) -> Vec<String> {
        let mut seen: BTreeMap<&str, usize> = BTreeMap::new();
        for entity in self.walk() {
            *seen.entry(entity.id.as_str()).or_insert(0) += 1;
        }
        seen.into_iter()
            .filter(|(_, count)| *count > 1)
            .map(|(id, _)| id.to_string())
            .collect()
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn tree() -> SceneDocument {
        let mut grandchild = SceneEntity::new("c", "Grandchild");
        grandchild.components.insert("Tag".to_string(), Value::Unit);

        let mut child = SceneEntity::new("b", "Child");
        child.children.push(grandchild);

        let mut root = SceneEntity::new("a", "Root");
        root.children.push(child);

        let mut document = SceneDocument::new("test");
        document.entities.push(root);
        document
    }

    #[test]
    fn walk_is_depth_first_so_it_matches_reading_order() {
        // Bound to a local: `walk` borrows from the document, so a temporary would be dropped
        // while the borrowed ids are still in use.
        let document = tree();
        let ids: Vec<&str> = document.walk().iter().map(|e| e.id.as_str()).collect();
        assert_eq!(ids, vec!["a", "b", "c"]);
    }

    #[test]
    fn find_reaches_into_the_tree() {
        let document = tree();
        assert_eq!(
            document.find("c").map(|e| e.name.as_str()),
            Some("Grandchild")
        );
        assert_eq!(document.find("nope"), None);
    }

    #[test]
    fn duplicate_ids_are_reported_not_rejected() {
        let mut document = tree();
        // A second entity claiming "b", nested somewhere else entirely.
        document.entities.push(SceneEntity::new("b", "Impostor"));

        assert_eq!(document.duplicate_ids(), vec!["b".to_string()]);
        // A clean document reports nothing.
        assert!(tree().duplicate_ids().is_empty());
    }

    #[test]
    fn an_entity_knows_whether_it_instances_a_prefab() {
        let mut entity = SceneEntity::new("a", "Door");
        assert!(!entity.is_instance());
        entity.prefab = Some("prefabs/door_metal".to_string());
        assert!(entity.is_instance());
    }
}
