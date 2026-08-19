//! Hosting the agent inside the game binary — the other half of ADR 0016.
//!
//! `amadeo-agent` owns the protocol: parsing a request, routing the methods that need only a world
//! and a registry, and shaping a reply. It cannot own the *hosting*, because hosting needs an
//! [`App`] and invariant I6 puts `amadeo-agent` above `amadeo-app` in the crate order. So the loop
//! lives here, one crate down, exactly the way ADR 0010 pushed the window loop up into the game
//! binary for the same reason.
//!
//! # Why this is in the engine and the window loop is not
//!
//! ADR 0010 left windowing to each game because a window is platform knowledge and every game wants
//! a different one. The agent server is the opposite: it must behave *identically* in every game, or
//! an agent has to learn each project separately, which is precisely what invariant I5 forbids. So
//! the game supplies one line and the engine supplies the behaviour.
//!
//! ```no_run
//! # fn build_app() -> amadeo_app::App { amadeo_app::App::new() }
//! fn main() -> Result<(), Box<dyn std::error::Error>> {
//!     let mut app = build_app();
//!
//!     // Hands over to the agent and exits, when launched with `--amadeo-agent`.
//!     if amadeo_app::serve_if_requested(&mut app)? {
//!         return Ok(());
//!     }
//!
//!     // Otherwise the game runs normally.
//!     Ok(())
//! }
//! ```
//!
//! # One request per line, one reply per line
//!
//! Newline-delimited JSON over stdin and stdout. No framing headers, no content lengths: a request
//! can be typed at a prompt and a transcript can be read with `cat`. The protocol is meant to be
//! inspectable by eye (`docs/03-ai-native-design.md`), and a length-prefixed frame is not.
//!
//! Diagnostics go to **stderr**, never stdout, because stdout is the protocol.

use crate::App;
use crate::schedule::Stage;
use amadeo_agent::{Json, Request, RpcError, WORLD_METHODS, dispatch_world, failure, success};
use amadeo_core::Tick;
use std::io::{BufRead, Write};

/// The flag that turns a game binary into an agent host.
pub const AGENT_FLAG: &str = "--amadeo-agent";

/// The flag that says how far to run before answering questions.
pub const TICKS_FLAG: &str = "--ticks";

/// The flag naming a `.replay` file to play instead of running free.
pub const REPLAY_FLAG: &str = "--replay";

/// The flag carrying the seed the app should have been built with.
///
/// See [`requested_seed`] for why a game reads this *before* building.
pub const SEED_FLAG: &str = "--seed";

/// The flag naming a `.snapshot` file to restore before anything runs.
pub const SNAPSHOT_FLAG: &str = "--snapshot";

/// The methods this host answers itself, on top of [`WORLD_METHODS`].
pub const APP_METHODS: &[&str] = &[
    "render.capture",
    "replay.status",
    "profile.frame",
    "scene.check",
    "schedule.list",
    "sim.status",
    "snapshot.take",
];

/// Something went wrong hosting the agent.
#[derive(Debug, thiserror::Error)]
pub enum AgentError {
    /// A command-line argument was malformed.
    #[error("{0}")]
    BadArguments(String),

    /// Running the requested ticks failed.
    #[error("could not run {ticks} ticks before serving: {source}")]
    Simulation {
        /// How many were asked for.
        ticks: u64,
        /// The schedule error underneath.
        #[source]
        source: crate::ScheduleError,
    },

    /// The snapshot file could not be read, parsed, or restored.
    ///
    /// Fatal on purpose, unlike most things in this engine. A snapshot says what the world *is*, so
    /// a failed restore leaves the process holding a world that is neither the recorded one nor a
    /// clean start — and every answer it then gave would be about the wrong moment.
    #[error("could not restore the snapshot `{path}`: {message}")]
    BadSnapshot {
        /// The file that was asked for.
        path: String,
        /// What was wrong with it.
        message: String,
    },

    /// The replay file could not be read or parsed.
    #[error("could not read the replay `{path}`: {message}")]
    BadReplay {
        /// The file that was asked for.
        path: String,
        /// What was wrong with it.
        message: String,
    },

    /// The app was built with a different seed than the replay needs.
    #[error(
        "this replay was recorded with seed {wanted}, but the game built its world with seed \
         {found}. A replay only reproduces against the seed it was made with.\n\
         Fix: have the game read `amadeo_app::requested_seed()` before building its App, and pass \
         that to `App::with_seed`"
    )]
    SeedMismatch {
        /// The seed the recording needs.
        wanted: u64,
        /// The seed the app actually has.
        found: u64,
    },

    /// stdin or stdout failed.
    #[error("agent transport failed: {0}")]
    Transport(#[from] std::io::Error),
}

/// How the agent was asked to run.
#[derive(Debug, Clone, PartialEq, Eq, Default)]
pub struct AgentOptions {
    /// How many ticks to simulate before answering anything.
    ///
    /// This is a launch argument rather than a method on purpose (ADR 0016). Every question is then
    /// asked of a world that got where it is by running a fixed number of ticks from a fixed seed,
    /// so the same command twice gives the same answer twice — and a question an agent asks is a
    /// question it can put in a test.
    ///
    /// Ignored when [`AgentOptions::replay`] is set: a recording says how long it is.
    pub ticks: u64,

    /// A `.replay` file to play instead of running free.
    ///
    /// A **path**, unlike `scene.check`, which takes text. The difference is not an inconsistency:
    /// a replay has to be installed before the first tick, so it cannot arrive as a method
    /// parameter — by the time a method could be called, the ticks have already run. The client
    /// makes the path absolute so there is no question which directory it resolves against.
    pub replay: Option<std::path::PathBuf>,

    /// The seed the app should have been built with. See [`requested_seed`].
    pub seed: Option<u64>,

    /// A `.snapshot` file to restore before anything runs.
    ///
    /// A **launch argument, not a method**, for exactly the reason [`AgentOptions::replay`] is: a
    /// snapshot says what the world *is*, so it has to be installed before the first tick. By the
    /// time a method could be called, the pre-roll has already run and the moment it was meant to
    /// replace is gone.
    ///
    /// Restoring happens **before** [`AgentOptions::ticks`], so the two compose: restore to tick
    /// 900, then run 30 more. That composition is the whole point — it is what turns "get back to
    /// the interesting moment" from 382 ms of re-simulation into a file read.
    pub snapshot: Option<std::path::PathBuf>,
}

/// The seed this process was asked to build its world with, if any.
///
/// # Why a game reads this before building its `App`
///
/// A recording only reproduces against the seed it was made with, and `App::with_seed` fixes the
/// seed at construction — which happens *before* [`serve_if_requested`] is reached. So a game that
/// wants `amadeo replay` to work against it has to ask first:
///
/// ```no_run
/// const DEFAULT_SEED: u64 = 0;
/// let seed = amadeo_app::requested_seed().unwrap_or(DEFAULT_SEED);
/// let mut app = amadeo_app::App::with_seed(seed);
/// ```
///
/// A game that skips this still works for everything else; it just gets a clear
/// [`AgentError::SeedMismatch`] if someone replays a recording made at another seed, rather than a
/// mysterious hash mismatch.
///
/// Deliberately *not* solved by having the host re-seed the app after construction: a world whose
/// construction consumed randomness would then differ from the one that was recorded, and the
/// resulting divergence would look like a real regression.
#[must_use]
pub fn requested_seed() -> Option<u64> {
    let arguments: Vec<String> = std::env::args().skip(1).collect();
    seed_from(&arguments)
}

