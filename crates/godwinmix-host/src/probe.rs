//! Does this build of a plugin still say hello?
//!
//! `gmx plugin update` needs one fact before it lets a new version take over:
//! that the new version starts and sends `initialize`. Everything else the
//! conformance harness checks is the author's problem and takes a minute;
//! this takes as long as the process takes to say its own name, and it is what
//! stands between an operator and a mixer whose plugin now exits on startup.
//!
//! A probe spawns the plugin exactly as the core would, reads its stderr until
//! an `initialize` arrives or the deadline passes, and kills it. Nothing is
//! answered, so the plugin never gets past the handshake and never opens a
//! media transport. 06 section 2: "an update that fails its handshake is
//! rolled back to the previous version automatically", and this is the check
//! behind the word "fails".

use crate::launch::Launch;
use godwinmix_protocol::plugin::wire::{Initialize, MAX_LINE_BYTES};
use std::io::{BufRead, BufReader};
use std::path::Path;
use std::process::{Command, Stdio};
use std::sync::mpsc;
use std::time::{Duration, Instant};

/// How long a new version has to say hello before an update is rolled back.
/// 06 section 2 does not name a number; ten seconds is what a Python plugin
/// with a cold import cache takes on a Raspberry Pi 4 and still leaves an
/// operator waiting less than a take.
pub const HANDSHAKE_DEADLINE: Duration = Duration::from_secs(10);

/// What the probe saw.
#[derive(Debug, Clone)]
pub struct Probed {
    pub hello: Initialize,
    /// How long it took, for the report line.
    pub took: Duration,
}

/// Start the plugin, wait for `initialize`, and stop it.
///
/// The error is what an operator reads when an update is rolled back, so it
/// carries the last lines the plugin said. A plugin that dies with a Python
/// traceback on stderr has the traceback in the message.
pub fn handshake(launch: &Launch, cwd: &Path, deadline: Duration) -> anyhow::Result<Probed> {
    let (program, args) = launch
        .argv
        .split_first()
        .ok_or_else(|| anyhow::anyhow!("there is no command to run the plugin with"))?;
    let started = Instant::now();
    let mut child = Command::new(program)
        .args(args)
        .current_dir(cwd)
        .envs(&launch.env)
        .stdin(Stdio::piped())
        .stdout(Stdio::null())
        .stderr(Stdio::piped())
        .spawn()
        .map_err(|e| anyhow::anyhow!("starting `{program}`: {e}"))?;
    let lines = reader(child.stderr.take());
    let outcome = wait_for_hello(&lines, deadline);
    let _ = child.kill();
    let _ = child.wait();
    outcome.map(|hello| Probed { hello, took: started.elapsed() })
}

/// A thread reading stderr, because a pipe read cannot be given a deadline.
fn reader(stderr: Option<std::process::ChildStderr>) -> mpsc::Receiver<String> {
    let (tx, rx) = mpsc::channel();
    let Some(stderr) = stderr else { return rx };
    std::thread::Builder::new()
        .name("plugin-probe".into())
        .spawn(move || {
            let mut reader = BufReader::new(stderr);
            let mut line = String::new();
            loop {
                line.clear();
                match reader.read_line(&mut line) {
                    Ok(0) | Err(_) => break,
                    Ok(_) => {
                        if line.len() > MAX_LINE_BYTES {
                            break;
                        }
                        if tx.send(line.trim_end().to_string()).is_err() {
                            break;
                        }
                    }
                }
            }
        })
        .ok();
    rx
}

