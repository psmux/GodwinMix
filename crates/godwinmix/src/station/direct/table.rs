//! The table the direct host is handed, and the thread that hands it.
//!
//! Asks pile up and are answered by one table: two hundred `show.add`s in a
//! row reconfigure the host once or twice, not two hundred times. Building
//! the table plans renditions and calling the plugin waits on it, which is
//! why neither happens on a handler.

use super::{outputs, Direct, EXTRA};
use crate::station::registry::Record;
use crate::station::state::Station;
use godwinmix_protocol::destination::StoredDestination;
use serde_json::{json, Value};
use crate::channels::transcode::RETRY;
use std::sync::mpsc::{Receiver, RecvTimeoutError};
use std::sync::Weak;
use tracing::warn;

pub fn start(st: Weak<Station>, wake: Receiver<()>) {
    let spawned = std::thread::Builder::new().name("direct-table".into()).spawn(move || {
        while woken(&st, &wake) {
            while wake.try_recv().is_ok() {}
            let Some(st) = st.upgrade() else { return };
            let asked = st.direct.asked();
            let table = st.direct.table(&st);
            st.direct.send(table);
            st.direct.hls.apply(&st);
            st.direct.done(asked);
        }
    });
    if let Err(e) = spawned {
        warn!(?e, "no thread for the direct table; shows without compositing will not run");
    }
}

/// Wait for an ask, and answer whether to build a table: true for an ask,
/// and true every [`RETRY`] while the governor has turned a rendition away,
/// so it is asked again once there is room. Before, a direct show's refused
/// rendition was asked about again only when something else changed, and a
/// show whose input had settled waited for ever on a machine that had long
/// since freed up: four minutes on the Windows runner, with the governor
/// showing two and a half cores free. The channels ask again on the same
/// clock (`channels::transcode::shed`). False once the station has gone.
fn woken(st: &Weak<Station>, wake: &Receiver<()>) -> bool {
    let refused = st.upgrade().is_some_and(|st| st.direct.transcode.refused());
    if !refused {
        return wake.recv().is_ok();
    }
    !matches!(wake.recv_timeout(RETRY), Err(RecvTimeoutError::Disconnected))
}

impl Direct {
    /// Every row, planned. A show is in the table when it is not stopped and
    /// has an input; one that composites has no outputs in it, so the host
    /// only feeds the hub its source reads.
    pub(crate) fn table(&self, st: &Station) -> Value {
        let records: Vec<Record> = st.registry.lock().records.iter().filter(|r| !r.stopped && r.input.is_some()).cloned().collect();
        let shows: Vec<(Record, Vec<StoredDestination>)> = records
            .into_iter()
            .map(|r| {
                let outs = if r.compositing { Vec::new() } else { outputs::stored(&r.id, &r.outputs) };
                (r, outs)
            })
            .collect();
        self.replan(&shows);
        Value::Array(shows.iter().map(|(r, o)| self.row(r, o)).collect())
    }

    fn row(&self, r: &Record, outs: &[StoredDestination]) -> Value {
        let rows: Vec<Value> = outs
            .iter()
            .filter(|d| d.enabled)
            .filter_map(|d| {
                let row = json!({"id": d.id, "platform": d.platform, "url": d.url(), "stream": "main"});
                self.transcode.row(&r.id, d, row)
            })
            .collect();
        let mut row = json!({
            "id": r.id,
            "name": r.name,
            "input": serde_json::to_value(r.input.clone().map(|i| super::inputs::unsealed(&r.id, i))).unwrap_or_default(),
            "outputs": rows,
            "monitor": monitor(r),
        });
        if let Some(streams) = self.transcode.streams(&r.id) {
            row["transcode"] = streams;
        }
        strip_nulls(&mut row);
        row
    }

    /// Lay the table over the plugin's settings and push it to the plugin
    /// if it runs. With no plugin yet, it is read when the plugin starts.
    fn send(&self, table: Value) {
        let Some(plugins) = self.plugins() else { return };
        match toml::Value::try_from(&table) {
            Ok(value) => plugins.set_extra(crate::channels::PLUGIN, EXTRA, Some(value)),
            Err(e) => return warn!(%e, "the direct table would not convert for the plugin"),
        }
        for (instance, answer) in plugins.configure_plugin(crate::channels::PLUGIN) {
            if let Err(e) = answer {
                warn!(%instance, error = %format!("{e:#}"), "the direct host did not take the new table");
            }
        }
    }
}

/// What the host watches: black, freeze and silence on by default for a
/// show without compositing and off for one that composites, the thresholds
/// a person set, and no pictures unless someone asks for a thumbnail, which
/// turns them on for a while by itself.
fn monitor(r: &Record) -> Value {
    let set = r.alarms.clone().unwrap_or_default();
    let mut m = json!({"alarms": set.enabled.unwrap_or(!r.compositing), "pictures": false});
    if set != Default::default() {
        m["thresholds"] = set.thresholds();
    }
    m
}

/// TOML has no null: a field that does not apply is left out.
pub fn strip_nulls(v: &mut Value) {
    match v {
        Value::Object(map) => {
            map.retain(|_, x| !x.is_null());
            map.values_mut().for_each(strip_nulls);
        }
        Value::Array(items) => items.iter_mut().for_each(strip_nulls),
        _ => {}
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn nulls_go_at_every_depth_so_the_table_converts_to_toml() {
        let mut v = json!({"a": null, "input": {"uri": "udp://@239.1.1.1:5000", "params": {"x": null, "y": 1}}, "outputs": [{"k": null}]});
        strip_nulls(&mut v);
        assert_eq!(v, json!({"input": {"uri": "udp://@239.1.1.1:5000", "params": {"y": 1}}, "outputs": [{}]}));
        assert!(toml::Value::try_from(&v).is_ok());
    }
}
