//! Turning a glTF file into engine text — ADR 0039.
//!
//! # What comes out, and why that split
//!
//! One `.glb` becomes:
//!
//! - a **`.scene`** file with the node hierarchy — where everything is, as nested entities;
//! - a **`.material`** file per glTF material;
//! - a **`.mesh`** file per glTF *primitive*, each a two-line pointer into the source file;
//! - a **sidecar** for the `.glb` itself, so the asset catalogue can find it.
//!
//! Geometry stays in the `.glb`. That is the line ADR 0039 draws, and the reason is invariant I1
//! rather than convenience: I1 says text files are the source of truth, and what people and agents
//! actually *author* is layout and materials — not vertex positions. A twenty-thousand-triangle
//! model written out as text would be megabytes of numbers, diffable in principle and unreadable in
//! practice. A `.glb` is source art, exactly as a `.png` already is (ADR 0026).
//!
//! This is also what Godot does: import a glTF and you get a Godot *scene* with the hierarchy
//! preserved, while the geometry stays a resource.
//!
//! # Re-importing overwrites
//!
//! Running this twice writes the same files again, so hand edits to generated files are lost. That
//! is deliberate and stated rather than worked around: the generated scene is a **starting point** to
//! copy from or instance, and the alternative — merging hand edits into regenerated output — is the
//! feature every engine gets wrong and Unity is notorious for.

use amadeo_scene::escape;
use anyhow::{Context, Result, bail};
use std::collections::BTreeSet;
use std::path::{Path, PathBuf};

/// What an import produced, for reporting and for tests.
#[derive(Debug, Default, PartialEq, Eq)]
pub(crate) struct Imported {
    /// Files written, in the order they were written.
    pub(crate) written: Vec<PathBuf>,
    /// The asset id given to the source `.glb`.
    pub(crate) source_id: String,
}

/// Reads a glTF file and writes the engine text for it.
///
/// `out` is the directory the generated files land in; the source file's own directory is the
/// default, so an import lands next to the art it came from.
///
/// # Errors
///
/// If the file cannot be read, is not glTF the engine can use, or the output cannot be written.
pub(crate) fn import_gltf(path: &Path, out: Option<&Path>, dry_run: bool) -> Result<Imported> {
    let bytes =
        std::fs::read(path).with_context(|| format!("could not read {}", path.display()))?;
    let document = amadeo_gltf::read(&bytes)
        .with_context(|| format!("could not import {}", path.display()))?;

    let stem = path
        .file_stem()
        .and_then(|stem| stem.to_str())
        .map(sanitise)
        .filter(|stem| !stem.is_empty())
        .context("the glTF file needs a usable file name to derive asset ids from")?;

    let directory = match out {
        Some(directory) => directory.to_path_buf(),
        None => path.parent().unwrap_or(Path::new(".")).to_path_buf(),
    };

    // The source file needs an id of its own, because that is what every generated `.mesh` points
    // at. `_glb` rather than the bare stem so it cannot collide with the scene of the same name.
    let source_id = format!("{stem}_glb");

    let mut plan: Vec<(PathBuf, String)> = Vec::new();

    // Sidecar for the source, so the catalogue can find it (ADR 0020). Written with the same
    // one-line shape `amadeo import` produces.
    let sidecar = PathBuf::from(format!("{}.ama-meta", path.display()));
    plan.push((sidecar, format!("id = \"{source_id}\"\n")));

    // --- Materials ---
    //
    // Named before the meshes, because a mesh file names the material it uses and both have to
    // agree on how a name becomes an id.
    let material_ids = unique_ids(
        &stem,
        document.materials.iter().map(|material| &material.name),
    );
    for (material, id) in document.materials.iter().zip(&material_ids) {
        let text = canonical(&material_text(id, &material.name, material), "a material")?;
        plan.push((directory.join(format!("{id}.material")), text));
        plan.push((
            directory.join(format!("{id}.material.ama-meta")),
            format!("id = \"{id}\"\n"),
        ));
    }

    // --- Meshes: one file per primitive ---
    //
    // A glTF mesh holds one primitive per material, and Amadeo's `Mesh` draws one thing with one
    // material -- so a primitive is what corresponds to a mesh asset. A mesh with several
    // primitives gets several files, suffixed by index.
    let mut mesh_ids: Vec<Vec<String>> = Vec::new();
    let mut taken: BTreeSet<String> = material_ids.iter().cloned().collect();
    for (mesh_index, mesh) in document.meshes.iter().enumerate() {
        let mut per_primitive = Vec::new();
        for primitive_index in 0..mesh.primitives.len() {
            let base = if mesh.primitives.len() == 1 {
                format!("{stem}_{}", sanitise(&mesh.name))
            } else {
                format!("{stem}_{}_{primitive_index}", sanitise(&mesh.name))
            };
            let id = distinct(base, &mut taken);
            let text = canonical(
                &mesh_text(&id, &mesh.name, &source_id, mesh_index, primitive_index),
                "a mesh file",
            )?;
            plan.push((directory.join(format!("{id}.mesh")), text));
            plan.push((
                directory.join(format!("{id}.mesh.ama-meta")),
                format!("id = \"{id}\"\n"),
            ));
            per_primitive.push(id);
        }
        mesh_ids.push(per_primitive);
    }

    // --- The scene ---
    let scene = canonical(
        &scene_text(&stem, &document, &mesh_ids, &material_ids, &source_id),
        "a scene",
    )?;
    plan.push((directory.join(format!("{stem}.scene")), scene));

    if dry_run {
        return Ok(Imported {
            written: plan.into_iter().map(|(path, _)| path).collect(),
            source_id,
        });
    }

    // Prepare-then-apply, the same shape `amadeo import` uses: nothing is written until every file
    // has been built, so a failure part-way through does not leave a half-imported model behind.
    if let Some(parent) = directory.parent()
        && !parent.as_os_str().is_empty()
        && !parent.exists()
    {
        bail!(
            "{} does not exist; create it or pass --out to an existing directory",
            parent.display()
        );
    }
    std::fs::create_dir_all(&directory)
        .with_context(|| format!("could not create {}", directory.display()))?;

    let mut written = Vec::new();
    for (target, text) in plan {
        // LF endings, written as bytes rather than through any platform-aware helper -- invariant
        // I2, and the same rule every other text file in this project follows.
        std::fs::write(&target, text.as_bytes())
            .with_context(|| format!("could not write {}", target.display()))?;
        written.push(target);
    }

    Ok(Imported { written, source_id })
}

