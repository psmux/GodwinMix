//! `gmx shows set`: one show's name, input, or compositing.
//!
//! A switch of compositing answers with a task and finishes in the
//! background, so this waits on it the way `gmx plugin add` waits on an
//! install, and prints `show.set`'s usual answer once it is done.

use anyhow::Result;
use serde_json::{json, Value};

pub async fn run(api: &crate::ctl::Api, id: &str, compositing: Option<bool>, name: Option<String>, input: Option<String>) -> Result<()> {
    let mut req = json!({ "id": id });
    if let Some(c) = compositing {
        req["compositing"] = json!(c);
    }
    if let Some(n) = name {
        req["name"] = json!(n);
    }
    if let Some(uri) = input {
        req["input"] = json!({ "uri": uri });
    }
    let answer: Value = api.call("show.set", Some(id), &req).await?;
    if let Some(task) = answer["task_id"].as_str() {
        let way = answer["switching"].as_str().unwrap_or("over");
        eprintln!("switching compositing {way} for {id} (task {task}); this can take half a minute");
    }
    let show = crate::cli::tasks::wait(api, answer, "switch").await?;
    println!("{}", serde_json::to_string_pretty(&show)?);
    Ok(())
}
