//! `path.list` and `path.create`: a folder picker for a page that cannot see
//! the mixer's disk.
//!
//! A recording folder or a media folder is a path on the machine the mixer
//! runs on, which a browser in another room knows nothing about. These two
//! methods let a page walk the folders on the mixer, on any drive, and make a
//! new one, without turning the control port into a file browser: folders
//! only, never files, nothing hidden.

mod places;
mod walk;
#[cfg(test)]
mod tests;

use super::{body, handler};
use crate::control::call::Call;
use godwinmix_protocol::error::{ErrorCode, RpcError};
use godwinmix_protocol::method::{schema_of, MethodDef, Registry};
use godwinmix_protocol::scope::Scope;
use serde::{Deserialize, Serialize};
use std::path::PathBuf;
use walk::Refusal;

#[derive(Debug, Default, Deserialize, schemars::JsonSchema)]
pub struct PathListRequest {
    /// The folder to list. Absent, empty or `~` is the mixer's home folder; a
    /// relative path is taken from there, and a bare drive letter such as `D:`
    /// is the top of that drive.
    #[serde(default)]
    pub path: Option<String>,
}

#[derive(Debug, Deserialize, schemars::JsonSchema)]
pub struct PathCreateRequest {
    /// The folder to make it in, as `path.list` names it.
    pub parent: String,
    /// The new folder's name: one folder, no separators, not hidden.
    pub name: String,
}

/// One folder inside the one listed.
#[derive(Debug, Serialize, schemars::JsonSchema)]
pub struct PathEntry {
    pub name: String,
    /// Absolute, ready to pass back as `path`.
    pub path: String,
    /// Whether the mixer can write into it.
    pub writable: bool,
}

/// A place the picker may start from.
#[derive(Debug, Serialize, schemars::JsonSchema)]
pub struct PathRoot {
    /// `Home`, `Media folder` and `Config folder`, then on Windows each drive
    /// as `C:`, `D:`, and on Linux and macOS `Computer` for `/` and each
    /// volume mounted under `/Volumes`, `/media`, `/run/media` or `/mnt` by
    /// its folder name.
    pub label: String,
    pub path: String,
}

/// What `path.list` and `path.create` answer with.
#[derive(Debug, Serialize, schemars::JsonSchema)]
pub struct PathListing {
    /// The folder listed, absolute and with links resolved, written the way a
    /// person types it (`D:\Recordings`, never `\\?\D:\Recordings`).
    pub path: String,
    /// One level up, or null at the top of a drive or of `/`.
    pub parent: Option<String>,
    /// Whether the mixer can write into this folder.
    pub writable: bool,
    /// The folders in it, sorted by name. Files are never listed.
    pub dirs: Vec<PathEntry>,
    /// True when there were more than 500 and the rest were left out.
    pub truncated: bool,
    pub roots: Vec<PathRoot>,
}

/// Home, then the mixer's own folders, in the order a picker offers them.
/// Only names the paths; the disk is touched on the blocking thread.
fn own_folders(call: &Call) -> [(&'static str, Option<PathBuf>); 3] {
    let home = std::env::var_os("USERPROFILE").or_else(|| std::env::var_os("HOME")).map(PathBuf::from);
    let config = call.app.config_path.parent().map(|p| if p.as_os_str().is_empty() { PathBuf::from(".") } else { p.to_path_buf() });
    [
        ("Home", home),
        ("Media folder", Some(call.app.library.dir().to_path_buf())),
        ("Config folder", config),
    ]
}

/// Work out the roots and run `f` with them, off the async threads, since a
/// drive that is asleep or offline can take seconds to answer.
async fn blocking<F>(call: &Call, f: F) -> Result<serde_json::Value, RpcError>
where
    F: FnOnce(&[(String, PathBuf)]) -> Result<PathListing, Refusal> + Send + 'static,
{
    let own = own_folders(call);
    let answer = tokio::task::spawn_blocking(move || f(&walk::roots(&own, places::system())))
        .await
        .map_err(|e| RpcError::internal(format!("the folder listing stopped: {e}")))?;
    answer.map_err(refusal).and_then(body)
}

/// Each refusal says where the person is and where they can go instead.
pub(crate) fn refusal(r: Refusal) -> RpcError {
    match r {
        Refusal::Missing { path, nearest } => {
            let next = match &nearest {
                Some(n) => format!("The nearest folder that exists is {n}; list that, or make the folder there with path.create."),
                None => "List the home folder instead.".into(),
            };
            RpcError::new(ErrorCode::NotFound, format!("{path} does not exist. {next}"))
                .with("path", path)
                .with("nearest", nearest)
        }
        Refusal::Unreadable { path, reason } => RpcError::new(
            ErrorCode::NotInState,
            format!("{path} cannot be read ({reason}). Pick another folder."),
        )
        .with("path", path)
        .with("reason", reason),
        Refusal::BadName(name) => RpcError::invalid_params(format!(
            "'{name}' is not a folder name this mixer will make. Use one name with no slashes, \
             not starting with a dot, and none of : * ? \" < > |."
        ))
        .with("name", name),
    }
}

pub fn register(reg: &mut Registry<Call>) {
    reg.register(
        MethodDef::new(
            "path.list",
            Scope::Read,
            "The folders in one folder on the mixer, on any drive, and whether each is writable, \
             for a folder picker. The roots offer home, the mixer's own folders and each drive or \
             mounted volume. Files and hidden folders are never shown.",
            handler(|call: Call, params| async move {
                let req: PathListRequest = call.params(&params)?;
                blocking(&call, move |roots| walk::list(roots, req.path.as_deref())).await
            }),
        )
        .params(schema_of::<PathListRequest>)
        .result(schema_of::<PathListing>),
    );

    reg.register(
        MethodDef::new(
            "path.create",
            Scope::Operate,
            "Make one new folder inside a folder path.list shows, and list it. A folder that \
             is already there is listed rather than refused.",
            handler(|call: Call, params| async move {
                let req: PathCreateRequest = call.params(&params)?;
                if call.dry_run {
                    let diff = vec![format!("make the folder {} in {}", req.name, req.parent)];
                    return Ok(call.dry_run_answer(true, diff));
                }
                blocking(&call, move |roots| walk::create(roots, &req.parent, &req.name)).await
            }),
        )
        .params(schema_of::<PathCreateRequest>)
        .result(schema_of::<PathListing>),
    );
}
