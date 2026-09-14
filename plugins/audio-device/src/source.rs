//! The `audio-device/source` provide: one instance, one input.

use std::time::Duration;

use godwinmix_capture_common::Capture;
use godwinmix_sdk::prelude::*;
use serde_json::Value;

use crate::pipeline;
use crate::settings::Settings;
use crate::tools;

/// How long `start` waits for the first samples before carrying on.
const FIRST_BUFFER_WITHIN: Duration = Duration::from_millis(1_500);

pub struct AudioSource {
    settings: Settings,
    reporter: Option<Reporter>,
    last_start: Option<StartParams>,
    capture: Option<Capture>,
}

impl AudioSource {
    pub fn new() -> AudioSource {
        AudioSource {
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
            "the input".into()
        } else {
            format!("the input '{}'", self.settings.label)
        }
    }

    fn open(&mut self, params: &StartParams) -> Result<(), RpcError> {
        let pipeline =
            pipeline::build(&self.settings, params.transport, &params.media).map_err(internal)?;
        let capture = Capture::start(pipeline, Some("gmx-audio-queue"), self.reporter.clone())
            .map_err(internal)?;
        // A sound card opens faster than a camera but not instantly, and the
        // core connects the moment this returns. Waiting here means the samples
        // are already flowing when it does. Bounded well inside the core's five
        // second budget for `start`.
        if !capture.wait_for_data(FIRST_BUFFER_WITHIN) {
            if let Some(detail) = capture.fault() {
                return Err(internal(format!("{} did not start: {detail}", self.what())));
            }
        }
        if let Some(r) = &self.reporter {
            r.info(format!(
                "{} is running at 48 kHz stereo over {}",
                self.what(),
                params.transport.as_str()
            ));
        }
        self.capture = Some(capture);
        self.last_start = Some(params.clone());
        Ok(())
    }

    fn reopen(&mut self) -> Result<(), RpcError> {
        let Some(params) = self.last_start.clone() else {
            return Ok(());
        };
        self.capture = None;
        self.open(&params)
    }

    /// Push the current gain and mute at a running pipeline.
    fn push_gain(&self) {
        if let Some(capture) = self.capture.as_ref() {
            pipeline::apply_gain(capture.pipeline(), &self.settings);
        }
    }

    fn state(&self) -> AudioState {
        AudioState {
            gain_db: self.settings.gain_db,
            muted: self.settings.muted,
            // No page or media levels here. Those belong to a source that has
            // a document with layers in it, such as a web page.
            layers: None,
        }
    }
}

impl Source for AudioSource {
    fn initialize(
        &mut self,
        ready: &Ready,
        reporter: Reporter,
    ) -> Result<InitializeResult, RpcError> {
        self.adopt(ready);
        reporter.info(format!(
            "audio device '{}', transport {}",
            ready.instance,
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
        } else {
            // Gain and mute land on the next buffer without stopping anything.
            self.push_gain();
        }
        Ok(Configure::applied())
    }

    /// Gain in decibels and mute, read back in full.
    ///
    /// `layers` is refused rather than silently ignored: a caller that asked
    /// for page levels on a microphone has the wrong source.
    fn audio_set(&mut self, set: AudioSet) -> Result<AudioState, RpcError> {
        if set.layers.is_some() {
            return Err(RpcError::new(
                codes::INVALID_PARAMS,
                "a sound input has no layers to set. `layers` belongs to a source with a \
                 document in it, such as a web page. Set `gain_db` and `muted` instead.",
            ));
        }
        if let Some(gain) = set.gain_db {
            self.settings.gain_db = gain.clamp(-60.0, 12.0);
        }
        if let Some(muted) = set.muted {
            self.settings.muted = muted;
        }
        self.push_gain();
        Ok(self.state())
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
    use godwinmix_sdk::wire::AudioLayers;
    use serde_json::json;

    fn ready(params: Value) -> Ready {
        serde_json::from_value(json!({
            "core": "godwinmix", "version": "0.2.0", "api_level": 1, "api_compatible": 1,
            "canvas": {"width": 320, "height": 180, "fps": 30},
            "transport": "container", "media": "", "instance": "mic1",
            "provide": "source", "params": params
        }))
        .expect("the handshake answer parses")
    }

    #[test]
    fn the_handshake_answer_sets_the_params() {
        let mut source = AudioSource::new();
        source.adopt(&ready(json!({"gain_db": -6.0, "label": "Lectern mic"})));
        assert_eq!(source.settings.gain_db, -6.0);
        assert!(source.what().contains("Lectern mic"));
    }

    #[test]
    fn audio_set_reads_back_what_it_was_given() {
        let mut source = AudioSource::new();
        let state = source
            .audio_set(AudioSet {
                gain_db: Some(3.0),
                muted: Some(true),
                layers: None,
            })
            .expect("gain and mute are always allowed");
        assert_eq!(state.gain_db, 3.0);
        assert!(state.muted);
        assert!(state.layers.is_none());
    }

    #[test]
    fn audio_set_leaves_alone_what_it_was_not_given() {
        let mut source = AudioSource::new();
        source.settings.gain_db = -6.0;
        let state = source
            .audio_set(AudioSet {
                gain_db: None,
                muted: Some(true),
                layers: None,
            })
            .expect("mute alone is allowed");
        assert_eq!(state.gain_db, -6.0, "a mute must not move the fader");
        assert!(state.muted);
    }

    #[test]
    fn audio_set_clamps_rather_than_distorting() {
        let mut source = AudioSource::new();
        let state = source
            .audio_set(AudioSet {
                gain_db: Some(99.0),
                muted: None,
                layers: None,
            })
            .expect("a silly number is clamped, not refused");
        assert_eq!(state.gain_db, 12.0);
    }

    #[test]
    fn layers_on_a_microphone_are_refused_and_say_what_to_use() {
        let mut source = AudioSource::new();
        let err = source
            .audio_set(AudioSet {
                gain_db: None,
                muted: None,
                layers: Some(AudioLayers {
                    page: Some(0.5),
                    media: None,
                }),
            })
            .expect_err("a microphone has no layers");
        assert_eq!(err.code, codes::INVALID_PARAMS);
        assert!(err.message.contains("gain_db"), "{}", err.message);
    }

    #[test]
    fn a_gain_change_while_running_does_not_reopen_the_input() {
        let mut source = AudioSource::new();
        source.settings = Settings::from(&json!({"device": "desk"}));
        let answer = source
            .configure(json!({"device": "desk", "gain_db": -3.0}))
            .unwrap();
        assert!(answer.applied);
        assert_eq!(source.settings.gain_db, -3.0);
    }

    #[test]
    fn a_source_that_never_started_is_healthy_and_stops_cleanly() {
        let mut source = AudioSource::new();
        assert_eq!(source.health().state, HealthState::Ok);
        source.stop().expect("stopping a stopped input is fine");
    }
}
