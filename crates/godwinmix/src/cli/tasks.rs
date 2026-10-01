//! Waiting on work that outlives the call that started it.
//!
//! A method that might take more than five seconds answers with
//! `{task_id, poll_interval_ms}` and carries on; `plugin.add` and a switch
//! of compositing with `show.set` both do. This polls `task.get` by its path,
//! `GET /api/v1/tasks/{id}`, the same request a plain REST client sends.

use crate::ctl::Api;
use anyhow::Result;
use serde_json::Value;

/// Poll a task handle until it has an answer.
pub async fn wait(api: &Api, started: Value, what: &str) -> Result<Value> {
    // A core that answered outright rather than with a handle: take it.
    let Some(task_id) = started["task_id"].as_str().map(str::to_string) else {
        return Ok(started);
    };
    let wait = started["poll_interval_ms"].as_u64().unwrap_or(200).clamp(50, 2_000);
    let deadline = std::time::Instant::now() + std::time::Duration::from_secs(600);
    loop {
        tokio::time::sleep(std::time::Duration::from_millis(wait)).await;
        // `task.get` is `GET /api/v1/tasks/{id}`: the id is the path and
        // nothing else, so this sends what a plain REST client sends.
        let task: Value = api.get("task.get", Some(&task_id), &[]).await?;
        match task["state"].as_str().unwrap_or("running") {
            "completed" => return Ok(task["result"].clone()),
            "failed" => anyhow::bail!(
                "{}",
                task["error"]
                    .as_str()
                    .unwrap_or("it failed and said nothing, which is a bug worth reporting")
            ),
            "cancelled" => anyhow::bail!("the {what} was cancelled"),
            _ => {}
        }
        anyhow::ensure!(
            std::time::Instant::now() < deadline,
            "the {what} is still running after ten minutes. It has not been cancelled; \
             read it back with `gmx ctl ... task.get {task_id}`."
        );
    }
}

