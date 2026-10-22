//! NVML, opened at run time from the driver's own `libnvidia-ml.so.1`, so
//! the binary links nothing of NVIDIA's and runs the same on a box with no
//! card. Each call is a read of what the driver already counts; a second's
//! reading of one GPU is a dozen of them, microseconds in all.

use super::gpu::Read;
use godwinmix_protocol::machine::GpuSample;
use std::ffi::{c_char, c_int, c_uint, c_void, CStr};

type Dev = *mut c_void;
type Ret = c_int;

#[repr(C)]
#[derive(Default)]
struct Utilization {
    gpu: c_uint,
    memory: c_uint,
}

#[repr(C)]
#[derive(Default)]
struct Memory {
    total: u64,
    free: u64,
    used: u64,
}

/// The functions used, looked up by name once.
pub struct Nvml {
    count: unsafe extern "C" fn(*mut c_uint) -> Ret,
    handle: unsafe extern "C" fn(c_uint, *mut Dev) -> Ret,
    name: unsafe extern "C" fn(Dev, *mut c_char, c_uint) -> Ret,
    driver: unsafe extern "C" fn(*mut c_char, c_uint) -> Ret,
    util: unsafe extern "C" fn(Dev, *mut Utilization) -> Ret,
    enc: unsafe extern "C" fn(Dev, *mut c_uint, *mut c_uint) -> Ret,
    dec: unsafe extern "C" fn(Dev, *mut c_uint, *mut c_uint) -> Ret,
    mem: unsafe extern "C" fn(Dev, *mut Memory) -> Ret,
    sessions: unsafe extern "C" fn(Dev, *mut c_uint, *mut c_uint, *mut c_uint) -> Ret,
    temp: unsafe extern "C" fn(Dev, c_int, *mut c_uint) -> Ret,
    power: unsafe extern "C" fn(Dev, *mut c_uint) -> Ret,
    limit: unsafe extern "C" fn(Dev, *mut c_uint) -> Ret,
    clock: unsafe extern "C" fn(Dev, c_int, *mut c_uint) -> Ret,
    fan: unsafe extern "C" fn(Dev, *mut c_uint) -> Ret,
}

// SAFETY: NVML is documented thread safe, and the pointers are plain functions.
unsafe impl Send for Nvml {}

/// One symbol out of the library, as the function type it is declared with.
unsafe fn sym<T>(lib: *mut c_void, name: &CStr) -> Option<T> {
    let p = libc::dlsym(lib, name.as_ptr());
    (!p.is_null()).then(|| std::mem::transmute_copy(&p))
}

impl Nvml {
    /// Open the library and initialise it, or `None` on a machine without it.
    pub fn open() -> Option<Nvml> {
        // SAFETY: dlopen of a library by name; every symbol is checked for
        // null and given the signature NVML's header declares for it.
        unsafe {
            let lib = libc::dlopen(c"libnvidia-ml.so.1".as_ptr(), libc::RTLD_NOW | libc::RTLD_LOCAL);
            if lib.is_null() {
                return None;
            }
            let init: unsafe extern "C" fn() -> Ret = sym(lib, c"nvmlInit_v2")?;
            if init() != 0 {
                return None;
            }
            Some(Nvml {
                count: sym(lib, c"nvmlDeviceGetCount_v2")?,
                handle: sym(lib, c"nvmlDeviceGetHandleByIndex_v2")?,
                name: sym(lib, c"nvmlDeviceGetName")?,
                driver: sym(lib, c"nvmlSystemGetDriverVersion")?,
                util: sym(lib, c"nvmlDeviceGetUtilizationRates")?,
                enc: sym(lib, c"nvmlDeviceGetEncoderUtilization")?,
                dec: sym(lib, c"nvmlDeviceGetDecoderUtilization")?,
                mem: sym(lib, c"nvmlDeviceGetMemoryInfo")?,
                sessions: sym(lib, c"nvmlDeviceGetEncoderStats")?,
                temp: sym(lib, c"nvmlDeviceGetTemperature")?,
                power: sym(lib, c"nvmlDeviceGetPowerUsage")?,
                limit: sym(lib, c"nvmlDeviceGetEnforcedPowerLimit")?,
                clock: sym(lib, c"nvmlDeviceGetClockInfo")?,
                fan: sym(lib, c"nvmlDeviceGetFanSpeed")?,
            })
        }
    }

    pub fn read(&mut self) -> Vec<Read> {
        let mut n: c_uint = 0;
        // SAFETY: the out pointers are locals of the types NVML writes.
        if unsafe { (self.count)(&mut n) } != 0 {
            return Vec::new();
        }
        let driver = self.text(|b, l| unsafe { (self.driver)(b, l) });
        (0..n).filter_map(|i| self.device(i, driver.clone())).collect()
    }

    fn text(&self, f: impl Fn(*mut c_char, c_uint) -> Ret) -> Option<String> {
        let mut buf = [0 as c_char; 96];
        if f(buf.as_mut_ptr(), buf.len() as c_uint) != 0 {
            return None;
        }
        // SAFETY: NVML writes a terminated string within the length given.
        Some(unsafe { CStr::from_ptr(buf.as_ptr()) }.to_string_lossy().into_owned())
    }

    fn device(&self, index: c_uint, driver: Option<String>) -> Option<Read> {
        let mut d: Dev = std::ptr::null_mut();
        // SAFETY: every call below writes only into the locals it is handed.
        unsafe {
            if (self.handle)(index, &mut d) != 0 {
                return None;
            }
            let one = |f: &dyn Fn(*mut c_uint) -> Ret| {
                let mut v: c_uint = 0;
                (f(&mut v) == 0).then_some(v)
            };
            let (mut u, mut m) = (Utilization::default(), Memory::default());
            let util_ok = (self.util)(d, &mut u) == 0;
            let mem_ok = (self.mem)(d, &mut m) == 0;
            let (mut sessions, mut fps, mut latency) = (0, 0, 0);
            let enc_ok = (self.sessions)(d, &mut sessions, &mut fps, &mut latency) == 0;
            let sample = GpuSample {
                at_ms: super::now_ms(),
                gpu_pct: util_ok.then_some(u.gpu),
                encoder_pct: one(&|v| (self.enc)(d, v, &mut 0)),
                decoder_pct: one(&|v| (self.dec)(d, v, &mut 0)),
                memory_used_mib: mem_ok.then_some(m.used / 1_048_576),
                memory_total_mib: mem_ok.then_some(m.total / 1_048_576),
                encoder_sessions: enc_ok.then_some(sessions),
                encoder_fps: enc_ok.then_some(fps),
                temperature_c: one(&|v| (self.temp)(d, 0, v)),
                power_w: one(&|v| (self.power)(d, v)).map(|mw| (mw as f64 / 100.0).round() / 10.0),
                power_limit_w: one(&|v| (self.limit)(d, v)).map(|mw| (mw as f64 / 100.0).round() / 10.0),
                graphics_mhz: one(&|v| (self.clock)(d, 0, v)),
                memory_mhz: one(&|v| (self.clock)(d, 2, v)),
                video_mhz: one(&|v| (self.clock)(d, 3, v)),
                fan_pct: one(&|v| (self.fan)(d, v)),
            };
            let name = self.text(|b, l| (self.name)(d, b, l)).unwrap_or_else(|| format!("GPU {index}"));
            Some(Read { index, name, driver, sample })
        }
    }
}
