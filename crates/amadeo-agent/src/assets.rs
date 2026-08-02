//! `assets.list` — what assets exist, by the name a scene refers to them by.
//!
//! # Why this method has to exist before ids are used anywhere
//!
//! ADR 0020 made an asset's identity a declared `id` rather than its path, and was explicit about
//! the one real cost of that: with paths, an agent can list a directory and reference what it sees;
//! with ids it cannot, because the id lives inside a sidecar. The ADR names this method as the
//! mitigation and says it must exist **before** ids become the reference syntax, "otherwise the
//! first agent to author a scene has to guess. Guessing is exactly the plausible-but-wrong failure
//! Pillar 2 exists to eliminate."
//!
//! So this is not a convenience listing. It is the ground truth that makes ADR 0020 safe to rely on.
//!
//! # It reports the complaints too
//!
//! An asset file with no sidecar is invisible to the catalogue, and the obvious failure — asking for
//! `wall` and being told no such asset exists, while `wall.png` sits right there in the tree — is
//! the papercut ADR 0020 predicted. So `unimported` is reported alongside, and the "did you mean"
//! on a failed lookup checks it. The answer becomes "that file exists but has not been imported",
//! which is a different sentence with a different fix.

use crate::json::Json;
use amadeo_assets::Assets;
use amadeo_ecs::World;

/// Renders the asset catalogue as JSON.
///
/// Reports an empty listing, rather than an error, when a game installed no catalogue — a game with
/// no assets is a perfectly ordinary thing and `installed: false` says so without making the caller
/// handle a failure.
#[must_use]
pub fn list(world: &World) -> Json {
    let Some(assets) = world.service::<Assets>() else {
        return Json::object([
            ("installed", Json::Bool(false)),
            ("count", Json::Int(0)),
            ("assets", Json::Array(Vec::new())),
            ("unimported", Json::Array(Vec::new())),
            ("orphaned", Json::Array(Vec::new())),
            (
                "note",
                Json::string(
                    "this game installed no asset catalogue. A game scans one with \
                     `App::scan_assets(\"assets\")` before it runs",
                ),
            ),
        ]);
    };

    let entries: Vec<Json> = assets
        .catalogue
        .iter()
        .map(|entry| {
            let mut members = vec![
                ("id", Json::string(&entry.id)),
                ("source", Json::string(show(&entry.source))),
                // ADR 0021: gameplay may never observe this, but an agent inspecting from outside
                // the simulation is not gameplay, and it is exactly who needs to know.
                ("state", Json::string("catalogued")),
            ];

            if !entry.settings.is_empty() {
                members.push((
                    "settings",
                    Json::object(
                        entry
                            .settings
                            .iter()
                            .map(|(key, value)| (key.as_str(), Json::string(value)))
                            .collect::<Vec<_>>(),
                    ),
                ));
            }

            Json::object(members)
        })
        .collect();

    let mut members = vec![
        ("installed", Json::Bool(true)),
        ("count", Json::Int(entries.len() as i64)),
        ("assets", Json::Array(entries)),
        (
            "unimported",
            Json::Array(
                assets
                    .unimported
                    .iter()
                    .map(|p| Json::string(show(p)))
                    .collect(),
            ),
        ),
        (
            "orphaned",
            Json::Array(
                assets
                    .orphaned
                    .iter()
                    .map(|p| Json::string(show(p)))
                    .collect(),
            ),
        ),
    ];

    // Where the scan looked, and by which rule. "I looked in the wrong place" and "the files are
    // missing" have identical symptoms and different fixes, so the reply distinguishes them.
    if let Some(root) = &assets.root {
        members.push(("root", Json::string(show(&root.path))));
        members.push(("root_anchor", Json::string(root.anchor.name())));
    }

    Json::object(members)
}

/// A path as it should appear in a reply: forward slashes, everywhere.
///
/// The catalogue already normalises what it stores; the asset *root* is an absolute path built from
/// the local filesystem and has not been through that. Normalising here keeps a reply from having
/// two different path conventions in it.
fn show(path: &std::path::Path) -> String {
    path.to_string_lossy().replace('\\', "/")
}

#[cfg(test)]
mod tests {
    use super::*;
    use amadeo_assets::{AssetCatalogue, Sidecar};
    use std::path::Path;

    fn world_with(entries: &[(&str, &str)]) -> World {
        let mut catalogue = AssetCatalogue::new();
        for (id, path) in entries {
            catalogue
                .insert(Sidecar::new(*id), Path::new(path))
                .expect("distinct ids");
        }

        let mut world = World::new();
        world.insert_service(Assets::from_catalogue(catalogue));
        world
    }

    #[test]
    fn lists_every_id_with_the_file_behind_it() {
        // The whole point: an agent can learn the ids without opening a single sidecar.
        let world = world_with(&[
            ("wall_concrete", "textures/wall_concrete.png"),
            ("footstep", "audio/step.ogg"),
        ]);
        let text = list(&world).to_compact();

        assert!(text.contains(r#""id":"wall_concrete""#), "got: {text}");
        assert!(
            text.contains(r#""source":"textures/wall_concrete.png""#),
            "got: {text}"
        );
        assert!(text.contains(r#""count":2"#), "got: {text}");
    }

    #[test]
    fn the_listing_is_ordered_so_it_can_be_diffed() {
        // Invariant I3 reaching the protocol: two machines must produce the same reply.
        let forwards = world_with(&[("a", "a.png"), ("z", "z.png")]);
        let backwards = world_with(&[("z", "z.png"), ("a", "a.png")]);

        assert_eq!(list(&forwards).to_compact(), list(&backwards).to_compact());
    }

    #[test]
    fn a_game_with_no_catalogue_gets_an_empty_listing_not_an_error() {
        let world = World::new();
        let text = list(&world).to_compact();

        assert!(text.contains(r#""installed":false"#), "got: {text}");
        assert!(text.contains(r#""count":0"#), "got: {text}");
        // And it says how to install one, because an empty listing otherwise reads as "no assets".
        assert!(text.contains("scan_assets"), "got: {text}");
    }

    #[test]
    fn unimported_files_are_reported_so_the_papercut_is_visible() {
        // ADR 0020 predicted this exact confusion: `wall.png` is right there, and asking for `wall`
        // says no such asset. The listing has to show it or the message cannot explain itself.
        let mut world = world_with(&[("floor", "floor.png")]);
        world
            .service_mut::<Assets>()
            .expect("installed")
            .unimported
            .push(std::path::PathBuf::from("textures/wall.png"));

        let text = list(&world).to_compact();
        assert!(
            text.contains(r#""unimported":["textures/wall.png"]"#),
            "got: {text}"
        );
    }

    #[test]
    fn settings_are_reported_when_there_are_any() {
        let mut catalogue = AssetCatalogue::new();
        let mut sidecar = Sidecar::new("wall");
        sidecar.settings.insert("filter".into(), "nearest".into());
        catalogue
            .insert(sidecar, Path::new("wall.png"))
            .expect("inserts");

        let mut world = World::new();
        world.insert_service(Assets::from_catalogue(catalogue));

        let text = list(&world).to_compact();
        assert!(
            text.contains(r#""settings":{"filter":"nearest"}"#),
            "got: {text}"
        );
    }

    #[test]
    fn an_asset_with_no_settings_omits_the_key_rather_than_sending_an_empty_object() {
        let world = world_with(&[("wall", "wall.png")]);
        assert!(!list(&world).to_compact().contains("settings"));
    }
}
