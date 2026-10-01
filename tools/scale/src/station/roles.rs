//! What each process under a station is, and its CPU and memory by role
//! across a run.

use crate::procs::Proc;
use serde_json::{json, Value};
use std::collections::{BTreeMap, HashMap};

/// The station, its shows, the direct host, and plugins by binary name.
pub fn classify(tree: &[Proc], root: u32, direct: &str) -> Vec<(String, Proc)> {
    tree.iter()
        .map(|p| {
            let role = if p.pid == root {
                "station".to_string()
            } else if p.command.contains(" --show ") {
                "shows".to_string()
            } else if p.command.contains(direct) {
                "direct host".to_string()
            } else {
                let bin = p.command.split_whitespace().next().unwrap_or("");
                format!("plugin {}", bin.rsplit('/').next().unwrap_or(bin))
            };
            (role, p.clone())
        })
        .collect()
}

#[derive(Default)]
struct Track {
    seconds: u64,
    cpu_sum: f64,
    cpu_peak: f64,
    rss_sum: f64,
    rss_peak: f64,
    count_max: usize,
}

#[derive(Default)]
pub struct Roles {
    by: BTreeMap<String, Track>,
}

impl Roles {
    /// One second's processes. Answers `(role, count, cpu %, rss MiB)` per
    /// role, the whole station tree as `total`, and counts it when `counted`.
    pub fn add(&mut self, tree: &[(String, Proc)], extra: &[(String, Proc)], last: &HashMap<u32, f64>, dt: f64, counted: bool) -> Vec<(String, usize, f64, f64)> {
        let mut now: BTreeMap<String, (usize, f64, f64)> = BTreeMap::new();
        let mut put = |role: &str, p: &Proc| {
            let cpu = last.get(&p.pid).map_or(0.0, |c| (p.cpu_seconds - c).max(0.0) / dt.max(0.001) * 100.0);
            let e = now.entry(role.to_string()).or_default();
            *e = (e.0 + 1, e.1 + cpu, e.2 + p.rss_kib as f64 / 1024.0);
        };
        for (role, p) in tree {
            put(role, p);
            put("total", p);
        }
        for (role, p) in extra {
            put(role, p);
        }
        if counted {
            for (role, (n, cpu, rss)) in &now {
                let t = self.by.entry(role.clone()).or_default();
                t.seconds += 1;
                (t.cpu_sum, t.rss_sum) = (t.cpu_sum + cpu, t.rss_sum + rss);
                (t.cpu_peak, t.rss_peak, t.count_max) = (t.cpu_peak.max(*cpu), t.rss_peak.max(*rss), t.count_max.max(*n));
            }
        }
        now.into_iter().map(|(r, (n, c, m))| (r, n, c, m)).collect()
    }

    pub fn summary(&self) -> Value {
        let r1 = |v: f64| (v * 10.0).round() / 10.0;
        let each: serde_json::Map<String, Value> = self
            .by
            .iter()
            .map(|(role, t)| {
                let s = t.seconds.max(1) as f64;
                let v = json!({
                    "processes": t.count_max,
                    "cpu_avg_percent": r1(t.cpu_sum / s),
                    "cpu_peak_percent": r1(t.cpu_peak),
                    "rss_avg_mib": r1(t.rss_sum / s),
                    "rss_peak_mib": r1(t.rss_peak),
                    "seconds": t.seconds,
                });
                (role.clone(), v)
            })
            .collect();
        Value::Object(each)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn p(pid: u32, cpu: f64, cmd: &str) -> Proc {
        Proc { pid, ppid: 1, cpu_seconds: cpu, rss_kib: 10240, command: cmd.into() }
    }

    #[test]
    fn names_roles_and_adds_them_up() {
        let tree = vec![p(10, 1.0, "/x/godwinmix --config a.toml"), p(11, 2.0, "/x/godwinmix --config b --show s1 --station x"), p(12, 0.5, "/p/udp/bin/gmx-udp")];
        let named = classify(&tree, 10, "direct");
        let roles: Vec<&str> = named.iter().map(|(r, _)| r.as_str()).collect();
        assert_eq!(roles, vec!["station", "shows", "plugin gmx-udp"]);
        let mut r = Roles::default();
        let last: HashMap<u32, f64> = [(10, 0.5), (11, 1.0), (12, 0.5)].into();
        let rows = r.add(&named, &[], &last, 1.0, true);
        let total = rows.iter().find(|x| x.0 == "total").unwrap();
        assert_eq!(total.1, 3);
        assert!((total.2 - 150.0).abs() < 1e-9);
        assert!((total.3 - 30.0).abs() < 1e-9);
        assert_eq!(r.summary()["shows"]["cpu_avg_percent"], 100.0);
    }
}
