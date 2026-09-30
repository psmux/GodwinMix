//! Anything else: nothing is read, and the governor counts only what it
//! granted.

pub fn system_ticks() -> Option<(u64, u64)> {
    None
}

pub fn memory_mib() -> Option<(u64, u64)> {
    None
}

pub fn cpu_model() -> String {
    "unknown".into()
}

pub fn device_busy() -> Vec<(String, u32)> {
    Vec::new()
}
