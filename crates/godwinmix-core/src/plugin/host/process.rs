//! One plugin process: spawn, handshake, talk, poll, stop.
//!
//! The process machinery is `input.rs`'s, unchanged: the same process group,
//! the same teardown that signals the tree and reaps it, the same PID 1
//! reaper, the same Windows stdout reader. What is added here is a control
//! channel. The child's stderr is read line by line and every line that is a
//! JSON-RPC message is one; every line that is not goes to the log at info,
//! tagged with the instance, so a Python traceback lands in the log rather
//! than in the protocol.
//!
//! Nothing in here touches GStreamer. `transport.rs` builds the media end and
//! the kinds in `source.rs`, `output.rs` and `filter.rs` put the two together.

use crate::input::{ExecChild, ExecSpec, ExecStdout, ExecStdoutHeld, StderrReader};
use anyhow::{Context, Result};
use godwinmix_host::channel::{read_line, Channel, LineError};
use godwinmix_host::handshake::{self, Negotiated, HANDSHAKE_TIMEOUT};
use godwinmix_host::launch::Launch;
use godwinmix_host::lifecycle::Lifecycle;
use godwinmix_host::SHUTDOWN_GRACE_SECS;
use godwinmix_protocol::error::ErrorCode;
use godwinmix_protocol::plugin::manifest::Manifest as PluginManifest;
use godwinmix_protocol::plugin::wire::{
    self, Canvas, Frame, Initialize, InstanceState, LogLevel, Transport, WireError,
};
use parking_lot::Mutex;
use serde_json::{json, Value};
use std::sync::mpsc;
use std::sync::Arc;
use std::time::{Duration, Instant};
use tracing::{debug, info, warn};

/// How long a call into a plugin may take before the core stops waiting. The
/// protocol's own ceiling; a health poll uses far less.
pub const CALL_TIMEOUT: Duration = Duration::from_secs(5);
/// How long the health poll waits. Short, because it runs every second and a
/// plugin that cannot answer in this time is the thing the poll is looking
/// for.
pub const HEALTH_TIMEOUT: Duration = Duration::from_millis(900);

/// What a plugin told the core outside a reply: a log line, an event, a media
/// report, a health change. Drained by the supervisor rather than acted on
/// inside the reader thread, so nothing a plugin says can block its own pipe.
#[derive(Debug, Clone)]
pub enum Notice {
    Log {
        level: LogLevel,
        message: String,
    },
    Event {
        name: String,
        params: Value,
    },
    MediaReport(Value),
    HealthChanged {
        state: String,
        detail: Option<String>,
    },
    /// The plugin made a request of the core. Answered by the supervisor,
    /// which is the only part that can reach a mixer.
    Request {
        id: Value,
        method: String,
        params: Value,
    },
    /// The channel broke. The instance goes to `failed` with this reason.
    Broken(String),
}

/// The shared state a reader thread and the owning kind both touch.
struct Shared {
    instance: String,
    channel: Channel,
    notices: Mutex<Vec<Notice>>,
    hello: Mutex<Option<Initialize>>,
    hello_signal: Mutex<Option<mpsc::Sender<()>>>,
}

/// One running plugin process and its control channel.
pub struct Sidecar {
    shared: Arc<Shared>,
    /// Dropping this kills the process. See `ExecChild`.
    child: Option<ExecChild>,
    /// The child's stdout, held until a container transport attaches it.
    stdout: Option<ExecStdout>,
    /// What the container transport handed back to be held: on unix the read
    /// end of the pipe, which `fdsrc` reads but never closes. Letting go of it
    /// is what closes it. See `ExecStdout`.
    stdout_held: ExecStdoutHeld,
    stderr: Option<StderrReader>,
    pid: Option<u32>,
    life: Lifecycle,
    negotiated: Option<Negotiated>,
    spec: ExecSpec,
}

impl Sidecar {
    /// Start the process and read its stderr. Nothing has been said yet.
    pub fn spawn(instance: &str, launch: &Launch) -> Result<Self> {
        let spec = ExecSpec {
            argv: launch.argv.clone(),
            env: launch
                .env
                .iter()
                .map(|(k, v)| (k.clone(), v.clone()))
                .collect(),
            pipe_stdin: true,
            cwd: Some(launch.cwd.clone()),
        };
        Self::spawn_spec(instance, spec)
    }

