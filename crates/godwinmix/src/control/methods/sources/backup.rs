//! A source's backup: `source.on_loss.set` and `.clear`, and the two things
//! an operator does while it is on, `source.backup.return` and `.stay`.
//!
//! Checked here first, with the facts a caller needs in `data`, and again in
//! the mixer, which is the one that acts. See `godwinmix_core::failover`.

use super::super::{body, handler};
use crate::control::call::Call;
use godwinmix_core::mixer::{BackupCommand, Command};
use godwinmix_protocol::backup::{BackupRequest, SetOnLossRequest};
use godwinmix_protocol::error::RpcError;
use godwinmix_protocol::method::{schema_of, MethodDef, Registry, Tier};
use godwinmix_protocol::requests::IdRequest;
use godwinmix_protocol::scope::Scope;
use godwinmix_protocol::types::SourceStatus;
use serde_json::Value;

pub(super) fn register(reg: &mut Registry<Call>) {
    reg.register(
        MethodDef::new(
            "source.on_loss.set",
            Scope::Operate,
            "Give a source a backup: another source drawn in its place, wherever it appears, \
             once it has had no picture for after_secs, and taken off again when it is back. \
             Only the fields named change; a source with no backup yet needs `backup`.",
            handler(set),
        )
        .params(schema_of::<SetOnLossRequest>)
        .result(schema_of::<SourceStatus>)
        .tool(
            "set_source_backup",
            Tier::Search,
            "Keep the show on air when a feed drops: {\"id\": \"cam-main\", \"backup\": \
             \"cam-wide\"} draws cam-wide wherever cam-main appears once cam-main has had no \
             picture for after_secs (3 by default), over `transition` (fade by default), and \
             goes back by itself once cam-main has been live for return_after_secs (5), or \
             only when told with \"return\": \"manual\". A backup that is down too hands on \
             to its own backup. An unknown backup or a loop is refused with the ids there are.",
        ),
    );
    reg.register(
        MethodDef::new(
            "source.on_loss.clear",
            Scope::Operate,
            "Take a source's backup away. If the backup is on now, the source is drawn as \
             itself again at once.",
            handler(clear),
        )
        .params(schema_of::<IdRequest>)
        .result(schema_of::<SourceStatus>),
    );
    reg.register(
        MethodDef::new(
            "source.backup.return",
            Scope::Operate,
            "Go back from the backup to the source now, over the rule's transition. Refused \
             while the source has no picture.",
            handler(return_now),
        )
        .params(schema_of::<BackupRequest>)
        .result(schema_of::<SourceStatus>)
        .tool(
            "return_from_backup",
            Tier::Search,
            "Put a source back on air in place of the backup standing in for it, now: \
             {\"id\": \"cam-main\"}. Refused while cam-main still has no picture, because that \
             would put a frozen frame or the slate on air. list_sources shows each source's \
             backup and its phase.",
        ),
    );
    reg.register(
        MethodDef::new(
            "source.backup.stay",
            Scope::Operate,
            "Keep the backup on even once the source is back, until source.backup.return.",
            handler(stay),
        )
        .params(schema_of::<BackupRequest>)
        .result(schema_of::<SourceStatus>),
    );
}

async fn set(call: Call, params: Value) -> Result<Value, RpcError> {
    let req: SetOnLossRequest = call.params(&params)?;
    let status = call.app.mixer.status().await.map_err(|e| call.mixer_error(e))?;
    let ids: Vec<String> = status.sources.iter().map(|s| s.id.clone()).collect();
    let Some(current) = status.sources.iter().find(|s| s.id == req.id) else {
        return Err(RpcError::not_found("source", &req.id, &ids));
    };
    let rule = req.over(current.on_loss.as_ref()).map_err(|m| RpcError::invalid_params(m).with("id", req.id.clone()))?;
    super::backup_check::check(&call, &status.sources, &req.id, &rule)?;
    if call.dry_run {
        let diff = vec![format!("{} falls back to {} after {} s", req.id, rule.backup, rule.after_secs)];
        return Ok(call.dry_run_answer(true, diff));
    }
    send(&call, BackupCommand::Set(req.id.clone(), Some(rule))).await?;
    answer(&call, &req.id).await
}

async fn clear(call: Call, params: Value) -> Result<Value, RpcError> {
    let req: IdRequest = call.params(&params)?;
    if call.dry_run {
        return Ok(call.dry_run_answer(true, vec![format!("{} has no backup", req.id)]));
    }
    send(&call, BackupCommand::Set(req.id.clone(), None)).await?;
    answer(&call, &req.id).await
}

async fn return_now(call: Call, params: Value) -> Result<Value, RpcError> {
    let req: BackupRequest = call.params(&params)?;
    send(&call, BackupCommand::Return(req.id.clone())).await?;
    answer(&call, &req.id).await
}

async fn stay(call: Call, params: Value) -> Result<Value, RpcError> {
    let req: BackupRequest = call.params(&params)?;
    send(&call, BackupCommand::Stay(req.id.clone())).await?;
    answer(&call, &req.id).await
}

/// Hand the command to the mixer; a refusal comes back as a state error with
/// the source named, since every refusal here is about where it stands.
async fn send(call: &Call, cmd: BackupCommand) -> Result<(), RpcError> {
    let id = match &cmd {
        BackupCommand::Set(id, _) | BackupCommand::Return(id) | BackupCommand::Stay(id) => id.clone(),
    };
    let ids = call.source_ids().await?;
    if !ids.contains(&id) {
        return Err(RpcError::not_found("source", &id, &ids));
    }
    call.app
        .mixer
        .request(|ack| Command::Backup(cmd, Some(ack)))
        .await
        .map_err(|e| RpcError::not_in_state(e.to_string()).with("id", id))
}

/// The source's record, with its rule and where it stands.
async fn answer(call: &Call, id: &str) -> Result<Value, RpcError> {
    let status = call.app.mixer.status().await.map_err(|e| call.mixer_error(e))?;
    match status.sources.into_iter().find(|s| s.id == id) {
        Some(record) => body(record),
        None => Err(RpcError::not_found("source", id, &call.source_ids().await?)),
    }
}
