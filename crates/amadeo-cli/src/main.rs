//! `amadeo` — the command line for the engine.
//!
//! # The split that shapes this whole binary
//!
//! ADR 0011 compiles game logic into the game binary. So this program **cannot know a real
//! project's components**: they are Rust types it has never linked. ADR 0016 draws the line:
//!
//! | Command | Runs |
//! |---|---|
//! | `fmt` | here — pure syntax, no schema needed |
//! | `describe`, `query`, `entity`, `schedule`, `status`, `call` | in the game, over JSON-RPC |
//!
//! Everything in the second row launches `cargo run -p <game> -- --amadeo-agent`, asks one
//! question, prints the answer, and exits. One invocation is one fresh deterministic run, so the
//! same command twice gives the same answer twice — which is what lets a question an agent asks
//! become a test it writes.
//!
//! Anything this can do, the RPC can do, because this *is* the RPC (invariant I5). There is no
//! privileged path, here or in the editor later.

mod gltf_import;
mod launch;
mod project;

use amadeo_agent::Json;
use amadeo_assets::ImportPlan;
use anyhow::{Context, Result, bail};
use launch::{ask_once, request};
use project::Project;
use std::path::{Path, PathBuf};

/// What the user asked for.
#[derive(Debug)]
enum Command {
    /// The schema — everything, or one type.
    Describe { type_name: Option<String> },
    /// A worked example of one type: the scene spelling and the JSON spelling.
    DescribeExample { type_name: String },
    /// Entities carrying all of the named components.
    Query { components: Vec<String> },
    /// One entity's components, by slot index.
    Entity { index: i64 },
    /// Systems in resolved execution order.
    Schedule { stage: Option<String> },
    /// Tick, state hash, and what is registered.
    Status,
    /// Any method, with raw params. The escape hatch, so the CLI never lags the protocol.
    Call { method: String, params: String },
    /// Canonically format scene files in place.
    Fmt { paths: Vec<PathBuf>, check: bool },
    /// Validate scene files against the game's real component schema.
    Check { paths: Vec<PathBuf> },
    /// Replay a recording in a fresh process and verify its checkpoint hashes.
    Replay { path: PathBuf },
    /// List every asset id, the file behind it, and anything not yet importable.
    Assets,
    /// Write a sidecar for every asset file that has none.
    ///
    /// `assets` names the directory directly, so a project whose game will not start can still be
    /// repaired (Q19). Without it the game is asked, which is authoritative.
    Import { check: bool, assets: Option<String> },
    /// Turn a glTF file into engine text: a scene, materials, and a mesh file per primitive.
    ///
    /// Geometry stays in the source file; what becomes text is what people author (ADR 0039).
    ImportGltf {
        path: PathBuf,
        out: Option<PathBuf>,
        dry_run: bool,
    },
    /// Capture the world to a `.snapshot` file.
    Snapshot { path: PathBuf },
    /// Render the world offscreen and write it as a PNG.
    Capture {
        path: PathBuf,
        width: Option<i64>,
        height: Option<i64>,
    },
}

/// Options that apply to any command that launches the game.
#[derive(Debug)]
struct Options {
    /// Override the package from `amadeo.toml`.
    package: Option<String>,
    /// How far to simulate before answering.
    ticks: u64,
    /// A `.snapshot` file to restore before anything runs.
    ///
    /// Composes with `ticks`: restore to the recorded moment, then run that many more. Replacing
    /// re-simulation with a file read is the whole point (ADR 0028).
    from: Option<String>,
    /// Print compact JSON rather than indented.
    compact: bool,
}

impl Options {
    /// The launch arguments these options imply, after `--amadeo-agent`.
    ///
    /// Both are *launch* arguments rather than methods, and for the same reason: they decide what
    /// the world is before the first question can be asked (ADR 0016).
    fn launch_args(&self) -> Vec<String> {
        let mut args = vec!["--ticks".to_string(), self.ticks.to_string()];
        if let Some(path) = &self.from {
            args.push("--snapshot".to_string());
            // Made absolute here so there is no question which directory it resolves against: the
            // game is launched with its working directory set to the project root, which is not
            // necessarily where the user typed the command.
            let absolute = std::path::absolute(path)
                .map_or_else(|_| path.clone(), |p| p.display().to_string());
            args.push(absolute);
        }
        args
    }
}

fn main() -> Result<()> {
    let arguments: Vec<String> = std::env::args().skip(1).collect();

    if arguments.is_empty() || arguments[0] == "--help" || arguments[0] == "-h" {
        print!("{USAGE}");
        return Ok(());
    }

    let (command, options) = parse(&arguments)?;
    run(command, &options)
}