/// The argument-scanning half of [`requested_seed`], separated so it can be tested.
#[must_use]
pub fn seed_from(arguments: &[String]) -> Option<u64> {
    let position = arguments.iter().position(|a| a == SEED_FLAG)?;
    arguments.get(position + 1)?.parse().ok()
}

/// Reads agent options out of the process arguments.
///
/// Returns `Ok(None)` when [`AGENT_FLAG`] is absent, which is the normal case: the game was
/// launched to be played.
///
/// # Errors
///
/// [`AgentError::BadArguments`] if `--ticks` is present without a valid whole number.
pub fn agent_options() -> Result<Option<AgentOptions>, AgentError> {
    let arguments: Vec<String> = std::env::args().skip(1).collect();
    agent_options_from(&arguments)
}

/// The argument-parsing half of [`agent_options`], separated so it can be tested without a process.
///
/// # Errors
///
/// [`AgentError::BadArguments`] if `--ticks` is present without a valid whole number.
pub fn agent_options_from(arguments: &[String]) -> Result<Option<AgentOptions>, AgentError> {
    if !arguments.iter().any(|argument| argument == AGENT_FLAG) {
        return Ok(None);
    }

    let mut options = AgentOptions::default();
    let mut index = 0;

    while index < arguments.len() {
        match arguments[index].as_str() {
            TICKS_FLAG => {
                let Some(value) = arguments.get(index + 1) else {
                    return Err(AgentError::BadArguments(format!(
                        "{TICKS_FLAG} needs a number, as in `{TICKS_FLAG} 600`"
                    )));
                };
                options.ticks = value.parse().map_err(|_| {
                    AgentError::BadArguments(format!(
                        "`{value}` is not a tick count; {TICKS_FLAG} takes a whole number of ticks \
                         (60 per simulated second)"
                    ))
                })?;
                index += 2;
            }
            REPLAY_FLAG => {
                let Some(value) = arguments.get(index + 1) else {
                    return Err(AgentError::BadArguments(format!(
                        "{REPLAY_FLAG} needs a path to a .replay file"
                    )));
                };
                options.replay = Some(std::path::PathBuf::from(value));
                index += 2;
            }
            SNAPSHOT_FLAG => {
                let Some(value) = arguments.get(index + 1) else {
                    return Err(AgentError::BadArguments(format!(
                        "{SNAPSHOT_FLAG} needs a path to a .snapshot file"
                    )));
                };
                options.snapshot = Some(std::path::PathBuf::from(value));
                index += 2;
            }
            SEED_FLAG => {
                let Some(value) = arguments.get(index + 1) else {
                    return Err(AgentError::BadArguments(format!(
                        "{SEED_FLAG} needs a number"
                    )));
                };
                options.seed = Some(value.parse().map_err(|_| {
                    AgentError::BadArguments(format!(
                        "`{value}` is not a seed; it is a whole number"
                    ))
                })?);
                index += 2;
            }
            _ => index += 1,
        }
    }

    Ok(Some(options))
}

/// Hands the app over to the agent if this process was launched to be one.
///
/// Returns `true` when it served, meaning the caller should exit rather than start a game loop.
/// Returns `false` when the agent flag was absent and the game should run normally.
///
/// # Errors
///
/// [`AgentError`] if the arguments were malformed, the pre-roll ticks failed, or stdio failed.
pub fn serve_if_requested(app: &mut App) -> Result<bool, AgentError> {
    let Some(options) = agent_options()? else {
        return Ok(false);
    };

    let stdin = std::io::stdin();
    let stdout = std::io::stdout();
    serve(app, options, stdin.lock(), stdout.lock())?;
    Ok(true)
}

/// Runs the request/response loop until the input ends.
///
/// Split out from [`serve_if_requested`] so tests can drive it with a string instead of a process.
///
/// # Errors
///
/// [`AgentError`] if the pre-roll ticks failed or the streams failed. A malformed *request* is not
/// an error here — it is answered with an error reply, and the loop continues.
pub fn serve(
    app: &mut App,
    options: AgentOptions,
    input: impl BufRead,
    mut output: impl Write,
) -> Result<(), AgentError> {
    // Restoring comes first, so a snapshot and `--ticks` compose: land on tick 900 from a file, then
    // run 30 more. That composition is the point — it is what replaces re-simulating from zero.
    if let Some(path) = &options.snapshot {
        restore_snapshot(app, path)?;
    }

    // The whole pre-roll happens before the first reply, so every answer describes the same world.
    let replay = match &options.replay {
        Some(path) => Some(play_replay(app, path)?),
        None => {
            if options.ticks > 0 {
                app.run_ticks(options.ticks)
                    .map_err(|source| AgentError::Simulation {
                        ticks: options.ticks,
                        source,
                    })?;
            }
            None
        }
    };

    for line in input.lines() {
        let line = line?;
        if line.trim().is_empty() {
            continue;
        }

        let reply = answer(app, &line, replay.as_ref());
        writeln!(output, "{}", reply.to_compact())?;
        // Flushed per reply: a client that sent one request and is waiting for one answer would
        // otherwise block on a buffer that only empties at exit.
        output.flush()?;
    }

    Ok(())
}

/// What a replay run found.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ReplayOutcome {
    /// The file that was played.
    pub path: String,
    /// The seed it was recorded at.
    pub seed: u64,
    /// How many ticks were run.
    pub ticks: u64,
    /// How many checkpoints were compared.
    pub checked: usize,
    /// Every checkpoint whose hash did not match: `(tick, expected, found)`.
    pub mismatches: Vec<(u64, u64, u64)>,
}

impl ReplayOutcome {
    /// Whether every checkpoint matched.
    #[must_use]
    pub fn passed(&self) -> bool {
        self.mismatches.is_empty()
    }
}

/// Plays a recording against the app, checking every checkpoint on the way.
///
/// Restores a `.snapshot` file into the app, before anything runs.
///
/// Every failure is fatal, which is unusual for this engine — most things here are survivable and
/// reported. A snapshot is different: it says what the world *is*, so a half-applied one leaves a
/// process holding neither the recorded world nor a clean start, and every answer it gave after that
/// would be about a moment that never existed.
///
/// The game's own setup has already run by this point, which is what makes resources restorable:
/// `World::insert_resource` records how to rebuild each type as it goes, so the defaults the game
/// inserted are what the snapshot then overwrites.
fn restore_snapshot(app: &mut App, path: &std::path::Path) -> Result<(), AgentError> {
    let fail = |message: String| AgentError::BadSnapshot {
        path: path.display().to_string(),
        message,
    };

    let text = std::fs::read_to_string(path).map_err(|error| fail(error.to_string()))?;
    let document = amadeo_snapshot::parse(&text).map_err(|error| fail(error.to_string()))?;

    // The registry is taken out and put back, because `restore` needs the world mutably and the
    // registry shared, and `App` owns both — the same shape `World::with_service_taken` uses.
    let registry = app.take_registry();
    let result = amadeo_snapshot::restore(&mut app.world, &registry, &document);
    app.put_registry(registry);

    result.map_err(|error| fail(error.to_string()))
}