/// Turns a name from an authoring tool into something usable as an asset id.
///
/// Blender happily names a mesh `Cube.001` or `Wall Segment`, and an asset id is referenced from
/// scene files where a bare word is the readable spelling. Lowercase, and anything that is not a
/// letter, digit or underscore becomes an underscore.
fn sanitise(name: &str) -> String {
    let mut out = String::with_capacity(name.len());
    let mut last_was_underscore = false;
    for character in name.chars() {
        if character.is_ascii_alphanumeric() {
            out.push(character.to_ascii_lowercase());
            last_was_underscore = false;
        } else if !last_was_underscore {
            out.push('_');
            last_was_underscore = true;
        }
    }
    out.trim_matches('_').to_string()
}

/// Makes `base` distinct from everything already in `taken`, and records it.
///
/// glTF does not require names to be unique, and two meshes called `Cube` would otherwise produce
/// two files with one id — which the asset scanner refuses, naming both files. Better to disambiguate
/// here than to hand someone an import that will not scan.
fn distinct(base: String, taken: &mut BTreeSet<String>) -> String {
    let base = if base.is_empty() {
        "unnamed".to_string()
    } else {
        base
    };
    if taken.insert(base.clone()) {
        return base;
    }
    for suffix in 2.. {
        let candidate = format!("{base}_{suffix}");
        if taken.insert(candidate.clone()) {
            return candidate;
        }
    }
    unreachable!("the loop above returns")
}

/// Asset ids for a list of names, prefixed and made unique.
fn unique_ids<'a>(stem: &str, names: impl Iterator<Item = &'a String>) -> Vec<String> {
    let mut taken = BTreeSet::new();
    names
        .map(|name| distinct(format!("{stem}_{}", sanitise(name)), &mut taken))
        .collect()
}

/// Formats a float the way `amadeo-scene`'s canonical writer does.
///
/// Delegated rather than reimplemented: `format_float` is subtle, and ADR 0028 already records that
/// two copies of it would drift. Generated files have to come out **already canonical**, or
/// `amadeo fmt --check` fails on this tool's own output.
fn number(value: f32) -> String {
    // `+ 0.0` collapses negative zero to positive. `to_euler_degrees` produces `-0.0` for an
    // unrotated node — `asin(-0.0)` is `-0.0` — and while `rotation -0.0 0.0 0.0` parses and round
    // trips perfectly well, it is a strange thing to hand someone as the rotation of a thing that is
    // not rotated. In IEEE arithmetic `-0.0 + 0.0` is `+0.0`, and nothing else is affected.
    amadeo_scene::format_float(f64::from(value + 0.0))
}