fn run(command: Command, options: &Options) -> Result<()> {
    // `fmt` is the one command that needs no game, so it never pays for a build.
    if let Command::Fmt { paths, check } = command {
        return format_scenes(&paths, check);
    }

    // `check` needs the game — validating a component name means knowing which ones exist — but it
    // reads the files here, and sends their text. See `scene.check` in the protocol.
    if let Command::Check { paths } = command {
        return check_scenes(&paths, options);
    }

    if let Command::Replay { path } = command {
        return replay(&path, options);
    }

    if let Command::Assets = command {
        return list_assets(options);
    }

    if let Command::Import { check, assets } = command {
        return import_assets(check, assets.as_deref(), options);
    }

    // Standalone, like `fmt`: it reads a file and writes text files, and never needs to ask a game
    // anything. That is what lets it run against a project whose game does not compile yet — which
    // is the state a project is in *while* someone is importing art into it.
    if let Command::ImportGltf { path, out, dry_run } = command {
        let imported = gltf_import::import_gltf(&path, out.as_deref(), dry_run)?;
        for written in &imported.written {
            println!(
                "{} {}",
                if dry_run { "would write" } else { "wrote" },
                written.display()
            );
        }
        println!(
            "\n{} file(s) from {}. The source keeps its geometry and is now asset id `{}`.",
            imported.written.len(),
            path.display(),
            imported.source_id
        );
        return Ok(());
    }

    if let Command::Snapshot { path } = command {
        return take_snapshot(&path, options);
    }

    if let Command::Capture {
        path,
        width,
        height,
    } = command
    {
        return capture(&path, width, height, options);
    }

    let (method, params) = match &command {
        Command::Describe { type_name } => (
            "describe",
            match type_name {
                Some(name) => Json::object([("type", Json::string(name))]),
                None => Json::object([] as [(&str, Json); 0]),
            },
        ),
        Command::DescribeExample { type_name } => (
            "describe.example",
            Json::object([("type", Json::string(type_name))]),
        ),
        Command::Query { components } => (
            "world.query",
            Json::object([(
                "components",
                Json::Array(components.iter().map(Json::string).collect()),
            )]),
        ),
        Command::Entity { index } => (
            "world.entity",
            Json::object([("entity", Json::Int(*index))]),
        ),
        Command::Schedule { stage } => (
            "schedule.list",
            match stage {
                Some(name) => Json::object([("stage", Json::string(name))]),
                None => Json::object([] as [(&str, Json); 0]),
            },
        ),
        Command::Status => ("sim.status", Json::object([] as [(&str, Json); 0])),
        Command::Call { method, params } => {
            let parsed = Json::parse(params)
                .map_err(|error| anyhow::anyhow!("--params is not valid JSON: {error}"))?;
            if !matches!(parsed, Json::Object(_)) {
                bail!("--params must be a JSON object, such as '{{\"type\":\"Quad\"}}'");
            }
            (method.as_str(), parsed)
        }
        Command::Fmt { .. }
        | Command::Check { .. }
        | Command::Replay { .. }
        | Command::Assets
        | Command::Import { .. }
        | Command::ImportGltf { .. }
        | Command::Snapshot { .. }
        | Command::Capture { .. } => {
            unreachable!("handled above")
        }
    };

    let here = std::env::current_dir().context("could not read the current directory")?;
    let project = Project::discover(&here)?;
    let package = options.package.clone().unwrap_or(project.game);

    let result = ask_once(
        &project.root,
        &package,
        &options.launch_args(),
        request(method, params, 1),
    )?;

    if options.compact {
        println!("{}", result.to_compact());
    } else {
        print!("{}", result.to_pretty());
    }
    Ok(())
}

/// Replays a recording in a fresh process and checks every checkpoint hash.
///
/// This is the **separate-process** half of the golden-replay mechanism. The in-process test proves
/// a recording survives a rebuild; this proves it survives a fresh process, which is the stronger
/// claim and is what M0's exit gate asked for.
///
/// The seed goes on the command line rather than over the protocol, because the game fixes its seed
/// when it builds its `App` — which happens before the agent handover is reached.
fn replay(path: &PathBuf, options: &Options) -> Result<()> {
    let text = std::fs::read_to_string(path)
        .with_context(|| format!("could not read {}", path.display()))?;

    // Parsed here as well as in the game, so a malformed file costs a clear error instead of a
    // build. The seed also has to be known before launching, which is the other reason.
    let seed = seed_of_replay(&text).with_context(|| {
        format!(
            "{}: no `seed` line; is this an .replay file?",
            path.display()
        )
    })?;

    // Absolute, so it resolves the same whatever directory the game is launched in.
    let absolute = std::fs::canonicalize(path)
        .with_context(|| format!("could not resolve {}", path.display()))?;

    let here = std::env::current_dir().context("could not read the current directory")?;
    let project = Project::discover(&here)?;
    let package = options.package.clone().unwrap_or(project.game);

    let mut session = launch::Session::start_with(
        &project.root,
        &package,
        &[
            "--replay".to_string(),
            absolute.to_string_lossy().into_owned(),
            "--seed".to_string(),
            seed.to_string(),
        ],
    )?;

    let replies = session.ask(&[launch::request(
        "replay.status",
        Json::object([] as [(&str, Json); 0]),
        1,
    )])?;

    let result = launch::unwrap_reply(replies.into_iter().next().context("no reply")?)?;
    let Json::Object(status) = &result else {
        bail!("unexpected reply: {}", result.to_compact());
    };

    let checked = match status.get("checked") {
        Some(Json::Int(value)) => *value,
        _ => 0,
    };
    let ticks = match status.get("ticks") {
        Some(Json::Int(value)) => *value,
        _ => 0,
    };
    let passed = matches!(status.get("passed"), Some(Json::Bool(true)));

    if passed {
        // A replay with no checkpoints proves nothing, and silently "passing" it is exactly the
        // kind of green test that hides a regression for months.
        if checked == 0 {
            bail!(
                "{}: replayed {ticks} ticks, but the file has no checkpoint lines, so nothing was \
                 verified. A replay without checkpoints is not a test",
                path.display()
            );
        }
        println!(
            "ok  {} — {ticks} ticks, {checked} checkpoint(s) matched, seed {seed}",
            path.display()
        );
        return Ok(());
    }

    if let Some(Json::Array(mismatches)) = status.get("mismatches") {
        for mismatch in mismatches {
            let Json::Object(fields) = mismatch else {
                continue;
            };
            let at = match fields.get("tick") {
                Some(Json::Int(value)) => *value,
                _ => 0,
            };
            let expected = match fields.get("expected") {
                Some(Json::String(text)) => text.as_str(),
                _ => "?",
            };
            let found = match fields.get("found") {
                Some(Json::String(text)) => text.as_str(),
                _ => "?",
            };
            eprintln!(
                "{}: tick {at}: expected {expected}, got {found}",
                path.display()
            );
        }
    }

    bail!(
        "{}: the simulation no longer reproduces this recording.\n\
         Find out WHY before regenerating it — this usually means behaviour changed, which \
         invalidates every other recorded replay too.",
        path.display()
    )
}