fn wait_for_hello(
    lines: &mpsc::Receiver<String>,
    deadline: Duration,
) -> anyhow::Result<Initialize> {
    let until = Instant::now() + deadline;
    let mut seen: Vec<String> = Vec::new();
    while let Some(left) = until.checked_duration_since(Instant::now()) {
        let Ok(line) = lines.recv_timeout(left) else { break };
        if line.trim().is_empty() {
            continue;
        }
        match parse_hello(&line) {
            Some(hello) => return Ok(hello),
            None => seen.push(line),
        }
    }
    let tail: Vec<&str> = seen.iter().rev().take(5).map(String::as_str).collect();
    anyhow::bail!(
        "it did not send `initialize` within {} s.{}",
        deadline.as_secs(),
        if tail.is_empty() {
            " It said nothing at all on stderr, so it probably exited at once; run it by \
             hand to see why."
                .to_string()
        } else {
            format!(
                " The last lines it did send:\n    {}",
                tail.into_iter().rev().collect::<Vec<_>>().join("\n    ")
            )
        }
    )
}

/// One JSON line, if it is the `initialize` request.
fn parse_hello(line: &str) -> Option<Initialize> {
    let value: serde_json::Value = serde_json::from_str(line).ok()?;
    if value.get("method")?.as_str()? != "initialize" {
        return None;
    }
    serde_json::from_value(value.get("params")?.clone()).ok()
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::collections::BTreeMap;

    fn launch_of(argv: Vec<String>) -> Launch {
        Launch {
            argv,
            env: BTreeMap::new(),
            cwd: std::env::temp_dir(),
            runtime: crate::launch::Runtime::Shell,
        }
    }

    #[test]
    fn an_initialize_line_is_recognised_and_nothing_else_is() {
        let line = "{\"jsonrpc\":\"2.0\",\"id\":0,\"method\":\"initialize\",\"params\":\
                    {\"plugin\":\"clock\",\"version\":\"1.0.0\",\"api\":1,\"transports\":[\"container\"]}}";
        let hello = parse_hello(line).expect("that is the handshake");
        assert_eq!(hello.plugin, "clock");
        assert_eq!(hello.api, 1);
        assert!(parse_hello("starting up, please wait").is_none());
        assert!(parse_hello("{\"jsonrpc\":\"2.0\",\"method\":\"initialized\"}").is_none());
    }

    /// The three probe tests below are Unix only: each needs a process that
    /// says a chosen thing on stderr and then behaves in a chosen way, and
    /// `sh -c` is the shortest honest way to get one. The parsing half,
    /// which is where the bugs are, is tested above without a process.
    #[cfg(unix)]
    #[test]
    fn a_plugin_that_says_hello_passes_the_probe() {
        let line = "{\"jsonrpc\":\"2.0\",\"id\":0,\"method\":\"initialize\",\"params\":\
                    {\"plugin\":\"clock\",\"version\":\"1.0.0\",\"api\":1}}";
        let launch = launch_of(vec![
            "sh".into(),
            "-c".into(),
            format!("echo '{line}' >&2; sleep 30"),
        ]);
        let probed = handshake(&launch, &std::env::temp_dir(), Duration::from_secs(5))
            .expect("it says hello");
        assert_eq!(probed.hello.plugin, "clock");
    }

    #[cfg(unix)]
    #[test]
    fn a_plugin_that_dies_on_startup_fails_with_what_it_printed() {
        let launch = launch_of(vec![
            "sh".into(),
            "-c".into(),
            "echo 'ImportError: no module named gmx' >&2; exit 1".into(),
        ]);
        let err = handshake(&launch, &std::env::temp_dir(), Duration::from_secs(5))
            .expect_err("it never says hello");
        let text = format!("{err}");
        assert!(text.contains("ImportError"), "{text}");
    }

    #[cfg(unix)]
    #[test]
    fn a_plugin_that_says_nothing_is_given_the_deadline_and_no_longer() {
        let launch = launch_of(vec!["sh".into(), "-c".into(), "sleep 30".into()]);
        let started = Instant::now();
        let err = handshake(&launch, &std::env::temp_dir(), Duration::from_millis(400))
            .expect_err("it never says hello");
        assert!(started.elapsed() < Duration::from_secs(5), "the deadline is honoured");
        assert!(format!("{err}").contains("nothing at all"), "{err}");
    }
}
