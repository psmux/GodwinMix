//! Replay a recorded transcript against the built binary, with no core running.
//!
//! Bytes in, bytes out. No sockets, no clock, no GStreamer. This is what
//! `gmx plugin test --offline` will run in the plugin's CI, and it uses the
//! same transcript format and the same subset matcher from the SDK, so a
//! transcript recorded here replays there unchanged.

use std::io::{BufRead, BufReader, Write};
use std::process::{Command, Stdio};
use std::sync::mpsc::{channel, RecvTimeoutError};
use std::time::Duration;

use godwinmix_sdk::transcript::{self, Step};

const ANSWER_TIMEOUT: Duration = Duration::from_secs(10);
const EXIT_TIMEOUT: Duration = Duration::from_secs(8);

#[test]
fn the_recorded_transcript_replays() {
    let root = env!("CARGO_MANIFEST_DIR");
    let text = std::fs::read_to_string(format!("{root}/tests/transcript.jsonl"))
        .expect("tests/transcript.jsonl is missing");
    let steps = transcript::steps(&text).expect("the transcript does not parse");

    let mut plugin = Command::new(env!("CARGO_BIN_EXE_gmx-audio-device"))
        .current_dir(root)
        .env("GMX_PLUGIN", "audio-device")
        .env("GMX_PROVIDE", "source")
        .env("GMX_INSTANCE", "test")
        .env("GMX_API_LEVEL", "1")
        .env("GMX_PLUGIN_ROOT", root)
        .stdin(Stdio::piped())
        .stdout(Stdio::null())
        .stderr(Stdio::piped())
        .spawn()
        .expect("could not start the plugin");

    // stderr is drained on its own thread, so a plugin that says nothing cannot
    // wedge the writer and the timeout below is real.
    let (tx, rx) = channel::<String>();
    let stderr = plugin.stderr.take().expect("no stderr");
    std::thread::spawn(move || {
        for line in BufReader::new(stderr).lines().map_while(Result::ok) {
            if tx.send(line).is_err() {
                break;
            }
        }
    });

    let mut stdin = plugin.stdin.take().expect("no stdin");
    let mut seen: Vec<String> = Vec::new();
    for (line, step) in steps {
        match step {
            Step::Core(value) => {
                let encoded = serde_json::to_string(&value).expect("could not encode a core line");
                if writeln!(stdin, "{encoded}").is_err() {
                    panic!(
                        "line {line}: the plugin closed stdin early.\n{}",
                        said(&seen)
                    );
                }
                stdin.flush().ok();
            }
            Step::Plugin(expected) => {
                let mut found = false;
                let deadline = std::time::Instant::now() + ANSWER_TIMEOUT;
                while !found {
                    let left = deadline.saturating_duration_since(std::time::Instant::now());
                    match rx.recv_timeout(left) {
                        Ok(raw) => {
                            seen.push(raw.clone());
                            if let Ok(actual) = serde_json::from_str::<serde_json::Value>(&raw) {
                                // Anything the transcript does not mention is a
                                // log line, not a failure. Adding a log must
                                // never break a plugin's test.
                                found = transcript::matches(&expected, &actual);
                            }
                        }
                        Err(RecvTimeoutError::Timeout) | Err(RecvTimeoutError::Disconnected) => {
                            panic!(
                                "line {line}: {}\n{}",
                                transcript::explain(&expected, &serde_json::Value::Null),
                                said(&seen)
                            );
                        }
                    }
                }
            }
            Step::Ignored => {}
        }
    }

    drop(stdin);
    let exited = wait_for_exit(&mut plugin, EXIT_TIMEOUT);
    assert!(
        exited,
        "the plugin did not exit within {} seconds of shutdown.\n{}",
        EXIT_TIMEOUT.as_secs(),
        said(&seen)
    );
}

fn wait_for_exit(plugin: &mut std::process::Child, within: Duration) -> bool {
    let deadline = std::time::Instant::now() + within;
    while std::time::Instant::now() < deadline {
        match plugin.try_wait() {
            Ok(Some(_)) => return true,
            Ok(None) => std::thread::sleep(Duration::from_millis(20)),
            Err(_) => return false,
        }
    }
    let _ = plugin.kill();
    false
}

fn said(lines: &[String]) -> String {
    let mut out = String::from("what the plugin actually said:\n");
    for line in lines.iter().take(40) {
        out.push_str("  ");
        out.push_str(&line[..line.len().min(200)]);
        out.push('\n');
    }
    out
}
