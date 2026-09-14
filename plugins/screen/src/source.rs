//! The `screen/source` provide: one instance, one screen or part of one.

use std::time::Duration;

use godwinmix_capture_common::{capture, Capture};
use godwinmix_sdk::prelude::*;
use serde_json::Value;

use crate::pipeline;
use crate::settings::Settings;
use crate::tools;

/// How long `start` waits for the first frame before carrying on.
///
/// A screen capture is usually instant. It is not on macOS the first time,
/// where the system puts up a permission dialogue and hands over nothing until
/// somebody answers it. Waiting the whole budget would not help: nobody
/// answers a dialogue in two seconds. So this waits the same short while as a
/// camera and then says so in `health`.
const FIRST_FRAME_WITHIN: Duration = Duration::from_millis(2_000);

/// How many times to ask for the screen before giving up. `restart-in-place`
/// asks again within milliseconds of letting go, and the platform is not
/// always ready to hand it back that fast.
const OPEN_ATTEMPTS: u32 = 3;
const OPEN_GAP: Duration = Duration::from_millis(250);

pub struct ScreenSource {
    settings: Settings,
    reporter: Option<Reporter>,
    last_start: Option<StartParams>,
    capture: Option<Capture>,
}

impl ScreenSource {
    pub fn new() -> ScreenSource {
        ScreenSource {
            settings: Settings::default(),
            reporter: None,
            last_start: None,
            capture: None,
        }
    }

    fn adopt(&mut self, ready: &Ready) {
        self.settings = Settings::from(&ready.params);
    }

    fn what(&self) -> String {
        if self.settings.label.is_empty() {
            format!("the screen capture of monitor {}", self.settings.monitor)
        } else {
            format!("the screen capture '{}'", self.settings.label)
        }
    }

    fn open(&mut self, params: &StartParams) -> Result<(), RpcError> {
        let settings = self.settings.clone();
        let params = params.clone();
        let reporter = self.reporter.clone();
        let capture = capture::open_with_retry(
            OPEN_ATTEMPTS,
            OPEN_GAP,
            FIRST_FRAME_WITHIN,
            reporter.as_ref(),
            || {
                let pipeline =
                    pipeline::build(&settings, params.canvas, params.transport, &params.media)?;
                Capture::start(pipeline, Some("gmx-video-queue"), reporter.clone())
            },
        )
        .map_err(|why| internal(format!("the screen capture would not start: {why}")))?;
        if capture.buffers() == 0 {
            if let Some(r) = &self.reporter {
                r.warn(format!(
                    "{} has not sent a frame yet. On macOS the system is probably asking for \
                     screen recording permission; answer it and the picture appears.",
                    self.what()
                ));
            }
        }
        if let Some(r) = &self.reporter {
            r.info(format!(
                "{} is running at {}x{}@{} over {}",
                self.what(),
                params.canvas.width,
                params.canvas.height,
                params.canvas.fps,
                params.transport.as_str()
            ));
        }
        self.capture = Some(capture);
        self.last_start = Some(params);
        Ok(())
    }

    fn reopen(&mut self) -> Result<(), RpcError> {
        let Some(params) = self.last_start.clone() else {
            return Ok(());
        };
        self.capture = None;
        self.open(&params)
    }
}

impl Source for ScreenSource {
    fn initialize(
        &mut self,
        ready: &Ready,
        reporter: Reporter,
    ) -> Result<InitializeResult, RpcError> {
        self.adopt(ready);
        reporter.info(format!(
            "screen '{}' on a {}x{}@{} canvas, transport {}",
            ready.instance,
            ready.canvas.width,
            ready.canvas.height,
            ready.canvas.fps,
            ready.transport.as_str()
        ));
        self.reporter = Some(reporter);
        Ok(InitializeResult { latency_ms: None })
    }

    fn start(&mut self, params: &StartParams) -> Result<StartResult, RpcError> {
        self.open(params)?;
        Ok(StartResult { latency_ms: None })
    }

    fn stop(&mut self) -> Result<(), RpcError> {
        if let Some(capture) = self.capture.as_ref() {
            capture.drain(Duration::from_millis(100));
        }
        self.capture = None;
        Ok(())
    }

    fn configure(&mut self, params: Value) -> Result<Configure, RpcError> {
        let next = Settings::from(&params);
        let restart = self.settings.needs_restart(&next);
        self.settings = next;
        if restart && self.capture.is_some() {
            self.reopen()?;
        }
        Ok(Configure::applied())
    }

    fn health(&mut self) -> Health {
        match self.capture.as_ref() {
            Some(capture) => capture.health(&self.what()),
            None => Health::ok(),
        }
    }

    fn call(&mut self, method: &str, params: Value) -> Result<Value, RpcError> {
        tools::dispatch(method, params)
    }
}

fn internal(message: String) -> RpcError {
    RpcError::new(codes::INTERNAL_ERROR, message).with_data(serde_json::json!({"retryable": true}))
}

#[cfg(test)]
mod tests {
    use super::*;
    use serde_json::json;

    fn ready(params: Value) -> Ready {
        serde_json::from_value(json!({
            "core": "godwinmix", "version": "0.2.0", "api_level": 1, "api_compatible": 1,
            "canvas": {"width": 320, "height": 180, "fps": 30},
            "transport": "container", "media": "", "instance": "lyrics",
            "provide": "source", "params": params
        }))
        .expect("the handshake answer parses")
    }

    #[test]
    fn the_handshake_answer_sets_the_params() {
        let mut source = ScreenSource::new();
        source.adopt(&ready(json!({"monitor": 1, "label": "Lyrics screen"})));
        assert_eq!(source.settings.monitor, 1);
        assert!(source.what().contains("Lyrics screen"));
    }

    #[test]
    fn with_no_label_the_health_message_names_the_monitor() {
        let mut source = ScreenSource::new();
        source.adopt(&ready(json!({"monitor": 2})));
        assert!(source.what().contains("monitor 2"), "{}", source.what());
    }

    #[test]
    fn a_source_that_never_started_is_healthy_and_stops_cleanly() {
        let mut source = ScreenSource::new();
        assert_eq!(source.health().state, HealthState::Ok);
        source.stop().expect("stopping a stopped capture is fine");
    }

    #[test]
    fn configure_before_start_is_applied_and_opens_nothing() {
        let mut source = ScreenSource::new();
        let answer = source
            .configure(json!({"monitor": 1, "show_cursor": false}))
            .unwrap();
        assert!(answer.applied);
        assert!(source.capture.is_none());
    }

    #[test]
    fn an_unknown_method_is_refused_rather_than_ignored() {
        let mut source = ScreenSource::new();
        let err = source
            .call("teleport", json!({}))
            .expect_err("no such method");
        assert_eq!(err.code, codes::METHOD_NOT_FOUND);
    }
}