/// This is the separate-process half of the golden-replay mechanism. The in-process test in
/// `tests/golden_replay.rs` proves a recording survives a rebuild; this proves it survives a fresh
/// process, which is the stronger claim and the one M0's exit gate actually asked for.
///
/// **It does not stop at the first mismatch.** Knowing that ticks 60 and 300 diverged but 180 did
/// not is a different and much more useful fact than knowing 60 diverged.
fn play_replay(app: &mut App, path: &std::path::Path) -> Result<ReplayOutcome, AgentError> {
    let text = std::fs::read_to_string(path).map_err(|error| AgentError::BadReplay {
        path: path.display().to_string(),
        message: error.to_string(),
    })?;

    let recording =
        amadeo_input::Recording::parse(&text).map_err(|error| AgentError::BadReplay {
            path: path.display().to_string(),
            message: error.to_string(),
        })?;

    if recording.seed != app.seed() {
        return Err(AgentError::SeedMismatch {
            wanted: recording.seed,
            found: app.seed(),
        });
    }

    let ticks = recording.ticks;
    let expected: Vec<(Tick, u64)> = recording.checkpoints().collect();

    // Replaces whatever input source the game installed. A replay is exactly the claim that the
    // simulation cannot tell a recorded session from a live one.
    amadeo_input::install(
        &mut app.world,
        amadeo_input::InputDriver::replaying(recording),
    );

    let mut mismatches = Vec::new();
    let mut checked = 0;

    for tick in 1..=ticks {
        app.step().map_err(|source| AgentError::Simulation {
            ticks: tick,
            source,
        })?;

        if let Some((_, wanted)) = expected.iter().find(|(at, _)| at.0 == tick) {
            checked += 1;
            let found = app.state_hash();
            if found != *wanted {
                mismatches.push((tick, *wanted, found));
            }
        }
    }

    Ok(ReplayOutcome {
        path: path.display().to_string(),
        seed: app.seed(),
        ticks,
        checked,
        mismatches,
    })
}

/// Turns one line of input into one reply, never failing.
///
/// Every path produces a JSON-RPC envelope. A server that dies on a bad request is a server that an
/// agent cannot recover from without a human.
fn answer(app: &mut App, line: &str, replay: Option<&ReplayOutcome>) -> Json {
    let request = match Request::parse(line) {
        Ok(request) => request,
        // The id is unknowable when the request did not parse, so the spec says to reply with null.
        Err(error) => return failure(&Json::Null, &error),
    };

    match dispatch(app, &request, replay) {
        Ok(result) => success(&request.id, result),
        Err(error) => failure(&request.id, &error),
    }
}

