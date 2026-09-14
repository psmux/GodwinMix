//! Work that outlives the call that started it.
//!
//! `task.get` and `task.cancel`, over the table in `godwinmix_core::tasks`.
//! A method whose work would run past the five second budget answers with
//! `{task_id, poll_interval_ms}` and the work carries on; these two are how a
//! client finds out what happened. A timed out call is `indeterminate`, never
//! `failed`.
//!
//! For a method author: call `spawn_task` with the work and answer with what
//! it returns. The plugin loader's `plugin.add` is the next caller.

use super::{body, handler};
use godwinmix_protocol::error::RpcError;
use godwinmix_protocol::method::{any_object, schema_of, MethodDef, Registry, Tier};
use godwinmix_protocol::scope::Scope;
use crate::control::call::Call;
use godwinmix_core::tasks::{TaskView, Tasks};
use schemars::JsonSchema;
use serde::{Deserialize, Serialize};
use serde_json::Value;
use std::sync::Arc;

#[derive(Debug, Clone, Serialize, Deserialize, JsonSchema)]
pub struct TaskRequest {
    /// The id a long running method answered with. Spelled `id` on the REST
    /// route, where it is in the path, and `task_id` everywhere else, which
    /// is what 03 section 6 calls it.
    #[serde(alias = "id")]
    pub task_id: String,
}

pub fn register(reg: &mut Registry<Call>) {
    reg.register(
        MethodDef::new(
            "task.get",
            Scope::Read,
            "How a piece of long running work is getting on, and its answer once it has one.",
            handler(|call: Call, params| async move {
                let req: TaskRequest = call.params(&params)?;
                body(find(&call, &req.task_id)?)
            }),
        )
        .params(schema_of::<TaskRequest>)
        .result(schema_of::<TaskView>)
        .mutating(false)
        .tool(
            "task_get",
            // Not in the hot list: 09 section 5 item 1 puts a ceiling of
            // twelve on it, an MCP client that speaks the tasks extension
            // reads a task through the protocol rather than through a tool,
            // and the handle body names the way here for one that does not.
            Tier::Search,
            "Read a background job by the `task_id` a method gave you. Answers `state` \
             (running, completed, failed or cancelled), `progress` where the work can say, \
             `result` with the body the method would have returned, and `error` with the \
             reason it stopped. A call that timed out is not a failure: the work is still \
             going and this is how you find out what it did. Ask again after \
             `poll_interval_ms`.",
        ),
    );

    reg.register(
        MethodDef::new(
            "task.list",
            Scope::Read,
            "Every background job this core knows about, newest first.",
            handler(|call: Call, _| async move { body(call.app.tasks.list()) }),
        )
        .result(schema_of::<Vec<TaskView>>)
        .mutating(false),
    );

    reg.register(
        MethodDef::new(
            "task.cancel",
            Scope::Operate,
            "Ask a piece of long running work to stop. Cooperative: the answer says the \
             request landed, not that the work has stopped yet.",
            handler(|call: Call, params| async move {
                let req: TaskRequest = call.params(&params)?;
                let _ = find(&call, &req.task_id)?;
                let view = call
                    .app
                    .tasks
                    .cancel(&req.task_id)
                    .ok_or_else(|| not_found(&call, &req.task_id))?;
                body(view)
            }),
        )
        .params(schema_of::<TaskRequest>)
        .result(any_object)
        .tool(
            "task_cancel",
            Tier::Search,
            "Ask a background job to stop, by its `task_id`. The work stops at the next \
             point it can, so read `task_get` afterwards to see that it did. Nothing that \
             has already finished is undone by this.",
        ),
    );
}

fn find(call: &Call, task_id: &str) -> Result<TaskView, RpcError> {
    call.app.tasks.get(task_id).ok_or_else(|| not_found(call, task_id))
}

/// An unknown id answers with the ids that exist, like every other -32004.
fn not_found(call: &Call, task_id: &str) -> RpcError {
    let mut ids = call.app.tasks.ids();
    ids.sort();
    RpcError::not_found("task", task_id, &ids).with(
        "detail",
        "a task is kept for an hour after it finishes, so an older id is gone rather than \
         lost. Start the work again.",
    )
}

/// Start a long running piece of work and get the body to answer with.
///
/// The shape 03 section 6 asks for: `{task_id, poll_interval_ms}` plus
/// whatever the method already knew. `extra` is folded in, so `media.convert`
/// can answer with the conversion state and the handle in one body.
///
/// ```ignore
/// let body = spawn_task(&call.app.tasks, "plugin.add", Some(json!({"plugin": name})), |ctx| async move {
///     install(ctx).await.map_err(|e| e.to_string())
/// });
/// ```
pub fn spawn_task<F, Fut>(
    tasks: &Arc<Tasks>,
    kind: &str,
    extra: Option<Value>,
    work: F,
) -> Value
where
    F: FnOnce(godwinmix_core::tasks::TaskContext) -> Fut + Send + 'static,
    Fut: std::future::Future<Output = Result<Value, String>> + Send + 'static,
{
    let id = tasks.spawn(kind, work);
    godwinmix_core::tasks::handle_body(&id, extra)
}
