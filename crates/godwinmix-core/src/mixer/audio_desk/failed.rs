//! A bus encoder that would not come up.
//!
//! It comes up on a thread of its own (`buses::encode`), so the `output.add`
//! that built it has answered by then and cannot carry the error. Before this
//! the failure was one line in the log and the output stayed on the list,
//! connected, with no sound. Now every output on that encoder comes off the
//! air and waits in `unattached`, where the status shows it failed and why,
//! and is tried again by itself; an alert says the same once.

use super::Mixer;
use crate::buses::encode::EncoderFailed;
use godwinmix_protocol::types::Severity;
use godwinmix_protocol::ErrorAction;
use tracing::warn;

impl Mixer {
    pub(super) fn bus_encoder_failed(&mut self, failed: EncoderFailed) {
        let users = self.desk.encoder_failed(&failed);
        if users.is_empty() {
            return;
        }
        for id in users {
            let Some(pos) = self.outputs.iter().position(|o| o.id() == &id) else { continue };
            let slot = self.outputs.remove(pos);
            let cfg = slot.cfg.clone();
            self.detach_off_thread(slot);
            self.release_encoder_for(&id);
            self.drop_rendition(&id);
            let message = off_air(&id, &failed);
            warn!(output = %id, bus = %failed.bus, why = %failed.why, "an output came off the air: its bus would not encode");
            self.keep_unattached(&cfg, &anyhow::anyhow!(message.clone()));
            self.alert_with(Severity::Error, message, ErrorAction::open_panel("Show Outputs", "core/outputs"));
        }
        self.note_on_air();
        self.broadcast_status();
    }
}

/// What the status and the alert say, with the next step in it.
fn off_air(id: &str, failed: &EncoderFailed) -> String {
    format!(
        "{id} did not start: the sound of bus {bus} could not be encoded ({why}). It is tried again by itself. \
         If it keeps failing, remove it and add it again without `audio`, to send it the programme's sound instead",
        bus = failed.bus,
        why = failed.why
    )
}
