//! The `camera/source` provide: one instance, one camera.

use std::time::Duration;

use godwinmix_capture_common::Capture;
use godwinmix_sdk::prelude::*;
use serde_json::Value;

use crate::pipeline;
use crate::settings::Settings;
use crate::tools;

pub struct CameraSource {
    canvas: Canvas,
    settings: Settings,
    reporter: Option<Reporter>,
    /// What `start` was given, so `configure` can open the same transport
    /// again without waiting to be started a second time.
    last_start: Option<StartParams>,
    capture: Option<Capture>,
}

impl CameraSource {
    pub fn new() -> CameraSource {
        CameraSource {
            canvas: Canvas::default(),
            settings: Settings::default(),
            reporter: None,
            last_start: None,
            capture: None,
        }
    }

    /// Take the canvas and the params from the handshake answer.
    ///
    /// Split out of `initialize` so it can be tested: the SDK's `Reporter`
    /// has no public constructor, so a test cannot call `initialize` itself.
    fn adopt(&mut self, ready: &Ready) {
        self.canvas = ready.canvas;
        self.settings = Settings::from(&ready.params);
    }

    fn what(&self) -> String {
        if self.settings.label.is_empty() {
            "the camera".into()
        } else {
            format!("the camera '{}'", self.settings.label)
        }
    }

    fn open(&mut self, params: &StartParams) -> Result<(), RpcError> {
        let pipeline = pipeline::build(
            &self.settings,
            params.canvas,
            params.transport,
            &params.media,
        )
        .map_err(internal)?;
        let capture = Capture::start(pipeline, Some("gmx-video-queue"), self.reporter.clone())
            .map_err(internal)?;
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
        self.canvas = params.canvas;
        self.capture = Some(capture);
        self.last_start = Some(params.clone());
        Ok(())
    }

    /// Close the camera and open it again with the settings as they are now.
    fn reopen(&mut self) -> Result<(), RpcError> {
        let Some(params) = self.last_start.clone() else {
            return Ok(());
        };
        self.capture = None;
        self.open(&params)
    }
}

impl Source for CameraSource {
    fn initialize(
        &mut self,
        ready: &Ready,
        reporter: Reporter,
    ) -> Result<InitializeResult, RpcError> {
        self.adopt(ready);
        reporter.info(format!(
            "camera '{}' on a {}x{}@{} canvas, transport {}",
            ready.instance,
            ready.canvas.width,
            ready.canvas.height,
            ready.canvas.fps,
            ready.transport.as_str()
        ));
        self.reporter = Some(reporter);
        // The device's own latency is not declared: `latency-report` is not in
        // the manifest's capabilities, so the aligner absorbs whatever arrives.
        Ok(InitializeResult { latency_ms: None })
    }

    fn start(&mut self, params: &StartParams) -> Result<StartResult, RpcError> {
        self.open(params)?;
        Ok(StartResult { latency_ms: None })
    }

    fn stop(&mut self) -> Result<(), RpcError> {
        if let Some(capture) = self.capture.as_ref() {
            // A camera holds no buffered picture worth saving, but the device
            // is handed back faster when the pipeline is drained first.
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
            // The core covers the gap with a freeze frame. Saying `applied` is
            // honest: the process is the same one and the setting is live by
            // the time this answer is read.
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

    fn ready() -> Ready {
        serde_json::from_value(json!({
            "core": "godwinmix", "version": "0.2.0", "api_level": 1, "api_compatible": 1,
            "canvas": {"width": 320, "height": 180, "fps": 30},
            "transport": "container", "media": "", "instance": "cam1",
            "provide": "source", "params": {"element": "videotestsrc", "label": "Camera 1"}
        }))
        .expect("the handshake answer parses")
    }

    #[test]
    fn a_source_that_never_started_is_healthy_and_stops_cleanly() {
        let mut source = CameraSource::new();
        assert_eq!(source.health().state, HealthState::Ok);
        source.stop().expect("stopping a stopped camera is fine");
    }

    #[test]
    fn the_label_reaches_the_health_message() {
        let mut source = CameraSource::new();
        source.settings = Settings::from(&json!({"label": "Stage wide"}));
        assert!(source.what().contains("Stage wide"));
        source.settings = Settings::from(&json!({}));
        assert_eq!(source.what(), "the camera");
    }

    #[test]
    fn configure_before_start_is_applied_and_opens_nothing() {
        let mut source = CameraSource::new();
        let answer = source
            .configure(json!({"device": "/dev/video9", "label": "Camera 2"}))
            .expect("configure never fails on a valid object");
        assert!(answer.applied);
        assert_eq!(source.settings.device, "/dev/video9");
        assert!(source.capture.is_none());
    }

    #[test]
    fn the_handshake_answer_sets_the_canvas_and_the_params() {
        let mut source = CameraSource::new();
        source.adopt(&ready());
        assert_eq!(source.canvas.width, 320);
        assert_eq!(source.canvas.fps, 30);
        assert_eq!(source.settings.element, "videotestsrc");
        assert_eq!(source.settings.label, "Camera 1");
    }

    #[test]
    fn an_unknown_method_is_refused_rather_than_ignored() {
        let mut source = CameraSource::new();
        let err = source
            .call("teleport", json!({}))
            .expect_err("no such method");
        assert_eq!(err.code, codes::METHOD_NOT_FOUND);
    }
}
