//! The host: the table in, the shows running, and what the station reads.
//!
//! `apply` takes the whole table every time and changes only what changed:
//! a new row starts a show, a row gone stops one at once, a row whose input
//! changed starts its input again, and a row whose outputs changed starts
//! and stops those outputs and leaves the rest sending. The renditions for
//! every show are built by one `Transcoders`, the same code that converts a
//! channel's streams.

use std::collections::BTreeMap;
use std::sync::atomic::AtomicBool;
use std::sync::{Arc, Mutex, MutexGuard};

use serde_json::{json, Value};

use super::events::{self, Emit};
use super::input::Context;
use super::show::Show;
use super::table::{self, STREAM};
use super::vitals::Vitals;
use crate::hub::Hub;
use crate::transcode::Transcoders;

/// Where a show's input can be read by a hub reader: the relay's address.
pub type Relay = Arc<dyn Fn() -> String + Send + Sync>;

pub struct Host {
    pub(super) hub: Hub,
    pub(super) transcoders: Transcoders,
    pub(super) vitals: Arc<Vitals>,
    pub(super) shows: Mutex<BTreeMap<String, Show>>,
    pub(super) emit: Emit,
    pub(super) relay: Relay,
    ctx: Context,
    pub(super) watching: AtomicBool,
}

impl Host {
    /// A host publishing on `hub`, the channels' own, so a show's input can
    /// be read by the relay like any channel stream, and a show's input can
    /// be a channel's stream.
    pub fn new(hub: Hub, emit: Emit, relay: Relay) -> Arc<Host> {
        let vitals = Vitals::start(hub.clone(), emit.clone(), 2);
        let ctx = Context { hub: Some(hub.clone()) };
        Arc::new(Host {
            transcoders: Transcoders::sharing(hub.clone()),
            hub,
            vitals,
            shows: Mutex::default(),
            emit,
            relay,
            ctx,
            watching: AtomicBool::new(false),
        })
    }

    pub(super) fn lock(&self) -> MutexGuard<'_, BTreeMap<String, Show>> {
        self.shows.lock().unwrap_or_else(|e| e.into_inner())
    }

    /// Take the table: `params.direct`. Answers a sentence for each row
    /// that could not be read, which the caller logs.
    pub fn apply(self: &Arc<Host>, params: &Value) -> Vec<String> {
        let (rows, refused) = table::rows(params);
        let specs = rows.iter().flat_map(|r| r.transcode.clone()).collect();
        let wanted: Vec<_> = rows.iter().flat_map(|r| r.outputs.clone()).collect();
        // The pairs first, so a converting output has its pair to read.
        self.transcoders.apply(specs, &wanted);
        self.share_pictures(&rows);
        let renditions = self.transcoders.renditions();
        let mut shows = self.lock();
        shows.retain(|id, _| rows.iter().any(|r| &r.id == id));
        for row in &rows {
            self.vitals.watch(&row.id, &row.app(), STREAM, &row.monitor);
            match shows.get_mut(&row.id) {
                Some(show) if show.row.input == row.input => show.set_outputs(row, &self.hub, &renditions),
                Some(show) => {
                    show.stop();
                    *show = Show::start(row.clone(), &self.hub, &renditions, &self.ctx);
                }
                None => {
                    shows.insert(row.id.clone(), Show::start(row.clone(), &self.hub, &renditions, &self.ctx));
                }
            }
        }
        let ids: Vec<&str> = rows.iter().map(|r| r.id.as_str()).collect();
        self.vitals.keep(&ids);
        let any = !shows.is_empty();
        drop(shows);
        if any {
            events::watch(self);
        }
        refused
    }

    /// A show that converts already decodes its input: its pictures go to
    /// the vitals from that decode, about one a second, so the vitals need
    /// not decode its keyframes a second time.
    fn share_pictures(&self, rows: &[super::table::Row]) {
        for row in rows.iter().filter(|r| !r.transcode.is_empty()) {
            let (vitals, id) = (Arc::downgrade(&self.vitals), row.id.clone());
            let tap: crate::transcode::Tap = Arc::new(move |sample| {
                if let Some(v) = vitals.upgrade() {
                    v.offer_frame(&id, sample);
                }
            });
            self.transcoders.set_tap(&row.app(), STREAM, Some(tap));
        }
    }

    /// Every show's numbers, as `event/direct.stats` carries them, or only
    /// the shows in `ids`.
    pub fn stats(&self, ids: Option<&[String]>) -> Value {
        let shows = self.lock();
        let rows: Vec<Value> = shows
            .values()
            .filter(|s| ids.is_none_or(|ids| ids.contains(&s.row.id)))
            .map(|s| super::stats::show_stats(self, s))
            .collect();
        json!({"shows": rows})
    }

    /// The calls the station makes as `tool.call`: `direct.stats {ids?}`,
    /// and `direct.thumbnail {show, width?}` and `channel.thumbnail {app,
    /// stream, width?}`, which the vitals answer.
    /// `None` for a name that is not the host's.
    pub fn call(&self, name: &str, arguments: &Value) -> Option<Value> {
        if name == "direct.stats" {
            let ids: Option<Vec<String>> = arguments.get("ids").and_then(|v| serde_json::from_value(v.clone()).ok());
            return Some(self.stats(ids.as_deref()));
        }
        self.vitals.call(name, arguments)
    }

    /// The rows running now, for a test.
    #[cfg(test)]
    pub fn rows(&self) -> Vec<super::table::Row> {
        self.lock().values().map(|s| s.row.clone()).collect()
    }
}

impl Drop for Host {
    fn drop(&mut self) {
        for (_, mut show) in std::mem::take(&mut *self.lock()) {
            show.stop();
        }
    }
}
