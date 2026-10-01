//! A show that composites, as far as its health goes: what it says of its
//! own programme over the link, what the station concludes when the link
//! closes, and the alarm settings the station hands to its vitals.
//!
//! ```text
//!   show's vitals ──show.health──► link ──► seen.show_health ──► event/show.health
//!   link closed, process failed ─────────► seen.lost_ms ───────► alarm: stall
//!   show.set {alarms} / hello ──vitals.set──► show's vitals
//! ```

use super::Direct;
use crate::station::state::Station;
use godwinmix_protocol::health::Health;
use godwinmix_protocol::shows::AlarmSettings;
use serde_json::{json, Value};
use std::sync::Arc;
use std::time::{Duration, SystemTime, UNIX_EPOCH};
use tracing::{debug, warn};

fn now_ms() -> u64 {
    SystemTime::now().duration_since(UNIX_EPOCH).map(|d| d.as_millis() as u64).unwrap_or(0)
}

/// How long the station waits for a show to take its alarm settings.
const HAND_WAIT: Duration = Duration::from_secs(5);

impl Direct {
    /// The show said what its vitals judged.
    pub fn take_show_health(&self, st: &Station, id: &str, health: Health) {
        self.seen.lock().entry(id.to_string()).or_default().show_health = Some(health);
        self.announce_health(st, id);
    }

    /// The show linked: whatever it said before, and the link it lost, are
    /// past.
    pub fn linked(&self, id: &str) {
        let mut seen = self.seen.lock();
        let s = seen.entry(id.to_string()).or_default();
        s.show_health = None;
        s.lost_ms = None;
    }

    /// Its link closed or its process failed. Kept from the first time, so
    /// the alarm's `since_ms` is when the programme went.
    pub fn lost(&self, st: &Station, id: &str) {
        {
            let mut seen = self.seen.lock();
            let s = seen.entry(id.to_string()).or_default();
            s.show_health = None;
            s.lost_ms.get_or_insert_with(now_ms);
        }
        self.announce_health(st, id);
    }

    /// Started on purpose: not lost, and not monitored until it links.
    pub fn starting(&self, id: &str) {
        if let Some(s) = self.seen.lock().get_mut(id) {
            s.show_health = None;
            s.lost_ms = None;
        }
    }
}

/// What `vitals.set` takes for a show's alarm settings: the wall's
/// milliseconds as the vitals' seconds, and `enabled` as `alarms`, off when
/// unset because a show that composites usually has somebody watching it.
pub fn vitals_settings(set: &AlarmSettings) -> Value {
    let mut v = set.thresholds();
    v["alarms"] = json!(set.enabled.unwrap_or(false));
    v
}

/// Hand a running show that composites the alarm settings the station keeps
/// for it. Nothing to do for a show with none set: its vitals start on
/// their defaults, or on its own `[vitals]`.
pub async fn hand_alarms(st: &Arc<Station>, id: &str) {
    let set = {
        let reg = st.registry.lock();
        let Some(r) = reg.get(id).filter(|r| r.compositing) else { return };
        let Some(set) = r.alarms.clone() else { return };
        set
    };
    match st.ask_show(id, "vitals.set", vitals_settings(&set), HAND_WAIT).await {
        Ok(_) => debug!(show = id, "the show took its alarm settings"),
        Err(e) => warn!(show = id, error = %e.message, "the show would not take its alarm settings; its vitals keep what they had"),
    }
}
