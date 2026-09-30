//! What machine this is, and a short key that changes when it does.
//!
//! The key covers the CPU, the core count, the memory, the operating system,
//! the GStreamer version and the encoder elements present. A new driver that
//! adds or removes an encoder, a GStreamer upgrade, or the same disk in a
//! different box all make a new key, and so a fresh calibration.

use crate::load::sys;
use serde::{Deserialize, Serialize};

#[derive(Debug, Clone, Default, PartialEq, Eq, Serialize, Deserialize)]
pub struct Machine {
    pub cpu: String,
    pub cores: u32,
    pub memory_mib: u64,
    pub os: String,
    pub arch: String,
}

impl Machine {
    /// This machine, read cheaply from the operating system.
    pub fn current() -> Machine {
        Machine {
            cpu: sys::cpu_model(),
            cores: cores(),
            memory_mib: sys::memory_mib().map(|(total, _)| total).unwrap_or(0),
            os: std::env::consts::OS.to_string(),
            arch: std::env::consts::ARCH.to_string(),
        }
    }
}

/// Logical cores this process may use.
pub fn cores() -> u32 {
    std::thread::available_parallelism().map(|n| n.get() as u32).unwrap_or(1)
}

/// The key a calibration is stored under: sixteen hex digits.
///
/// `encoders` is every encoder element calibration would try, and `media`
/// the GStreamer version, so either changing measures the machine again.
pub fn fingerprint(machine: &Machine, encoders: &[String], media: &str) -> String {
    let mut sorted: Vec<&str> = encoders.iter().map(String::as_str).collect();
    sorted.sort_unstable();
    sorted.dedup();
    let text = format!(
        "{}|{}|{}|{}|{}|{}|{}",
        machine.cpu,
        machine.cores,
        // Rounded to the GiB: the figure an OS reports moves by a few
        // megabytes between boots on some machines.
        machine.memory_mib / 1024,
        machine.os,
        machine.arch,
        media,
        sorted.join(","),
    );
    format!("{:016x}", fnv1a(text.as_bytes()))
}

/// FNV-1a, 64 bit. A key for a file name, not a security boundary, so a
/// dozen lines beat a hashing crate.
fn fnv1a(bytes: &[u8]) -> u64 {
    let mut h: u64 = 0xcbf2_9ce4_8422_2325;
    for b in bytes {
        h ^= u64::from(*b);
        h = h.wrapping_mul(0x0000_0100_0000_01b3);
    }
    h
}

#[cfg(test)]
mod tests {
    use super::*;

    fn m() -> Machine {
        Machine { cpu: "Test CPU".into(), cores: 8, memory_mib: 16_384, os: "linux".into(), arch: "x86_64".into() }
    }

    #[test]
    fn the_same_machine_gives_the_same_key_whatever_the_order() {
        let a = fingerprint(&m(), &["x264enc".into(), "vtenc_h264_hw".into()], "1.28.7");
        let b = fingerprint(&m(), &["vtenc_h264_hw".into(), "x264enc".into()], "1.28.7");
        assert_eq!(a, b);
        assert_eq!(a.len(), 16);
    }

    #[test]
    fn a_new_encoder_or_gstreamer_or_cpu_changes_the_key() {
        let base = fingerprint(&m(), &["x264enc".into()], "1.28.7");
        assert_ne!(base, fingerprint(&m(), &["x264enc".into(), "nvh264enc".into()], "1.28.7"));
        assert_ne!(base, fingerprint(&m(), &["x264enc".into()], "1.30.0"));
        let mut other = m();
        other.cores = 4;
        assert_ne!(base, fingerprint(&other, &["x264enc".into()], "1.28.7"));
    }

    #[test]
    fn this_machine_reads_something() {
        let here = Machine::current();
        assert!(here.cores >= 1);
        assert!(!here.cpu.is_empty());
    }
}
