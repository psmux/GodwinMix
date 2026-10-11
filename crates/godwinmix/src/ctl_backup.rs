//! `gmx ctl source backup`: a source's backup, from a terminal.
//!
//! Four `source.on_loss.*` and `source.backup.*` calls with the shapes a
//! person types: `gmx ctl source backup set cam-main cam-wide --after 3`.

use super::ctl_rpc::call;
use super::Api;
use anyhow::Result;
use godwinmix_protocol::types::SourceStatus;
use serde_json::{json, Map, Value};

#[derive(Debug, Clone, clap::Subcommand)]
pub enum BackupCmd {
    /// Show another source in this one's place when it goes down.
    Set {
        /// The source that may go down.
        id: String,
        /// The source to show instead. Leave it out to change only the
        /// other settings of a backup the source already has.
        backup: Option<String>,
        /// Seconds to hold the last frame before switching. 3 by default.
        #[arg(long)]
        after: Option<f64>,
        /// The transition both ways: fade, wipe, dip, cut and so on.
        #[arg(long)]
        transition: Option<String>,
        /// How long the transition runs, in milliseconds.
        #[arg(long)]
        duration_ms: Option<u64>,
        /// Stay on the backup until `gmx ctl source backup return`, rather
        /// than going back once the source is steady.
        #[arg(long)]
        manual: bool,
        /// Go back by itself once steady (the default). Undoes --manual.
        #[arg(long, conflicts_with = "manual")]
        auto: bool,
        /// Seconds the source has to stay live before it goes back. 5 by
        /// default.
        #[arg(long)]
        return_after: Option<f64>,
    },
    /// Take the backup away.
    Clear { id: String },
    /// Put the source back on air now, in place of its backup.
    Return { id: String },
    /// Keep the backup on until `return`, even once the source is back.
    Stay { id: String },
}

pub async fn run(api: &Api, cmd: BackupCmd) -> Result<()> {
    let (method, params) = match cmd {
        BackupCmd::Set { id, backup, after, transition, duration_ms, manual, auto, return_after } => {
            let mut p = Map::new();
            p.insert("id".into(), json!(id));
            let mut put = |k: &str, v: Option<Value>| {
                if let Some(v) = v {
                    p.insert(k.into(), v);
                }
            };
            put("backup", backup.map(Value::from));
            put("after_secs", after.map(Value::from));
            put("transition", transition.map(Value::from));
            put("duration_ms", duration_ms.map(Value::from));
            put("return_after_secs", return_after.map(Value::from));
            put("return", (manual || auto).then(|| Value::from(if manual { "manual" } else { "auto" })));
            ("source.on_loss.set", Value::Object(p))
        }
        BackupCmd::Clear { id } => ("source.on_loss.clear", json!({ "id": id })),
        BackupCmd::Return { id } => ("source.backup.return", json!({ "id": id })),
        BackupCmd::Stay { id } => ("source.backup.stay", json!({ "id": id })),
    };
    let answer = call(api, method, params).await?;
    let status: SourceStatus = serde_json::from_value(answer)?;
    println!("{}", line(&status));
    Ok(())
}

/// One source's backup in a line: the rule, and where it stands.
pub fn line(s: &SourceStatus) -> String {
    let Some(rule) = &s.on_loss else { return format!("{} has no backup", s.id) };
    let mode = match rule.returns {
        godwinmix_protocol::backup::ReturnMode::Auto => format!("back by itself after {} s steady", rule.return_after_secs),
        godwinmix_protocol::backup::ReturnMode::Manual => "back when told".to_string(),
    };
    format!(
        "{} falls back to {} after {} s with {}, {}{}",
        s.id,
        rule.backup,
        rule.after_secs,
        rule.transition_name(),
        mode,
        note(s).map(|n| format!(" ({n})")).unwrap_or_default()
    )
}

/// Where a source stands with its backup, in a few words, when it matters:
/// `lost 0:12, showing cam-wide`.
pub fn note(s: &SourceStatus) -> Option<String> {
    use godwinmix_protocol::backup::BackupPhase as P;
    let b = s.backup.as_ref()?;
    let clock = |ms: u64| format!("{}:{:02}", ms / 60_000, (ms / 1000) % 60);
    let showing = b.showing.as_deref().unwrap_or("its last frame");
    let text = match b.phase {
        P::Ready => return b.warning.clone(),
        P::Lost => format!("lost {}, backup in {} s", clock(b.lost_ms.unwrap_or(0)), b.switch_in_ms.unwrap_or(0).div_ceil(1000)),
        P::OnBackup => format!("lost {}, showing {showing}", clock(b.lost_ms.unwrap_or(0))),
        P::Returning => format!("back, returning in {} s, showing {showing}", b.return_in_ms.unwrap_or(0).div_ceil(1000)),
        P::MainBack => format!("back, showing {showing} until returned"),
    };
    Some(text)
}
