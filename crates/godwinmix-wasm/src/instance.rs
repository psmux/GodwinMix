//! One component, on a thread of its own.
//!
//! # Why a thread and not the caller's
//!
//! Because the caller is sometimes the mixer. A transition's `render` is asked
//! for while a take is being set up, and if the component ran on that thread a
//! slow one would hold the mixer's command queue for as long as it liked. So
//! the store lives on a worker, a call is a message with a reply channel, and
//! the caller waits with a deadline. When the deadline passes the caller gives
//! up and the take falls back to the built in cut; the worker is cut
//! separately by fuel or by the epoch and goes back to waiting for the next
//! job. Neither end can hold the other.
//!
//! ```text
//!   mixer / hooks / tool.call            worker thread ("wasm-<instance>")
//!        |                                     |
//!        |-- Job::Call{method, params} ------->|  set fuel, set epoch deadline
//!        |                                     |  call the export
//!        |<------------- Result ---------------|  read the answer
//!        |   (or the deadline, whichever first)
//! ```
//!
//! # Why the store is never shared
//!
//! One store per instance is the isolation. A component cannot see another's
//! memory, spend another's fuel, or be waiting on another's call. It is also
//! what makes the worker simple: the store is owned by one thread and never
//! locked.

use crate::bindings;
use crate::hostcalls::Callbacks;
use anyhow::{Context, Result};
use godwinmix_core::plugin::wasm::{Grant, Instance as CoreInstance, Spec};
use godwinmix_protocol::plugin::wire::InstanceState;
use parking_lot::Mutex;
use serde_json::Value;
use std::sync::atomic::{AtomicU64, Ordering};
use std::sync::mpsc;
use std::sync::Arc;
use std::time::Duration;

/// One unit of work for the worker.
pub(crate) enum Job {
    Call {
        method: String,
        params: Value,
        within: Duration,
        reply: mpsc::Sender<Result<Value>>,
    },
    Shutdown {
        reason: String,
    },
}

/// A running component, as the core sees it.
pub struct Component {
    plugin: String,
    instance: String,
    jobs: Mutex<Option<mpsc::Sender<Job>>>,
    state: Mutex<InstanceState>,
    tools: Vec<String>,
    hooks: Vec<String>,
    memory: Arc<AtomicU64>,
    deadline: Duration,
}

impl Component {
    /// Bring one up: compile, instantiate, hand shake, and leave the worker
    /// waiting for jobs.
    ///
    /// Everything that can fail happens before this returns, so a component
    /// that will not load never reaches the supervisor's table.
    pub fn start(spec: Spec) -> Result<Arc<dyn CoreInstance>> {
        let (jobs, inbox) = mpsc::channel::<Job>();
        let (ready_tx, ready_rx) = mpsc::channel::<Result<Handshake>>();
        let memory = Arc::new(AtomicU64::new(0));
        let name = spec.instance.clone();
        let plugin = spec.plugin.clone();
        let deadline = spec.grant.deadline;
        let watched = memory.clone();
        std::thread::Builder::new()
            .name(format!("wasm-{name}"))
            .spawn(move || super::worker::run(spec, inbox, ready_tx, watched))
            .with_context(|| format!("starting the worker thread for `{name}`"))?;
        // Compiling a component takes as long as it takes; the handshake after
        // it is held to the instance's own deadline by the worker.
        let ready = ready_rx
            .recv_timeout(START_TIMEOUT)
            .with_context(|| {
                format!(
                    "`{name}` did not load inside {} seconds. A component this slow to compile \
                     is usually one built without --release.",
                    START_TIMEOUT.as_secs()
                )
            })?
            .with_context(|| format!("loading `{name}`"))?;
        Ok(Arc::new(Component {
            plugin,
            instance: name,
            jobs: Mutex::new(Some(jobs)),
            state: Mutex::new(InstanceState::Ready),
            tools: ready.tools,
            hooks: ready.hooks,
            memory,
            deadline,
        }))
    }

