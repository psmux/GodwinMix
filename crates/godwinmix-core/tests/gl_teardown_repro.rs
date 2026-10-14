//! Throwaway: run the GL shader probe in a process that stays up, the way a
//! show does after `fx.list`, and see whether the process survives.

#[cfg(windows)]
mod veh {
    #[repr(C)]
    struct Record { code: u32, flags: u32, rec: *mut Record, addr: *mut u8, n: u32, info: [usize; 15] }
    #[repr(C)]
    struct Pointers { rec: *mut Record, ctx: *mut u8 }
    extern "system" {
        fn AddVectoredExceptionHandler(first: u32, h: unsafe extern "system" fn(*mut Pointers) -> i32) -> *mut u8;
        fn GetModuleHandleExW(flags: u32, addr: *const u8, out: *mut *mut u8) -> i32;
        fn GetModuleFileNameW(m: *mut u8, buf: *mut u16, n: u32) -> u32;
        fn GetCurrentThreadId() -> u32;
    }
    unsafe extern "system" fn handler(p: *mut Pointers) -> i32 {
        let r = &*(*p).rec;
        if r.code == 0xC000_0005 {
            let mut m = std::ptr::null_mut();
            let mut buf = [0u16; 260];
            let mut name = String::from("?");
            if GetModuleHandleExW(6, r.addr, &mut m) != 0 {
                let n = GetModuleFileNameW(m, buf.as_mut_ptr(), 260);
                name = String::from_utf16_lossy(&buf[..n as usize]);
            }
            eprintln!("ACCESS VIOLATION thread {} in {} +0x{:x} reading 0x{:x}", GetCurrentThreadId(), name, r.addr as usize - m as usize, r.info[1]);
        }
        0
    }
    pub fn install() {
        unsafe { AddVectoredExceptionHandler(1, handler) };
    }
}

#[test]
#[ignore]
fn gl_probe_then_wait() {
    #[cfg(windows)]
    veh::install();
    let rounds: u32 = std::env::var("GMX_REPRO_ROUNDS").ok().and_then(|v| v.parse().ok()).unwrap_or(1);
    let wait: u64 = std::env::var("GMX_REPRO_WAIT").ok().and_then(|v| v.parse().ok()).unwrap_or(40);
    let t0 = std::time::Instant::now();
    use gstreamer::prelude::*;
    gstreamer::init().unwrap();
    let _decode = std::env::var("GMX_REPRO_DEC").ok().map(|desc| {
        let p = gstreamer::parse::launch(&desc).unwrap();
        p.set_state(gstreamer::State::Playing).unwrap();
        std::thread::sleep(std::time::Duration::from_secs(3));
        eprintln!("decode pipeline running at {:?}", t0.elapsed());
        p
    });
    let work = move || {
        eprintln!("probe says gpu = {}", godwinmix_core::fx::shader::probe::available());
        let frag = godwinmix_core::fx::shader::fragment("vec4 transition(vec2 uv) { return mix(getFromColor(uv), getToColor(uv), progress); }").unwrap();
        for i in 0..rounds {
            let gl = godwinmix_core::fx::shader::gl::Gl::start(&frag, (128, 72)).unwrap();
            std::thread::sleep(std::time::Duration::from_millis(300));
            gl.close();
            eprintln!("round {i} closed at {:?}", t0.elapsed());
        }
    };
    if std::env::var("GMX_REPRO_NOGL").is_ok() {
        eprintln!("no GL in this run");
    } else if std::env::var("GMX_REPRO_THREAD").is_ok() {
        // As `fx.list` does: on a worker thread that then ends.
        std::thread::spawn(work).join().unwrap();
        eprintln!("worker thread ended at {:?}", t0.elapsed());
    } else {
        work();
    }
    std::thread::sleep(std::time::Duration::from_secs(wait));
    eprintln!("still alive at {:?}", t0.elapsed());
}
