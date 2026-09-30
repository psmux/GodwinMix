//! What is left: capacity, less what other programs take at their peak, less
//! what this process takes or has been promised, less a reserve worked out
//! for the kind of machine it is. Plain arithmetic, no clock and no I/O.

use crate::load::Load;
use godwinmix_protocol::rendition::Cost;

/// "No limit known" in a [`Cost`] field of what is left.
pub const UNLIMITED: u32 = u32::MAX;

/// The CPU to keep free, in thousandths of a core.
///
/// On a desktop the page, the window system and whatever else the person
/// has open share the machine, so a fifth of it with at least a core and a
/// half. On a headless server a twelfth with at least half a core. On top,
/// how far the machine's load has jumped above its mean lately, so a bursty
/// machine keeps more room. Never more than half the machine. An override
/// from the config replaces all of it.
pub fn reserve(cores: u32, desktop: bool, jitter: u32, override_millicores: Option<u32>) -> u32 {
    let cap = cores.max(1) * 1000;
    if let Some(o) = override_millicores {
        return o.min(cap);
    }
    let base = if desktop { (cap / 5).max(1500) } else { (cap / 12).max(500) };
    (base + jitter.min(cap / 4)).min(cap / 2)
}

/// Memory to keep free: a tenth, at least 512 MiB.
pub fn memory_reserve(total_mib: u64) -> u64 {
    (total_mib / 10).max(512)
}

/// The device a claim runs on, and what it already holds there.
#[derive(Debug, Clone, Copy, Default)]
pub struct DeviceUse {
    pub committed_millis: u32,
    pub committed_sessions: u32,
    pub session_limit: Option<u32>,
    /// How busy the platform says the device is, when it says.
    pub measured_millis: Option<u32>,
}

/// Everything [`have`] needs.
#[derive(Debug, Clone)]
pub struct Inputs<'a> {
    pub cores: u32,
    pub memory_total_mib: u64,
    pub load: &'a Load,
    /// Every ticket held.
    pub committed: Cost,
    pub reserve_millicores: u32,
    pub device: Option<DeviceUse>,
    pub uplink_kbps: Option<u32>,
}

/// What could be admitted now.
pub fn have(i: &Inputs) -> Cost {
    let cap = i64::from(i.cores.max(1)) * 1000;
    // Other programs at their recent peak, and this process at whichever is
    // larger of what it measures and what it has promised: a ticket granted
    // a moment ago is not in the measurement yet, and work started without
    // a ticket (the compositor, the page's own previews) is only in the
    // measurement.
    let own = i64::from(i.load.own_millicores.max(i.committed.cpu_millicores));
    let cpu = cap - i64::from(i.reserve_millicores) - i64::from(i.load.others_peak_millicores) - own;
    let mem_reserve = memory_reserve(i.memory_total_mib);
    let by_book = i.memory_total_mib.saturating_sub(mem_reserve + u64::from(i.committed.memory_mib));
    let memory = match i.load.available_mib {
        Some(a) => a.saturating_sub(mem_reserve).min(by_book),
        None => by_book,
    };
    let (device_millis, device_sessions) = match i.device {
        Some(d) => {
            let busy = d.measured_millis.unwrap_or(0).max(d.committed_millis);
            // A tenth of the device held back, as the CPU reserve is.
            let millis = 900u32.saturating_sub(busy);
            let sessions = d.session_limit.map_or(UNLIMITED, |l| l.saturating_sub(d.committed_sessions));
            (millis, sessions)
        }
        None => (0, 0),
    };
    let egress = match i.uplink_kbps {
        // Four fifths of the uplink: a link run at its limit drops packets.
        Some(u) => (u / 5 * 4).saturating_sub(i.committed.egress_kbps),
        None => UNLIMITED,
    };
    Cost {
        cpu_millicores: cpu.clamp(0, i64::from(u32::MAX)) as u32,
        device_millis,
        device_sessions,
        memory_mib: memory.min(u64::from(u32::MAX)) as u32,
        egress_kbps: egress,
    }
}

/// Which parts of `need` do not fit in `have`, by name. Empty means it fits.
pub fn short(need: &Cost, have: &Cost) -> Vec<&'static str> {
    let mut out = Vec::new();
    if need.cpu_millicores > have.cpu_millicores {
        out.push("cpu");
    }
    if need.device_millis > have.device_millis {
        out.push("device");
    }
    if need.device_sessions > have.device_sessions {
        out.push("sessions");
    }
    if need.memory_mib > have.memory_mib {
        out.push("memory");
    }
    if need.egress_kbps > have.egress_kbps {
        out.push("uplink");
    }
    out
}

/// One of the names [`short`] gives, back from a string that crossed a
/// process boundary. `None` for a name this build does not know.
pub fn short_name(name: &str) -> Option<&'static str> {
    ["cpu", "device", "sessions", "memory", "uplink"].into_iter().find(|n| *n == name)
}

#[cfg(test)]
mod tests;
