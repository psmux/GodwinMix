//! The one thread the channels' renditions run, and only while there is
//! something to watch: a ticket held, a shed destination waiting to come
//! back, or a refusal to ask about again. It exits when there is not.

use std::sync::atomic::Ordering;
use std::sync::Arc;
use std::time::Duration;

use godwinmix_core::state::Severity;
use tracing::warn;

use crate::channels::Channels;

/// How often the machine is looked at while a transcode runs.
const LOOK: Duration = Duration::from_secs(2);

impl Channels {
    /// Start the watch thread unless it is running or has nothing to do.
    pub(in crate::channels) fn watch_renditions(&self) {
        if !self.transcode.busy() || self.watching.swap(true, Ordering::AcqRel) {
            return;
        }
        let Some(me) = self.me.get().cloned() else {
            self.watching.store(false, Ordering::Release);
            return;
        };
        let started = std::thread::Builder::new().name("channel-renditions".into()).spawn(move || loop {
            std::thread::sleep(LOOK);
            let Some(channels) = me.upgrade() else { return };
            if !channels.transcode.busy() {
                channels.watching.store(false, Ordering::Release);
                return;
            }
            look(&channels);
        });
        if let Err(e) = started {
            warn!(?e, "no thread to watch channel renditions; nothing will be shed or brought back");
            self.watching.store(false, Ordering::Release);
        }
    }
}

fn look(channels: &Arc<Channels>) {
    let tick = channels.transcode.tick();
    for alert in &tick.alerts {
        warn!(%alert, "a channel rendition was shed");
        channels.mixer.publish_alert(Severity::Warning, alert.clone());
    }
    if tick.replan {
        channels.replan();
    }
}
