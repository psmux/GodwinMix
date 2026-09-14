//! Work that takes longer than a call is allowed to.
//!
//! 09 section 5 items 9 and 10, and 03 section 6: no method blocks for more
//! than five seconds. A method whose work would run over answers at once with
//! `{task_id, poll_interval_ms}`, the work carries on, and `task.get` reads
//! the outcome later. A client that gave up waiting is told the outcome is
//! `indeterminate`, never `failed`, because a plugin that keeps installing
//! after the caller was told it failed is worse than an honest "unknown, read
//! back before retrying".
//!
//! The table is in the engine rather than in the control plane so that the
//! plugin loader, the converter and anything else long running can put work
//! on it without depending on a server. `spawn` is the one way in.

use parking_lot::Mutex;
use serde::{Deserialize, Serialize};
use serde_json::Value;
use std::collections::HashMap;
use std::sync::atomic::{AtomicBool, AtomicU64, Ordering};
use std::sync::Arc;
use std::time::{Duration, Instant};

/// How long a finished task can still be read. 03 section 6 says an hour.
pub const TTL: Duration = Duration::from_secs(60 * 60);
/// How often a client should ask. Long enough not to be a poll loop, short
/// enough that a conversion finishing is noticed in the same breath.
pub const POLL_INTERVAL_MS: u64 = 1_000;
/// Tasks held at once. A show that ran for a week with a broken plugin
/// reloading in a loop must not be a leak.
const MAX_TASKS: usize = 256;

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize, schemars::JsonSchema)]
#[serde(rename_all = "lowercase")]
pub enum TaskState {
    Running,
    Completed,
    Failed,
    Cancelled,
}

impl TaskState {
    pub fn finished(self) -> bool {
        !matches!(self, Self::Running)
    }
}

/// What `task.get` answers with.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize, schemars::JsonSchema)]
pub struct TaskView {
    pub task_id: String,
    /// The method that started it, so a client reading a list knows what it is
    /// looking at.
    pub kind: String,
    pub state: TaskState,
    /// 0 to 1 where the work can say, absent where it cannot.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub progress: Option<f64>,
    /// The body the method would have answered with, once it is done.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub result: Option<Value>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub error: Option<String>,
    /// Seconds since the task was started.
    pub age_secs: u64,
    /// How long to wait before asking again, while it is still running.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub poll_interval_ms: Option<u64>,
}

struct Entry {
    kind: String,
    state: TaskState,
    progress: Option<f64>,
    result: Option<Value>,
    error: Option<String>,
    started: Instant,
    finished_at: Option<Instant>,
    cancel: Arc<AtomicBool>,
}

/// Every task this core knows about.
#[derive(Default)]
pub struct Tasks {
    entries: Mutex<HashMap<String, Entry>>,
    next: AtomicU64,
}

/// What the work is given: a way to report progress and a way to find out it
/// is no longer wanted.
#[derive(Clone)]
pub struct TaskContext {
    id: String,
    tasks: Arc<Tasks>,
    cancel: Arc<AtomicBool>,
}

impl TaskContext {
    pub fn id(&self) -> &str {
        &self.id
    }

    /// Say how far along the work is, 0 to 1.
    pub fn progress(&self, fraction: f64) {
        let mut entries = self.tasks.entries.lock();
        if let Some(entry) = entries.get_mut(&self.id) {
            entry.progress = Some(fraction.clamp(0.0, 1.0));
        }
    }

    /// `task.cancel` is cooperative: the work checks this and stops tidily.
    pub fn cancelled(&self) -> bool {
        self.cancel.load(Ordering::Relaxed)
    }
}

impl Tasks {
    pub fn new() -> Arc<Self> {
        Arc::new(Self::default())
    }

    /// Start a piece of work and answer with the handle for it.
    ///
    /// The future runs on the Tokio runtime the caller is on. It is expected
    /// not to block: work that does belongs on `spawn_blocking` inside the
    /// future. The task's result is the body the method would have answered
    /// with had it been quick enough.
    pub fn spawn<F, Fut>(self: &Arc<Self>, kind: &str, work: F) -> String
    where
        F: FnOnce(TaskContext) -> Fut + Send + 'static,
        Fut: std::future::Future<Output = Result<Value, String>> + Send + 'static,
    {
        let id = self.reserve(kind);
        let cancel = self
            .entries
            .lock()
            .get(&id)
            .map(|e| e.cancel.clone())
            .unwrap_or_default();
        let ctx = TaskContext { id: id.clone(), tasks: self.clone(), cancel };
        let tasks = self.clone();
        tokio::spawn(async move {
            let outcome = work(ctx.clone()).await;
            tasks.finish(&ctx.id, outcome);
        });
        id
    }

