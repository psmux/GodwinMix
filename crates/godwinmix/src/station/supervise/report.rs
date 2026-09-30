//! What the supervisor tells everyone when a show's process ends.

use super::super::state::Station;
use godwinmix_core::state::Severity;
use godwinmix_protocol::shows::ShowState;
use std::time::Duration;
use tracing::{info, warn};

impl Station {
    /// The task is done with this show: say where it ended up.
    pub(super) fn settle(&self, id: &str, state: ShowState, error: Option<String>) {
        if let Some(p) = self.procs.lock().get_mut(id) {
            p.state = state;
            p.error = error;
            p.pid = None;
            p.stop = None;
            p.addr.send_replace(None);
        }
        self.on_air.lock().remove(id);
        self.announce(id);
    }

    /// Shut down on purpose (`core.shutdown`): stopped, not dead. With one
    /// show that was the whole mixer stopping, so the station goes too.
    pub(super) fn shut_down(&self, id: &str) {
        info!(show = %id, "show shut down on request");
        self.settle(id, ShowState::Stopped, None);
        if self.registry.lock().records.len() == 1 {
            self.quit.notify_one();
        }
    }

    pub(super) fn died(&self, id: &str, why: &str, wait: Duration, counted: bool) {
        if let Some(p) = self.procs.lock().get_mut(id) {
            p.state = ShowState::Starting;
            p.pid = None;
            p.addr.send_replace(None);
            if counted {
                p.restarts += 1;
                p.error = Some(format!("{why}; the station is starting it again"));
            }
        }
        self.on_air.lock().remove(id);
        if counted {
            let when = if wait.is_zero() { "now".to_string() } else { format!("in {} seconds", wait.as_secs()) };
            warn!(show = id, why, "a show died and is being started again");
            self.events.publish_alert(Severity::Warning, format!("Show {id} stopped: {why}. Starting it again {when}."));
        }
        self.announce(id);
    }
}
