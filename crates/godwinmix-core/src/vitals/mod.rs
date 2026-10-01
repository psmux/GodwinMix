//! Vitals for a show that composites: the programme's health, from what the
//! mixer already measures.
//!
//! ```text
//!   pgm-level meter (10 a second, always) ──► silence
//!   the multiview's programme cell ─────────► black, and freeze from the
//!     (the snapshot tracker's frame and          tracker's motion score
//!      motion, while it follows the mosaic)
//!   the status, once a second ──────────────► output-failed, shed
//!                     │
//!                     └──► event/health on a change of state or alarm kinds
//! ```
//!
//! Sound costs nothing new: the programme meter posts its peaks whether or
//! not anyone listens. The picture checks read the mosaic, which exists only
//! while somebody is looking; with `[vitals] alarms = true` this module asks
//! the tracker for it once a second and so keeps a mosaic going all the time,
//! which is what that switch costs. With it off (the default for a show
//! that composites, whose operator is usually watching it), black and freeze
//! are judged only while the mosaic is up anyway.
//!
//! The direct host's vitals (`plugins/ingest/src/direct/vitals/`) do the
//! same for shows that do not composite. `docs/reference/show-health.md`
//! has both.

mod judge;
mod look;
mod shared;
#[cfg(test)]
mod tests;

use std::sync::Arc;
use std::time::Duration;

use godwinmix_protocol::health::Health;
use tokio::sync::broadcast::error::RecvError;

use crate::mixer::MixerHandle;
use crate::snapshot::Tracker;
use crate::state::Event;
use judge::Judge;
pub use shared::{configure, process, Shared, VitalsConfig};

fn now_ms() -> u64 {
    std::time::SystemTime::now().duration_since(std::time::UNIX_EPOCH).map(|d| d.as_millis() as u64).unwrap_or(0)
}

/// Watch the programme until the mixer's event stream closes, with this
/// process's settings.
pub fn spawn(mixer: MixerHandle, tracker: Arc<Tracker>) -> tokio::task::JoinHandle<()> {
    tokio::spawn(run(mixer, tracker, process()))
}

async fn run(mixer: MixerHandle, tracker: Arc<Tracker>, shared: Arc<Shared>) {
    let mut events = mixer.subscribe();
    let mut tick = tokio::time::interval(Duration::from_secs(1));
    tick.set_missed_tick_behavior(tokio::time::MissedTickBehavior::Delay);
    let mut judge = Judge::new(shared.settings().thresholds);
    let mut reported: Option<Health> = None;
    loop {
        tokio::select! {
            got = events.recv() => match got {
                Ok(envelope) => {
                    if let Event::AudioLevel { peak_db } = envelope.event {
                        let peak = peak_db.into_iter().reduce(f64::max).unwrap_or(f64::NEG_INFINITY);
                        judge.sound(peak, now_ms());
                    }
                }
                Err(RecvError::Lagged(_)) => continue,
                Err(RecvError::Closed) => return,
            },
            _ = tick.tick() => {
                let cfg = shared.settings();
                judge.limits = cfg.thresholds.clone();
                turn(&mixer, &tracker, &cfg, &mut judge).await;
                let health = judge.health(now_ms());
                if reported.as_ref().is_none_or(|r| health.changed_from(r)) {
                    mixer.emit(Event::Health { health: Box::new(health.clone()) });
                }
                shared.judged(health.clone());
                reported = Some(health);
            }
        }
    }
}

/// One look at the picture and the outputs.
async fn turn(mixer: &MixerHandle, tracker: &Arc<Tracker>, cfg: &VitalsConfig, judge: &mut Judge) {
    if cfg.alarms {
        tracker.want();
    }
    match tracker.latest() {
        Some(latest) => {
            let luma = cfg.thresholds.black_luma;
            // A JPEG decode: off the runtime's own threads.
            let looked = tokio::task::spawn_blocking(move || look::programme(&latest, luma)).await.ok().flatten();
            if let Some((black, motion)) = looked {
                judge.picture(black, motion, now_ms());
            }
        }
        None => judge.no_picture(),
    }
    if let Ok(status) = mixer.status().await {
        judge.outputs(&status.outputs, now_ms());
    }
}