    fn spawn_spec(instance: &str, spec: ExecSpec) -> Result<Self> {
        let (stdout, mut child, _) = {
            // A placeholder handler: the real one needs the `Shared` that the
            // channel lives in, and the channel needs the child's stdin, which
            // only exists once the process is up. So the process is started
            // with no reader and one is attached below.
            let mut child = crate::input::spawn_child(&spec)?;
            let stdout_handle = child
                .stdout
                .take()
                .context("the plugin produced no stdout")?;
            #[cfg(unix)]
            let out = ExecStdout::Fd(std::os::fd::OwnedFd::from(stdout_handle));
            #[cfg(not(unix))]
            let out = ExecStdout::Pipe(stdout_handle);
            (out, child, ())
        };
        let stdin = child
            .stdin
            .take()
            .context("the plugin was given no stdin to read")?;
        let stderr = child.stderr.take();
        let pid = child.id();
        let shared = Arc::new(Shared {
            instance: instance.to_string(),
            channel: Channel::new(Box::new(stdin)),
            notices: Mutex::new(Vec::new()),
            hello: Mutex::new(None),
            hello_signal: Mutex::new(None),
        });
        let reader = stderr.map(|err| {
            let shared = shared.clone();
            StderrReader::spawn(format!("plugin-{instance}"), err, move |line| {
                absorb(&shared, line)
            })
        });
        info!(instance, pid, program = %spec.argv.first().map(String::as_str).unwrap_or(""), "started a plugin process");
        Ok(Self {
            shared,
            child: Some(ExecChild::new(child, spec.env.clone(), None, None)),
            stdout: Some(stdout),
            stdout_held: None,
            stderr: reader,
            pid: Some(pid),
            life: Lifecycle::new(),
            negotiated: None,
            spec,
        })
    }

    pub fn instance(&self) -> &str {
        &self.shared.instance
    }

    pub fn pid(&self) -> Option<u32> {
        self.pid
    }

    pub fn state(&self) -> InstanceState {
        self.life.state()
    }

    pub fn lifecycle(&self) -> &Lifecycle {
        &self.life
    }

    pub fn lifecycle_mut(&mut self) -> &mut Lifecycle {
        &mut self.life
    }

    pub fn transport(&self) -> Option<Transport> {
        self.negotiated.as_ref().map(|n| n.transport)
    }

    pub fn media_address(&self) -> &str {
        self.negotiated
            .as_ref()
            .map(|n| n.media.as_str())
            .unwrap_or("")
    }

    /// Take the child's stdout, for a container transport to attach.
    pub fn take_stdout(&mut self) -> Option<ExecStdout> {
        self.stdout.take()
    }

    /// Hold whatever the attach handed back for as long as the element reads
    /// it. On unix that is the descriptor; on Windows the reader thread owns
    /// the pipe and there is nothing to hold.
    pub fn hold_stdout(&mut self, held: ExecStdoutHeld) {
        self.stdout_held = held;
    }

    /// Everything the plugin has said since the last time this was called.
    pub fn drain(&self) -> Vec<Notice> {
        std::mem::take(&mut *self.shared.notices.lock())
    }

    /// Wait for `initialize`, answer it, and settle the transport.
    ///
    /// A plugin that has not sent it within five seconds is killed and the
    /// reason is the one this returns, which is what reaches
    /// `event/plugin.state`.
    #[allow(clippy::too_many_arguments)]
    pub fn handshake(
        &mut self,
        manifest: Option<&PluginManifest>,
        canvas: Canvas,
        provide: &str,
        params: Value,
        media_for: impl FnOnce(Transport) -> Result<String>,
    ) -> Result<&Negotiated> {
        let (tx, rx) = mpsc::channel();
        *self.shared.hello_signal.lock() = Some(tx);
        if self.shared.hello.lock().is_none() && rx.recv_timeout(HANDSHAKE_TIMEOUT).is_err() {
            anyhow::bail!(
                "the plugin did not send `initialize` within {} s. It is killed. Check its own \
                 log: a plugin writes JSON-RPC on stderr, and anything else there is in the \
                 core's log tagged with instance={}.",
                HANDSHAKE_TIMEOUT.as_secs(),
                self.shared.instance
            );
        }
        let hello = self
            .shared
            .hello
            .lock()
            .clone()
            .context("the handshake vanished")?;
        let negotiated = handshake::negotiate(
            &hello,
            manifest,
            super::super::API_LEVEL,
            super::super::API_COMPATIBLE,
            media_for,
        )?;
        let answer = handshake::ready(
            env!("CARGO_PKG_VERSION"),
            super::super::API_LEVEL,
            super::super::API_COMPATIBLE,
            canvas,
            &negotiated,
            &self.shared.instance,
            provide,
            params,
        );
        self.shared
            .channel
            .answer(&json!(0), Ok(serde_json::to_value(&answer)?))
            .context("answering the plugin's initialize")?;
        self.life.initialized();
        self.negotiated = Some(negotiated);
        debug!(
            instance = %self.shared.instance,
            transport = %self.transport().map(|t| t.as_str()).unwrap_or("none"),
            "the handshake settled"
        );
        Ok(self.negotiated.as_ref().expect("just set"))
    }

