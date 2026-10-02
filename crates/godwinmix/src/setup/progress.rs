//! What a job says as it goes, and how it ends.
//!
//! Each step says one plain sentence and, during a download, how far it has
//! got; `finish` turns the job's outcome into the piece's last status and,
//! when it is ready, starts the sources that were waiting on it.

use super::registry::{look, publish, REGISTRY};
use super::Failure;
use godwinmix_core::setup::names;
use godwinmix_protocol::setup::{SetupState, SetupStatus};
use tokio::sync::watch;

/// What a job says as it goes.
pub struct Progress {
    pub(super) tx: watch::Sender<SetupStatus>,
    pub(super) piece: String,
}

impl Progress {
    /// A new step, in a person's words, and how far through a download it
    /// is. Sent when the words change or the fraction moves a percent.
    pub fn say(&self, message: &str, fraction: Option<f64>) {
        let mut next = self.tx.borrow().clone();
        let moved = match (next.progress, fraction) {
            (Some(a), Some(b)) => (a - b).abs() >= 0.01,
            (a, b) => a.is_some() != b.is_some(),
        };
        if next.message == message && !moved {
            return;
        }
        next.message = message.to_string();
        next.progress = fraction;
        publish(&self.tx, next);
    }

    pub(super) fn finish(&self, outcome: Result<(), Failure>) {
        let mut next = look(&self.piece);
        match outcome {
            Ok(()) if next.state == SetupState::Ready => {
                tracing::info!(piece = %self.piece, "set up and ready");
                if let Some(reg) = REGISTRY.get() {
                    super::resume::ready(reg.app.clone(), self.piece.clone());
                }
            }
            Ok(()) => {
                tracing::warn!(piece = %self.piece, detail = %next.detail, "set up, but still not found");
                next.state = SetupState::Failed;
                next.message = format!("{} did not finish setting up. Press Try again.", names::title(&self.piece));
                next.action = Some(godwinmix_protocol::ErrorAction::setup("Try again", &self.piece));
            }
            Err(f) => {
                let f = f.into_inner();
                tracing::warn!(piece = %self.piece, why = %f.message, detail = %f.detail, "setting up did not finish");
                next.state = SetupState::Failed;
                next.message = f.message;
                next.action = f.action.or(Some(godwinmix_protocol::ErrorAction::setup("Try again", &self.piece)));
                next.detail = f.detail;
            }
        }
        publish(&self.tx, next);
    }
}
