//! The main loop: the handshake, then JSON lines until the core says stop.
//!
//! Two threads. The reader thread owns stdin and the state machine and never
//! blocks on the plugin, so `health` is answered in microseconds while a slow
//! `start` is still running. The worker thread owns the plugin and runs one
//! call at a time, in the order they arrived.
//!
//! That split is the whole reason the rule in 03 section 6 is keepable: "a
//! plugin must keep answering `health` while a slow `start` or `configure` is
//! pending".

use std::sync::mpsc::{channel, Sender};
use std::sync::{Arc, Mutex};

use serde_json::Value;

use crate::framing::{FramingError, Reader, Writer};
use crate::handshake::{check_ready, initialize_params, Machine};
use crate::manifest::Manifest;
use crate::plugin::{Handler, Reporter};
use crate::wire::{codes, Health, Id, Message, Ready, Request, RpcError, Transport};

/// What can end a run.
#[derive(Debug)]
pub enum RunError {
    /// The core never answered `initialize`, or answered with something else.
    Handshake(String),
    /// The core sent a line the channel cannot recover from.
    Framing(FramingError),
    /// stderr or stdin failed.
    Io(std::io::Error),
    /// The plugin refused the handshake.
    Refused(RpcError),
}

impl std::fmt::Display for RunError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            RunError::Handshake(s) => write!(f, "the handshake failed: {s}"),
            RunError::Framing(e) => write!(f, "{e}"),
            RunError::Io(e) => write!(f, "{e}"),
            RunError::Refused(e) => write!(f, "{e}"),
        }
    }
}

impl std::error::Error for RunError {}

/// One job for the worker thread.
enum Job {
    Call {
        id: Option<Id>,
        method: String,
        params: Value,
    },
    Shutdown {
        id: Option<Id>,
        reason: String,
    },
}

/// Run a plugin against this process's stdin and stderr.
///
/// Returns when the core sends `shutdown` or closes stdin. In container mode
/// stdout belongs to the media writer and this function never touches it.
pub fn run<H: Handler + 'static>(manifest: &Manifest, handler: H) -> Result<(), RunError> {
    crate::crash::install_from_env();
    let writer = Writer::stderr();
    let stdin = std::io::stdin();
    let reader = Reader::new(stdin.lock());
    run_on(manifest, handler, reader, writer)
}

/// Run against any reader and writer. The tests drive this directly, and so
/// does an offline replay.
pub fn run_on<H: Handler + 'static, R: std::io::BufRead>(
    manifest: &Manifest,
    mut handler: H,
    mut reader: Reader<R>,
    writer: Arc<Writer>,
) -> Result<(), RunError> {
    let transports = first_provide_transports(manifest);
    let ready = handshake(manifest, transports, &mut reader, &writer)?;

    let health = Arc::new(Mutex::new(Health::ok()));
    let reporter = Reporter::new(Arc::clone(&writer), Arc::clone(&health));
    let result = handler
        .on_initialize(&ready, reporter)
        .map_err(RunError::Refused)?;
    if let Some(latency) = result.latency_ms {
        let _ = writer.notify(
            "media.report",
            serde_json::json!({"latency_ms": latency}),
        );
    }
    // The handshake is complete only once this notification is out.
    writer
        .send(&Request::notify("initialized", serde_json::json!({})))
        .map_err(RunError::Io)?;

    let machine = Arc::new(Mutex::new(Machine::new()));
    machine.lock().unwrap().initialized();
    let has_media = handler.starts("start");

    let (jobs, worker) = spawn_worker(handler, Arc::clone(&writer), Arc::clone(&health), Arc::clone(&machine));
    let outcome = read_loop(
        &mut reader,
        &writer,
        &health,
        &machine,
        &jobs,
        has_media,
    );
    drop(jobs);
    let _ = worker.join();
    outcome
}

fn first_provide_transports(manifest: &Manifest) -> Vec<Transport> {
    manifest
        .provides
        .first()
        .map(|p| p.transports.clone())
        .filter(|t| !t.is_empty())
        .unwrap_or_else(|| vec![Transport::Container])
}

