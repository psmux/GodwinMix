//! What the station's own child processes cost: every show process and
//! every plugin the station started, the ingest plugin among them, which is
//! the direct host every show without compositing runs in.
//!
//! The governor's sampler sees the station's process and what a show holding
//! a ticket reports of itself. A show that mixes but holds no ticket (its
//! programme encode is not admitted, only its renditions are) and the direct
//! host report nothing, so on their own they read as some other program's
//! load and `governor.status` said a busy station used none.
//!
//! Read only when a caller asks (`governor.status`, `show.stats`, the
//! `show.list` sampler) and at most once a second however many ask: one
//! `ps` for every pid at a time, or one read of `/proc` each on Linux.

use super::state::Station;
use godwinmix_host::sampler::{Sample, Sampler};
use parking_lot::Mutex;
use std::collections::BTreeMap;
use std::sync::Arc;
use std::time::{Duration, Instant};

/// How old a reading may be and still be answered.
const FRESH: Duration = Duration::from_secs(1);
/// On Linux a first reading of a pid has nothing to difference against, so
/// it is read again after this long rather than answered as nothing.
#[cfg(target_os = "linux")]
const SECOND_LOOK: Duration = Duration::from_millis(250);

/// One reading of every child.
#[derive(Debug, Clone, Default)]
pub struct Children {
    /// Each show process, by show id.
    pub shows: BTreeMap<String, Sample>,
    /// Each plugin process the station started.
    pub plugins: Vec<Sample>,
}

impl Children {
    /// Every child's CPU summed, thousandths of a core: zero with no child,
    /// None when there are children and this machine cannot read any of them
    /// (Windows reports memory only).
    pub fn millicores(&self) -> Option<u32> {
        let mut all = self.shows.values().chain(&self.plugins).peekable();
        if all.peek().is_none() {
            return Some(0);
        }
        all.filter_map(|s| s.cpu_percent).fold(None, |sum, p| Some(sum.unwrap_or(0) + millicores(p)))
    }

    /// One show's CPU, thousandths of a core.
    pub fn show(&self, id: &str) -> Option<u32> {
        self.shows.get(id).and_then(|s| s.cpu_percent).map(millicores)
    }
}

pub fn millicores(percent: f64) -> u32 {
    (percent * 10.0).round().max(0.0) as u32
}

#[derive(Default)]
pub struct Usage {
    held: Mutex<(Sampler, Option<(Instant, Children)>)>,
}

impl Station {
    /// Every child's cost, fresh within a second. Blocking: it may run `ps`.
    pub fn children_now(&self) -> Children {
        let mut held = self.usage.held.lock();
        if let Some((at, last)) = &held.1 {
            if at.elapsed() < FRESH {
                return last.clone();
            }
        }
        let shows: Vec<(String, u32)> = self.procs.lock().iter().filter_map(|(id, p)| p.pid.map(|pid| (id.clone(), pid))).collect();
        // The HLS packager counts with the plugins: a child the station started.
        let mut plugins: Vec<u32> = godwinmix_core::plugin::loader::stats().into_iter().filter_map(|s| s.pid).collect();
        plugins.extend(self.direct.hls.pid());
        let pids: Vec<u32> = shows.iter().map(|(_, p)| *p).chain(plugins.iter().copied()).collect();
        let samples = read(&mut held.0, &pids);
        let children = Children {
            shows: shows.iter().filter_map(|(id, pid)| Some((id.clone(), *samples.get(pid)?))).collect(),
            plugins: plugins.iter().filter_map(|pid| samples.get(pid).copied()).collect(),
        };
        held.1 = Some((Instant::now(), children.clone()));
        children
    }
}

/// `children_now` off the async runtime, for a handler.
pub async fn children(st: &Arc<Station>) -> Children {
    let st = st.clone();
    tokio::task::spawn_blocking(move || st.children_now()).await.unwrap_or_default()
}

fn read(sampler: &mut Sampler, pids: &[u32]) -> std::collections::HashMap<u32, Sample> {
    let samples = sampler.sample(pids);
    #[cfg(target_os = "linux")]
    if samples.values().any(|s| s.cpu_percent.is_none()) {
        std::thread::sleep(SECOND_LOOK);
        return sampler.sample(pids);
    }
    samples
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn a_child_that_burns_a_core_is_counted_on_the_first_read() {
        // A real process doing real work: the shell spinning on its own.
        let Ok(mut child) = std::process::Command::new("sh").args(["-c", "while :; do :; done"]).spawn() else { return };
        std::thread::sleep(Duration::from_millis(1500));
        let mut sampler = Sampler::new();
        let samples = read(&mut sampler, &[child.id()]);
        let _ = child.kill();
        let _ = child.wait();
        let Some(sample) = samples.get(&child.id()) else { return };
        if cfg!(windows) {
            assert!(sample.cpu_percent.is_none(), "Windows reports memory only");
            return;
        }
        let got = Children { plugins: vec![*sample], ..Default::default() }.millicores().expect("a reading");
        assert!(got > 300, "a spinning shell read {got} millicores on its first read");
    }

    #[test]
    fn nothing_measured_is_none_not_zero() {
        let none = Children { shows: [("a".into(), Sample { cpu_percent: None, rss_bytes: Some(1) })].into(), plugins: Vec::new() };
        assert_eq!(none.millicores(), None);
        assert_eq!(Children::default().millicores(), Some(0), "no child costs nothing");
        let two = Children { shows: [("a".into(), Sample { cpu_percent: Some(12.5), rss_bytes: None })].into(), plugins: vec![Sample { cpu_percent: Some(50.0), rss_bytes: None }] };
        assert_eq!(two.millicores(), Some(625));
        assert_eq!(two.show("a"), Some(125));
    }
}