/// Reads the `seed` line out of a `.replay` file.
///
/// Only the seed, deliberately: the CLI has no business understanding the rest of the format, and
/// the game parses it properly. See `amadeo_input::Recording` for the real parser.
fn seed_of_replay(text: &str) -> Option<u64> {
    text.lines()
        .find_map(|line| line.trim().strip_prefix("seed "))
        .and_then(|value| value.trim().parse().ok())
}

/// Validates scene files against the game's registry, reporting every problem in one pass.
///
/// One launch for all the files, not one per file — a build per scene would make checking a
/// directory unusable.
fn check_scenes(paths: &[PathBuf], options: &Options) -> Result<()> {
    if paths.is_empty() {
        bail!("`amadeo check` needs at least one file. Try `amadeo check scenes/*.scene`");
    }

    let here = std::env::current_dir().context("could not read the current directory")?;
    let project = Project::discover(&here)?;
    let package = options.package.clone().unwrap_or(project.game);

    let mut sources = Vec::with_capacity(paths.len());
    let mut requests = Vec::with_capacity(paths.len());

    for (index, path) in paths.iter().enumerate() {
        let text = std::fs::read_to_string(path)
            .with_context(|| format!("could not read {}", path.display()))?;

        requests.push(launch::request(
            "scene.check",
            Json::object([("text", Json::string(&text))]),
            index as i64 + 1,
        ));
        sources.push(text);
    }

    let mut session = launch::Session::start(&project.root, &package, &options.launch_args())?;
    let replies = session.ask(&requests)?;

    let mut bad_files = 0;

    for ((path, source), reply) in paths.iter().zip(&sources).zip(replies) {
        let result =
            launch::unwrap_reply(reply).with_context(|| format!("checking {}", path.display()))?;

        let Json::Object(members) = &result else {
            bail!(
                "{}: unexpected reply {}",
                path.display(),
                result.to_compact()
            );
        };

        let diagnostics = match members.get("diagnostics") {
            Some(Json::Array(items)) => items,
            _ => bail!("{}: reply carried no diagnostics", path.display()),
        };

        if diagnostics.is_empty() {
            println!("ok       {}", path.display());
            continue;
        }

        bad_files += 1;
        for diagnostic in diagnostics {
            let Json::Object(fields) = diagnostic else {
                continue;
            };

            let message = match fields.get("message") {
                Some(Json::String(text)) => text.as_str(),
                _ => "(no message)",
            };
            let entity = match fields.get("entity") {
                Some(Json::String(text)) => Some(text.as_str()),
                _ => None,
            };
            let component = match fields.get("component") {
                Some(Json::String(text)) => Some(text.as_str()),
                _ => None,
            };

            // A syntax error already carries its line. A schema error carries an entity id, and the
            // line is recovered here because the CLI is the side that still has the file.
            let line = match fields.get("line") {
                Some(Json::Int(value)) => Some(*value as usize),
                _ => entity.and_then(|id| line_of_entity(source, id)),
            };

            let where_ = match line {
                Some(line) => format!("{}:{line}", path.display()),
                None => path.display().to_string(),
            };
            // The component is deliberately not repeated here when the message already names it,
            // which it does for every schema error. Three copies of `Transform` in one line is
            // noise, and noise is what stops people reading diagnostics.
            let names_component =
                component.is_some_and(|name| message.contains(&format!("`{name}`")));
            let what = match (entity, component) {
                (Some(entity), Some(component)) if !names_component => {
                    format!("entity `{entity}`, `{component}`: ")
                }
                (Some(entity), _) => format!("entity `{entity}`: "),
                _ => String::new(),
            };

            eprintln!("{where_}: {what}{message}");
        }
    }

    if bad_files > 0 {
        bail!("{bad_files} of {} file(s) have problems", paths.len());
    }
    Ok(())
}

/// Finds the 1-based line declaring `entity <id>`, so a diagnostic can point at it.
///
/// Done by scanning the source rather than by the document carrying positions: a `SceneDocument` is
/// compared for equality in the round-trip test, and hanging source positions off it would make two
/// identical scenes from different files unequal.
fn line_of_entity(source: &str, id: &str) -> Option<usize> {
    source
        .lines()
        .position(|line| {
            let trimmed = line.trim_start();
            trimmed
                .strip_prefix("entity ")
                .and_then(|rest| rest.split_whitespace().next())
                == Some(id)
        })
        .map(|index| index + 1)
}

/// Rewrites scene files in their canonical form, or checks that they already are.
///
/// This is invariant I2 made operational: `amadeo fmt` is the single formatting authority, so an
/// editor save and a hand edit produce the same bytes and diffs stay reviewable.
fn format_scenes(paths: &[PathBuf], check_only: bool) -> Result<()> {
    if paths.is_empty() {
        bail!("`amadeo fmt` needs at least one file. Try `amadeo fmt scenes/*.scene`");
    }

    let mut unformatted = Vec::new();

    for path in paths {
        let original = std::fs::read_to_string(path)
            .with_context(|| format!("could not read {}", path.display()))?;

        let document = amadeo_scene::parse(&original)
            .with_context(|| format!("{}: could not parse", path.display()))?;
        let canonical = amadeo_scene::to_text(&document);

        if canonical == original {
            continue;
        }

        if check_only {
            unformatted.push(path.clone());
        } else {
            std::fs::write(path, &canonical)
                .with_context(|| format!("could not write {}", path.display()))?;
            println!("formatted {}", path.display());
        }
    }

    if !unformatted.is_empty() {
        for path in &unformatted {
            eprintln!("not formatted: {}", path.display());
        }
        bail!(
            "{} file(s) are not canonically formatted. Run `amadeo fmt` without --check to fix them",
            unformatted.len()
        );
    }

    Ok(())
}