/// Routes a request: world methods first, then this host's own.
fn dispatch(
    app: &mut App,
    request: &Request,
    replay: Option<&ReplayOutcome>,
) -> Result<Json, RpcError> {
    // Borrowed separately because `dispatch_world` takes both and `App` owns both.
    if let Some(result) = dispatch_world(request, &app.world, app.components())? {
        return Ok(result);
    }

    match request.method.as_str() {
        "sim.status" => Ok(Json::object([
            ("tick", Json::Int(app.tick().0 as i64)),
            // The hash is the identity of the whole simulation state, and it is what a replay
            // assertion compares. Rendered as a string because it is a 64-bit value and JSON
            // numbers are f64 — above 2^53 a client would silently read a different number.
            (
                "state_hash",
                Json::string(format!("{:016x}", app.state_hash())),
            ),
            (
                "events",
                Json::Array(
                    app.registered_events()
                        .into_iter()
                        .map(Json::string)
                        .collect(),
                ),
            ),
            (
                "components",
                Json::Array(app.components().names().map(Json::string).collect()),
            ),
        ])),

        // Returns the snapshot's **text**, not a file. The game process produces it and the CLI
        // writes it, which is the same division `amadeo check` and `amadeo import` use: the game
        // knows what the world is, the CLI is the side that touches the filesystem (ADR 0016).
        //
        // Where the frame goes — ADR 0040, and `docs/04-subsystems.md` §18's stated reason: an agent
        // cannot *feel* a frame-rate problem, so the only way it finds one is by being told.
        //
        // Times are **reported, never asserted** here, exactly as `sprite_throughput.rs` established:
        // a duration is a fact about the machine that produced it, and treating one as a pass/fail
        // is what makes a perf gate flaky enough that people stop believing it.
        "profile.frame" => {
            let Some(profiler) = app.world.service::<crate::Profiler>() else {
                // Cannot happen through `App`, which installs one — but a world assembled by hand
                // is a legitimate thing, and saying so beats reporting zeroes as if they were real.
                return Ok(Json::object([
                    ("ticks", Json::Int(0)),
                    ("systems", Json::Array(Vec::new())),
                    (
                        "note",
                        Json::string("no profiler is installed on this world"),
                    ),
                ]));
            };

            let systems: Vec<Json> = profiler
                .systems()
                .map(|(label, timing)| {
                    Json::object([
                        ("system", Json::string(label)),
                        ("runs", Json::Int(timing.runs as i64)),
                        // Microseconds as a float: nanoseconds would overflow the readable range of
                        // a JSON number for a long session, and milliseconds lose the resolution
                        // that matters for a system costing a few microseconds.
                        ("mean_us", Json::Float(timing.mean().as_secs_f64() * 1e6)),
                        ("worst_us", Json::Float(timing.worst.as_secs_f64() * 1e6)),
                    ])
                })
                .collect();

            Ok(Json::object([
                ("tick", Json::Int(app.tick().0 as i64)),
                ("ticks_measured", Json::Int(profiler.ticks() as i64)),
                (
                    "mean_tick_us",
                    Json::Float(profiler.mean_tick().as_secs_f64() * 1e6),
                ),
                // The 60 Hz frame this has to fit inside, so a reader does not have to know it.
                ("frame_budget_us", Json::Float(16_666.7)),
                ("systems", Json::Array(systems)),
            ]))
        }

        // There is no `snapshot.restore` method to match, deliberately. Restoring has to happen
        // before the first tick, so it is the `--snapshot` launch flag instead — the same shape,
        // and for the same reason, as `--replay`.
        "snapshot.take" => {
            let snapshot = amadeo_snapshot::capture(&app.world, app.components());
            Ok(Json::object([
                ("tick", Json::Int(app.tick().0 as i64)),
                (
                    "state_hash",
                    Json::string(format!("{:016x}", snapshot.state_hash)),
                ),
                ("entities", Json::Int(snapshot.entities.len() as i64)),
                ("resources", Json::Int(snapshot.resources.len() as i64)),
                ("text", Json::string(amadeo_snapshot::to_text(&snapshot))),
            ]))
        }

        // ADR 0021 named capture as the agent's eyes, and this is the last piece of it. The image
        // goes to a **file** rather than into the reply: a screenshot is hundreds of kilobytes, and
        // base64 in a JSON-RPC line would be unreadable in a transcript that is meant to be read.
        //
        // PNG rather than the PPM this engine already handles, because the whole point of a capture
        // is that a human opens it — and nothing opens a PPM. Lossless either way, so what lands on
        // disk is what the GPU produced.
        "render.capture" => {
            let path = request.string_param("path")?;
            let width = request.optional_int_param("width")?.unwrap_or(1280);
            let height = request.optional_int_param("height")?.unwrap_or(720);
            // Optional, and absent means "wherever the game is pointing" — which is what every
            // capture before this one did.
            let pitch = request.optional_number_param("pitch")?;
            let yaw = request.optional_number_param("yaw")?;
            capture_to_png(app, path, width, height, pitch, yaw, request)
        }

        "replay.status" => {
            let Some(outcome) = replay else {
                return Err(request.bad_params(format!(
                    "this process was not launched with a replay. \
                     Pass `{REPLAY_FLAG} <path>` alongside `{AGENT_FLAG}`, \
                     or use `amadeo replay <path>`"
                )));
            };

            let mismatches: Vec<Json> = outcome
                .mismatches
                .iter()
                .map(|(tick, wanted, found)| {
                    Json::object([
                        ("tick", Json::Int(*tick as i64)),
                        // Hex strings for the same reason `sim.status` uses one: a u64 above 2^53
                        // does not survive a JSON number.
                        ("expected", Json::string(format!("{wanted:016x}"))),
                        ("found", Json::string(format!("{found:016x}"))),
                    ])
                })
                .collect();

            Ok(Json::object([
                ("passed", Json::Bool(outcome.passed())),
                ("path", Json::string(&outcome.path)),
                ("seed", Json::Int(outcome.seed as i64)),
                ("ticks", Json::Int(outcome.ticks as i64)),
                ("checked", Json::Int(outcome.checked as i64)),
                ("mismatches", Json::Array(mismatches)),
            ]))
        }

        // The *text* is sent, not a path. The game process has no business reading the client's
        // filesystem, and a path would be resolved relative to whichever directory the game was
        // launched in -- which is not the one the user typed the path in. The CLI reads the file;
        // the game, which is the only process holding the registry, judges it.
        "scene.check" => {
            let text = request.string_param("text")?;

            let document = match amadeo_scene::parse(text) {
                Ok(document) => document,
                Err(error) => {
                    // A syntax error is reported the same way a schema error is, so a client has
                    // one shape to handle rather than two. `ParseError` already carries the line.
                    return Ok(Json::object([
                        ("ok", Json::Bool(false)),
                        (
                            "diagnostics",
                            Json::Array(vec![Json::object([
                                ("line", Json::Int(error.line as i64)),
                                ("message", Json::string(error.kind.to_string())),
                            ])]),
                        ),
                    ]));
                }
            };

            // The catalogue as well as the registry, so `amadeo check` catches a scene naming an
            // asset that does not exist — ADR 0020 gave it that job by name. `None` when the game
            // installed no catalogue, which skips the asset half rather than calling every id
            // missing.
            let catalogue = app
                .world
                .service::<amadeo_assets::Assets>()
                .map(|assets| &assets.catalogue);
            let found = amadeo_scene::validate(&document, app.components(), catalogue);
            let diagnostics: Vec<Json> = found
                .iter()
                .map(|diagnostic| {
                    let mut members = vec![
                        ("entity", Json::string(&diagnostic.entity)),
                        ("message", Json::string(&diagnostic.message)),
                    ];
                    if let Some(component) = &diagnostic.component {
                        members.push(("component", Json::string(component)));
                    }
                    Json::object(members)
                })
                .collect();

            Ok(Json::object([
                ("ok", Json::Bool(found.is_empty())),
                ("diagnostics", Json::Array(diagnostics)),
                ("entities", Json::Int(count_entities(&document) as i64)),
            ]))
        }

        "schedule.list" => {
            let wanted = match request.optional_string_param("stage")? {
                None => Stage::ALL.to_vec(),
                Some(name) => {
                    let stage = Stage::from_name(name).ok_or_else(|| {
                        let known: Vec<&str> =
                            Stage::ALL.iter().map(|stage| stage.name()).collect();
                        request.bad_params(format!(
                            "no stage named `{name}`. The stages are: {}",
                            known.join(", ")
                        ))
                    })?;
                    vec![stage]
                }
            };

            let mut stages = Vec::with_capacity(wanted.len());
            for stage in wanted {
                // A cycle or an unknown ordering constraint is a setup bug in the game, and it is
                // far more useful reported here than as a crash at the first tick.
                let systems = app.resolved_order(stage).map_err(|error| {
                    request.bad_params(format!("stage `{}`: {error}", stage.name()))
                })?;
                // ADR 0065. A second array rather than turning `systems` into objects, so a client
                // written against the old shape keeps working — and so the common case, a stage
                // where nothing is flagged, stays one empty array rather than a wrapper per system.
                let while_paused = app.while_paused_order(stage).map_err(|error| {
                    request.bad_params(format!("stage `{}`: {error}", stage.name()))
                })?;

                stages.push(Json::object([
                    ("stage", Json::string(stage.name())),
                    ("deterministic", Json::Bool(stage.is_deterministic())),
                    (
                        "systems",
                        Json::Array(systems.into_iter().map(Json::string).collect()),
                    ),
                    (
                        "runs_while_paused",
                        Json::Array(while_paused.into_iter().map(Json::string).collect()),
                    ),
                ]));
            }

            Ok(Json::object([("stages", Json::Array(stages))]))
        }

        other => {
            let mut known: Vec<&str> = WORLD_METHODS.iter().chain(APP_METHODS).copied().collect();
            known.sort_unstable();
            Err(RpcError::UnknownMethod {
                method: other.to_string(),
                known: known.join(", "),
            })
        }
    }
}

/// How many entities a document declares, children included.
fn count_entities(document: &amadeo_scene::SceneDocument) -> usize {
    fn count(entity: &amadeo_scene::SceneEntity) -> usize {
        1 + entity.children.iter().map(count).sum::<usize>()
    }
    document.entities.iter().map(count).sum()
}

