//! One `show.stats` read a second (or `show.list` on a station without it),
//! what each read cost, and what the last one said in total.

use super::rpc::Client;
use serde_json::{json, Value};
use std::collections::BTreeMap;
use std::time::Instant;

#[derive(Default)]
pub struct Stats {
    method: &'static str,
    calls: u64,
    failed: u64,
    ms_sum: f64,
    ms_max: f64,
    last: Value,
    alarms_peak: BTreeMap<String, u64>,
}

impl Stats {
    pub fn read(&mut self, c: &Client) {
        self.method = if c.has("show.stats") { "show.stats" } else { "show.list" };
        let t = Instant::now();
        let got = c.call(self.method, json!({}), None);
        let ms = t.elapsed().as_secs_f64() * 1000.0;
        match got {
            Ok(a) if a.ok() => {
                self.calls += 1;
                (self.ms_sum, self.ms_max) = (self.ms_sum + ms, self.ms_max.max(ms));
                for (kind, n) in alarms(&a.body) {
                    let e = self.alarms_peak.entry(kind).or_default();
                    *e = (*e).max(n);
                }
                self.last = a.body;
            }
            _ => self.failed += 1,
        }
    }

    pub fn summary(&self) -> Value {
        let shows = self.last["shows"].as_array().cloned().unwrap_or_default();
        let sum = |f: &dyn Fn(&Value) -> f64| shows.iter().map(f).sum::<f64>();
        let mut by_state: BTreeMap<String, u64> = BTreeMap::new();
        let mut outputs: BTreeMap<String, u64> = BTreeMap::new();
        for s in &shows {
            let state = s["health"]["state"].as_str().or(s["state"].as_str()).unwrap_or("unknown");
            *by_state.entry(state.to_string()).or_default() += 1;
            for o in s["outputs"].as_array().into_iter().flatten() {
                *outputs.entry(o["state"].as_str().unwrap_or("unknown").to_string()).or_default() += 1;
            }
        }
        let outs = |k: &str| sum(&|s: &Value| s["outputs"].as_array().into_iter().flatten().map(|o| o[k].as_f64().unwrap_or(0.0)).sum());
        json!({
            "method": self.method,
            "reads": self.calls,
            "failed_reads": self.failed,
            "read_ms_avg": if self.calls > 0 { (self.ms_sum / self.calls as f64 * 10.0).round() / 10.0 } else { 0.0 },
            "read_ms_max": (self.ms_max * 10.0).round() / 10.0,
            "shows": shows.len(),
            "shows_by_state": by_state,
            "alarms_peak": self.alarms_peak,
            "input_kbps": sum(&|s| s["input"]["kbps"].as_f64().unwrap_or(0.0)).round(),
            "input_cc_errors": sum(&|s| s["input"]["cc_errors"].as_f64().unwrap_or(0.0)),
            "input_packets_lost": sum(&|s| s["input"]["packets_lost"].as_f64().unwrap_or(0.0)),
            "outputs_by_state": outputs,
            "output_reconnects": outs("reconnects"),
            "output_cpu_millicores": outs("cpu_millicores"),
            "show_list_cpu_millicores": sum(&|s| s["cpu_millicores"].as_f64().unwrap_or(0.0)),
            "show_list_memory_mib": sum(&|s| s["memory_mib"].as_f64().unwrap_or(0.0)),
        })
    }
}

/// How many shows have each kind of alarm in one read.
fn alarms(body: &Value) -> BTreeMap<String, u64> {
    let mut out = BTreeMap::new();
    for s in body["shows"].as_array().into_iter().flatten() {
        for a in s["health"]["alarms"].as_array().into_iter().flatten() {
            *out.entry(a["kind"].as_str().unwrap_or("unknown").to_string()).or_default() += 1;
        }
    }
    out
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn totals_a_read_in_the_contract_shape() {
        let body = json!({"shows": [
            {"id": "a", "health": {"state": "ok", "alarms": []}, "input": {"kbps": 8000, "cc_errors": 0, "packets_lost": 0},
             "outputs": [{"state": "running", "kbps": 8000, "reconnects": 0, "cpu_millicores": 3}]},
            {"id": "b", "health": {"state": "alarm", "alarms": [{"kind": "cc-errors"}]}, "input": {"kbps": 4000, "cc_errors": 5, "packets_lost": 35},
             "outputs": [{"state": "failed", "kbps": 0, "reconnects": 2}]}
        ]});
        let s = Stats { method: "show.stats", calls: 1, alarms_peak: alarms(&body), last: body, ..Stats::default() };
        let v = s.summary();
        assert_eq!(v["shows"], 2);
        assert_eq!(v["shows_by_state"]["alarm"], 1);
        assert_eq!(v["alarms_peak"]["cc-errors"], 1);
        assert_eq!(v["input_kbps"], 12000.0);
        assert_eq!(v["input_packets_lost"], 35.0);
        assert_eq!(v["outputs_by_state"]["failed"], 1);
        assert_eq!(v["output_reconnects"], 2.0);
    }
}
