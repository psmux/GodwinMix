//! What the receiver says about itself, worked out once a second from the
//! counters. Pure, so each sentence has a test.

use std::sync::atomic::Ordering::Relaxed;

use godwinmix_sdk::wire::Health;

use crate::counters::{Catalog, Counters};

/// Silent this long and the feed is reported as stopped.
pub const STOPPED_MS: u64 = 2000;
/// More loss than this over one reading raises an alert.
const LOSSY: f64 = 0.005;

/// The last reading, so the next one can say what changed in between.
#[derive(Debug, Default, Clone, Copy)]
pub struct Reading {
    bytes: u64,
    packets: u64,
    lost: u64,
    at: Option<std::time::Instant>,
}

pub struct Inputs<'a> {
    pub address: &'a str,
    pub failure: Option<String>,
    pub counters: &'a Counters,
    pub catalog: &'a Catalog,
}

/// The health now, and the reading to compare the next one with.
pub fn assess(i: &Inputs<'_>, before: Reading) -> (Health, Reading) {
    let n = i.counters;
    let now = Reading {
        bytes: n.bytes_in.load(Relaxed),
        packets: n.bytes_in.load(Relaxed) / 188,
        lost: n.lost(),
        at: Some(std::time::Instant::now()),
    };
    let secs = before.at.map(|t| t.elapsed().as_secs_f64()).unwrap_or(0.0);
    let mbps = if secs > 0.0 { (now.bytes - before.bytes) as f64 * 8.0 / secs / 1e6 } else { 0.0 };
    let lost = now.lost - before.lost;
    let share = lost as f64 / ((now.packets - before.packets) + lost).max(1) as f64;
    (verdict(i, mbps, lost, share), now)
}

fn verdict(i: &Inputs<'_>, mbps: f64, lost: u64, share: f64) -> Health {
    if let Some(f) = &i.failure {
        return Health::failing(explain(f, i.address));
    }
    let n = i.counters;
    let Some(silent) = n.silent_ms() else {
        return Health::degraded(format!(
            "listening on {} and nothing has arrived yet. Check that the sender is sending to \
             this address and port, that a firewall is not dropping UDP, and for multicast \
             that the switch forwards the group to this machine's interface.",
            i.address
        ));
    };
    if silent > STOPPED_MS {
        return Health::degraded(format!(
            "the feed on {} stopped {} s ago. Still listening; the picture comes back by \
             itself when the sender does.",
            i.address,
            silent / 1000
        ));
    }
    if let Some(problem) = &i.catalog.problem {
        return Health::degraded(problem.clone());
    }
    if i.catalog.chosen.is_none() && i.catalog.programs.is_empty() && n.bytes_out.load(Relaxed) == 0 {
        return Health::degraded(format!(
            "datagrams are arriving on {} but no MPEG-TS program table has come yet. If this \
             lasts, the sender is not sending MPEG-TS.",
            i.address
        ));
    }
    let mut words = format!("{mbps:.1} Mbit/s on {}, {} packets lost", i.address, n.lost());
    if let Some(p) = i.catalog.phrase() {
        words = format!("{words}; {p}");
    }
    if share > LOSSY {
        return Health::degraded(format!(
            "losing {:.1}% of packets ({lost} in the last second). The picture carries on with \
             damage. {words}",
            share * 100.0
        ));
    }
    let mut h = Health::ok();
    h.detail = Some(words);
    h
}

/// A bus error, said in terms of what to do about it.
pub fn explain(failure: &str, address: &str) -> String {
    let lower = failure.to_ascii_lowercase();
    if lower.contains("address already in use") || lower.contains("could not bind") {
        return format!(
            "{address} is already taken on this machine: another source or program is \
             receiving on that port. Choose another port, or remove the other source. ({failure})"
        );
    }
    if lower.contains("multicast") || lower.contains("no such device") {
        return format!(
            "could not join the group on {address}. Check the interface name in the source's \
             settings (leave it empty for the default route). ({failure})"
        );
    }
    format!("receiving on {address} failed: {failure}")
}

#[cfg(test)]
mod tests {
    use super::*;
    use godwinmix_sdk::wire::HealthState;

    fn inputs<'a>(n: &'a Counters, c: &'a Catalog, failure: Option<&str>) -> Inputs<'a> {
        Inputs { address: "udp://@239.1.1.1:5000", failure: failure.map(str::to_string), counters: n, catalog: c }
    }

    #[test]
    fn nothing_yet_is_degraded_and_names_the_three_usual_causes() {
        let (n, c) = (Counters::default(), Catalog::default());
        let (h, _) = assess(&inputs(&n, &c, None), Reading::default());
        assert_eq!(h.state, HealthState::Degraded);
        assert!(h.detail.unwrap().contains("firewall"));
    }

    #[test]
    fn a_port_in_use_says_so_and_what_to_do() {
        let (n, c) = (Counters::default(), Catalog::default());
        let (h, _) = assess(&inputs(&n, &c, Some("Could not bind: Address already in use")), Reading::default());
        assert_eq!(h.state, HealthState::Failing);
        assert!(h.detail.unwrap().contains("Choose another port"));
    }

    #[test]
    fn a_flowing_feed_is_ok_and_says_how_much_is_lost() {
        let (n, c) = (Counters::default(), Catalog::default());
        n.arrived(1316);
        Counters::add(&n.bytes_out, 1316);
        let (h, _) = assess(&inputs(&n, &c, None), Reading::default());
        assert_eq!(h.state, HealthState::Ok, "{:?}", h.detail);
        assert!(h.detail.unwrap().contains("0 packets lost"));
    }
}