/// Runs generated text through the canonical writer, so this tool cannot disagree with `amadeo fmt`.
///
/// # Why generate text and then reformat it rather than get it right first time
///
/// Invariant I2 says `amadeo fmt` is the **single authority** on canonical form. A generator that
/// reimplemented the rules would be a second authority, and the moment the two disagreed — over a
/// trailing blank line, say, which is exactly what happened here — every file this tool writes would
/// fail `amadeo fmt --check` in CI.
///
/// Parsing its own output also means a generator bug that produced unparseable text fails **here**,
/// naming the file, rather than later when someone tries to load it.
fn canonical(text: &str, what: &str) -> Result<String> {
    let document = amadeo_scene::parse(text)
        .with_context(|| format!("the importer generated {what} that does not parse:\n{text}"))?;
    Ok(amadeo_scene::to_text(&document))
}

fn material_text(id: &str, name: &str, material: &amadeo_gltf::GltfMaterial) -> String {
    // Fields in alphabetical order, because that is what the canonical writer emits. Written
    // directly rather than built as a `SceneDocument` and formatted, because this crate has no
    // `Material` type to reflect -- `amadeo-cli` deliberately does not depend on `amadeo-render`.
    format!(
        "scene {id}\nversion 1\n\nentity material \"{}\"\n  Material\n    base_colour {} {} {} {}\n    \
         base_colour_texture \"\"\n    emissive {} {} {}\n    metallic {}\n    roughness {}\n",
        escape(name),
        number(material.base_colour[0]),
        number(material.base_colour[1]),
        number(material.base_colour[2]),
        number(material.base_colour[3]),
        number(material.emissive[0]),
        number(material.emissive[1]),
        number(material.emissive[2]),
        number(material.metallic),
        number(material.roughness),
    )
}

fn mesh_text(id: &str, name: &str, source: &str, mesh: usize, primitive: usize) -> String {
    format!(
        "scene {id}\nversion 1\n\nentity mesh \"{}\"\n  GltfPart\n    mesh {mesh}\n    \
         primitive {primitive}\n    source \"{source}\"\n",
        escape(name),
    )
}

/// Builds the scene text for the node hierarchy.
fn scene_text(
    stem: &str,
    document: &amadeo_gltf::GltfDocument,
    mesh_ids: &[Vec<String>],
    material_ids: &[String],
    source_id: &str,
) -> String {
    let mut out = format!("scene {stem}\nversion 1\n\n");

    // Every id the scene refers to, declared so the load barrier makes them resident before any
    // entity naming one exists (ADR 0021). The source file is in here too: a `.mesh` pointing into
    // it is useless without its bytes.
    let mut assets: BTreeSet<&str> = BTreeSet::new();
    assets.insert(source_id);
    for ids in mesh_ids {
        for id in ids {
            assets.insert(id);
        }
    }
    for id in material_ids {
        assets.insert(id);
    }
    if !assets.is_empty() {
        out.push_str("assets\n");
        for id in &assets {
            out.push_str(&format!("  {id}\n"));
        }
        out.push('\n');
    }

    let mut taken = BTreeSet::new();
    for root in &document.roots {
        write_node(
            &mut out,
            document,
            *root,
            0,
            mesh_ids,
            material_ids,
            &mut taken,
        );
    }
    out
}

