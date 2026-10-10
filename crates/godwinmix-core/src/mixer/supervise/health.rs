//! A plugin's own view of itself: asked off the mixer thread, told to the
//! operator once per change, and acted on when it says `failing` for long
//! enough. See `mixer::supervise`.

use super::super::Mixer;
use crate::plugin::{Health, PluginState};
use crate::state::{Event, Severity, SourceId};
use std::sync::atomic::Ordering;
use std::time::{Duration, Instant};
use tracing::info;

/// How often a plugin that answers `health` is asked.
pub const HEALTH_EVERY: Duration = Duration::from_secs(2);

impl Mixer {
    /// Ask each plugin that answers `health`, off this thread.
    pub(in crate::mixer) fn ask_plugin_health(&mut self, now: Instant) {
        for slot in &mut self.sources {
            let w = &mut slot.watch;
            if !slot.input.answers_health()
                || w.asked_at.is_some_and(|at| now.duration_since(at) < HEALTH_EVERY)
                || w.asking.swap(true, Ordering::SeqCst)
            {
                continue;
            }
            w.asked_at = Some(now);
            let (input, asking, answer) = (slot.input.clone(), w.asking.clone(), w.answer.clone());
            let spawned = std::thread::Builder::new()
                .name(format!("health-{}", slot.input.id))
                .spawn(move || {
                    if let Some(h) = input.try_plugin_health() {
                        *answer.lock() = Some(h);
                    }
                    asking.store(false, Ordering::SeqCst);
                });
            if spawned.is_err() {
                w.asking.store(false, Ordering::SeqCst);
            }
        }
    }

    /// Tell the operator what a plugin says, and restart one that keeps
    /// saying it is failing.
    pub(in crate::mixer) fn act_on_plugin_health(&mut self, now: Instant) {
        let grace = Duration::from_secs(self.cfg.stall.restart_after_secs);
        let mut restart = Vec::new();
        let mut alerts = Vec::new();
        for slot in &mut self.sources {
            let Some(h) = slot.watch.answer.lock().take() else { continue };
            let id = slot.input.id.clone();
            let w = &mut slot.watch;
            if !matches!(h.state, PluginState::Failed) {
                w.failing_since = None;
            }
            let worth_telling = matches!(h.state, PluginState::Failed | PluginState::Degraded);
            if worth_telling && w.told != Some(h.state) {
                alerts.push(health_alert(&id, &h));
            }
            w.told = Some(h.state);
            if matches!(h.state, PluginState::Failed)
                && now.duration_since(*w.failing_since.get_or_insert(now)) >= grace
            {
                w.failing_since = None;
                restart.push(id);
            }
        }
        for alert in alerts {
            let _ = self.events.send(alert);
        }
        for id in restart {
            info!(source = %id, "the plugin has said it is failing for too long; restarting it");
            self.arm_strike(id, "its plugin has reported itself failing");
        }
    }
}

fn health_alert(id: &SourceId, h: &Health) -> Event {
    let (severity, said) = match h.state {
        PluginState::Failed => (Severity::Error, "failing; it is restarted if it stays that way"),
        _ => (Severity::Warning, "degraded"),
    };
    let detail = h.detail.as_deref().map(|d| format!(": {d}")).unwrap_or_default();
    Event::Alert {
        severity,
        message: format!("{id} says it is {said}{detail}"),
        action: None,
    }
}