/// Asks the game for its asset catalogue and prints it.
///
/// ADR 0020 requires this to exist before ids are used as the reference syntax, so that authoring a
/// scene means *looking up* an id rather than guessing one. It goes through the game rather than
/// scanning here for the same reason `describe` does: the game is the process that knows where its
/// assets are, and a second implementation in the CLI could disagree with it.
///
/// Rendered as a table rather than dumped as JSON, because a listing is the one thing here a human
/// reads straight through. `--compact` still gives the raw reply for a script.
fn list_assets(options: &Options) -> Result<()> {
    let reply = ask_game(
        "assets.list",
        Json::object([] as [(&str, Json); 0]),
        options,
    )?;

    if options.compact {
        println!("{}", reply.to_compact());
        return Ok(());
    }

    let Json::Object(result) = &reply else {
        bail!("assets.list replied with something that is not an object");
    };

    if result.get("installed") == Some(&Json::Bool(false)) {
        println!("This game has no asset catalogue.");
        if let Some(Json::String(note)) = result.get("note") {
            println!("{note}.");
        }
        return Ok(());
    }

    if let (Some(Json::String(root)), Some(Json::String(anchor))) =
        (result.get("root"), result.get("root_anchor"))
    {
        // Where it looked and why. "Looked in the wrong place" and "the files are missing" have
        // identical symptoms, so the listing always says which directory it is describing.
        println!("root  {root}  ({anchor})");
        println!();
    }

    let assets = match result.get("assets") {
        Some(Json::Array(items)) => items.clone(),
        _ => Vec::new(),
    };

    if assets.is_empty() {
        println!("No assets are catalogued.");
    } else {
        // Column widths from the data, so ids and paths line up without a fixed guess that is
        // either too narrow or mostly blank.
        let width = assets
            .iter()
            .filter_map(|asset| string_field(asset, "id"))
            .map(str::len)
            .max()
            .unwrap_or(2)
            .max(2);

        // `resident` means the bytes are in memory; `catalogued` means the engine knows the id but
        // has not been asked to load it. Both are normal — a scene loads what it declares, not the
        // whole project.
        println!("{:<width$}  {:<10}  SOURCE", "ID", "STATE", width = width);
        for asset in &assets {
            let id = string_field(asset, "id").unwrap_or("?");
            let state = string_field(asset, "state").unwrap_or("?");
            let source = string_field(asset, "source").unwrap_or("?");
            println!("{id:<width$}  {state:<10}  {source}", width = width);
        }
        println!();
        println!("{} asset(s)", assets.len());
    }

    // Anything that failed to load. ADR 0021 makes a missing asset survivable — the game draws a
    // placeholder and keeps running — so this listing is the only place it is visible at all.
    if let Some(Json::Array(failures)) = result.get("failures")
        && !failures.is_empty()
    {
        println!();
        println!("FAILED TO LOAD");
        for failure in failures {
            if let Some(message) = string_field(failure, "message") {
                println!("    {message}");
            }
        }
    }

    print_paths(
        result.get("unimported"),
        "NOT IMPORTED (no .ama-meta sidecar, so nothing can refer to them)",
        Some("Run `amadeo import` to give each one an id from its filename."),
    );
    print_paths(
        result.get("orphaned"),
        "ORPHANED SIDECARS (the asset file they describe is gone)",
        Some("Delete them, or restore the files they name."),
    );

    Ok(())
}

/// Writes a sidecar for every asset file that has none.
///
/// The root comes from the game, via `assets.list`, but the *writing* happens here. That is the same
/// division `scene.check` uses: the game knows things the CLI cannot, and the CLI touches the
/// filesystem the game has no business touching.
/// Where the game keeps its assets, **without launching it when the caller says where**.
///
/// # Why this exists — Q19
///
/// `amadeo import` writes the `.ama-meta` sidecar an asset needs before anything can name it. It
/// learned the asset directory by launching the game and calling `assets.list`, which became a
/// deadlock the moment prefabs became assets (ADR 0029): the game refuses to start while an asset its
/// scene names has no sidecar, so **the tool that fixes the problem could not run**. The Vault's two
/// prefab sidecars had to be written by hand.
///
/// Importing is a filesystem operation over a directory, and the only thing the game supplied was the
/// directory's name. `--assets <dir>` supplies it directly, which breaks the cycle and makes `import`
/// work on a project that does not currently compile — a good property for a repair tool.
///
/// Asking the game stays the default, because it is authoritative: the path is a constant in the
/// game's own source, so nothing can disagree with it. The flag is the escape hatch for exactly the
/// case where the game will not start.
fn asset_directory(explicit: Option<&str>, options: &Options) -> Result<PathBuf> {
    if let Some(directory) = explicit {
        let path = PathBuf::from(directory);
        if !path.is_dir() {
            bail!("--assets {directory}: not a directory");
        }
        return Ok(path);
    }

    let reply = ask_game(
        "assets.list",
        Json::object([] as [(&str, Json); 0]),
        options,
    )?;

    let Json::Object(result) = &reply else {
        bail!("assets.list replied with something that is not an object");
    };

    let Some(Json::String(root)) = result.get("root") else {
        bail!(
            "this game has no asset directory, so there is nothing to import into. \
             A game scans one with `App::scan_assets(\"assets\")` before it runs"
        );
    };

    Ok(PathBuf::from(root))
}