    /// Call a method and wait for the answer, with the protocol's ceiling.
    pub fn call(&self, method: &str, params: Value) -> Result<Value> {
        self.call_within(method, params, CALL_TIMEOUT)
    }

    /// The same, with a deadline of the caller's choosing.
    pub fn call_within(&self, method: &str, params: Value, within: Duration) -> Result<Value> {
        anyhow::ensure!(self.life.may_call(method), "{}", self.life.refusal(method));
        let rx = self.shared.channel.call(method, params)?;
        match rx.recv_timeout(within) {
            Ok(Ok(value)) => Ok(value),
            Ok(Err(e)) => Err(as_error(&self.shared.instance, method, e)),
            Err(mpsc::RecvTimeoutError::Timeout) => anyhow::bail!(
                "the plugin did not answer `{method}` within {} s. It is still running and the \
                 call was not cancelled; read the state back rather than assuming it failed.",
                within.as_secs_f64()
            ),
            Err(mpsc::RecvTimeoutError::Disconnected) => anyhow::bail!(
                "the plugin died while answering `{method}`. The supervisor will restart it; \
                 wait for event/plugin.state."
            ),
        }
    }

    /// Send a notification. Nothing comes back.
    pub fn notify(&self, method: &str, params: Value) -> Result<()> {
        self.shared.channel.notify(method, params)
    }

    /// Answer a request the plugin made of the core. The supervisor calls this
    /// once it has carried the request out, because it is the only part that
    /// can reach a mixer.
    pub fn answer_request(
        &self,
        id: &Value,
        result: Result<Value, godwinmix_protocol::plugin::wire::WireError>,
    ) -> Result<()> {
        self.shared.channel.answer(id, result)
    }

    /// Forward a log level change, so `log.set {instance, level}` reaches the
    /// plugin's own output rather than only the core's view of it.
    pub fn configure_log(&self, level: &str) -> Result<()> {
        self.notify("configure_log", json!({ "level": level }))
    }

    /// Ask the plugin how it is. Combined by the caller with what the buffers
    /// say, because a plugin that thinks it is fine and is producing nothing
    /// is not fine.
    pub fn health(&self) -> Result<wire::Health> {
        let value = self.call_within("health", json!({}), HEALTH_TIMEOUT)?;
        Ok(serde_json::from_value(value).unwrap_or_default())
    }

    /// Stop, then shutdown, then the process group after eight seconds.
    ///
    /// The order is 03 section 7's. Every step is allowed to fail: a plugin
    /// that has already crashed is stopped by the last one, which is the whole
    /// reason the last one exists.
    pub fn shutdown(&mut self, reason: &str) {
        let instance = self.shared.instance.clone();
        if self.life.may_call("stop") {
            if let Err(e) = self.call_within("stop", json!({}), Duration::from_secs(2)) {
                debug!(%instance, ?e, "the plugin did not answer `stop`");
            }
        }
        if self.life.may_call("shutdown") {
            let _ = self
                .shared
                .channel
                .notify("shutdown", json!({ "reason": reason }));
        }
        let deadline = Instant::now() + Duration::from_secs(SHUTDOWN_GRACE_SECS);
        while Instant::now() < deadline {
            if !self.running() {
                break;
            }
            std::thread::sleep(Duration::from_millis(50));
        }
        self.shared.channel.abandon(reason);
        if let Some(mut reader) = self.stderr.take() {
            reader.stop();
        }
        // Letting go of the child is what signals the group, waits, insists,
        // reaps and sweeps up. See `ExecChild`.
        self.child.take();
        self.stdout.take();
        self.stdout_held.take();
        self.life
            .to(InstanceState::Stopped, Some(reason.to_string()));
        info!(%instance, %reason, "stopped a plugin process");
    }