/// Send `initialize`, wait for the answer, check it.
fn handshake<R: std::io::BufRead>(
    manifest: &Manifest,
    transports: Vec<Transport>,
    reader: &mut Reader<R>,
    writer: &Arc<Writer>,
) -> Result<Ready, RunError> {
    let id = writer.next_id();
    let params = initialize_params(
        &manifest.plugin.name,
        &manifest.plugin.version,
        manifest.plugin.api,
        transports,
        manifest.provides_json(),
    );
    let request = Request::call(
        id.clone(),
        "initialize",
        serde_json::to_value(&params)
            .map_err(|e| RunError::Handshake(format!("could not encode initialize: {e}")))?,
    );
    writer.send(&request).map_err(RunError::Io)?;

    loop {
        let message = reader.next_message().map_err(RunError::Framing)?;
        let Some(message) = message else {
            return Err(RunError::Handshake(
                "the core closed stdin before answering initialize.".into(),
            ));
        };
        match message {
            Message::Response(response) if response.id == Some(id.clone()) => {
                if let Some(error) = response.error {
                    return Err(RunError::Handshake(format!(
                        "the core refused initialize: {error}"
                    )));
                }
                let value = response.result.unwrap_or(Value::Null);
                let ready: Ready = serde_json::from_value(value).map_err(|e| {
                    RunError::Handshake(format!("the core's answer did not parse: {e}"))
                })?;
                check_ready(&ready, manifest.plugin.api).map_err(RunError::Refused)?;
                return Ok(ready);
            }
            Message::NonJson(line) if !line.trim().is_empty() => {
                let _ = writer.log(
                    crate::wire::LogLevel::Debug,
                    &format!("ignored a non JSON line during the handshake: {line}"),
                );
            }
            Message::Request(request) => {
                // Nothing is legal before `initialized`. Say so rather than
                // leaving the caller waiting.
                let _ = writer.respond_error(
                    request.id,
                    crate::handshake::State::Starting.refuse(&request.method),
                );
            }
            _ => {}
        }
    }
}

fn spawn_worker<H: Handler + 'static>(
    mut handler: H,
    writer: Arc<Writer>,
    health: Arc<Mutex<Health>>,
    machine: Arc<Mutex<Machine>>,
) -> (Sender<Job>, std::thread::JoinHandle<()>) {
    let (tx, rx) = channel::<Job>();
    let thread = std::thread::Builder::new()
        .name("gmx-plugin".into())
        .spawn(move || {
            while let Ok(job) = rx.recv() {
                match job {
                    Job::Call { id, method, params } => {
                        let outcome = handler.on_call(&method, params);
                        if outcome.is_err() && method == "start" {
                            // The reader moved us to running optimistically.
                            machine.lock().unwrap_or_else(|e| e.into_inner()).stopped();
                        }
                        respond(&writer, id, outcome);
                        let fresh = handler.on_health();
                        *health.lock().unwrap_or_else(|e| e.into_inner()) = fresh;
                    }
                    Job::Shutdown { id, reason } => {
                        handler.on_shutdown(&reason);
                        respond(&writer, id, Ok(serde_json::json!({})));
                        break;
                    }
                }
            }
        })
        .expect("could not start the plugin worker thread");
    (tx, thread)
}

fn respond(writer: &Arc<Writer>, id: Option<Id>, outcome: Result<Value, RpcError>) {
    let Some(id) = id else {
        // A notification wants nothing back, but an error in one is worth a log
        // line rather than silence.
        if let Err(e) = outcome {
            let _ = writer.log(crate::wire::LogLevel::Warn, &format!("notification failed: {e}"));
        }
        return;
    };
    let _ = match outcome {
        Ok(value) => writer.respond(id, value),
        Err(error) => writer.respond_error(Some(id), error),
    };
}