fn import_assets(check: bool, assets: Option<&str>, options: &Options) -> Result<()> {
    let root = asset_directory(assets, options)?;
    let plan = ImportPlan::prepare(&root)?;

    if plan.is_empty() {
        println!(
            "Nothing to import — all {} asset(s) already have a sidecar.",
            plan.already_imported
        );
        return Ok(());
    }

    for planned in &plan.sidecars {
        println!(
            "{}  {}  ->  {}",
            if check { "would create" } else { "created    " },
            planned.asset.display(),
            planned.id
        );
    }

    if check {
        // Same shape as `fmt --check`: report and fail, so it can gate a commit.
        bail!(
            "{} asset(s) have no sidecar. Run `amadeo import` to create them",
            plan.sidecars.len()
        );
    }

    let written = plan.apply()?;
    println!();
    println!("{} sidecar(s) written.", written.len());
    Ok(())
}

/// Launches the game, asks one question, and returns the result.
///
/// A thin wrapper over [`ask_once`] that does the project discovery every game command repeats.
fn ask_game(method: &str, params: Json, options: &Options) -> Result<Json> {
    let here = std::env::current_dir().context("could not read the current directory")?;
    let project = Project::discover(&here)?;
    let package = options.package.clone().unwrap_or(project.game);

    ask_once(
        &project.root,
        &package,
        &options.launch_args(),
        request(method, params, 1),
    )
}

/// Captures the world and writes it to a file.
///
/// The game produces the text and the CLI writes it — the same division `amadeo check` and
/// `amadeo import` use, and the reason `snapshot.take` returns a string rather than a path
/// (ADR 0016: the game knows what the world is, the CLI is the side that touches the filesystem).
/// Renders the world offscreen and writes a PNG.
///
/// **The agent's eyes** (ADR 0021). Unlike `snapshot`, the file is written by the *game* rather than
/// here: the image is hundreds of kilobytes, and shipping it back through the JSON reply as base64
/// would make a transcript unreadable for no gain. So the path is sent and the game writes it.
///
/// That is why the path is made absolute first — the game is launched with its working directory at
/// the project root, which is not necessarily where the command was typed.
fn capture(path: &Path, width: Option<i64>, height: Option<i64>, options: &Options) -> Result<()> {
    let here = std::env::current_dir().context("could not read the current directory")?;
    let project = Project::discover(&here)?;
    let package = options.package.clone().unwrap_or(project.game);

    let absolute = std::path::absolute(path).unwrap_or_else(|_| path.to_path_buf());
    let mut params = vec![("path", Json::string(absolute.display().to_string()))];
    if let Some(width) = width {
        params.push(("width", Json::Int(width)));
    }
    if let Some(height) = height {
        params.push(("height", Json::Int(height)));
    }

    let result = ask_once(
        &project.root,
        &package,
        &options.launch_args(),
        request("render.capture", Json::object(params), 1),
    )?;

    let width = number_field(&result, "width").unwrap_or(0);
    let height = number_field(&result, "height").unwrap_or(0);
    let bytes = number_field(&result, "bytes").unwrap_or(0);
    let tick = number_field(&result, "tick").unwrap_or(0);
    let drawn = number_field(&result, "drawn").unwrap_or(0);

    // `drawn` is reported because "the file is tiny and the world is empty" and "the file is tiny
    // and something is wrong" look identical otherwise.
    println!(
        "wrote {} — {width}x{height}, {bytes} bytes, tick {tick}, {drawn} drawable entities",
        path.display()
    );
    Ok(())
}

fn take_snapshot(path: &Path, options: &Options) -> Result<()> {
    let here = std::env::current_dir().context("could not read the current directory")?;
    let project = Project::discover(&here)?;
    let package = options.package.clone().unwrap_or(project.game);

    let result = ask_once(
        &project.root,
        &package,
        &options.launch_args(),
        request("snapshot.take", Json::object([] as [(&str, Json); 0]), 1),
    )?;

    let text = string_field(&result, "text")
        .context("the game's reply had no snapshot text in it, which is an engine bug")?;

    std::fs::write(path, text)
        .with_context(|| format!("could not write the snapshot to {}", path.display()))?;

    let entities = number_field(&result, "entities").unwrap_or(0);
    let resources = number_field(&result, "resources").unwrap_or(0);
    let tick = number_field(&result, "tick").unwrap_or(0);
    let hash = string_field(&result, "state_hash").unwrap_or("?");

    println!(
        "wrote {} — tick {tick}, {entities} entities, {resources} resources, state {hash}",
        path.display()
    );
    println!("Restore it with: amadeo status --from {}", path.display());
    Ok(())
}

/// An integer field out of a JSON object, or `None` if it is missing or not a number.
fn number_field(value: &Json, name: &str) -> Option<i64> {
    let Json::Object(members) = value else {
        return None;
    };
    match members.get(name) {
        Some(Json::Int(number)) => Some(*number),
        _ => None,
    }
}

/// A string field out of a JSON object, or `None` if it is missing or not a string.
fn string_field<'a>(value: &'a Json, name: &str) -> Option<&'a str> {
    let Json::Object(members) = value else {
        return None;
    };
    match members.get(name) {
        Some(Json::String(text)) => Some(text),
        _ => None,
    }
}

