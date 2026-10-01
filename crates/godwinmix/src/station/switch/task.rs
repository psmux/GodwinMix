//! A switch of compositing as a task, so `show.set` answers at once.
//!
//! Turning compositing on starts a process and waits for the outputs to be
//! live again, up to half a minute when no input is flowing; turning it off
//! waits the same way. No call may take more than five seconds, so
//! `show.set` checks what it can, starts the switch here and answers with
//! `{task_id, poll_interval_ms}` like `plugin.add`. `task.get` with that id
//! is answered by the station from this table and carries `show.set`'s
//! usual answer (the show and the switch report) once it is done; every
//! client also hears `event/show.changed` as the show moves.
//!
//! One switch per show at a time: a second while the first runs is refused
//! with the task to wait for.

use super::super::state::Station;
use godwinmix_core::tasks::{handle_body, TaskView, Tasks};
use godwinmix_protocol::error::RpcError;
use godwinmix_protocol::shows::{ShowSetResult, SwitchReport};
use parking_lot::Mutex;
use serde_json::{json, Value};
use std::collections::BTreeMap;
use std::sync::Arc;

/// The kind a switch's task carries. Its ids are `show-set-<n>`, which is
/// how the station tells its own tasks from a show's.
pub const KIND: &str = "show.set";
const PREFIX: &str = "show-set-";

/// The station's tasks, and which show each running switch is for.
#[derive(Default)]
pub struct Switches {
    pub tasks: Arc<Tasks>,
    busy: Mutex<BTreeMap<String, String>>,
}

/// Whether `id` names one of the station's tasks rather than a show's.
pub fn is_station_task(id: &str) -> bool {
    id.starts_with(PREFIX)
}

impl Switches {
    pub fn get(&self, task_id: &str) -> Option<TaskView> {
        self.tasks.get(task_id)
    }
}

/// Start switching `id` to `on`, with `moving` the outputs a switch off
/// checked, and answer with the handle and the show as it is now.
pub fn start(st: &Arc<Station>, id: &str, on: bool, moving: Vec<String>) -> Result<Value, RpcError> {
    let mut busy = st.switches.busy.lock();
    if let Some(task) = busy.get(id) {
        let msg = format!("show {id} is already switching compositing. Wait for task {task} with task.get, then ask again.");
        return Err(RpcError::not_in_state(msg).with("show", id).with("task_id", task.clone()).with("retry_after_ms", 1000));
    }
    let (station, show) = (st.clone(), id.to_string());
    let task = st.switches.tasks.spawn(KIND, move |_ctx| async move {
        let done = run(&station, &show, on, moving).await;
        station.switches.busy.lock().remove(&show);
        station.announce(&show);
        done
    });
    busy.insert(id.to_string(), task.clone());
    drop(busy);
    st.announce(id);
    Ok(handle_body(&task, Some(json!({ "show": st.view(id), "switching": if on { "on" } else { "off" } }))))
}

async fn run(st: &Arc<Station>, id: &str, on: bool, moving: Vec<String>) -> Result<Value, String> {
    let report: Result<SwitchReport, RpcError> = if on { super::on(st, id).await } else { super::off(st, id, moving).await };
    let switch = report.map_err(|e| e.message)?;
    let show = st.view(id).ok_or_else(|| format!("show {id} went while it was being switched"))?;
    Ok(serde_json::to_value(ShowSetResult { show, switch: Some(switch) }).unwrap_or_default())
}

/// `task.get` and `task.cancel` for one of the station's tasks.
pub fn call(st: &Station, method: &str, task_id: &str) -> Result<Value, RpcError> {
    let found = st.switches.get(task_id).ok_or_else(|| {
        let mut ids = st.switches.tasks.ids();
        ids.sort();
        RpcError::not_found("task", task_id, &ids).with("detail", "a task is kept for an hour after it finishes, so an older id is gone rather than lost.")
    })?;
    if method == "task.cancel" && found.state == godwinmix_core::tasks::TaskState::Running {
        let msg = format!("task {task_id} is switching compositing and cannot stop halfway without leaving the outputs nowhere. Wait for it, then switch back with show.set.");
        return Err(RpcError::not_in_state(msg).with("task_id", task_id));
    }
    Ok(serde_json::to_value(found).unwrap_or_default())
}