/// The reader thread. Answers `health` itself, queues everything else.
fn read_loop<R: std::io::BufRead>(
    reader: &mut Reader<R>,
    writer: &Arc<Writer>,
    health: &Arc<Mutex<Health>>,
    machine: &Arc<Mutex<Machine>>,
    jobs: &Sender<Job>,
    has_media: bool,
) -> Result<(), RunError> {
    loop {
        let message = match reader.next_message() {
            Ok(Some(m)) => m,
            Ok(None) => {
                // stdin closed. The core is gone; stop the plugin and leave.
                let _ = jobs.send(Job::Shutdown {
                    id: None,
                    reason: "stdin closed".into(),
                });
                return Ok(());
            }
            Err(e @ FramingError::LineTooLong { .. }) => {
                let _ = writer.respond_error(None, e.into());
                let _ = jobs.send(Job::Shutdown {
                    id: None,
                    reason: "line too long".into(),
                });
                return Ok(());
            }
            Err(e) => return Err(RunError::Framing(e)),
        };

        let request = match message {
            Message::Request(r) | Message::Notification(r) => r,
            Message::Response(_) => continue,
            Message::NonJson(line) => {
                if !line.trim().is_empty() {
                    let _ = writer.log(
                        crate::wire::LogLevel::Debug,
                        &format!("ignored a non JSON line on stdin: {line}"),
                    );
                }
                continue;
            }
        };

        let method = request.method.clone();
        let params = request.params.clone().unwrap_or(Value::Null);

        // Answered here, not in the worker, so a slow start never hides it.
        if method == "health" {
            let current = health.lock().unwrap_or_else(|e| e.into_inner()).clone();
            if let Some(id) = request.id {
                let _ = writer.respond(
                    id,
                    serde_json::to_value(&current).unwrap_or(Value::Null),
                );
            }
            continue;
        }

        if method == "shutdown" {
            let reason = params
                .get("reason")
                .and_then(Value::as_str)
                .unwrap_or("")
                .to_string();
            let _ = jobs.send(Job::Shutdown {
                id: request.id,
                reason,
            });
            return Ok(());
        }

        {
            let mut m = machine.lock().unwrap_or_else(|e| e.into_inner());
            if let Err(error) = m.check(&method) {
                let _ = writer.respond_error(request.id, error);
                continue;
            }
            // Move optimistically so a `stop` arriving behind a slow `start` is
            // legal rather than refused. The worker rolls back on failure.
            if has_media {
                match method.as_str() {
                    "start" => m.started(),
                    "stop" => m.stopped(),
                    _ => {}
                }
            }
        }

        if jobs
            .send(Job::Call {
                id: request.id.clone(),
                method,
                params,
            })
            .is_err()
        {
            let _ = writer.respond_error(
                request.id,
                RpcError::new(
                    codes::PLUGIN_DIED,
                    "the plugin's worker thread is gone. The supervisor will restart this \
                     process; retry after event/plugin.state says running.",
                ),
            );
            return Ok(());
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::plugin::{Source, SourceHandler};
    use crate::wire::{Canvas, Configure, InitializeResult, StartParams, StartResult};
    use std::io::Cursor;
    use std::sync::atomic::{AtomicUsize, Ordering};

    const MANIFEST: &str = r#"
[plugin]
name = "bars"
version = "0.1.0"
api = 1
description = "Colour bars for a test."
license = "MIT"
platforms = ["linux-x86_64", "macos-aarch64", "windows-x86_64"]
placements = ["sidecar"]
[run]
bin = { "linux-x86_64" = "bars", "macos-aarch64" = "bars", "windows-x86_64" = "bars.exe" }
[[provides]]
kind = "source"
id = "source"
media = { video = "raw", audio = "none" }
transports = ["container"]
settings = "settings.json"
"#;

    #[derive(Clone, Default)]
    struct Sink(Arc<Mutex<Vec<u8>>>);

    impl std::io::Write for Sink {
        fn write(&mut self, buf: &[u8]) -> std::io::Result<usize> {
            self.0.lock().unwrap().extend_from_slice(buf);
            Ok(buf.len())
        }
        fn flush(&mut self) -> std::io::Result<()> {
            Ok(())
        }
    }

    impl Sink {
        fn lines(&self) -> Vec<Value> {
            let bytes = self.0.lock().unwrap().clone();
            String::from_utf8(bytes)
                .unwrap()
                .lines()
                .filter(|l| !l.trim().is_empty())
                .map(|l| serde_json::from_str(l).unwrap_or_else(|e| panic!("{l}: {e}")))
                .collect()
        }
    }

    struct Counting {
        started: Arc<AtomicUsize>,
        stopped: Arc<AtomicUsize>,
        slow_start: bool,
    }

    impl Source for Counting {
        fn initialize(
            &mut self,
            ready: &Ready,
            reporter: Reporter,
        ) -> Result<InitializeResult, RpcError> {
            reporter.info(format!("canvas {}x{}", ready.canvas.width, ready.canvas.height));
            Ok(InitializeResult {
                latency_ms: Some(0),
            })
        }
        fn start(&mut self, _: &StartParams) -> Result<StartResult, RpcError> {
            if self.slow_start {
                std::thread::sleep(std::time::Duration::from_millis(150));
            }
            self.started.fetch_add(1, Ordering::SeqCst);
            Ok(StartResult::default())
        }
        fn stop(&mut self) -> Result<(), RpcError> {
            self.stopped.fetch_add(1, Ordering::SeqCst);
            Ok(())
        }
        fn configure(&mut self, _: Value) -> Result<Configure, RpcError> {
            Ok(Configure::applied())
        }
    }

    fn ready_line(id: i64) -> String {
        let ready = Ready::for_test(Canvas::new(1280, 720, 30));
        serde_json::to_string(&serde_json::json!({
            "jsonrpc": "2.0", "id": id, "result": ready
        }))
        .unwrap()
    }

    fn drive(core_lines: &[String], slow_start: bool) -> (Sink, usize, usize) {
        let manifest = Manifest::parse(MANIFEST).unwrap();
        let started = Arc::new(AtomicUsize::new(0));
        let stopped = Arc::new(AtomicUsize::new(0));
        let source = Counting {
            started: Arc::clone(&started),
            stopped: Arc::clone(&stopped),
            slow_start,
        };
        let sink = Sink::default();
        let writer = Writer::new(Box::new(sink.clone()));
        let input = core_lines.join("\n") + "\n";
        let reader = Reader::new(Cursor::new(input));
        run_on(&manifest, SourceHandler(source), reader, writer).unwrap();
        (
            sink,
            started.load(Ordering::SeqCst),
            stopped.load(Ordering::SeqCst),
        )
    }

    #[test]
    fn the_plugin_speaks_first_and_says_initialized_after_the_answer() {
        let (sink, _, _) = drive(&[ready_line(0)], false);
        let lines = sink.lines();
        assert_eq!(lines[0]["method"], "initialize");
        assert_eq!(lines[0]["params"]["plugin"], "bars");
        assert_eq!(lines[0]["params"]["api"], 1);
        assert_eq!(lines[0]["params"]["transports"][0], "container");
        assert_eq!(lines[0]["params"]["provides"][0]["kind"], "source");
        let initialized = lines
            .iter()
            .position(|l| l["method"] == "initialized")
            .expect("no initialized notification");
        assert!(initialized > 0);
        assert!(lines[initialized].get("id").is_none());
    }

    #[test]
    fn start_stop_and_shutdown_run_in_order() {
        let (sink, started, stopped) = drive(
            &[
                ready_line(0),
                r#"{"jsonrpc":"2.0","id":1,"method":"start","params":{"canvas":{"width":1280,"height":720,"fps":30},"transport":"container","media":""}}"#.into(),
                r#"{"jsonrpc":"2.0","id":2,"method":"stop","params":{}}"#.into(),
                r#"{"jsonrpc":"2.0","id":3,"method":"shutdown","params":{"reason":"test"}}"#.into(),
            ],
            false,
        );
        assert_eq!(started, 1);
        // stop once from the call, once more from on_shutdown.
        assert!(stopped >= 1);
        let lines = sink.lines();
        for id in [1, 2, 3] {
            assert!(
                lines.iter().any(|l| l["id"] == id && l.get("result").is_some()),
                "no result for id {id} in {lines:#?}"
            );
        }
    }

    #[test]
    fn health_is_answered_while_a_slow_start_is_pending() {
        let (sink, started, _) = drive(
            &[
                ready_line(0),
                r#"{"jsonrpc":"2.0","id":1,"method":"start","params":{"canvas":{"width":1280,"height":720,"fps":30},"transport":"container","media":""}}"#.into(),
                r#"{"jsonrpc":"2.0","id":2,"method":"health","params":{}}"#.into(),
                r#"{"jsonrpc":"2.0","id":3,"method":"shutdown","params":{}}"#.into(),
            ],
            true,
        );
        assert_eq!(started, 1);
        let lines = sink.lines();
        let health_at = lines
            .iter()
            .position(|l| l["id"] == 2)
            .expect("health was never answered");
        let start_at = lines
            .iter()
            .position(|l| l["id"] == 1)
            .expect("start was never answered");
        assert!(
            health_at < start_at,
            "health must be answered before a slow start returns: {lines:#?}"
        );
        assert_eq!(lines[health_at]["result"]["state"], "ok");
    }

    #[test]
    fn a_call_in_the_wrong_state_is_minus_32001() {
        let (sink, _, _) = drive(
            &[
                ready_line(0),
                r#"{"jsonrpc":"2.0","id":1,"method":"stop","params":{}}"#.into(),
            ],
            false,
        );
        let lines = sink.lines();
        let refusal = lines.iter().find(|l| l["id"] == 1).expect("no answer");
        assert_eq!(refusal["error"]["code"], codes::WRONG_STATE);
        assert!(
            refusal["error"]["message"]
                .as_str()
                .unwrap()
                .contains("ready"),
            "{refusal}"
        );
    }

    #[test]
    fn closing_stdin_stops_the_plugin_cleanly() {
        let (_, _, stopped) = drive(&[ready_line(0)], false);
        assert_eq!(stopped, 1, "on_shutdown must stop the source");
    }

    #[test]
    fn a_non_json_line_on_stdin_is_a_log_not_a_crash() {
        let (sink, _, _) = drive(&[ready_line(0), "this is not JSON".into()], false);
        let lines = sink.lines();
        assert!(lines
            .iter()
            .any(|l| l["method"] == "log" && l["params"]["message"].as_str().unwrap().contains("not JSON")));
    }

    #[test]
    fn a_core_that_never_answers_initialize_is_named_as_the_problem() {
        let manifest = Manifest::parse(MANIFEST).unwrap();
        let source = Counting {
            started: Arc::new(AtomicUsize::new(0)),
            stopped: Arc::new(AtomicUsize::new(0)),
            slow_start: false,
        };
        let writer = Writer::new(Box::new(std::io::sink()));
        let reader = Reader::new(Cursor::new(String::new()));
        let err = run_on(&manifest, SourceHandler(source), reader, writer).unwrap_err();
        assert!(err.to_string().contains("closed stdin"), "{err}");
    }
}