/// Prints a titled list of paths, or nothing at all when the list is empty.
fn print_paths(value: Option<&Json>, title: &str, hint: Option<&str>) {
    let Some(Json::Array(items)) = value else {
        return;
    };
    if items.is_empty() {
        return;
    }

    println!();
    println!("{title}");
    for item in items {
        if let Json::String(path) = item {
            println!("    {path}");
        }
    }
    if let Some(hint) = hint {
        println!("{hint}");
    }
}

/// Splits the arguments into a command and the options around it.
fn parse(arguments: &[String]) -> Result<(Command, Options)> {
    let mut options = Options {
        package: None,
        ticks: 0,
        from: None,
        compact: false,
    };

    // Positional arguments, with the flags stripped out. Flags may appear anywhere, which is what
    // people expect and costs nothing to allow.
    let mut positional: Vec<String> = Vec::new();
    let mut type_name: Option<String> = None;
    let mut stage: Option<String> = None;
    let mut params: Option<String> = None;
    let mut check = false;
    let mut example = false;
    let mut assets_dir: Option<String> = None;
    let mut width: Option<i64> = None;
    let mut height: Option<i64> = None;

    let mut index = 0;
    while index < arguments.len() {
        let argument = &arguments[index];

        // Reads the value after a flag, or explains what was expected.
        let value_after = |flag: &str| -> Result<String> {
            arguments
                .get(index + 1)
                .cloned()
                .with_context(|| format!("{flag} needs a value"))
        };

        match argument.as_str() {
            "--package" | "-p" => {
                options.package = Some(value_after("--package")?);
                index += 2;
            }
            "--ticks" => {
                let raw = value_after("--ticks")?;
                options.ticks = raw.parse().with_context(|| {
                    format!("`{raw}` is not a tick count; --ticks takes a whole number (60 per simulated second)")
                })?;
                index += 2;
            }
            "--from" => {
                options.from = Some(value_after("--from")?);
                index += 2;
            }
            "--type" => {
                type_name = Some(value_after("--type")?);
                index += 2;
            }
            "--stage" => {
                stage = Some(value_after("--stage")?);
                index += 2;
            }
            "--params" => {
                params = Some(value_after("--params")?);
                index += 2;
            }
            "--compact" => {
                options.compact = true;
                index += 1;
            }
            "--check" => {
                check = true;
                index += 1;
            }
            "--example" => {
                example = true;
                index += 1;
            }
            "--assets" => {
                assets_dir = Some(value_after("--assets")?);
                index += 2;
            }
            "--width" => {
                width = Some(
                    value_after("--width")?
                        .parse()
                        .context("--width must be a whole number of pixels")?,
                );
                index += 2;
            }
            "--height" => {
                height = Some(
                    value_after("--height")?
                        .parse()
                        .context("--height must be a whole number of pixels")?,
                );
                index += 2;
            }
            other if other.starts_with('-') => {
                bail!("unknown option `{other}`. Run `amadeo --help` for what there is")
            }
            other => {
                positional.push(other.to_string());
                index += 1;
            }
        }
    }

    let (name, rest) = positional
        .split_first()
        .context("expected a command. Run `amadeo --help` for what there is")?;

    let command = match name.as_str() {
        "describe" => {
            // Both `amadeo describe Quad` and `amadeo describe --type Quad` work; the first is
            // what anyone types.
            let named = type_name.or_else(|| rest.first().cloned());
            if example {
                Command::DescribeExample {
                    type_name: named.context(
                        "`--example` needs a type, as in `amadeo describe Transform --example`. \
                         There is no example of the whole schema",
                    )?,
                }
            } else {
                Command::Describe { type_name: named }
            }
        }
        "query" => {
            if rest.is_empty() {
                bail!(
                    "`amadeo query` needs at least one component, as in `amadeo query Transform`"
                );
            }
            Command::Query {
                components: rest.to_vec(),
            }
        }
        "entity" => {
            let raw = rest
                .first()
                .context("`amadeo entity` needs a slot index, as in `amadeo entity 5`")?;
            Command::Entity {
                index: raw
                    .parse()
                    .with_context(|| format!("`{raw}` is not an entity index"))?,
            }
        }
        "schedule" => Command::Schedule {
            stage: stage.or_else(|| rest.first().cloned()),
        },
        "status" => Command::Status,
        "call" => {
            let method = rest
                .first()
                .context("`amadeo call` needs a method name, as in `amadeo call sim.status`")?;
            Command::Call {
                method: method.clone(),
                params: params.unwrap_or_else(|| "{}".to_string()),
            }
        }
        "fmt" => Command::Fmt {
            paths: rest.iter().map(PathBuf::from).collect(),
            check,
        },
        "check" => Command::Check {
            paths: rest.iter().map(PathBuf::from).collect(),
        },
        "assets" => Command::Assets,
        "import" => Command::Import {
            check,
            assets: assets_dir,
        },
        "import-gltf" => {
            let Some(path) = rest.first() else {
                bail!(
                    "import-gltf needs a file, as in `amadeo import-gltf games/atrium/assets/models/level.glb`"
                );
            };
            Command::ImportGltf {
                path: PathBuf::from(path),
                // `--assets` is reused as the output directory rather than inventing a second
                // directory flag: it already means "the directory to work in" on `import`.
                out: assets_dir.as_deref().map(PathBuf::from),
                dry_run: check,
            }
        }
        "capture" => {
            let Some(path) = rest.first() else {
                bail!(
                    "capture needs a file to write to, such as `amadeo capture --ticks 60 shot.png`"
                );
            };
            Command::Capture {
                path: PathBuf::from(path),
                width,
                height,
            }
        }
        "snapshot" => {
            let Some(path) = rest.first() else {
                bail!(
                    "snapshot needs a file to write to, such as `amadeo snapshot --ticks 600 \
                     mid-fight.snapshot`"
                );
            };
            Command::Snapshot {
                path: PathBuf::from(path),
            }
        }
        "replay" => Command::Replay {
            path: rest
                .first()
                .map(PathBuf::from)
                .context("`amadeo replay` needs a file, as in `amadeo replay walk.replay`")?,
        },
        other => bail!("unknown command `{other}`. Run `amadeo --help` for what there is"),
    };

    Ok((command, options))
}

