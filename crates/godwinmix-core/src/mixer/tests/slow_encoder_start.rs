//! The first consumer of the programme encoder starts it, and a hardware
//! encoder opens its session inside its state change: a second or more for
//! Quick Sync and Media Foundation. On 2026-11-02 a guest's return was that
//! first consumer on a Windows laptop and its `output.add` held the mixer for
//! more than five seconds. Starting the encoder costs the mixer thread nothing.

use super::*;
use std::sync::atomic::{AtomicU64, Ordering};
use std::sync::Arc;

/// How long the encoder takes to open. Longer than any answer accepted.
pub(super) const OPENING: Duration = Duration::from_secs(3);

/// The longest the `output.add` that starts the encoder may take to answer.
const ANSWERED: Duration = Duration::from_millis(500);

/// Hold `el`'s state lock on a thread of its own for `OPENING`, as an encoder
/// opening its session holds its own state change. Says once it has it.
pub(super) fn hold_state_lock(el: gst::Element) -> std::sync::mpsc::Receiver<()> {
    let (tx, rx) = std::sync::mpsc::channel();
    std::thread::spawn(move || {
        let raw: *mut gst::ffi::GstElement = el.as_ptr();
        // SAFETY: `el` lives to the end of this closure, and the lock is a
        // recursive mutex taken and given back on this one thread.
        unsafe { gst::glib::ffi::g_rec_mutex_lock(std::ptr::addr_of_mut!((*raw).state_lock)) };
        let _ = tx.send(());
        std::thread::sleep(OPENING);
        unsafe { gst::glib::ffi::g_rec_mutex_unlock(std::ptr::addr_of_mut!((*raw).state_lock)) };
    });
    rx
}

fn count_buffers(pad: &gst::Pad) -> Arc<AtomicU64> {
    let n = Arc::new(AtomicU64::new(0));
    let count = n.clone();
    pad.add_probe(gst::PadProbeType::BUFFER, move |_, _| {
        count.fetch_add(1, Ordering::Relaxed);
        gst::PadProbeReturn::Ok
    });
    n
}

#[tokio::test(flavor = "multi_thread")]
async fn an_encoder_slow_to_open_does_not_hold_the_output_add_that_starts_it() {
    let _ = gst::init();
    let mut cfg = programme_config(crate::config::Accel::Software);
    cfg.program.encoder = "on-demand".into();
    let (mut mix, handle, cmd_rx, _bus) = Mixer::build(cfg).expect("mixer builds");
    mix.start().expect("the programme starts");
    let bars: SourceConfig = toml::from_str("id = \"bars\"\nuri = \"test://smpte\"\n").unwrap();
    mix.add_source(&bars, None).unwrap();
    let venc = mix.program.by_name("venc").expect("the programme's video encoder");
    let encoded = count_buffers(&venc.static_pad("src").unwrap());
    let enc = mix.encoder_handle();
    let thread = spawn(mix, cmd_rx, handle.clone());
    hold_state_lock(venc).recv().expect("the encoder's state lock is held");

    let uri = format!("rtmp://127.0.0.1:{}/live/key", super::slow_output::silent_server());
    let asked = Instant::now();
    handle.request(|ack| Command::AddOutput(Box::new(OutputConfig::bare("first", &uri)), Some(ack))).await.expect("the output is added");
    let took = asked.elapsed();
    let asked = Instant::now();
    handle.status().await.expect("status answers while the encoder opens");
    let status_took = asked.elapsed();

    let until = Instant::now() + OPENING + Duration::from_secs(10).mul_f64(crate::plugin::harness::timing_slack());
    while (!enc.is_running() || encoded.load(Ordering::Relaxed) == 0) && Instant::now() < until {
        tokio::time::sleep(Duration::from_millis(50)).await;
    }
    let (running, frames) = (enc.is_running(), encoded.load(Ordering::Relaxed));
    let _ = handle.send(Command::Shutdown);
    tokio::task::spawn_blocking(move || thread.join()).await.unwrap().unwrap();
    assert!(took < ANSWERED, "output.add took {took:?} while the encoder opened");
    assert!(status_took < ANSWERED, "status took {status_took:?} while the encoder opened");
    assert!(running && frames > 0, "the encoder never came up once it had opened: running {running}, {frames} frames");
}
