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

mod launch;
mod project;

use amadeo_agent::Json;
use anyhow::{Context, Result, bail};
use launch::{ask_once, request};
use project::Project;
use std::path::PathBuf;

/// What the user asked for.
#[derive(Debug)]
enum Command {
    /// The schema — everything, or one type.
    Describe { type_name: Option<String> },
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
}

/// Options that apply to any command that launches the game.
#[derive(Debug)]
struct Options {
    /// Override the package from `amadeo.toml`.
    package: Option<String>,
    /// How far to simulate before answering.
    ticks: u64,
    /// Print compact JSON rather than indented.
    compact: bool,
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

    let (method, params) = match &command {
        Command::Describe { type_name } => (
            "describe",
            match type_name {
                Some(name) => Json::object([("type", Json::string(name))]),
                None => Json::object([] as [(&str, Json); 0]),
            },
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
        Command::Fmt { .. } => unreachable!("handled above"),
    };

    let here = std::env::current_dir().context("could not read the current directory")?;
    let project = Project::discover(&here)?;
    let package = options.package.clone().unwrap_or(project.game);

    let result = ask_once(
        &project.root,
        &package,
        options.ticks,
        request(method, params, 1),
    )?;

    if options.compact {
        println!("{}", result.to_compact());
    } else {
        print!("{}", result.to_pretty());
    }
    Ok(())
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

/// Splits the arguments into a command and the options around it.
fn parse(arguments: &[String]) -> Result<(Command, Options)> {
    let mut options = Options {
        package: None,
        ticks: 0,
        compact: false,
    };

    // Positional arguments, with the flags stripped out. Flags may appear anywhere, which is what
    // people expect and costs nothing to allow.
    let mut positional: Vec<String> = Vec::new();
    let mut type_name: Option<String> = None;
    let mut stage: Option<String> = None;
    let mut params: Option<String> = None;
    let mut check = false;

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
        "describe" => Command::Describe {
            // Both `amadeo describe Quad` and `amadeo describe --type Quad` work; the first is
            // what anyone types.
            type_name: type_name.or_else(|| rest.first().cloned()),
        },
        "query" => {
            if rest.is_empty() {
                bail!(
                    "`amadeo query` needs at least one component, as in `amadeo query Transform2d`"
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
    describe [type]          the component schema — everything, or one type
    query <component>...     entities carrying all of the named components
    entity <index>           one entity's components
    schedule [stage]         systems in resolved execution order
    status                   tick, state hash, and what is registered
    call <method>            any protocol method
        --params <json>      its arguments, as a JSON object

OPTIONS
    -p, --package <name>     override the game named in amadeo.toml
        --ticks <n>          simulate n ticks before answering (60 = 1 second)
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
            parse_args(&["--ticks", "600", "query", "Transform2d", "--compact"]).expect("parses");

        assert_eq!(options.ticks, 600);
        assert!(options.compact);
        match command {
            Command::Query { components } => assert_eq!(components, vec!["Transform2d"]),
            other => panic!("expected query, got {other:?}"),
        }
    }

    #[test]
    fn query_with_no_components_says_what_to_type() {
        let error = parse_args(&["query"]).expect_err("needs a component");
        assert!(
            error.to_string().contains("amadeo query Transform2d"),
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
    fn the_usage_text_keeps_the_two_column_split_visible() {
        // ADR 0016's split is the thing a reader most needs to understand about this CLI.
        assert!(USAGE.contains("RUNS HERE"));
        assert!(USAGE.contains("RUNS IN THE GAME"));
    }
}
