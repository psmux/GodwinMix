//! The crash that `fx::shader::gate` keeps out of the show, on demand: GL
//! started in a process that already decodes on D3D12 and encodes through
//! Media Foundation. On the Intel laptop where it was found, a thread of the
//! GL driver's own read a null pointer at `igxelpgicd64.dll` offset `0xd3c80`
//! in 11 runs of 15, a few seconds after GL started. Run it after a driver
//! update to see whether `GODWINMIX_FX_GL=on` has become safe there. It needs
//! several runs, three at a time works, because one run proves little:
//!
//! ```text
//! cargo test -p godwinmix-core --test gl_beside_hardware_video -- --ignored --nocapture
//! ```
//!
//! A run that crashes ends with exit code 0xc0000005 and prints the module
//! and offset of the fault on its way down.

#[cfg(windows)]
mod fault {
    //! Prints where an access violation happened, since the process ends
    //! before anything else can say.
    #[repr(C)]
    struct Record { code: u32, flags: u32, next: *mut Record, addr: *mut u8, n: u32, info: [usize; 15] }
    #[repr(C)]
    struct Pointers { rec: *mut Record, ctx: *mut u8 }
    extern "system" {
        fn AddVectoredExceptionHandler(first: u32, h: unsafe extern "system" fn(*mut Pointers) -> i32) -> *mut u8;
        fn GetModuleHandleExW(flags: u32, addr: *const u8, out: *mut *mut u8) -> i32;
        fn GetModuleFileNameW(m: *mut u8, buf: *mut u16, n: u32) -> u32;
    }
    unsafe extern "system" fn report(p: *mut Pointers) -> i32 {
        let r = &*(*p).rec;
        if r.code == 0xC000_0005 {
            let (mut module, mut buf) = (std::ptr::null_mut(), [0u16; 260]);
            // FROM_ADDRESS | UNCHANGED_REFCOUNT
            let n = if GetModuleHandleExW(6, r.addr, &mut module) != 0 { GetModuleFileNameW(module, buf.as_mut_ptr(), 260) } else { 0 };
            let name = String::from_utf16_lossy(&buf[..n as usize]);
            eprintln!("access violation in {name} at +0x{:x}, reading 0x{:x}", r.addr as usize - module as usize, r.info[1]);
        }
        0 // keep searching: the process still crashes, as it would have
    }
    pub fn install() {
        unsafe { AddVectoredExceptionHandler(1, report) };
    }
}

#[test]
#[ignore]
fn gl_started_beside_hardware_video_survives() {
    use godwinmix_core::fx::shader::{fragment, gate, gl::Gl, probe};
    use gstreamer::prelude::*;
    use std::time::{Duration, Instant};
    #[cfg(windows)]
    fault::install();
    std::env::set_var(gate::SWITCH_ENV, "on");
    gstreamer::init().unwrap();
    let video = "videotestsrc is-live=true pattern=ball ! video/x-raw,width=1920,height=1080,framerate=30/1 \
                 ! mfh264enc ! h264parse ! d3d12h264dec ! d3d12download ! fakesink";
    let decode = gstreamer::parse::launch(video).expect("this test needs mfh264enc and d3d12h264dec");
    decode.set_state(gstreamer::State::Playing).unwrap();
    std::thread::sleep(Duration::from_secs(3));
    let t0 = Instant::now();
    // As `fx.list` does it: on a worker thread that then ends.
    // The probe sends frames, which is what makes the GL context; a pipeline
    // that is started and closed with nothing sent never reaches the driver.
    std::thread::spawn(|| {
        eprintln!("the probe says GL runs: {}", probe::available());
        let frag = fragment("vec4 transition(vec2 uv) { return mix(getFromColor(uv), getToColor(uv), progress); }").unwrap();
        for _ in 0..3 {
            let gl = Gl::start(&frag, (64, 36)).unwrap();
            let _ = probe::moves(&gl, (64, 36));
            gl.close();
        }
    })
    .join()
    .unwrap();
    std::thread::sleep(Duration::from_secs(30));
    eprintln!("alive {:?} after GL started beside the hardware video", t0.elapsed());
    decode.set_state(gstreamer::State::Null).unwrap();
}