/// Renders the current world offscreen and writes it to `path` as a PNG.
///
/// # Why this creates a backend rather than using the installed one
///
/// Agent mode is headless (ADR 0016 launches a game with `--amadeo-agent` and no window), so there
/// is usually no [`Renderer`](amadeo_render::Renderer) at all — and if a game did install one, it
/// would be the null backend, which draws nothing.
///
/// So capture builds its own offscreen wgpu backend, uses it, and drops it. That costs a device
/// creation per call, which is the right trade for an introspection method nobody calls in a loop:
/// the alternative is holding a GPU device open for every headless run, including the thousands that
/// never capture anything.
///
/// The frame is rendered *from the world*, exactly as `render_quads` does in a real game — so what
/// lands on disk is what this world would look like, not a reconstruction.
#[cfg(feature = "gpu")]
fn capture_to_png(
    app: &mut App,
    path: &str,
    width: i64,
    height: i64,
    pitch: Option<f64>,
    yaw: Option<f64>,
    request: &Request,
) -> Result<Json, RpcError> {
    let width = u32::try_from(width.max(1)).unwrap_or(1280);
    let height = u32::try_from(height.max(1)).unwrap_or(720);

    // **Aim the cameras somewhere other than where the game points them, then put them back.**
    //
    // This exists because a capture could only ever show the authored view, and the engine is judged
    // almost entirely by capture. Checking a ceiling, a sky or anything behind the player meant
    // editing the game's scene file, capturing, and reverting — which was done repeatedly, is
    // error-prone, and twice left an edit behind. A reviewer looking at a feature should not have to
    // modify the game to see it.
    let posed = pose_cameras(app, pitch, yaw);

    let backend = amadeo_render::WgpuBackend::offscreen(width, height).map_err(|error| {
        request.bad_params(format!(
            "could not open a GPU for capture: {error}. \
             `render.describe` answers what should be on screen without one"
        ))
    })?;

    // Installed, used, and removed again, so the world this method was asked about is the world it
    // leaves behind. A `Renderer` is a service, so none of this can reach the state hash (ADR 0009).
    app.world
        .insert_service(amadeo_render::Renderer::new(Box::new(backend)));

    // **The rest of the `Render` stage has to run, and it has to run after the renderer exists.**
    //
    // This used to call `render_quads` alone, which meant a capture saw only what the *world's*
    // cameras drew — no interface, and no anything else a game contributes to the frame. That is a
    // hole in the agent's eyes rather than a missing feature: `amadeo-ui` fills an `Overlay` from a
    // `Render`-stage system, and a method that skips the stage cannot see it (ADR 0062).
    //
    // After the renderer, because those systems ask it how big the screen is. Before `render_quads`,
    // because that is what drains the overlay into the frame.
    let already_draws = app
        .resolved_order(Stage::Render)
        .map(|labels| labels.contains(&amadeo_render::RENDER_QUADS))
        .unwrap_or(false);
    if let Err(error) = app.render() {
        return Err(request.bad_params(format!("the render stage failed: {error}")));
    }

    // Only when the game did not already draw. A second pass would overwrite the first with a frame
    // whose overlay had already been drained — the interface would vanish, and only in captures.
    if !already_draws {
        amadeo_render::render_quads(&mut app.world);
    }
    let mut renderer = app
        .world
        .remove_service::<amadeo_render::Renderer>()
        .ok_or_else(|| request.bad_params("the capture renderer vanished".to_string()))?;

    // Put the cameras back before anything can fail, so the world this method was asked about is the
    // world it leaves behind — the promise the temporary renderer above is also keeping. Restoring
    // after the `?` below would leave a game aimed at the ceiling whenever a capture failed, and the
    // agent host outlives one request.
    restore_cameras(app, posed);

    let image = renderer
        .capture()
        .map_err(|error| request.bad_params(format!("capture failed: {error}")))?;
    let encoded = amadeo_render::encode_png(&image)
        .map_err(|error| request.bad_params(format!("could not encode the capture: {error}")))?;

    std::fs::write(path, &encoded).map_err(|error| {
        request.bad_params(format!(
            "could not write `{path}`: {error}. \
             The path is resolved against the game's working directory, which the CLI sets to the \
             project root"
        ))
    })?;

    Ok(Json::object([
        ("path", Json::string(path)),
        ("width", Json::Int(i64::from(image.width))),
        ("height", Json::Int(i64::from(image.height))),
        ("bytes", Json::Int(encoded.len() as i64)),
        ("tick", Json::Int(app.tick().0 as i64)),
        ("drawn", Json::Int(drawn_count(&app.world))),
    ]))
}

/// Aims every camera entity at an absolute pitch and yaw, returning what they held before.
///
/// # Absolute, not an offset
///
/// `pitch 40` means forty degrees above the horizon whatever the game had, rather than forty more
/// than it. An offset would need the caller to know the current angle to predict the result, which
/// is exactly the thing a capture is being used to find out.
///
/// An absent angle leaves that axis alone, so `--pitch` on its own turns the view up without also
/// spinning it to face north. **Roll and position are never touched** — this aims a camera, it does
/// not move one, so a follow camera stays at the end of its arm and a first-person one stays in its
/// head.
///
/// Returns the previous [`Transform`] of each camera, for [`restore_cameras`].
#[cfg(feature = "gpu")]
fn pose_cameras(
    app: &mut App,
    pitch: Option<f64>,
    yaw: Option<f64>,
) -> Vec<(amadeo_ecs::Entity, amadeo_transform::Transform)> {
    if pitch.is_none() && yaw.is_none() {
        return Vec::new();
    }

    // Collected first because the query borrows the world and the writes below need it mutably.
    let cameras: Vec<amadeo_ecs::Entity> = app
        .world
        .query::<(&amadeo_render::Camera,)>()
        .map(|(entity, _)| entity)
        .collect();

    let mut previous = Vec::new();
    for entity in cameras {
        let Some(transform) = app
            .world
            .get::<amadeo_transform::Transform>(entity)
            .copied()
        else {
            continue;
        };
        previous.push((entity, transform));

        let mut aimed = transform;
        // Rotation is Euler degrees, X then Y (ADR 0018): X is pitch, Y is yaw, Z is roll.
        if let Some(pitch) = pitch {
            aimed.rotation[0] = pitch as f32;
        }
        if let Some(yaw) = yaw {
            aimed.rotation[1] = yaw as f32;
        }
        app.world.insert(entity, aimed);
    }

    // **`GlobalTransform` is what the renderer reads, and nothing recomputes it between here and the
    // draw.** `propagate_transforms` runs in `PostSimulation`, which has already happened by the
    // time a capture is being taken, so writing the local transform alone poses a camera that draws
    // from exactly where it was before — a silent no-op, and the first thing this got wrong.
    if !previous.is_empty() {
        amadeo_transform::propagate_transforms(&mut app.world);
    }
    previous
}

/// Puts back what [`pose_cameras`] changed.
#[cfg(feature = "gpu")]
fn restore_cameras(
    app: &mut App,
    previous: Vec<(amadeo_ecs::Entity, amadeo_transform::Transform)>,
) {
    if previous.is_empty() {
        return;
    }
    for (entity, transform) in previous {
        app.world.insert(entity, transform);
    }
    // Re-derived for the same reason it was derived above: leaving a stale `GlobalTransform` behind
    // would mean a capture changed where the *next* frame draws from.
    amadeo_transform::propagate_transforms(&mut app.world);
}

/// The same method when the engine was built without a GPU.
///
/// A clear refusal rather than a silent blank image, and it names the two ways forward: build with
/// the feature, or ask `render.describe`, which answers most of the same questions without one.
#[cfg(not(feature = "gpu"))]
fn capture_to_png(
    _app: &mut App,
    _path: &str,
    _width: i64,
    _height: i64,
    _pitch: Option<f64>,
    _yaw: Option<f64>,
    request: &Request,
) -> Result<Json, RpcError> {
    Err(request.bad_params(
        "this build has no GPU support, so it cannot capture. Build the game with \
         `amadeo-app/gpu` enabled, or use `render.describe`, which reports what is on screen \
         without a GPU at all"
            .to_string(),
    ))
}

/// How many entities the current world would draw, for the capture reply.
///
/// Reported alongside the image because "the file is 3 KB and nothing is in it" and "the file is
/// 3 KB and the world is empty" are different problems, and the second is much more common.
fn drawn_count(world: &amadeo_ecs::World) -> i64 {
    amadeo_render::describe_frame(world).drawn.len() as i64
}
#[cfg(test)]
mod tests {
    use super::*;
    use crate::{Stage, system};
    use amadeo_core::StableHash;
    use amadeo_ecs::{Component, World};
    use amadeo_reflect::Reflect;

