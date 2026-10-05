//! Opening the camera, on the thread capture-common's `Opening` gives it.
//!
//! `start` hands this to `Opening` and answers at once; see that module for
//! why. What is the camera's own is the order it is asked for in, and what a
//! failure is called when it is reported.

use std::time::Duration;

use godwinmix_capture_common::opening::Cancel;
use godwinmix_capture_common::{capture, Capture};
use godwinmix_sdk::prelude::*;

use crate::device::Route;
use crate::pipeline;
use crate::settings::Settings;

/// How long an open waits for the camera's first frame before it says the
/// camera is open anyway. Off the protocol's clock now, so it can wait for a
/// real camera, and `health` says "opening" until it does.
const FIRST_FRAME_WITHIN: Duration = Duration::from_secs(3);

/// How long the Kernel Streaming route has to deliver a first frame before
/// Media Foundation is tried instead. It took about one second on the webcam
/// it was measured on.
const FAST_FIRST_FRAME_WITHIN: Duration = Duration::from_secs(3);

/// How many times to ask for the camera in one round, before the round is
/// reported and `Opening` waits to try again.
///
/// `restart-in-place` hands the device back and asks for it again within
/// milliseconds, and on macOS it is not free yet. On Windows the first try is
/// Kernel Streaming and the next two are the device monitor's choice.
const OPEN_ATTEMPTS: u32 = 3;
const OPEN_GAP: Duration = Duration::from_millis(250);

/// One camera to open, with everything the open needs.
pub struct CameraOpen {
    pub settings: Settings,
    pub start: StartParams,
    pub reporter: Option<Reporter>,
    pub what: String,
}

impl CameraOpen {
    /// One round of attempts. The error, when there is one, is the reason a
    /// person reads in `health`, with what to do about it.
    pub fn run(&self, cancel: &Cancel) -> Result<Capture, String> {
        let mut seen = Vec::new();
        let mut attempt = 0;
        let opened = capture::open_with_retry(
            OPEN_ATTEMPTS,
            OPEN_GAP,
            FIRST_FRAME_WITHIN,
            self.reporter.as_ref(),
            || {
                if cancel.is_set() {
                    return Err("the source was stopped".into());
                }
                attempt += 1;
                let route = if attempt == 1 {
                    Route::Fast
                } else {
                    Route::Monitor
                };
                self.attempt(route).inspect_err(|e| seen.push(e.clone()))
            },
        );
        opened.map_err(|last| {
            let why = explain(&seen, &last);
            if let Some(r) = &self.reporter {
                r.warn(format!("{} would not start: {why}", self.what));
            }
            why
        })
    }

    fn attempt(&self, route: Route) -> Result<Capture, String> {
        let s = &self.start;
        let (pipeline, via) =
            pipeline::build(&self.settings, s.canvas, s.transport, &s.media, route)?;
        let capture = Capture::start(pipeline, Some("gmx-video-queue"), self.reporter.clone())?;
        if via == "ksvideosrc" && !capture.wait_for_data(FAST_FIRST_FRAME_WITHIN) {
            let fault = capture
                .fault()
                .map(|f| format!(": {f}"))
                .unwrap_or_default();
            // Dropped on the way out, which hands the device back before
            // Media Foundation asks for it.
            return Err(format!(
                "Kernel Streaming opened the camera but it sent nothing in {} s{fault}",
                FAST_FIRST_FRAME_WITHIN.as_secs()
            ));
        }
        if let Some(r) = &self.reporter {
            r.info(format!(
                "{} is running at {}x{}@{} over {}, through {via}",
                self.what,
                s.canvas.width,
                s.canvas.height,
                s.canvas.fps,
                s.transport.as_str()
            ));
        }
        Ok(capture)
    }
}

/// The reason for a failed round, with the next step. A camera another app
/// holds is the common case, and Kernel Streaming says so where Media
/// Foundation only says "Internal data stream error", so every attempt's
/// error is looked at, not only the last.
pub fn explain(seen: &[String], last: &str) -> String {
    let busy = seen
        .iter()
        .chain(std::iter::once(&last.to_string()))
        .any(|e| {
            let e = e.to_ascii_lowercase();
            e.contains("in use") || e.contains("busy") || e.contains("0xc00d3704")
        });
    if busy {
        format!(
            "another app is using it ({last}). Close that app (a browser tab, Teams, Zoom or \
             the Camera app) and this source picks the camera up by itself"
        )
    } else {
        format!(
            "{last}. Check that the camera is plugged in and that no other app has it open; \
             this source keeps trying"
        )
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn a_camera_kernel_streaming_calls_in_use_is_reported_as_busy_with_the_next_step() {
        let seen = vec![
            "the pipeline would not reach playing: failed to start capture (device already in use)"
                .to_string(),
            "Internal data stream error".to_string(),
        ];
        let why = explain(&seen, "Internal data stream error");
        assert!(why.starts_with("another app is using it"), "{why}");
        assert!(
            why.contains("Close that app") && why.contains("by itself"),
            "{why}"
        );
    }

    #[test]
    fn any_other_failure_keeps_its_reason_and_says_what_to_check() {
        let why = explain(&[], "no device matches 'cam9'");
        assert!(why.starts_with("no device matches 'cam9'"), "{why}");
        assert!(why.contains("plugged in"), "{why}");
    }
}
