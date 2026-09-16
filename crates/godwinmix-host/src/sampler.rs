//! CPU and RSS for one process, read once a second and cheaply.
//!
//! Every plugin's cost sits next to its name in `plugin.list` and
//! `plugin.stats`, which is the one recommendation from the Claude Code
//! research that nothing else in the plan had adopted: an operator can only
//! drop dead weight if they can see what it weighs.
//!
//! Linux reads `/proc`, which is two small files and no process spawn. Other
//! Unixes ask `ps` once per sample for every pid at a time, because `ps`
//! accepts a list and starting it once a second for eight plugins is one
//! process, not eight. Windows has neither and reports memory only, from
//! `tasklist`, with cpu left as `None` rather than invented.

use std::collections::HashMap;
use std::time::{Duration, Instant};

/// What one process cost at the last sample.
#[derive(Debug, Clone, Copy, Default, PartialEq)]
pub struct Sample {
    /// Percent of one core, averaged over the interval since the last sample.
    pub cpu_percent: Option<f64>,
    pub rss_bytes: Option<u64>,
}

/// Reads a set of pids at a time and remembers enough to turn Linux's
/// cumulative jiffies into a percentage.
#[derive(Debug, Default)]
pub struct Sampler {
    last: HashMap<u32, (Instant, u64)>,
}

impl Sampler {
    pub fn new() -> Self {
        Self::default()
    }

    /// Sample every pid given. A pid that has gone is absent from the answer
    /// rather than reported as zero.
    pub fn sample(&mut self, pids: &[u32]) -> HashMap<u32, Sample> {
        #[allow(unused_mut)]
        let mut out = read_all(pids);
        #[cfg(target_os = "linux")]
        for (pid, sample) in out.iter_mut() {
            if let Some(ticks) = linux_ticks(*pid) {
                let now = Instant::now();
                if let Some((then, before)) = self.last.get(pid) {
                    let elapsed = now.duration_since(*then).as_secs_f64();
                    let hz = clock_ticks_per_second();
                    if elapsed > 0.0 && ticks >= *before {
                        let seconds = (ticks - before) as f64 / hz;
                        sample.cpu_percent = Some(100.0 * seconds / elapsed);
                    }
                }
                self.last.insert(*pid, (now, ticks));
            }
        }
        self.last.retain(|pid, _| pids.contains(pid));
        out
    }

    /// Forget a pid. Called when an instance goes away, so the map does not
    /// grow with every restart.
    pub fn forget(&mut self, pid: u32) {
        self.last.remove(&pid);
    }
}

#[cfg(target_os = "linux")]
fn clock_ticks_per_second() -> f64 {
    100.0
}

#[cfg(target_os = "linux")]
fn linux_ticks(pid: u32) -> Option<u64> {
    let text = std::fs::read_to_string(format!("/proc/{pid}/stat")).ok()?;
    // The comm field may contain spaces and brackets, so everything after the
    // last ')' is the part that can be split safely.
    let rest = text.rsplit_once(')')?.1;
    let fields: Vec<&str> = rest.split_whitespace().collect();
    // utime and stime are fields 14 and 15 of the whole line, which are 12 and
    // 13 of what follows the comm.
    let utime: u64 = fields.get(11)?.parse().ok()?;
    let stime: u64 = fields.get(12)?.parse().ok()?;
    Some(utime + stime)
}

#[cfg(target_os = "linux")]
fn read_all(pids: &[u32]) -> HashMap<u32, Sample> {
    let mut out = HashMap::new();
    for pid in pids {
        let Ok(text) = std::fs::read_to_string(format!("/proc/{pid}/statm")) else { continue };
        let pages: u64 = match text.split_whitespace().nth(1).and_then(|s| s.parse().ok()) {
            Some(p) => p,
            None => continue,
        };
        out.insert(*pid, Sample { cpu_percent: None, rss_bytes: Some(pages * page_size()) });
    }
    out
}

#[cfg(target_os = "linux")]
fn page_size() -> u64 {
    4096
}

/// macOS and the other Unixes: one `ps` for the whole set.
#[cfg(all(unix, not(target_os = "linux")))]
fn read_all(pids: &[u32]) -> HashMap<u32, Sample> {
    let mut out = HashMap::new();
    if pids.is_empty() {
        return out;
    }
    let list = pids.iter().map(u32::to_string).collect::<Vec<_>>().join(",");
    let Ok(result) = std::process::Command::new("ps")
        .args(["-o", "pid=,rss=,pcpu=", "-p", &list])
        .output()
    else {
        return out;
    };
    for line in String::from_utf8_lossy(&result.stdout).lines() {
        let fields: Vec<&str> = line.split_whitespace().collect();
        let (Some(pid), Some(rss), Some(cpu)) = (fields.first(), fields.get(1), fields.get(2))
        else {
            continue;
        };
        let Ok(pid) = pid.parse::<u32>() else { continue };
        out.insert(
            pid,
            Sample {
                // `ps` reports RSS in kilobytes.
                rss_bytes: rss.parse::<u64>().ok().map(|kb| kb * 1024),
                cpu_percent: cpu.parse::<f64>().ok(),
            },
        );
    }
    out
}

/// Windows: memory from `tasklist`, and cpu left unanswered rather than
/// guessed. A percentage that means nothing is worse than a blank.
#[cfg(not(unix))]
fn read_all(pids: &[u32]) -> HashMap<u32, Sample> {
    let mut out = HashMap::new();
    for pid in pids {
        let Ok(result) = std::process::Command::new("tasklist")
            .args(["/FI", &format!("PID eq {pid}"), "/FO", "CSV", "/NH"])
            .output()
        else {
            continue;
        };
        let text = String::from_utf8_lossy(&result.stdout);
        // The row is `"name","pid","session","session#","7,168 K"`: the
        // memory column is last and carries a thousands separator inside its
        // quotes, so splitting on the bare comma read `"7` and answered seven
        // kilobytes for every process. Take the last quoted field instead.
        let Some(field) = text.trim_end().rsplit("\",\"").next() else { continue };
        let digits: String = field.chars().filter(char::is_ascii_digit).collect();
        if let Ok(kb) = digits.parse::<u64>() {
            out.insert(*pid, Sample { cpu_percent: None, rss_bytes: Some(kb * 1024) });
        }
    }
    out
}

/// How often the numbers are refreshed. 03 section 6: every second.
pub const REFRESH: Duration = Duration::from_secs(1);

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn this_process_has_a_resident_size() {
        let mut sampler = Sampler::new();
        let me = std::process::id();
        let first = sampler.sample(&[me]);
        let Some(sample) = first.get(&me) else {
            // A container with no `ps` and no `/proc` is a real machine and
            // the sampler says nothing rather than failing the mixer.
            println!("skipping: this machine has no way to read its own footprint");
            return;
        };
        let rss = sample.rss_bytes.expect("a running process has a resident size");
        assert!(rss > 1024 * 1024, "{rss} bytes is too small to be a real process");
    }

    #[test]
    fn a_pid_that_has_gone_is_absent_rather_than_zero() {
        let mut sampler = Sampler::new();
        // A pid that cannot be running: one above the largest a 32 bit pid
        // space holds.
        let out = sampler.sample(&[u32::MAX]);
        assert!(!out.contains_key(&u32::MAX));
    }

    #[test]
    fn forgetting_a_pid_drops_what_was_held_for_it() {
        let mut sampler = Sampler::new();
        sampler.sample(&[std::process::id()]);
        sampler.forget(std::process::id());
        assert!(sampler.last.is_empty());
    }
}