    #[derive(Debug, Clone, Copy, PartialEq, StableHash, Reflect)]
    struct Position {
        /// Across.
        x: f32,
        /// Up.
        y: f32,
    }
    impl Component for Position {}

    #[derive(Debug, Clone, Copy, PartialEq, StableHash, Reflect)]
    struct Velocity {
        /// Across, per second.
        x: f32,
        /// Up, per second.
        y: f32,
    }
    impl Component for Velocity {}

    fn drift(world: &mut World) {
        world.for_each_pair_mut::<Position, Velocity>(|_entity, position, velocity| {
            position.x += velocity.x;
            position.y += velocity.y;
        });
    }

    /// A small app with one moving entity, so ticks visibly change the answers.
    fn test_app() -> App {
        let mut app = App::new();
        app.register_component::<Position>().expect("registers");
        app.register_component::<Velocity>().expect("registers");
        app.add_system(Stage::Simulation, system("drift", drift));

        let entity = app.world.spawn();
        app.world.insert(entity, Position { x: 0.0, y: 0.0 });
        app.world.insert(entity, Velocity { x: 1.0, y: 2.0 });
        app
    }

    /// Runs a script of requests and returns the replies, one per line.
    fn converse(app: &mut App, ticks: u64, requests: &[&str]) -> Vec<Json> {
        let input = requests.join("\n");
        let mut output: Vec<u8> = Vec::new();

        serve(
            app,
            AgentOptions {
                ticks,
                ..AgentOptions::default()
            },
            std::io::Cursor::new(input),
            &mut output,
        )
        .expect("serving should not fail");

        String::from_utf8(output)
            .expect("replies are UTF-8")
            .lines()
            .map(|line| Json::parse(line).expect("replies are JSON"))
            .collect()
    }

