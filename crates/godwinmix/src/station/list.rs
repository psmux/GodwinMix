//! `show.list`: every show, and for the running ones what is on air, what
//! goes out and what its process costs.
//!
//! Measured when asked and not otherwise: each running show is asked for its
//! status at once (a second at most), and every show's process is read by
//! the plugin host's sampler in one go, which on macOS is one `ps` for all
//! of them. CPU is the average since the previous `show.list`.

use super::state::Station;
use futures_util::future::join_all;
use godwinmix_protocol::shows::{Show, ShowList, ShowState};
use godwinmix_protocol::{MixerStatus, OutputState};
use std::time::Duration;

const STATUS_WAIT: Duration = Duration::from_secs(1);

impl Station {
    pub async fn list(&self) -> ShowList {
        let ids = self.registry.lock().ids();
        let statuses = join_all(ids.iter().map(|id| async move {
            match self.state_of(id) {
                Some(ShowState::Running) => self.status_of(id, STATUS_WAIT).await,
                _ => None,
            }
        }))
        .await;
        let pids: Vec<(String, u32)> =
            self.procs.lock().iter().filter_map(|(id, p)| p.pid.map(|pid| (id.clone(), pid))).collect();
        let samples = {
            let wanted: Vec<u32> = pids.iter().map(|(_, pid)| *pid).collect();
            self.sampler.lock().sample(&wanted)
        };
        let shows = ids
            .iter()
            .zip(statuses)
            .filter_map(|(id, status)| {
                let mut show = self.view(id)?;
                if let Some(status) = status {
                    fill(&mut show, &status);
                }
                if let Some(sample) = pids.iter().find(|(p, _)| p == id).and_then(|(_, pid)| samples.get(pid)) {
                    show.cpu_millicores = sample.cpu_percent.map(|p| (p * 10.0).round() as u32).unwrap_or(0);
                    show.memory_mib = sample.rss_bytes.map(|b| b >> 20).unwrap_or(0);
                }
                Some(show)
            })
            .collect();
        ShowList { shows, current: self.first() }
    }
}

/// What a show's own status says about it.
fn fill(show: &mut Show, status: &MixerStatus) {
    show.on_air = status.scene.clone().or_else(|| status.program.clone());
    show.programme_kbps = status
        .outputs
        .iter()
        .filter(|o| o.state == OutputState::Live)
        .filter_map(|o| o.extra.get("bitrate_kbps").and_then(serde_json::Value::as_u64))
        .sum();
}