/// Written by hand rather than generated, so the two-column split ADR 0016 created is the first
/// thing anyone reads.
const USAGE: &str = "\
amadeo — the Amadeo engine command line

USAGE
    amadeo <command> [options]

RUNS HERE (no game needed)
    fmt <file>...            rewrite scene files canonically
        --check              report unformatted files instead of fixing them

RUNS IN THE GAME (launches it, asks, exits)
    assets                   every asset id and the file behind it
    check <file>...          validate scene files against the real component schema
    describe [type]          the schema — components, resources, and every type they name
        --example            a minimal valid instance of one type, ready to paste
    import                   write a sidecar for each asset file that has none
        --check              report them instead of writing
        --assets <dir>       import into this directory without launching the game,
                             for when the game will not start until the sidecars exist
    query <component>...     entities carrying all of the named components
    entity <index>           one entity's components
    replay <file>            replay a recording and verify its checkpoint hashes
    schedule [stage]         systems in resolved execution order
    status                   tick, state hash, and what is registered
    capture <file>           render the world offscreen and write it as a PNG
        --width <n>          image width in pixels (default 1280)
        --height <n>         image height in pixels (default 720)
    snapshot <file>          capture the world to a .snapshot file
    call <method>            any protocol method
        --params <json>      its arguments, as a JSON object

OPTIONS
    -p, --package <name>     override the game named in amadeo.toml
        --ticks <n>          simulate n ticks before answering (60 = 1 second)
        --from <file>        restore a .snapshot first, then run --ticks more
        --compact            one line of JSON instead of indented
    -h, --help               this

Game commands compile and launch the game, because a game's components are Rust
types in the game binary and this program has never linked them (ADR 0016). Each
invocation is one fresh deterministic run: the same command twice gives the same
answer twice.
";

#[cfg(test)]
mod tests {
    use super::*;

    fn parse_args(arguments: &[&str]) -> Result<(Command, Options)> {
        let owned: Vec<String> = arguments.iter().map(|text| (*text).to_string()).collect();
        parse(&owned)
    }

    #[test]
    fn describe_takes_a_type_either_way() {
        // `amadeo describe Quad` is what anyone types; `--type Quad` is what a script generates.
        for arguments in [vec!["describe", "Quad"], vec!["describe", "--type", "Quad"]] {
            let (command, _) = parse_args(&arguments).expect("parses");
            match command {
                Command::Describe { type_name } => assert_eq!(type_name.as_deref(), Some("Quad")),
                other => panic!("expected describe, got {other:?}"),
            }
        }
    }

    #[test]
    fn flags_may_appear_anywhere() {
        let (command, options) =
            parse_args(&["--ticks", "600", "query", "Transform", "--compact"]).expect("parses");

        assert_eq!(options.ticks, 600);
        assert!(options.compact);
        match command {
            Command::Query { components } => assert_eq!(components, vec!["Transform"]),
            other => panic!("expected query, got {other:?}"),
        }
    }

    #[test]
    fn query_with_no_components_says_what_to_type() {
        let error = parse_args(&["query"]).expect_err("needs a component");
        assert!(
            error.to_string().contains("amadeo query Transform"),
            "got: {error}"
        );
    }

    #[test]
    fn a_bad_tick_count_spells_out_the_units() {
        let error = parse_args(&["status", "--ticks", "soon"]).expect_err("not a number");
        assert!(
            error.to_string().contains("60 per simulated second"),
            "got: {error}"
        );
    }

    #[test]
    fn unknown_commands_and_options_point_at_help() {
        assert!(
            parse_args(&["teleport"])
                .expect_err("no such command")
                .to_string()
                .contains("--help")
        );
        assert!(
            parse_args(&["status", "--turbo"])
                .expect_err("no such option")
                .to_string()
                .contains("--help")
        );
    }

    #[test]
    fn call_defaults_to_empty_params() {
        let (command, _) = parse_args(&["call", "sim.status"]).expect("parses");
        match command {
            Command::Call { method, params } => {
                assert_eq!(method, "sim.status");
                assert_eq!(params, "{}");
            }
            other => panic!("expected call, got {other:?}"),
        }
    }

    #[test]
    fn fmt_collects_paths_and_the_check_flag() {
        let (command, _) = parse_args(&["fmt", "a.scene", "b.scene", "--check"]).expect("parses");
        match command {
            Command::Fmt { paths, check } => {
                assert_eq!(paths.len(), 2);
                assert!(check);
            }
            other => panic!("expected fmt, got {other:?}"),
        }
    }

    #[test]
    fn an_entity_id_resolves_back_to_its_line() {
        // How a schema diagnostic, which only knows an entity id, becomes `file:line`.
        let source = "scene demo\nversion 1\n\nentity a1 \"One\"\n  Position\n    x 1\n  entity a2 \"Two\"\n    Player\n";

        assert_eq!(line_of_entity(source, "a1"), Some(4));
        // Nested entities are indented, so the prefix check has to trim first.
        assert_eq!(line_of_entity(source, "a2"), Some(7));
        assert_eq!(line_of_entity(source, "nope"), None);
    }