    fn send(&self, method: &str, params: Value, within: Duration) -> Result<Value> {
        let (reply, answer) = mpsc::channel();
        let job = Job::Call { method: method.to_string(), params, within, reply };
        {
            let guard = self.jobs.lock();
            let sender = guard.as_ref().with_context(|| {
                format!(
                    "`{}` has been shut down, so `{method}` has nowhere to go. \
                     `plugin.reload {}` starts it again.",
                    self.instance, self.plugin
                )
            })?;
            sender.send(job).map_err(|_| {
                anyhow::anyhow!(
                    "the worker for `{}` is gone, so `{method}` has nowhere to go",
                    self.instance
                )
            })?;
        }
        // A little longer than the component's own deadline, so the worker's
        // answer wins the race in the ordinary case and this is only reached
        // when the worker itself is wedged.
        match answer.recv_timeout(within + GRACE) {
            Ok(result) => result,
            Err(_) => {
                *self.state.lock() = InstanceState::Degraded;
                anyhow::bail!(
                    "`{method}` on `{}` did not answer inside {} ms and was abandoned. The \
                     component is held to that deadline by its epoch, so it is cut too; the \
                     next call starts clean.",
                    self.instance,
                    within.as_millis()
                )
            }
        }
    }
}

/// How long a component may take to compile and hand shake.
const START_TIMEOUT: Duration = Duration::from_secs(20);

/// The margin between the component's deadline and the caller's wait.
const GRACE: Duration = Duration::from_millis(50);

/// What the worker reports once the handshake is done.
pub struct Handshake {
    pub tools: Vec<String>,
    pub hooks: Vec<String>,
}

impl CoreInstance for Component {
    fn call(&self, method: &str, params: Value) -> Result<Value> {
        self.send(method, params, self.deadline)
    }

    fn call_within(&self, method: &str, params: Value, within: Duration) -> Result<Value> {
        self.send(method, params, within.min(self.deadline))
    }

    fn tools(&self) -> Vec<String> {
        self.tools.clone()
    }

    fn hooks(&self) -> Vec<String> {
        self.hooks.clone()
    }

    fn state(&self) -> InstanceState {
        *self.state.lock()
    }

    fn memory_bytes(&self) -> u64 {
        self.memory.load(Ordering::Relaxed)
    }

    fn shutdown(&self, reason: &str) {
        let sender = self.jobs.lock().take();
        if let Some(sender) = sender {
            let _ = sender.send(Job::Shutdown { reason: reason.to_string() });
        }
        *self.state.lock() = InstanceState::Stopped;
    }
}

// ---------------------------------------------------------------------------
// The worker's side of the message
// ---------------------------------------------------------------------------

/// The one error shape, from the component's side of the boundary to the
/// core's.
pub(crate) fn from_wit(e: &bindings::types::Error) -> anyhow::Error {
    let data = e.data_json.as_deref().unwrap_or("");
    if data.is_empty() {
        anyhow::anyhow!("{} (code {})", e.message, e.code)
    } else {
        anyhow::anyhow!("{} (code {}, data {data})", e.message, e.code)
    }
}

/// What the grant looks like on the component's side.
pub(crate) fn capabilities(grant: &Grant) -> bindings::types::Capabilities {
    bindings::types::Capabilities {
        core_call: grant.core_call,
        take: grant.take,
        events: grant.events,
        filesystem: grant.filesystem,
        network: grant.network,
        fuel_per_call: grant.fuel_per_call,
        deadline_ms: grant.deadline.as_millis().min(u32::MAX as u128) as u32,
        max_memory_mb: grant.max_memory_mb,
    }
}

/// Everything one instance's host functions need, built from the spec.
pub(crate) fn callbacks(spec: &Spec) -> Callbacks {
    Callbacks {
        plugin: spec.plugin.clone(),
        instance: spec.instance.clone(),
        grant: spec.grant.clone(),
    }
}