    /// A slug rather than a UUID, because 09 section 5 item 6 asks for ids a
    /// person can read back over a telephone.
    fn reserve(&self, kind: &str) -> String {
        let n = self.next.fetch_add(1, Ordering::Relaxed) + 1;
        let stem: String = kind.replace('.', "-");
        let id = format!("{stem}-{n}");
        let mut entries = self.entries.lock();
        prune(&mut entries);
        entries.insert(
            id.clone(),
            Entry {
                kind: kind.to_string(),
                state: TaskState::Running,
                progress: None,
                result: None,
                error: None,
                started: Instant::now(),
                finished_at: None,
                cancel: Arc::new(AtomicBool::new(false)),
            },
        );
        id
    }

    fn finish(&self, id: &str, outcome: Result<Value, String>) {
        let mut entries = self.entries.lock();
        let Some(entry) = entries.get_mut(id) else { return };
        entry.finished_at = Some(Instant::now());
        // A task that was asked to stop and then stopped is cancelled, not
        // failed: the caller asked for this and does not need an error.
        if entry.cancel.load(Ordering::Relaxed) {
            entry.state = TaskState::Cancelled;
            entry.error = outcome.err();
            return;
        }
        match outcome {
            Ok(value) => {
                entry.state = TaskState::Completed;
                entry.progress = Some(1.0);
                entry.result = Some(value);
            }
            Err(message) => {
                entry.state = TaskState::Failed;
                entry.error = Some(message);
            }
        }
    }

    pub fn get(&self, id: &str) -> Option<TaskView> {
        let mut entries = self.entries.lock();
        prune(&mut entries);
        entries.get(id).map(|e| view(id, e))
    }

    /// Every task, newest first, for `task.list` and for a support bundle.
    pub fn list(&self) -> Vec<TaskView> {
        let mut entries = self.entries.lock();
        prune(&mut entries);
        let mut all: Vec<TaskView> = entries.iter().map(|(id, e)| view(id, e)).collect();
        all.sort_by_key(|t| t.age_secs);
        all
    }

    /// Ask a task to stop. Cooperative: the answer says the request landed,
    /// not that the work has stopped yet.
    pub fn cancel(&self, id: &str) -> Option<TaskView> {
        let mut entries = self.entries.lock();
        let entry = entries.get_mut(id)?;
        if entry.state == TaskState::Running {
            entry.cancel.store(true, Ordering::Relaxed);
        }
        Some(view(id, entry))
    }

    /// The ids that exist, for an error that names the alternatives.
    pub fn ids(&self) -> Vec<String> {
        self.entries.lock().keys().cloned().collect()
    }

    pub fn len(&self) -> usize {
        self.entries.lock().len()
    }

    pub fn is_empty(&self) -> bool {
        self.len() == 0
    }
}

fn view(id: &str, e: &Entry) -> TaskView {
    TaskView {
        task_id: id.to_string(),
        kind: e.kind.clone(),
        state: e.state,
        progress: e.progress,
        result: e.result.clone(),
        error: e.error.clone(),
        age_secs: e.started.elapsed().as_secs(),
        poll_interval_ms: (e.state == TaskState::Running).then_some(POLL_INTERVAL_MS),
    }
}

/// Finished tasks older than the TTL go, and a table that has grown past the
/// cap loses its oldest finished entries. A running task is never dropped.
fn prune(entries: &mut HashMap<String, Entry>) {
    entries.retain(|_, e| {
        e.state == TaskState::Running
            || e.finished_at.is_none_or(|at| at.elapsed() < TTL)
    });
    while entries.len() > MAX_TASKS {
        let oldest = entries
            .iter()
            .filter(|(_, e)| e.state.finished())
            .max_by_key(|(_, e)| e.started.elapsed())
            .map(|(k, _)| k.clone());
        match oldest {
            Some(key) => {
                entries.remove(&key);
            }
            None => break,
        }
    }
}