    #[test]
    fn an_id_that_merely_prefixes_another_does_not_match_it() {
        // `entity a1` must not resolve to the line declaring `a10`.
        let source = "scene demo\nversion 1\n\nentity a10 \"Ten\"\nentity a1 \"One\"\n";
        assert_eq!(line_of_entity(source, "a1"), Some(5));
        assert_eq!(line_of_entity(source, "a10"), Some(4));
    }

    #[test]
    fn check_collects_paths() {
        let (command, _) = parse_args(&["check", "a.scene", "b.scene"]).expect("parses");
        match command {
            Command::Check { paths } => assert_eq!(paths.len(), 2),
            other => panic!("expected check, got {other:?}"),
        }
    }

    #[test]
    fn the_replay_seed_is_read_before_launching() {
        // The CLI has to know the seed up front: the game fixes it when it builds its App, which
        // happens before the agent handover. So this is read here, not asked for over the protocol.
        let file = "amadeo-replay 1\ntick-rate 60\nseed 1234\nticks 300\n\n0 axis move_x 1.0\n";
        assert_eq!(seed_of_replay(file), Some(1234));
        assert_eq!(seed_of_replay("amadeo-replay 1\nticks 10\n"), None);
        assert_eq!(seed_of_replay("not a replay file at all"), None);
    }

    #[test]
    fn replay_needs_a_file() {
        let error = parse_args(&["replay"]).expect_err("needs a path");
        assert!(
            error.to_string().contains("amadeo replay walk.replay"),
            "got: {error}"
        );
    }

    #[test]
    fn assets_takes_no_arguments() {
        let (command, _) = parse_args(&["assets"]).expect("parses");
        assert!(matches!(command, Command::Assets), "got {command:?}");
    }

    #[test]
    fn snapshot_takes_a_destination_file() {
        let (command, _) = parse_args(&["snapshot", "mid-fight.snapshot"]).expect("parses");
        match command {
            Command::Snapshot { path } => assert_eq!(path, PathBuf::from("mid-fight.snapshot")),
            other => panic!("got {other:?}"),
        }
    }

    #[test]
    fn snapshot_without_a_file_says_what_to_type() {
        let error = parse_args(&["snapshot"]).expect_err("no destination");
        let message = error.to_string();
        assert!(message.contains("amadeo snapshot"), "{message}");
        assert!(message.contains(".snapshot"), "{message}");
    }

    #[test]
    fn from_becomes_a_launch_argument_beside_ticks() {
        // Both are launch arguments rather than methods, and for the same reason: they decide what
        // the world *is* before the first question can be asked (ADR 0016).
        let (_, options) =
            parse_args(&["status", "--from", "saved.snapshot", "--ticks", "30"]).expect("parses");

        let args = options.launch_args();
        assert_eq!(args[0], "--ticks");
        assert_eq!(args[1], "30");
        assert_eq!(args[2], "--snapshot");
        // Made absolute, because the game runs with its working directory at the project root.
        assert!(
            args[3].ends_with("saved.snapshot") && args[3].len() > "saved.snapshot".len(),
            "expected an absolute path, got {}",
            args[3]
        );
    }

    #[test]
    fn without_from_the_launch_arguments_are_just_ticks() {
        let (_, options) = parse_args(&["status"]).expect("parses");
        assert_eq!(options.launch_args(), vec!["--ticks", "0"]);
    }

    #[test]
    fn usage_mentions_snapshotting() {
        assert!(USAGE.contains("snapshot <file>"));
        assert!(USAGE.contains("--from <file>"));
    }

    #[test]
    fn import_takes_an_asset_directory_so_it_need_not_launch_the_game() {
        // Q19: the sidecars `import` writes are what a game needs to *start*, so learning the asset
        // directory by launching the game was a deadlock. `--assets` is the way out, and it is the
        // only argument that makes `import` work on a project that does not compile.
        let (command, _) =
            parse_args(&["import", "--assets", "games/vault/assets"]).expect("parses");
        match command {
            Command::Import { check, assets } => {
                assert!(!check);
                assert_eq!(assets.as_deref(), Some("games/vault/assets"));
            }
            other => panic!("expected import, got {other:?}"),
        }
    }

    #[test]
    fn usage_mentions_the_asset_directory_escape_hatch() {
        // It is only discoverable from `--help`, and it is the answer to a confusing failure.
        assert!(USAGE.contains("--assets <dir>"));
    }

    #[test]
    fn import_carries_the_check_flag() {
        // `--check` means the same thing here as it does for `fmt`: report, do not write, and fail
        // so it can gate a commit.
        let (plain, _) = parse_args(&["import"]).expect("parses");
        assert!(
            matches!(
                plain,
                Command::Import {
                    check: false,
                    assets: None
                }
            ),
            "got {plain:?}"
        );

        let (checked, _) = parse_args(&["import", "--check"]).expect("parses");
        assert!(
            matches!(
                checked,
                Command::Import {
                    check: true,
                    assets: None
                }
            ),
            "got {checked:?}"
        );
    }

    #[test]
    fn the_usage_text_lists_the_asset_commands() {
        // ADR 0020 requires a way to look ids up before they are used as the reference syntax. If it
        // is not in --help, an agent will not find it, which defeats the point.
        assert!(USAGE.contains("assets"));
        assert!(USAGE.contains("import"));
    }

    #[test]
    fn the_usage_text_keeps_the_two_column_split_visible() {
        // ADR 0016's split is the thing a reader most needs to understand about this CLI.
        assert!(USAGE.contains("RUNS HERE"));
        assert!(USAGE.contains("RUNS IN THE GAME"));
    }
}
