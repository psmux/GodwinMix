//! `gmx plugin test --offline`: a transcript, a plugin binary, and no core.
//!
//! Bytes in, bytes out. The runner writes the `core` lines of a transcript to
//! the plugin's stdin and matches its stderr against the `plugin` lines, as a
//! subset, in order. No sockets, no GStreamer, no clock beyond one deadline,
//! so a plugin's own CI runs this on any runner in seconds. That promise is
//! 09 section 4 item 9, and it is what keeps the protocol implementable from
//! a standard library.

use godwinmix_protocol::plugin::transcript::{self, Step};
use godwinmix_protocol::plugin::wire::MAX_LINE_BYTES;
use serde_json::Value;
use std::collections::BTreeMap;
use std::io::{BufRead, BufReader, Write};
use std::path::Path;
use std::process::{Child, Command, Stdio};
use std::sync::mpsc;
use std::time::{Duration, Instant};

/// How long one expected line may take to arrive before the replay gives up.
pub const STEP_TIMEOUT: Duration = Duration::from_secs(5);

/// What the replay found.
#[derive(Debug, Clone)]
pub struct Replay {
    /// One line per step, in order, for a terminal or a CI log.
    pub steps: Vec<String>,
    pub matched: usize,
    pub sent: usize,
    /// The first failure, if there was one.
    pub failure: Option<String>,
}

impl Replay {
    pub fn passed(&self) -> bool {
        self.failure.is_none()
    }
}

/// Replay `transcript` against `argv`, started in `cwd` with `env`.
///
/// Errors are about running the thing at all; a plugin answering the wrong
/// line is a `Replay` with a `failure`, because that is a result, not a
/// breakage.
pub fn replay(
    argv: &[String],
    cwd: &Path,
    env: &BTreeMap<String, String>,
    transcript_text: &str,
) -> anyhow::Result<Replay> {
    let steps = transcript::steps(transcript_text)
        .map_err(|e| anyhow::anyhow!("the transcript does not parse: {e}"))?;
    anyhow::ensure!(
        !steps.is_empty(),
        "the transcript has no steps. Write one object per line, each with a single key: \
         'core' for a line the core sends, 'plugin' for one the plugin must send."
    );
    let (program, args) = argv
        .split_first()
        .ok_or_else(|| anyhow::anyhow!("there is no command to run the plugin with"))?;
    let mut child = Command::new(program)
        .args(args)
        .current_dir(cwd)
        .envs(env)
        .stdin(Stdio::piped())
        .stdout(Stdio::null())
        .stderr(Stdio::piped())
        .spawn()
        .map_err(|e| anyhow::anyhow!("starting `{program}`: {e}"))?;

    let lines = reader_thread(&mut child);
    let mut stdin = child.stdin.take().expect("a piped stdin");
    let outcome = walk(&steps, &mut stdin, &lines);
    drop(stdin);
    let _ = child.kill();
    let _ = child.wait();
    Ok(outcome)
}

