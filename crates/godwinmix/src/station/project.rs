//! `show.add {from: {project}}`: a new show opened from a project file.
//!
//! The show is made empty and started, then the file goes to it through the
//! public `project.import`, exactly as a person opening the file in that
//! show's page would send it. A build without `project.import` refuses before
//! anything is made, and says why.

use super::state::Station;
use godwinmix_protocol::error::RpcError;
use serde_json::{json, Value};
use std::time::Duration;

/// The method a project is opened with.
pub const IMPORT: &str = "project.import";

/// Whether this build can open a project, and the file if it can.
pub fn usable(project: &Value) -> Result<Value, RpcError> {
    if !super::methods::registry().iter().any(|m| m.name == IMPORT) {
        return Err(RpcError::not_in_state(
            "this build cannot open a project into a new show yet, because it has no project.import. \
             Make the show empty or as a copy, and open the project from its page once project.import is here.",
        )
        .with("missing", IMPORT));
    }
    if !project.is_object() && !project.is_string() {
        return Err(RpcError::invalid_params("from.project is the file project.export wrote: an object, or its text").with("field", "from.project"));
    }
    Ok(project.clone())
}

/// Open `project` into the show `id`, once it is running.
pub async fn open(st: &Station, id: &str, project: Value, wait: Duration) -> Result<Value, RpcError> {
    let params = json!({ "file": project, "mode": "replace", "dry_run": false });
    st.ask_show(id, IMPORT, params, wait).await.map_err(|e| {
        let message = format!("show {id} was made, but the project would not open in it: {}", e.message);
        RpcError { message, ..e }.with("show", id)
    })
}
