//! macOS: the Mach host statistics for the machine, sysctl for what it is.
//!
//! GPU and media engine load are not read: IOKit's accelerator statistics
//! need CoreFoundation dictionaries walked every second, which costs more
//! than the rest of the sampler together. VideoToolbox work is counted from
//! the calibrated share each ticket declares instead.

use std::sync::OnceLock;

/// The host port, fetched once: every `mach_host_self` call adds a send
/// right that is never given back.
fn host() -> libc::mach_port_t {
    static HOST: OnceLock<libc::mach_port_t> = OnceLock::new();
    // libc marks the Mach calls deprecated in favour of the mach2 crate; one
    // call is not worth a dependency, and the symbol is stable in macOS.
    // SAFETY: no arguments, returns a port name.
    #[allow(deprecated)]
    *HOST.get_or_init(|| unsafe { libc::mach_host_self() })
}

/// Busy and total ticks over every core since boot.
pub fn system_ticks() -> Option<(u64, u64)> {
    let mut info = libc::host_cpu_load_info { cpu_ticks: [0; libc::CPU_STATE_MAX as usize] };
    let mut count = libc::HOST_CPU_LOAD_INFO_COUNT;
    // SAFETY: the buffer is the struct the flavour names and count is its
    // size in natural_t units, as the header defines.
    let rc = unsafe {
        libc::host_statistics(
            host(),
            libc::HOST_CPU_LOAD_INFO,
            &mut info as *mut _ as libc::host_info_t,
            &mut count,
        )
    };
    if rc != libc::KERN_SUCCESS {
        return None;
    }
    let t = info.cpu_ticks.map(u64::from);
    let idle = t[libc::CPU_STATE_IDLE as usize];
    let total: u64 = t.iter().sum();
    Some((total - idle, total))
}

/// Total memory, and what could be handed out now (free, inactive and
/// speculative pages), in MiB.
pub fn memory_mib() -> Option<(u64, u64)> {
    let total = sysctl_u64("hw.memsize")? / 1_048_576;
    // SAFETY: an all zero vm_statistics64 is a valid value of the type.
    let mut vm: libc::vm_statistics64 = unsafe { std::mem::zeroed() };
    let mut count = libc::HOST_VM_INFO64_COUNT;
    // SAFETY: as above, the flavour's own struct and its own count.
    let rc = unsafe {
        libc::host_statistics64(host(), libc::HOST_VM_INFO64, &mut vm as *mut _ as libc::host_info64_t, &mut count)
    };
    if rc != libc::KERN_SUCCESS {
        return Some((total, total));
    }
    let page = sysctl_u64("hw.pagesize").unwrap_or(16_384);
    let pages = u64::from(vm.free_count) + u64::from(vm.inactive_count) + u64::from(vm.speculative_count);
    Some((total, pages * page / 1_048_576))
}

pub fn cpu_model() -> String {
    sysctl_string("machdep.cpu.brand_string").unwrap_or_else(|| "unknown".into())
}

/// No device load on macOS; see the module comment.
pub fn device_busy() -> Vec<(String, u32)> {
    Vec::new()
}

fn sysctl_u64(name: &str) -> Option<u64> {
    let c = std::ffi::CString::new(name).ok()?;
    let mut v: u64 = 0;
    let mut len = std::mem::size_of::<u64>();
    // SAFETY: a NUL terminated name and a buffer of the length we pass.
    let rc = unsafe { libc::sysctlbyname(c.as_ptr(), &mut v as *mut _ as *mut libc::c_void, &mut len, std::ptr::null_mut(), 0) };
    (rc == 0).then_some(v)
}

fn sysctl_string(name: &str) -> Option<String> {
    let c = std::ffi::CString::new(name).ok()?;
    let mut buf = [0u8; 256];
    let mut len = buf.len();
    // SAFETY: as above, with a byte buffer.
    let rc = unsafe { libc::sysctlbyname(c.as_ptr(), buf.as_mut_ptr() as *mut libc::c_void, &mut len, std::ptr::null_mut(), 0) };
    if rc != 0 {
        return None;
    }
    let s = String::from_utf8_lossy(&buf[..len]).trim_end_matches('\0').trim().to_string();
    (!s.is_empty()).then_some(s)
}
