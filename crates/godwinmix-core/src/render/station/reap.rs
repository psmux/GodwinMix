//! Collecting a calibration process that has been told to stop, without
//! letting it hold the stop up.
//!
//! A killed process is normally gone in milliseconds, but not always. On a
//! macOS runner on 2026-10-11 the VideoToolbox encoder service hung while the
//! calibration had sessions open, and the killed calibration then sat in the
//! kernel trying to exit, for good. `stop_calibrating` waited for it with a
//! plain `wait`, first thing in the station's shutdown, so the station never
//! stopped its shows and had to be killed itself twenty seconds later. The
//! stop now waits a moment and then leaves the collecting to a thread.

use std::process::Child;
use std::time::{Duration, Instant};
use tracing::warn;

/// How long a stop waits to collect the child before it goes on without.
pub const REAP_WAIT: Duration = Duration::from_secs(2);

/// Collect `child`, which has already been told to go, waiting at most
/// `limit`. True when it was collected. One that has not finished exiting by
/// then is handed to a thread of its own that waits for it, so it is still
/// collected if it ever does finish, and false is returned.
pub fn collect_within(mut child: Child, limit: Duration) -> bool {
    let started = Instant::now();
    loop {
        match child.try_wait() {
            Ok(Some(_)) | Err(_) => return true,
            Ok(None) if started.elapsed() >= limit => break,
            Ok(None) => std::thread::sleep(Duration::from_millis(20)),
        }
    }
    let pid = child.id();
    warn!(
        pid,
        waited_ms = started.elapsed().as_millis() as u64,
        "the calibration process was killed and has not finished exiting; collecting it on a thread so the stop goes on"
    );
    let spawned = std::thread::Builder::new().name(format!("reap-{pid}")).spawn(move || {
        let _ = child.wait();
    });
    if spawned.is_err() {
        warn!(pid, "no thread to collect the calibration process on; it is left to the system");
    }
    false
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::process::{Command, Stdio};

    fn sleeper() -> Child {
        let mut cmd = if cfg!(windows) {
            let mut c = Command::new("cmd");
            c.args(["/C", "ping -n 30 127.0.0.1 >NUL"]);
            c
        } else {
            let mut c = Command::new("sh");
            c.args(["-c", "sleep 30"]);
            c
        };
        cmd.stdin(Stdio::null()).stdout(Stdio::null()).spawn().expect("a process starts")
    }

    #[test]
    fn a_killed_child_is_collected_at_once() {
        let mut child = sleeper();
        let _ = child.kill();
        let began = Instant::now();
        assert!(collect_within(child, REAP_WAIT), "a killed child was not collected");
        assert!(began.elapsed() < REAP_WAIT, "it took {:?}", began.elapsed());
    }

    /// The hang, as the stop sees it: a child told to go that has not gone.
    /// Here the sleeper is simply not killed, which from the parent's side
    /// looks the same as one stuck in the kernel on its way out.
    #[test]
    fn a_child_that_does_not_go_does_not_hold_the_stop() {
        let child = sleeper();
        let pid = child.id();
        let began = Instant::now();
        assert!(!collect_within(child, Duration::from_millis(300)), "a running child read as collected");
        assert!(began.elapsed() < Duration::from_secs(2), "the stop waited {:?}", began.elapsed());
        end(pid);
    }

    /// The station's own stop, with a real calibrating child in its place.
    #[test]
    fn a_station_stops_its_calibration_and_goes_on() {
        let _ = gstreamer::init();
        let governor = godwinmix_govern::Governor::new(Default::default(), godwinmix_govern::Profile::uncalibrated());
        let cat = crate::catalogue::Catalogue::default();
        let st = super::super::Station::with_governor(governor, &cat, crate::config::Accel::Auto);
        *st.inner.child.lock() = Some(sleeper());
        let began = Instant::now();
        st.stop_calibrating();
        assert!(began.elapsed() < REAP_WAIT, "the stop waited {:?}", began.elapsed());
        assert!(st.inner.child.lock().is_none(), "the child is still held");
        assert!(st.inner.halted.load(std::sync::atomic::Ordering::SeqCst), "more calibrations may start");
    }

    #[cfg(unix)]
    fn end(pid: u32) {
        // SAFETY: a signal to the sleeper this test started, still uncollected.
        unsafe { libc::kill(pid as i32, libc::SIGKILL) };
    }

    #[cfg(windows)]
    fn end(pid: u32) {
        let _ = Command::new("taskkill").args(["/F", "/T", "/PID", &pid.to_string()]).status();
    }
}
