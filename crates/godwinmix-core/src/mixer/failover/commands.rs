//! What a caller can do about a backup: set or clear the rule, return to the
//! source now, or stay on the backup.

use super::super::Mixer;
use super::Seen;
use crate::failover::Liveness;
use crate::state::{SourceId, SourceStatus};
use anyhow::{anyhow, bail, Result};
use godwinmix_protocol::backup::OnLoss;
use std::time::Instant;

/// `Command::Backup`.
#[derive(Debug, Clone)]
pub enum BackupCommand {
    /// Give a source a rule, or take its rule away with `None`.
    Set(SourceId, Option<OnLoss>),
    /// Go back to the source now, over the rule's transition.
    Return(SourceId),
    /// Stay on the backup until the operator returns it.
    Stay(SourceId),
}

impl BackupCommand {
    /// The method a person called, for the watchdog's line.
    pub fn label(&self) -> &'static str {
        match self {
            BackupCommand::Set(_, Some(_)) => "source.on_loss.set",
            BackupCommand::Set(_, None) => "source.on_loss.clear",
            BackupCommand::Return(_) => "source.backup.return",
            BackupCommand::Stay(_) => "source.backup.stay",
        }
    }
}

impl Mixer {
    pub(in crate::mixer) fn backup_command(&mut self, cmd: BackupCommand) -> Result<()> {
        match cmd {
            BackupCommand::Set(id, rule) => self.set_on_loss(&id, rule),
            BackupCommand::Return(id) => {
                if self.backups.engaged(&id) && !Seen(&self.sources, &self.pending).live(&id) {
                    bail!(
                        "{id} has no picture yet, so going back to it now would put its last frame or \
                         the slate on air. Its backup stays on; return once {id} reads live."
                    );
                }
                self.backups.release(&id).map_err(|e| anyhow!(e))?;
                self.swap_stand_ins();
                self.broadcast_status();
                Ok(())
            }
            BackupCommand::Stay(id) => {
                self.backups.stay(&id).map_err(|e| anyhow!(e))?;
                self.broadcast_status();
                Ok(())
            }
        }
    }

    /// Every source id the operator could name: running, being probed, or
    /// waiting to start.
    fn known_ids(&self) -> Vec<String> {
        let mut ids: Vec<String> = self.sources.iter().map(|s| s.input.id.to_string()).collect();
        ids.extend(self.pending.iter().map(|c| c.id.clone()));
        ids.extend(self.unstarted.configs().map(|c| c.id.clone()));
        ids.sort();
        ids.dedup();
        ids
    }

    fn set_on_loss(&mut self, id: &SourceId, rule: Option<OnLoss>) -> Result<()> {
        let ids = self.known_ids();
        if !ids.iter().any(|s| s == id.as_str()) {
            bail!("there is no source '{id}' to give a backup. Call source.list to see the ids");
        }
        if let Some(rule) = &rule {
            let id_list: Vec<&str> = ids.iter().map(String::as_str).collect();
            let mut rules: Vec<(&str, &OnLoss)> =
                self.backups.rules().into_iter().filter(|(s, _)| *s != id.as_str()).collect();
            rules.push((id.as_str(), rule));
            godwinmix_protocol::backup::check(&rules, &id_list).map_err(|p| anyhow!(p.message))?;
        }
        self.backups.set(id, rule);
        self.persist_runtime();
        self.swap_stand_ins();
        self.broadcast_status();
        Ok(())
    }

    /// `on_loss` and `backup` on a source's status.
    pub(in crate::mixer) fn backup_status(&self, status: &mut SourceStatus, now: Instant) {
        status.on_loss = self.backups.rule(&status.id).cloned();
        status.backup = self.backups.state(&status.id, now, &Seen(&self.sources, &self.pending));
    }
}
