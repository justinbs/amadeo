//! Launching the game and talking to it.
//!
//! This is the mechanism ADR 0016 chose. The CLI cannot answer `describe` itself — under ADR 0011 a
//! game's components are Rust types compiled into the game binary, and this binary has never linked
//! them. So it starts that one and asks.
//!
//! # Why it goes through cargo
//!
//! `cargo run -p <game> -- --amadeo-agent` rather than executing a path directly, so the binary is
//! **rebuilt if stale**. That is the whole reason a generated manifest was rejected: a schema that
//! describes code which no longer exists is the plausible-but-wrong failure Pillar 2 exists to
//! eliminate, and a cached *binary* would have exactly the same problem as a cached *file*. The
//! measured cost is 0.9–3.2 s (ADR 0011), which is cheap for never being wrong.
//!
//! Cargo's progress output goes to stderr and is inherited, so the user sees the build happen while
//! stdout stays pure protocol.

use amadeo_agent::Json;
use anyhow::{Context, Result, bail};
use std::io::{BufRead, BufReader, Write};
use std::path::Path;
use std::process::{Child, Command, Stdio};

/// A running game binary, in agent mode, with its pipes held open.
#[derive(Debug)]
pub(crate) struct Session {
    child: Child,
    /// The package launched, so failures can name it.
    package: String,
}

impl Session {
    /// Builds and launches the game in agent mode.
    ///
    /// `ticks` is how far to simulate before the first question is answered — a launch argument
    /// rather than a method, which is what makes a session's answers reproducible (ADR 0016).
    ///
    /// # Errors
    ///
    /// If cargo cannot be started, or the build fails.
    pub(crate) fn start(root: &Path, package: &str, ticks: u64) -> Result<Session> {
        let mut command = Command::new("cargo");
        command
            .current_dir(root)
            .arg("run")
            // Quiet, so cargo's own chatter does not look like part of the conversation.
            .arg("--quiet")
            .arg("--package")
            .arg(package)
            .arg("--")
            .arg("--amadeo-agent")
            .arg("--ticks")
            .arg(ticks.to_string())
            .stdin(Stdio::piped())
            .stdout(Stdio::piped())
            // Inherited: build progress and panics reach the user unchanged, and never pollute the
            // protocol stream.
            .stderr(Stdio::inherit());

        let child = command.spawn().with_context(|| {
            format!(
                "could not run `cargo run -p {package}`. Is cargo on PATH, and is {} a cargo \
                 workspace?",
                root.display()
            )
        })?;

        Ok(Session {
            child,
            package: package.to_string(),
        })
    }

    /// Sends every request, then reads one reply per request.
    ///
    /// Written as send-all-then-read-all rather than one at a time because the batch model has no
    /// request that depends on a previous reply — and because interleaving writes and reads on two
    /// pipes is the classic way to deadlock when a buffer fills.
    ///
    /// # Errors
    ///
    /// If the pipes fail, or the game exits before answering.
    pub(crate) fn ask(&mut self, requests: &[Json]) -> Result<Vec<Json>> {
        {
            let stdin = self
                .child
                .stdin
                .as_mut()
                .context("the game's stdin was closed before anything could be sent")?;

            for request in requests {
                writeln!(stdin, "{}", request.to_compact())?;
            }
            stdin.flush()?;
        }
        // Dropping stdin sends end-of-input, which is what tells the game to stop reading and exit.
        drop(self.child.stdin.take());

        let stdout = self
            .child
            .stdout
            .take()
            .context("the game's stdout was closed")?;

        let mut replies = Vec::with_capacity(requests.len());
        for line in BufReader::new(stdout).lines() {
            let line = line?;
            if line.trim().is_empty() {
                continue;
            }
            let reply = Json::parse(&line).with_context(|| {
                format!(
                    "`{}` sent something that is not JSON: {line}\n\
                     Anything a game prints to stdout in agent mode corrupts the protocol — \
                     use eprintln! for diagnostics.",
                    self.package
                )
            })?;
            replies.push(reply);
        }

        let status = self.child.wait()?;
        if !status.success() {
            bail!(
                "`{}` exited with {status} before finishing the conversation",
                self.package
            );
        }

        if replies.len() != requests.len() {
            bail!(
                "sent {} requests to `{}` but got {} replies back",
                requests.len(),
                self.package,
                replies.len()
            );
        }

        Ok(replies)
    }
}

/// Runs one request against a freshly launched game and returns its result.
///
/// The whole one-shot batch model in one function: launch, ask, print, exit.
///
/// # Errors
///
/// If the launch fails, or the game answers with a JSON-RPC error — which is turned into a plain
/// error message here, so a failing `amadeo` command looks like a failing command rather than like
/// a successful command that printed an error object.
pub(crate) fn ask_once(root: &Path, package: &str, ticks: u64, request: Json) -> Result<Json> {
    let mut session = Session::start(root, package, ticks)?;
    let replies = session.ask(&[request])?;

    let reply = replies
        .into_iter()
        .next()
        .context("the game sent no reply")?;

    unwrap_reply(reply)
}

/// Pulls the `result` out of a JSON-RPC envelope, turning an `error` into a real error.
///
/// # Errors
///
/// If the envelope carries an error, or is not shaped like a reply.
pub(crate) fn unwrap_reply(reply: Json) -> Result<Json> {
    let Json::Object(members) = &reply else {
        bail!(
            "expected a JSON-RPC reply object, got: {}",
            reply.to_compact()
        );
    };

    if let Some(Json::Object(error)) = members.get("error") {
        let message = match error.get("message") {
            Some(Json::String(text)) => text.clone(),
            _ => reply.to_compact(),
        };
        // The code is printed too: it distinguishes "you asked for something that does not exist"
        // from "the request was malformed", which are different things to fix.
        let code = match error.get("code") {
            Some(Json::Int(value)) => *value,
            _ => 0,
        };
        bail!("{message} (JSON-RPC error {code})");
    }

    members
        .get("result")
        .cloned()
        .with_context(|| format!("reply has neither result nor error: {}", reply.to_compact()))
}

/// Builds a JSON-RPC request.
#[must_use]
pub(crate) fn request(method: &str, params: Json, id: i64) -> Json {
    Json::object([
        ("jsonrpc", Json::string("2.0")),
        ("method", Json::string(method)),
        ("params", params),
        ("id", Json::Int(id)),
    ])
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn a_result_envelope_yields_its_result() {
        let reply = Json::parse(r#"{"jsonrpc":"2.0","result":{"tick":3},"id":1}"#).expect("json");
        let result = unwrap_reply(reply).expect("has a result");
        assert_eq!(result.to_compact(), r#"{"tick":3}"#);
    }

    #[test]
    fn an_error_envelope_becomes_a_failed_command() {
        // Printing the error object as though it were output would make a failed command look
        // like it worked, which is how an agent ends up building on a wrong answer.
        let reply = Json::parse(
            r#"{"jsonrpc":"2.0","error":{"code":-32601,"message":"no method named `x`"},"id":1}"#,
        )
        .expect("json");

        let error = unwrap_reply(reply).expect_err("should fail");
        assert!(
            error.to_string().contains("no method named `x`"),
            "got: {error}"
        );
        assert!(error.to_string().contains("-32601"), "got: {error}");
    }

    #[test]
    fn requests_are_well_formed() {
        let built = request(
            "describe",
            Json::object([("type", Json::string("Quad"))]),
            1,
        );
        assert_eq!(
            built.to_compact(),
            r#"{"id":1,"jsonrpc":"2.0","method":"describe","params":{"type":"Quad"}}"#
        );
    }
}