/// The body a method answers with when it handed the work to a task.
///
/// `outcome: "indeterminate"` is the honest word for it: the work is still
/// running, nothing has failed, and the caller reads `task.get` to find out.
pub fn handle_body(task_id: &str, extra: Option<Value>) -> Value {
    let mut body = serde_json::json!({
        "task_id": task_id,
        "poll_interval_ms": POLL_INTERVAL_MS,
        "outcome": "indeterminate",
        "state": "running",
        "next": format!(
            "the work is still running. Call task.get with task_id '{task_id}' every \
             {POLL_INTERVAL_MS} ms until state is completed, failed or cancelled. Over MCP: \
             tasks/get if your client speaks the tasks extension, otherwise search_tools \
             for \"task\" and call task_get."
        ),
    });
    if let (Some(Value::Object(extra)), Some(map)) = (extra, body.as_object_mut()) {
        for (k, v) in extra {
            map.entry(k).or_insert(v);
        }
    }
    body
}

#[cfg(test)]
mod tests {
    use super::*;
    use serde_json::json;

    #[tokio::test]
    async fn a_task_runs_reports_progress_and_keeps_its_answer() {
        let tasks = Tasks::new();
        let id = tasks.spawn("media.convert", |ctx| async move {
            ctx.progress(0.5);
            Ok(json!({ "name": "clip.mp4", "converted": true }))
        });
        assert!(id.starts_with("media-convert-"), "a legible id, not a UUID: {id}");

        for _ in 0..100 {
            if tasks.get(&id).unwrap().state.finished() {
                break;
            }
            tokio::time::sleep(Duration::from_millis(5)).await;
        }
        let view = tasks.get(&id).expect("the task is still readable");
        assert_eq!(view.state, TaskState::Completed);
        assert_eq!(view.progress, Some(1.0));
        assert_eq!(view.result.unwrap()["name"], "clip.mp4");
        assert_eq!(view.poll_interval_ms, None, "a finished task is not polled again");
        assert_eq!(view.kind, "media.convert");
    }

    #[tokio::test]
    async fn work_that_goes_wrong_is_failed_with_its_reason() {
        let tasks = Tasks::new();
        let id = tasks.spawn("plugin.add", |_| async move {
            Err("the manifest names api = 2 and this core speaks 1. Ask the author for a \
                 build against api 1."
                .to_string())
        });
        for _ in 0..100 {
            if tasks.get(&id).unwrap().state.finished() {
                break;
            }
            tokio::time::sleep(Duration::from_millis(5)).await;
        }
        let view = tasks.get(&id).unwrap();
        assert_eq!(view.state, TaskState::Failed);
        assert!(view.error.unwrap().contains("api 1"));
    }

    #[tokio::test]
    async fn cancelling_is_cooperative_and_the_task_says_so() {
        let tasks = Tasks::new();
        let id = tasks.spawn("plugin.add", |ctx| async move {
            for _ in 0..200 {
                if ctx.cancelled() {
                    return Ok(json!({ "stopped": true }));
                }
                tokio::time::sleep(Duration::from_millis(5)).await;
            }
            Ok(json!({ "stopped": false }))
        });
        let view = tasks.cancel(&id).expect("the task exists");
        assert_eq!(view.state, TaskState::Running, "the request landed, the work has not stopped yet");
        for _ in 0..200 {
            if tasks.get(&id).unwrap().state.finished() {
                break;
            }
            tokio::time::sleep(Duration::from_millis(5)).await;
        }
        assert_eq!(tasks.get(&id).unwrap().state, TaskState::Cancelled);
        assert!(tasks.cancel("no-such-task").is_none());
    }

    #[test]
    fn the_handle_body_says_the_outcome_is_unknown_rather_than_failed() {
        let body = handle_body("media-convert-1", Some(json!({ "name": "clip.mp4" })));
        assert_eq!(body["task_id"], "media-convert-1");
        assert_eq!(body["poll_interval_ms"], POLL_INTERVAL_MS);
        assert_eq!(body["outcome"], "indeterminate");
        assert_eq!(body["name"], "clip.mp4", "what the method knew already rides along");
        assert!(body["next"].as_str().unwrap().contains("task.get"));
    }

    #[tokio::test]
    async fn the_table_is_bounded_and_forgets_finished_work_first() {
        let tasks = Tasks::new();
        for _ in 0..(MAX_TASKS + 20) {
            let id = tasks.spawn("media.convert", |_| async move { Ok(json!({})) });
            for _ in 0..100 {
                if tasks.get(&id).is_none_or(|v| v.state.finished()) {
                    break;
                }
                tokio::time::sleep(Duration::from_millis(1)).await;
            }
        }
        assert!(tasks.len() <= MAX_TASKS, "held {} tasks", tasks.len());
    }
}