/// A thread reading the child's stderr, so the walk below can wait with a
/// deadline. A pipe read cannot be given one.
fn reader_thread(child: &mut Child) -> mpsc::Receiver<String> {
    let (tx, rx) = mpsc::channel();
    let Some(stderr) = child.stderr.take() else { return rx };
    std::thread::Builder::new()
        .name("offline-replay".into())
        .spawn(move || {
            let mut reader = BufReader::new(stderr);
            let mut line = String::new();
            loop {
                line.clear();
                match reader.read_line(&mut line) {
                    Ok(0) | Err(_) => break,
                    Ok(_) => {
                        if line.len() > MAX_LINE_BYTES {
                            let _ = tx.send(format!(
                                "{{\"__too_long\":{}}}",
                                line.len()
                            ));
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

/// Walk the steps: write the core's lines, wait for the plugin's.
fn walk(
    steps: &[(usize, Step)],
    stdin: &mut impl Write,
    lines: &mpsc::Receiver<String>,
) -> Replay {
    let mut out = Replay { steps: Vec::new(), matched: 0, sent: 0, failure: None };
    for (number, step) in steps {
        match step {
            Step::Ignored => {}
            Step::Core(value) => {
                let line = format!("{value}\n");
                if let Err(e) = stdin.write_all(line.as_bytes()).and_then(|()| stdin.flush()) {
                    out.failure = Some(format!(
                        "line {number}: could not write to the plugin's stdin: {e}. \
                         It has probably exited; check its stderr."
                    ));
                    return out;
                }
                out.sent += 1;
                out.steps.push(format!("  -> {}", one_line(value)));
            }
            Step::Plugin(expected) => match await_match(expected, lines) {
                Ok(got) => {
                    out.matched += 1;
                    out.steps.push(format!("  <- {}", one_line(&got)));
                }
                Err(why) => {
                    out.steps.push(format!("  !! line {number}: {why}"));
                    out.failure = Some(format!("line {number}: {why}"));
                    return out;
                }
            },
        }
    }
    out
}

/// Wait for a line that contains what the step asked for. Lines that are not
/// JSON, and JSON that is not what this step wants, are skipped: a plugin is
/// allowed to log while it works.
fn await_match(expected: &Value, lines: &mpsc::Receiver<String>) -> Result<Value, String> {
    let deadline = Instant::now() + STEP_TIMEOUT;
    let mut seen: Vec<String> = Vec::new();
    while let Some(left) = deadline.checked_duration_since(Instant::now()) {
        let Ok(line) = lines.recv_timeout(left) else { break };
        let Ok(value) = serde_json::from_str::<Value>(&line) else {
            seen.push(line);
            continue;
        };
        if transcript::matches(expected, &value) {
            return Ok(value);
        }
        seen.push(line);
    }
    let tail: Vec<&String> = seen.iter().rev().take(3).collect();
    Err(format!(
        "{} after {} s.{}",
        transcript::explain(expected, &Value::Null)
            .lines()
            .next()
            .unwrap_or("no matching line")
            .to_string(),
        STEP_TIMEOUT.as_secs(),
        if tail.is_empty() {
            " The plugin said nothing at all.".to_string()
        } else {
            format!(" The last lines it did send: {tail:?}")
        }
    ))
}

fn one_line(value: &Value) -> String {
    let text = value.to_string();
    if text.len() <= 120 {
        text
    } else {
        format!("{}...", &text[..117])
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    const TRANSCRIPT: &str = r#"
# The handshake, and nothing else.
{"plugin": {"method": "initialize", "params": {"plugin": "echo"}}}
{"core":   {"jsonrpc": "2.0", "id": 0, "result": {"core": "godwinmix", "instance": "t"}}}
{"plugin": {"method": "initialized"}}
"#;

    /// A plugin in one line of shell: it says hello, waits for the answer, and
    /// says it is initialised. Enough to prove the replay drives a real
    /// process through a real pipe.
    #[cfg(unix)]
    #[test]
    fn a_shell_plugin_walks_the_handshake_with_no_core() {
        let script = r#"
            echo '{"jsonrpc":"2.0","id":0,"method":"initialize","params":{"plugin":"echo","version":"0.1.0","api":1,"transports":["container"]}}' >&2
            read -r _answer
            echo '{"jsonrpc":"2.0","method":"initialized"}' >&2
        "#;
        let argv = vec!["sh".to_string(), "-c".to_string(), script.to_string()];
        let out = replay(&argv, Path::new("."), &BTreeMap::new(), TRANSCRIPT)
            .expect("the replay runs");
        for line in &out.steps {
            println!("{line}");
        }
        assert!(out.passed(), "{:?}", out.failure);
        assert_eq!(out.matched, 2);
        assert_eq!(out.sent, 1);
    }

    #[cfg(unix)]
    #[test]
    fn a_plugin_that_says_the_wrong_thing_is_told_what_was_wanted() {
        let argv = vec![
            "sh".to_string(),
            "-c".to_string(),
            r#"echo '{"jsonrpc":"2.0","id":0,"method":"hello"}' >&2; sleep 1"#.to_string(),
        ];
        let out = replay(&argv, Path::new("."), &BTreeMap::new(), TRANSCRIPT)
            .expect("the replay runs");
        let failure = out.failure.expect("the plugin said the wrong thing");
        assert!(failure.contains("initialize"), "{failure}");
    }

    #[test]
    fn a_transcript_with_no_steps_is_refused_before_anything_is_started() {
        let err = replay(&["true".into()], Path::new("."), &BTreeMap::new(), "# nothing\n")
            .expect_err("an empty transcript is not a test");
        assert!(format!("{err}").contains("no steps"), "{err}");
    }
}
