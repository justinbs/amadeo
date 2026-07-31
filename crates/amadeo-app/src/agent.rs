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
use std::io::{BufRead, Write};

/// The flag that turns a game binary into an agent host.
pub const AGENT_FLAG: &str = "--amadeo-agent";

/// The flag that says how far to run before answering questions.
pub const TICKS_FLAG: &str = "--ticks";

/// The methods this host answers itself, on top of [`WORLD_METHODS`].
pub const APP_METHODS: &[&str] = &["scene.check", "schedule.list", "sim.status"];

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

    /// stdin or stdout failed.
    #[error("agent transport failed: {0}")]
    Transport(#[from] std::io::Error),
}

/// How the agent was asked to run.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct AgentOptions {
    /// How many ticks to simulate before answering anything.
    ///
    /// This is a launch argument rather than a method on purpose (ADR 0016). Every question is then
    /// asked of a world that got where it is by running a fixed number of ticks from a fixed seed,
    /// so the same command twice gives the same answer twice — and a question an agent asks is a
    /// question it can put in a test.
    pub ticks: u64,
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

    let mut ticks = 0_u64;
    let mut index = 0;
    while index < arguments.len() {
        if arguments[index] == TICKS_FLAG {
            let Some(value) = arguments.get(index + 1) else {
                return Err(AgentError::BadArguments(format!(
                    "{TICKS_FLAG} needs a number, as in `{TICKS_FLAG} 600`"
                )));
            };
            ticks = value.parse().map_err(|_| {
                AgentError::BadArguments(format!(
                    "`{value}` is not a tick count; {TICKS_FLAG} takes a whole number of ticks \
                     (60 per simulated second)"
                ))
            })?;
            index += 2;
            continue;
        }
        index += 1;
    }

    Ok(Some(AgentOptions { ticks }))
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
    // The whole pre-roll happens before the first reply, so every answer describes the same world.
    if options.ticks > 0 {
        app.run_ticks(options.ticks)
            .map_err(|source| AgentError::Simulation {
                ticks: options.ticks,
                source,
            })?;
    }

    for line in input.lines() {
        let line = line?;
        if line.trim().is_empty() {
            continue;
        }

        let reply = answer(app, &line);
        writeln!(output, "{}", reply.to_compact())?;
        // Flushed per reply: a client that sent one request and is waiting for one answer would
        // otherwise block on a buffer that only empties at exit.
        output.flush()?;
    }

    Ok(())
}

/// Turns one line of input into one reply, never failing.
///
/// Every path produces a JSON-RPC envelope. A server that dies on a bad request is a server that an
/// agent cannot recover from without a human.
fn answer(app: &mut App, line: &str) -> Json {
    let request = match Request::parse(line) {
        Ok(request) => request,
        // The id is unknowable when the request did not parse, so the spec says to reply with null.
        Err(error) => return failure(&Json::Null, &error),
    };

    match dispatch(app, &request) {
        Ok(result) => success(&request.id, result),
        Err(error) => failure(&request.id, &error),
    }
}

/// Routes a request: world methods first, then this host's own.
fn dispatch(app: &mut App, request: &Request) -> Result<Json, RpcError> {
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

            let found = amadeo_scene::validate(&document, app.components());
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

                stages.push(Json::object([
                    ("stage", Json::string(stage.name())),
                    ("deterministic", Json::Bool(stage.is_deterministic())),
                    (
                        "systems",
                        Json::Array(systems.into_iter().map(Json::string).collect()),
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
            AgentOptions { ticks },
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

    #[test]
    fn the_agent_flag_is_off_unless_asked_for() {
        // A game launched to be played must not start reading stdin as a protocol.
        assert_eq!(options_of(&[]), None);
        assert_eq!(options_of(&["--fullscreen"]), None);
    }

    #[test]
    fn tick_counts_parse_and_default_to_zero() {
        assert_eq!(options_of(&[AGENT_FLAG]), Some(AgentOptions { ticks: 0 }));
        assert_eq!(
            options_of(&[AGENT_FLAG, TICKS_FLAG, "600"]),
            Some(AgentOptions { ticks: 600 })
        );
        // Order does not matter, and unrelated arguments are ignored.
        assert_eq!(
            options_of(&["--windowed", TICKS_FLAG, "42", AGENT_FLAG]),
            Some(AgentOptions { ticks: 42 })
        );
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