/// Writes one node and its children, indented.
///
/// A glTF node carries a transform and *optionally* a mesh, and a mesh may have several primitives.
/// One entity per node carries the transform; extra primitives become child entities at the
/// identity, which is what keeps one entity drawing one thing.
fn write_node(
    out: &mut String,
    document: &amadeo_gltf::GltfDocument,
    index: usize,
    depth: usize,
    mesh_ids: &[Vec<String>],
    material_ids: &[String],
    taken: &mut BTreeSet<String>,
) {
    let Some(node) = document.nodes.get(index) else {
        return;
    };
    let pad = "  ".repeat(depth);
    let id = distinct(sanitise(&node.name), taken);

    out.push_str(&format!("{pad}entity {id} \"{}\"\n", escape(&node.name)));

    // The mesh, if this node draws one. The first primitive goes on the node itself; any others
    // become children, because one entity draws one thing with one material.
    let primitives: &[String] = node
        .mesh
        .and_then(|mesh| mesh_ids.get(mesh))
        .map_or(&[], Vec::as_slice);
    if let (Some(mesh_id), Some(mesh)) = (
        primitives.first(),
        node.mesh.and_then(|mesh| document.meshes.get(mesh)),
    ) {
        let material = mesh.primitives.first().and_then(|primitive| {
            primitive
                .material
                .and_then(|index| material_ids.get(index))
                .map(String::as_str)
        });
        write_mesh_component(out, depth + 1, mesh_id, material);
    }

    // ADR 0018's Euler degrees, converted from glTF's quaternion through `amadeo-transform` so the
    // exact Z-then-X-then-Y order is owned in one place.
    let rotation = amadeo_transform::Mat4::from_quaternion(node.rotation).to_euler_degrees();
    out.push_str(&format!("{pad}  Transform\n"));
    out.push_str(&format!(
        "{pad}    rotation {} {} {}\n",
        number(rotation[0]),
        number(rotation[1]),
        number(rotation[2])
    ));
    out.push_str(&format!(
        "{pad}    scale {} {} {}\n",
        number(node.scale[0]),
        number(node.scale[1]),
        number(node.scale[2])
    ));
    out.push_str(&format!(
        "{pad}    translation {} {} {}\n",
        number(node.translation[0]),
        number(node.translation[1]),
        number(node.translation[2])
    ));

    // Extra primitives, as children at the identity transform.
    if let Some(mesh) = node.mesh.and_then(|mesh| document.meshes.get(mesh)) {
        for (offset, mesh_id) in primitives.iter().enumerate().skip(1) {
            let child_id = distinct(format!("{id}_part_{offset}"), taken);
            out.push('\n');
            out.push_str(&format!(
                "{pad}  entity {child_id} \"{} part {offset}\"\n",
                escape(&node.name)
            ));
            let material = mesh.primitives.get(offset).and_then(|primitive| {
                primitive
                    .material
                    .and_then(|index| material_ids.get(index))
                    .map(String::as_str)
            });
            write_mesh_component(out, depth + 2, mesh_id, material);
            out.push_str(&format!("{pad}    Transform\n"));
            out.push_str(&format!("{pad}      rotation 0.0 0.0 0.0\n"));
            out.push_str(&format!("{pad}      scale 1.0 1.0 1.0\n"));
            out.push_str(&format!("{pad}      translation 0.0 0.0 0.0\n"));
        }
    }

    for child in &node.children {
        out.push('\n');
        write_node(
            out,
            document,
            *child,
            depth + 1,
            mesh_ids,
            material_ids,
            taken,
        );
    }

    if depth == 0 {
        out.push('\n');
    }
}

fn write_mesh_component(out: &mut String, depth: usize, mesh_id: &str, material: Option<&str>) {
    let pad = "  ".repeat(depth);
    out.push_str(&format!("{pad}Mesh\n"));
    out.push_str(&format!(
        "{pad}  material \"{}\"\n",
        material.unwrap_or_default()
    ));
    out.push_str(&format!("{pad}  mesh \"{mesh_id}\"\n"));
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn names_from_authoring_tools_become_usable_ids() {
        // Blender names things `Cube.001` and `Wall Segment` constantly, and an asset id is written
        // as a bare word in a scene file.
        assert_eq!(sanitise("Cube.001"), "cube_001");
        assert_eq!(sanitise("Wall Segment"), "wall_segment");
        assert_eq!(sanitise("  ...  "), "");
        assert_eq!(sanitise("Chair"), "chair");
    }

    #[test]
    fn duplicate_names_are_disambiguated_rather_than_colliding() {
        // glTF does not require names to be unique. Two meshes called `Cube` would otherwise produce
        // two files claiming one id, which the asset scanner refuses outright — so an import that
        // did not do this would hand back a project that will not even scan.
        let mut taken = BTreeSet::new();
        assert_eq!(distinct("cube".into(), &mut taken), "cube");
        assert_eq!(distinct("cube".into(), &mut taken), "cube_2");
        assert_eq!(distinct("cube".into(), &mut taken), "cube_3");
    }

    #[test]
    fn an_unnamed_thing_still_gets_an_id() {
        let mut taken = BTreeSet::new();
        assert_eq!(distinct(String::new(), &mut taken), "unnamed");
    }

    #[test]
    fn a_quote_in_a_name_cannot_break_the_file() {
        // A name is written into a quoted string in the scene file, so an unescaped quote would
        // produce text that no longer parses — from data an authoring tool chose, not the author.
        assert_eq!(escape(r#"say "hi""#), r#"say \"hi\""#);
    }
}
