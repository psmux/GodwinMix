//! `project.export` and `project.import`: a whole setup as one file.
//!
//! A project is what a show will be when one machine runs several: its
//! settings, sources, outputs with their renditions, channels, scenes, the
//! page layout the person sent along, and the names of its clips. Machine
//! settings travel too, but only go back in when asked, because an address or
//! a folder from another computer is rarely right on this one.
//!
//! Import reads and checks the whole file first and answers with what it
//! would change. It changes nothing unless `dry_run` is false.

mod apply;
pub mod bundle;
mod entries;
mod export;
mod ids;
mod media;
mod plan;
mod redact;
mod settings;

use super::{body, handler};
use crate::control::call::Call;
use godwinmix_protocol::error::{ErrorCode, RpcError};
use godwinmix_protocol::method::{schema_of, MethodDef, Registry};
use godwinmix_protocol::scope::Scope;
use schemars::JsonSchema;
use serde::{Deserialize, Serialize};
use serde_json::Value;
use std::sync::OnceLock;

#[derive(Debug, Default, Deserialize, JsonSchema)]
pub struct ExportRequest {
    /// What to call the project. Defaults to "GodwinMix project".
    #[serde(default)]
    pub name: Option<String>,
    /// Put stream keys, channel keys, destination addresses and the control
    /// token in the file. Off unless asked; admin scope either way.
    #[serde(default)]
    pub include_secrets: bool,
    /// Put the clips themselves in, as base64, rather than their names and
    /// sizes. Refused past 256 MB: copy the media folder instead.
    #[serde(default)]
    pub include_media: bool,
    /// Whatever the page wants back when the file is opened: its layout and
    /// its settings. Carried as it is.
    #[serde(default)]
    pub page: Value,
}

#[derive(Debug, Clone, Copy, Default, PartialEq, Eq, Deserialize, JsonSchema)]
#[serde(rename_all = "lowercase")]
pub enum Mode {
    /// The file's sources, outputs, channels and scenes become this mixer's.
    #[default]
    Replace,
    /// Everything in the file is added beside what is here, renamed on a clash.
    Merge,
}

#[derive(Debug, Deserialize, JsonSchema)]
pub struct ImportRequest {
    /// The project: the object `project.export` answered with, or its text.
    pub file: Value,
    #[serde(default)]
    pub mode: Mode,
    /// Answer with what would change and change nothing. True unless false is sent.
    #[serde(default)]
    pub dry_run: Option<bool>,
    /// Also write the file's machine settings: addresses, folders, hardware.
    #[serde(default)]
    pub machine: bool,
}

/// One thing an import does or would do.
#[derive(Debug, Clone, Serialize, JsonSchema)]
pub struct Change {
    /// `setting`, `source`, `output`, `channel`, `scene` or `media`.
    pub part: String,
    /// The id, key, scene name or clip name, as the file has it.
    pub id: String,
    /// `add`, `replace`, `remove`, `keep`, `rename`, `set`, `wait`, `skip` or `missing`.
    pub action: String,
    /// The new id or name, for a rename.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub to: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub note: Option<String>,
}

impl Change {
    pub fn new(part: &str, id: &str, action: &str) -> Self {
        Change { part: part.into(), id: id.into(), action: action.into(), to: None, note: None }
    }
    pub fn to(mut self, to: &str) -> Self {
        self.to = Some(to.into());
        self
    }
    pub fn note(mut self, note: &str) -> Self {
        self.note = Some(note.into());
        self
    }
}

/// What `project.import` answers with.
#[derive(Debug, Default, Serialize, JsonSchema)]
pub struct Report {
    pub dry_run: bool,
    pub name: String,
    pub written_by: String,
    pub changes: Vec<Change>,
    /// What a person still has to do: a key to give again, a clip to copy.
    pub waiting: Vec<String>,
    /// Settings written to the file that take effect on the next start.
    pub needs_restart: Vec<String>,
    /// What was tried and refused, each with the reason.
    pub failed: Vec<String>,
    /// The page part of the file, for the page to put back.
    pub page: Value,
}

pub fn register(reg: &mut Registry<Call>) {
    reg.register(
        MethodDef::new(
            "project.export",
            Scope::Admin,
            "This mixer as one project file: settings, sources, outputs and renditions, \
             channels, scenes, the page's layout, and its clips by name and size. Keys \
             only with include_secrets.",
            handler(export),
        )
        .params(schema_of::<ExportRequest>)
        .result(godwinmix_protocol::method::any_object)
        .mutating(false),
    );
    reg.register(
        MethodDef::new(
            "project.import",
            Scope::Admin,
            "Open a project file: answers with what it would change (dry_run is true \
             unless false is sent), then replaces this mixer's setup or merges beside it. \
             Says which settings wait for a restart.",
            handler(import),
        )
        .params(schema_of::<ImportRequest>)
        .result(schema_of::<Report>)
        .destructive()
        .not_idempotent(),
    );
}

async fn export(call: Call, params: Value) -> Result<Value, RpcError> {
    let req: ExportRequest = call.params(&params)?;
    let name = req.name.filter(|n| !n.trim().is_empty()).unwrap_or_else(|| "GodwinMix project".into());
    let ask = export::Ask { name, secrets: req.include_secrets, media: req.include_media, page: req.page };
    body(export::build(&call, ask).await?)
}

async fn import(call: Call, params: Value) -> Result<Value, RpcError> {
    let req: ImportRequest = call.params(&params)?;
    let file = bundle::read(&req.file)?;
    let dry_run = req.dry_run.unwrap_or(true) || call.dry_run;
    let replace = req.mode == Mode::Replace;
    let mut report = Report {
        dry_run,
        name: file.name.clone(),
        written_by: file.written_by.clone(),
        page: file.page.clone(),
        ..Report::default()
    };
    let plan = plan::build(&call, &file, replace, req.machine, &mut report).await?;
    apply::check_settings(&call, &plan).await?;
    if !dry_run {
        apply::run(&call, plan, replace, &mut report).await?;
    }
    body(report)
}

/// Run another method's handler as part of this one, with the same caller.
///
/// The scope was checked once, on `project.import`, which is admin; the
/// methods it runs are admin or less. Confirmation was asked once too.
pub(crate) async fn invoke(call: &Call, method: &'static str, params: Value) -> Result<Value, RpcError> {
    static TABLE: OnceLock<Registry<Call>> = OnceLock::new();
    let table = TABLE.get_or_init(super::registry);
    let def = table
        .get(method)
        .ok_or_else(|| RpcError::new(ErrorCode::InternalError, format!("{method} is missing from this core")))?;
    let mut inner = call.clone();
    inner.method = def.name;
    inner.dry_run = false;
    (def.handler)(inner, params).await
}