    fn call(method: &str, params: &str) -> String {
        format!(r#"{{"jsonrpc":"2.0","method":"{method}","params":{params},"id":1}}"#)
    }

    #[test]
    fn every_reply_is_a_json_rpc_envelope() {
        let replies = converse(
            &mut test_app(),
            0,
            &[
                call("sim.status", "{}"),
                call("nope.nothing", "{}"),
                "not json at all".to_string(),
            ]
            .iter()
            .map(String::as_str)
            .collect::<Vec<_>>(),
        );

        assert_eq!(replies.len(), 3);
        for reply in &replies {
            let text = reply.to_compact();
            assert!(text.contains(r#""jsonrpc":"2.0""#), "got: {text}");
            assert!(
                text.contains(r#""result":"#) || text.contains(r#""error":"#),
                "got: {text}"
            );
        }
    }

    #[test]
    fn a_bad_request_does_not_end_the_session() {
        // An agent that mistypes one method must be able to carry on, not restart the process.
        let replies = converse(&mut test_app(), 0, &["garbage", &call("sim.status", "{}")]);

        assert!(replies[0].to_compact().contains(r#""code":-32700"#));
        assert!(replies[1].to_compact().contains(r#""result":"#));
    }

    #[test]
    fn the_tick_argument_is_applied_before_the_first_answer() {
        // The core of the batch model: how far to run is set at launch, so every question in a
        // session sees the same world.
        let replies = converse(&mut test_app(), 10, &[&call("sim.status", "{}")]);
        assert!(
            replies[0].to_compact().contains(r#""tick":10"#),
            "got: {}",
            replies[0].to_compact()
        );
    }

    #[test]
    fn the_same_command_twice_gives_the_same_answer_twice() {
        // Determinism, from the outside. This is the property that makes an agent's question
        // repeatable and lets it become a test.
        let first = converse(
            &mut test_app(),
            120,
            &[&call("world.query", r#"{"components":["Position"]}"#)],
        );
        let second = converse(
            &mut test_app(),
            120,
            &[&call("world.query", r#"{"components":["Position"]}"#)],
        );

        assert_eq!(first, second);
        // And it actually moved, so the test is not passing on an empty world.
        assert!(
            first[0].to_compact().contains("120"),
            "got: {}",
            first[0].to_compact()
        );
    }

    #[test]
    fn world_methods_and_app_methods_both_route() {
        let replies = converse(
            &mut test_app(),
            0,
            &[
                &call("describe", "{}"),
                &call("world.list", "{}"),
                &call("schedule.list", "{}"),
                &call("sim.status", "{}"),
            ],
        );

        assert!(replies[0].to_compact().contains("Position"));
        assert!(replies[1].to_compact().contains(r#""count":1"#));
        assert!(replies[2].to_compact().contains("drift"));
        assert!(replies[3].to_compact().contains("state_hash"));
    }

    #[test]
    fn schedule_list_reports_resolved_order_and_which_stages_are_deterministic() {
        let replies = converse(
            &mut test_app(),
            0,
            &[&call("schedule.list", r#"{"stage":"Simulation"}"#)],
        );
        let text = replies[0].to_compact();

        assert!(text.contains(r#""stage":"Simulation""#), "got: {text}");
        assert!(text.contains(r#""deterministic":true"#), "got: {text}");
        assert!(text.contains(r#""systems":["drift"]"#), "got: {text}");
        // ADR 0065. Reported even when empty, because "no system runs while paused" and "this
        // build does not report it" are different answers and a client must be able to tell them
        // apart.
        assert!(text.contains(r#""runs_while_paused":[]"#), "got: {text}");
    }

    #[test]
    fn schedule_list_names_the_systems_that_survive_a_pause() {
        let mut app = test_app();
        app.add_system(
            Stage::Simulation,
            crate::system("navigate_menu", |_world: &mut World| {}).while_paused(),
        );

        let replies = converse(&mut app, 0, &[&call("schedule.list", r#"{}"#)]);
        let text = replies[0].to_compact();

        assert!(
            text.contains(r#""runs_while_paused":["navigate_menu"]"#),
            "got: {text}"
        );
    }

    #[test]
    fn scene_check_passes_a_scene_the_registry_understands() {
        let scene =
            "scene demo\\nversion 1\\n\\nentity a1 \\\"P\\\"\\n  Position\\n    x 1\\n    y 2\\n";
        let replies = converse(
            &mut test_app(),
            0,
            &[&call("scene.check", &format!(r#"{{"text":"{scene}"}}"#))],
        );
        let text = replies[0].to_compact();

        assert!(text.contains(r#""ok":true"#), "got: {text}");
        assert!(text.contains(r#""diagnostics":[]"#), "got: {text}");
        assert!(text.contains(r#""entities":1"#), "got: {text}");
    }

    #[test]
    fn scene_check_reports_every_schema_problem_at_once() {
        // One round trip per mistake is the thing this method exists to avoid.
        let scene = "scene demo\\nversion 1\\n\\nentity a1 \\\"P\\\"\\n  Nope\\n    x 1\\nentity a2 \\\"Q\\\"\\n  Position\\n    z 1\\n";
        let replies = converse(
            &mut test_app(),
            0,
            &[&call("scene.check", &format!(r#"{{"text":"{scene}"}}"#))],
        );
        let text = replies[0].to_compact();

        assert!(text.contains(r#""ok":false"#), "got: {text}");
        assert!(text.contains(r#""entity":"a1""#), "got: {text}");
        assert!(text.contains(r#""entity":"a2""#), "got: {text}");
        // And the unknown-component message lists what does exist.
        assert!(text.contains("Position, Velocity"), "got: {text}");
    }

    #[test]
    fn scene_check_reports_a_syntax_error_with_its_line() {
        // Same reply shape as a schema error, so a client handles one thing rather than two.
        let scene = "scene demo\\nversion 1\\n\\nentity oops\\n";
        let replies = converse(
            &mut test_app(),
            0,
            &[&call("scene.check", &format!(r#"{{"text":"{scene}"}}"#))],
        );
        let text = replies[0].to_compact();

        assert!(text.contains(r#""ok":false"#), "got: {text}");
        assert!(text.contains(r#""line":4"#), "got: {text}");
    }

    #[test]
    fn an_unknown_stage_lists_the_real_ones() {
        let replies = converse(
            &mut test_app(),
            0,
            &[&call("schedule.list", r#"{"stage":"Gameplay"}"#)],
        );
        let text = replies[0].to_compact();

        assert!(text.contains("no stage named `Gameplay`"), "got: {text}");
        assert!(text.contains("PreSimulation"), "got: {text}");
    }

    #[test]
    fn an_unknown_method_lists_every_method_there_is() {
        let replies = converse(&mut test_app(), 0, &[&call("world.teleport", "{}")]);
        let text = replies[0].to_compact();

        assert!(text.contains(r#""code":-32601"#), "got: {text}");
        // World methods and app methods in one list -- the split is an implementation detail and
        // the client should never have to know about it.
        assert!(text.contains("world.query"), "got: {text}");
        assert!(text.contains("sim.status"), "got: {text}");
    }

    #[test]
    fn the_state_hash_is_a_string_so_it_survives_json_numbers() {
        // A u64 above 2^53 does not survive a round trip through an f64, and every mainstream JSON
        // reader parses numbers as f64. A silently different hash would break replay assertions in
        // the least visible way available.
        let replies = converse(&mut test_app(), 3, &[&call("sim.status", "{}")]);
        let Json::Object(envelope) = &replies[0] else {
            panic!("expected an object");
        };
        let Some(Json::Object(result)) = envelope.get("result") else {
            panic!("expected a result object");
        };

        match result.get("state_hash") {
            Some(Json::String(hash)) => assert_eq!(hash.len(), 16, "expected 16 hex digits"),
            other => panic!("state_hash should be a string, got {other:?}"),
        }
    }

    #[test]
    fn blank_lines_are_skipped_rather_than_answered() {
        let replies = converse(&mut test_app(), 0, &["", &call("sim.status", "{}"), "   "]);
        assert_eq!(replies.len(), 1);
    }

    /// `AgentError` wraps `io::Error`, which is not comparable, so the tests unwrap rather than
    /// compare whole `Result`s.
    fn options_of(arguments: &[&str]) -> Option<AgentOptions> {
        let owned: Vec<String> = arguments.iter().map(|text| (*text).to_string()).collect();
        agent_options_from(&owned).expect("arguments should be valid")
    }

    /// Writes a replay file into a temp directory and serves against it.
    fn serve_replay(app: &mut App, replay_text: &str, request: &str) -> Json {
        let directory = std::env::temp_dir().join(format!(
            "amadeo-replay-test-{}-{:?}",
            std::process::id(),
            std::thread::current().id()
        ));
        std::fs::create_dir_all(&directory).expect("temp dir");
        let path = directory.join("session.replay");
        std::fs::write(&path, replay_text).expect("write replay");

        let mut output: Vec<u8> = Vec::new();
        serve(
            app,
            AgentOptions {
                replay: Some(path.clone()),
                ..AgentOptions::default()
            },
            std::io::Cursor::new(request.to_string()),
            &mut output,
        )
        .expect("serving should not fail");

        let _ = std::fs::remove_file(&path);
        let text = String::from_utf8(output).expect("UTF-8");
        Json::parse(text.lines().next().expect("one reply")).expect("JSON")
    }

    /// Runs the test app for `ticks` and returns its state hash, for building a fixture.
    ///
    /// Installs the input driver first, because `play_replay` does and `InputState` is a
    /// **resource** — so it is part of the state hash. An app with input installed and one without
    /// are genuinely different worlds, and a fixture generated from the wrong one would look like a
    /// replay failure. Found by this test failing exactly that way.
    fn hash_after(ticks: u64) -> u64 {
        let mut app = test_app();
        amadeo_input::install(
            &mut app.world,
            amadeo_input::InputDriver::replaying(amadeo_input::Recording::new(0)),
        );
        app.run_ticks(ticks).expect("schedule resolves");
        app.state_hash()
    }

    #[test]
    fn a_replay_whose_checkpoints_match_passes() {
        // The separate-process claim, exercised in-process here and end to end by the CLI: the same
        // recorded run reaches the same state hashes.
        let replay = format!(
            "amadeo-replay 1\ntick-rate 60\nseed 0\nticks 10\n\ncheckpoint 5 {:016x}\ncheckpoint 10 {:016x}\n",
            hash_after(5),
            hash_after(10)
        );

        let reply = serve_replay(&mut test_app(), &replay, &call("replay.status", "{}"));
        let text = reply.to_compact();

        assert!(text.contains(r#""passed":true"#), "got: {text}");
        assert!(text.contains(r#""checked":2"#), "got: {text}");
        assert!(text.contains(r#""mismatches":[]"#), "got: {text}");
    }

    #[test]
    fn a_wrong_checkpoint_is_reported_with_both_hashes() {
        // If a corrupted replay still passed, the checkpoints would be worthless.
        let replay = format!(
            "amadeo-replay 1\ntick-rate 60\nseed 0\nticks 10\n\ncheckpoint 5 {:016x}\ncheckpoint 10 deadbeefdeadbeef\n",
            hash_after(5)
        );

        let reply = serve_replay(&mut test_app(), &replay, &call("replay.status", "{}"));
        let text = reply.to_compact();

        assert!(text.contains(r#""passed":false"#), "got: {text}");
        // Both checkpoints were still compared -- it does not stop at the first mismatch, because
        // "60 and 300 diverged but 180 did not" is a much more useful fact than "60 diverged".
        assert!(text.contains(r#""checked":2"#), "got: {text}");
        assert!(
            text.contains(r#""expected":"deadbeefdeadbeef""#),
            "got: {text}"
        );
        assert!(text.contains(r#""tick":10"#), "got: {text}");
    }

    #[test]
    fn a_seed_mismatch_says_how_to_fix_it_rather_than_diverging() {
        // Replaying against the wrong seed would produce a hash mismatch that looks exactly like a
        // real regression. Refusing up front is the difference between a five-minute confusion and
        // an afternoon.
        let replay = "amadeo-replay 1\ntick-rate 60\nseed 999\nticks 5\n";
        let mut app = test_app();

        let error = serve(
            &mut app,
            AgentOptions {
                replay: Some(std::path::PathBuf::from("unused")),
                ..AgentOptions::default()
            },
            std::io::Cursor::new(String::new()),
            Vec::new(),
        );
        // The file does not exist, so this is the read error -- checked separately below.
        assert!(error.is_err());

        let directory = std::env::temp_dir().join("amadeo-seed-mismatch-test");
        std::fs::create_dir_all(&directory).expect("temp dir");
        let path = directory.join("wrong-seed.replay");
        std::fs::write(&path, replay).expect("write");

        let error = serve(
            &mut app,
            AgentOptions {
                replay: Some(path),
                ..AgentOptions::default()
            },
            std::io::Cursor::new(String::new()),
            Vec::new(),
        )
        .expect_err("seeds differ");

        let message = error.to_string();
        assert!(message.contains("seed 999"), "got: {message}");
        assert!(message.contains("requested_seed"), "got: {message}");
    }

    #[test]
    fn replay_status_without_a_replay_says_how_to_ask_for_one() {
        let replies = converse(&mut test_app(), 0, &[&call("replay.status", "{}")]);
        let text = replies[0].to_compact();

        assert!(text.contains("not launched with a replay"), "got: {text}");
        assert!(text.contains("amadeo replay"), "got: {text}");
    }

    #[test]
    fn the_agent_flag_is_off_unless_asked_for() {
        // A game launched to be played must not start reading stdin as a protocol.
        assert_eq!(options_of(&[]), None);
        assert_eq!(options_of(&["--fullscreen"]), None);
    }

    #[test]
    fn tick_counts_parse_and_default_to_zero() {
        assert_eq!(options_of(&[AGENT_FLAG]), Some(AgentOptions::default()));
        assert_eq!(
            options_of(&[AGENT_FLAG, TICKS_FLAG, "600"]).map(|o| o.ticks),
            Some(600)
        );
        // Order does not matter, and unrelated arguments are ignored.
        assert_eq!(
            options_of(&["--windowed", TICKS_FLAG, "42", AGENT_FLAG]).map(|o| o.ticks),
            Some(42)
        );
    }

    #[test]
    fn replay_and_seed_flags_parse() {
        let options = options_of(&[AGENT_FLAG, REPLAY_FLAG, "a.replay", SEED_FLAG, "1234"])
            .expect("agent mode");

        assert_eq!(options.replay, Some(std::path::PathBuf::from("a.replay")));
        assert_eq!(options.seed, Some(1234));
    }

    #[test]
    fn a_game_can_read_the_seed_before_it_builds() {
        // The whole point of `requested_seed`: App::with_seed happens before the handover, so the
        // seed has to be readable from argv rather than handed over afterwards.
        let arguments: Vec<String> = [SEED_FLAG, "77"].iter().map(|s| s.to_string()).collect();
        assert_eq!(seed_from(&arguments), Some(77));
        assert_eq!(seed_from(&[]), None);
    }

    #[test]
    fn a_malformed_tick_count_is_refused_with_the_units_spelled_out() {
        let flag = AGENT_FLAG.to_string();
        let error = agent_options_from(&[flag, TICKS_FLAG.to_string(), "a while".to_string()])
            .expect_err("not a number");
        assert!(
            error.to_string().contains("60 per simulated second"),
            "got: {error}"
        );
    }
}

/// Tests for `render.capture`'s pose override.
///
/// Behind `gpu` because the functions are: they exist only for the capture path, which needs a
/// device. Nothing here opens one — aiming a camera and putting it back is ordinary world editing.
#[cfg(all(test, feature = "gpu"))]
mod pose_tests {
    use super::*;
    use amadeo_render::Camera;
    use amadeo_transform::{GlobalTransform, Parent, Transform};

    /// A camera parented to something, which is the arrangement every game here actually uses — a
    /// follow camera or a first-person one is a child, never a loose entity.
    fn app_with_a_parented_camera() -> (App, amadeo_ecs::Entity) {
        let mut app = App::new();
        let body = app.world.spawn();
        app.world.insert(
            body,
            Transform {
                translation: [0.0, 1.0, 0.0],
                ..Transform::default()
            },
        );

        let eye = app.world.spawn();
        app.world.insert(
            eye,
            Transform {
                translation: [0.0, 2.0, 4.0],
                rotation: [-18.0, 0.0, 0.0],
                ..Transform::default()
            },
        );
        app.world.insert(eye, Parent(body));
        app.world.insert(eye, Camera::perspective(60.0));
        amadeo_transform::propagate_transforms(&mut app.world);
        (app, eye)
    }

    #[test]
    fn an_absent_angle_leaves_that_axis_alone() {
        // `--pitch` on its own must not also spin the view to face north, or checking a ceiling
        // would silently change which wall is in shot.
        let (mut app, eye) = app_with_a_parented_camera();
        let posed = pose_cameras(&mut app, Some(40.0), None);

        let after = app
            .world
            .get::<Transform>(eye)
            .copied()
            .expect("still there");
        assert_eq!(after.rotation[0], 40.0, "pitch is set");
        assert_eq!(after.rotation[1], 0.0, "yaw is untouched");
        assert_eq!(
            after.translation,
            [0.0, 2.0, 4.0],
            "a pose aims a camera, it does not move one"
        );
        assert_eq!(posed.len(), 1);
    }

    #[test]
    fn posing_updates_the_global_transform_the_renderer_actually_reads() {
        // **The one that matters.** `propagate_transforms` runs in `PostSimulation`, which is over by
        // the time a capture happens — so writing the local `Transform` alone poses a camera that
        // still draws from exactly where it was. That is a silent no-op producing a plausible
        // picture of the wrong thing, which is the worst possible failure for a tool whose entire
        // job is to be believed.
        let (mut app, eye) = app_with_a_parented_camera();
        let before = *app.world.get::<GlobalTransform>(eye).expect("propagated");

        pose_cameras(&mut app, Some(75.0), None);

        let after = *app.world.get::<GlobalTransform>(eye).expect("propagated");
        assert_ne!(
            before.matrix, after.matrix,
            "the composed matrix must reflect the new aim"
        );
    }

    #[test]
    fn a_capture_puts_the_cameras_back() {
        // A capture answers a question about a world; it must not change it. The agent host outlives
        // one request, so a camera left aimed at the ceiling would corrupt every later answer —
        // including a state hash, once anything writes one from a posed frame.
        let (mut app, eye) = app_with_a_parented_camera();
        let before_local = *app.world.get::<Transform>(eye).expect("there");
        let before_global = *app.world.get::<GlobalTransform>(eye).expect("propagated");

        let posed = pose_cameras(&mut app, Some(75.0), Some(180.0));
        restore_cameras(&mut app, posed);

        assert_eq!(
            *app.world.get::<Transform>(eye).expect("there"),
            before_local
        );
        assert_eq!(
            app.world
                .get::<GlobalTransform>(eye)
                .expect("propagated")
                .matrix,
            before_global.matrix,
            "the derived transform has to be put back too, or the next frame draws from the pose"
        );
    }

    #[test]
    fn no_angles_touches_nothing_at_all() {
        // Every capture taken before this feature existed goes down this path, so it has to be
        // exactly the old behaviour rather than a pose that happens to be the current one.
        let (mut app, eye) = app_with_a_parented_camera();
        let before = *app.world.get::<Transform>(eye).expect("there");

        let posed = pose_cameras(&mut app, None, None);

        assert!(posed.is_empty(), "nothing was posed, so nothing to restore");
        assert_eq!(*app.world.get::<Transform>(eye).expect("there"), before);
    }
}