    /// Is the process still there? One non blocking wait on the child we own.
    pub fn running(&mut self) -> bool {
        match self.child.as_mut() {
            None => false,
            Some(child) => !child.finished(),
        }
    }

    /// The command this was started with, for a restart in place.
    pub fn spec(&self) -> &ExecSpec {
        &self.spec
    }
}

impl Drop for Sidecar {
    fn drop(&mut self) {
        if self.child.is_some() {
            self.shutdown("the instance was removed");
        }
    }
}

/// One line off the plugin's stderr.
///
/// The only place that decides what a line means. Runs on the reader thread,
/// so it does nothing but parse and record: acting on a notice is the
/// supervisor's job, and a plugin must never be able to block its own pipe by
/// saying something that takes the core a while to think about.
fn absorb(shared: &Arc<Shared>, line: &str) {
    let frame = match read_line(line) {
        Ok(frame) => frame,
        Err(LineError::TooLong { bytes }) => {
            warn!(instance = %shared.instance, bytes, "a plugin line was over the 4 MiB limit");
            push(
                shared,
                Notice::Broken(LineError::TooLong { bytes }.to_string()),
            );
            return;
        }
    };
    match frame {
        Frame::NonJson(text) => {
            if !text.trim().is_empty() {
                // Inside the instance's span, so `log.set {instance, level}`
                // reaches it. A traceback belongs in the log, not the protocol.
                let name = shared.instance.clone();
                crate::observe::in_instance(&name, || info!(plugin = %name, "{text}"));
            }
        }
        Frame::Response { id, result } => {
            if !shared.channel.settle(&id, result) {
                debug!(instance = %shared.instance, %id, "an answer nobody was waiting for");
            }
        }
        Frame::Notification { method, params } => notice(shared, &method, params),
        Frame::Request { id, method, params } => {
            if method == "initialize" {
                let hello: Initialize = match serde_json::from_value(params) {
                    Ok(h) => h,
                    Err(e) => {
                        push(
                            shared,
                            Notice::Broken(format!("its `initialize` did not parse: {e}")),
                        );
                        return;
                    }
                };
                *shared.hello.lock() = Some(hello);
                if let Some(tx) = shared.hello_signal.lock().take() {
                    let _ = tx.send(());
                }
                return;
            }
            push(shared, Notice::Request { id, method, params });
        }
    }
}

fn notice(shared: &Arc<Shared>, method: &str, params: Value) {
    match method {
        "initialized" => {}
        "log" => {
            let level = params
                .get("level")
                .and_then(Value::as_str)
                .and_then(LogLevel::parse)
                .unwrap_or(LogLevel::Info);
            let message = params
                .get("message")
                .and_then(Value::as_str)
                .unwrap_or("")
                .to_string();
            push(shared, Notice::Log { level, message });
        }
        "event" => {
            let name = params
                .get("name")
                .and_then(Value::as_str)
                .unwrap_or("plugin.event")
                .to_string();
            let body = params.get("params").cloned().unwrap_or(Value::Null);
            push(shared, Notice::Event { name, params: body });
        }
        "media.report" => push(shared, Notice::MediaReport(params)),
        "health.changed" => {
            let state = params
                .get("state")
                .and_then(Value::as_str)
                .unwrap_or("ok")
                .to_string();
            let detail = params
                .get("detail")
                .and_then(Value::as_str)
                .map(str::to_string);
            push(shared, Notice::HealthChanged { state, detail });
        }
        other => {
            debug!(instance = %shared.instance, method = other, "a notification the core does not read");
        }
    }
}

/// Keep the queue bounded. A plugin in a loop printing events must not grow
/// the core's memory until somebody drains it.
const MAX_NOTICES: usize = 512;

fn push(shared: &Arc<Shared>, notice: Notice) {
    let mut held = shared.notices.lock();
    if held.len() >= MAX_NOTICES {
        held.remove(0);
    }
    held.push(notice);
}

/// Turn a plugin's refusal into an error whose message names the next step.
fn as_error(instance: &str, method: &str, e: WireError) -> anyhow::Error {
    if e.code == ErrorCode::RestartRequired.number() {
        return anyhow::anyhow!(
            "{instance} cannot take that change while running: {}. Call plugin.reload.",
            e.message
        );
    }
    anyhow::anyhow!("{instance} refused `{method}`: {}", e.message)
}
