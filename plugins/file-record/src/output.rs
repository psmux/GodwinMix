//! The `file-record/output` provide: the programme on its way to a file.

use std::path::PathBuf;
use std::time::{Duration, SystemTime};

use godwinmix_capture_common::{
    fifo::{Fifo, Pump},
    space, Capture,
};
use gstreamer::prelude::*;

use godwinmix_sdk::prelude::*;
use serde_json::Value;

use crate::naming::{self, Stamp};
use crate::pipeline;
use crate::settings::Settings;
use crate::tools;

/// How long `start` waits for the first bytes of programme to arrive.
///
/// Short on purpose. The core opens its end of the FIFO after this returns, so
/// waiting for data here would wait forever; this is only long enough to catch
/// a pipeline that will not start at all.
const SETTLE: Duration = Duration::from_millis(200);

pub struct FileRecorder {
    settings: Settings,
    instance: String,
    reporter: Option<Reporter>,
    /// The programme FIFO, opened at `initialize` so the core's own open does
    /// not block. See `godwinmix_capture_common::fifo`.
    fifo: Option<Fifo>,
    /// Where the FIFO is, so a second `start` can open it again.
    address: String,
    /// The thread carrying the programme from the FIFO into the pipeline.
    pump: Option<Pump>,
    capture: Option<Capture>,
    /// The file this recorder is writing, for `health` and `list_recordings`.
    current: Option<String>,
    folder: PathBuf,
    started_at: SystemTime,
}

impl FileRecorder {
    pub fn new() -> FileRecorder {
        FileRecorder {
            settings: Settings::default(),
            instance: "recording".into(),
            reporter: None,
            fifo: None,
            address: String::new(),
            pump: None,
            capture: None,
            current: None,
            folder: PathBuf::new(),
            started_at: SystemTime::now(),
        }
    }

    fn adopt(&mut self, ready: &Ready) {
        self.settings = Settings::from(&ready.params);
        self.instance = ready.instance.clone();
    }

    fn what(&self) -> String {
        if self.settings.label.is_empty() {
            "the recording".into()
        } else {
            format!("the recording '{}'", self.settings.label)
        }
    }

    /// Open the FIFO the core made, without waiting for the core to open its
    /// end. Called from `initialize`, which is the earliest the path is known.
    fn open_fifo(&mut self, address: &str) -> Result<(), RpcError> {
        if !address.is_empty() {
            self.address = address.to_string();
        }
        if self.fifo.is_some() || address.is_empty() {
            return Ok(());
        }
        let fifo = godwinmix_capture_common::fifo::open_read(std::path::Path::new(address))
            .map_err(|e| RpcError::new(codes::INTERNAL_ERROR, e))?;
        self.fifo = Some(fifo);
        Ok(())
    }

    fn open(&mut self, params: &StartParams) -> Result<(), RpcError> {
        self.open_fifo(&params.media)?;
        let fifo = self.fifo.take().ok_or_else(|| {
            RpcError::new(
                codes::INTERNAL_ERROR,
                "the core sent no media address, so there is no programme to record. This \
                 plugin is an output and receives the programme on a FIFO; see \
                 docs/reference/plugin-lifecycle.md.",
            )
        })?;

        let folder = self.settings.folder();
        std::fs::create_dir_all(&folder).map_err(|e| {
            RpcError::new(
                codes::INTERNAL_ERROR,
                format!(
                    "could not make the recording folder {}: {e}. Choose a folder you can \
                     write to in the `directory` setting.",
                    folder.display()
                ),
            )
        })?;
        let name = naming::location(
            &self.settings.pattern,
            &self.instance,
            self.settings.extension(),
            self.settings.split_after().is_some(),
            &Stamp::now(),
        );
        let location = folder.join(&name);
        // A pattern may put the date in a folder, and that folder has to exist
        // before the muxer opens the file in it.
        if let Some(parent) = location.parent() {
            let _ = std::fs::create_dir_all(parent);
        }
        let pipeline = pipeline::build(
            &self.settings,
            &location.to_string_lossy(),
            self.reporter.clone(),
        )
        .map_err(|e| RpcError::new(codes::INTERNAL_ERROR, e))?;
        let input = pipeline
            .by_name(pipeline::INPUT)
            .ok_or_else(|| RpcError::new(codes::INTERNAL_ERROR, "the recorder lost its input"))?;
        let capture = Capture::start(pipeline, None, self.reporter.clone())
            .map_err(|e| RpcError::new(codes::INTERNAL_ERROR, e))?;
        // The pump takes the descriptor from here and closes it when it stops.
        let pump = Pump::start(fifo, input);
        // Long enough to catch a pipeline that will not start; not long enough
        // to wait on the core, which may not have written anything yet.
        std::thread::sleep(SETTLE);
        if let Some(detail) = capture.fault() {
            return Err(RpcError::new(
                codes::INTERNAL_ERROR,
                format!("{} would not start: {detail}", self.what()),
            ));
        }
        if let Some(r) = &self.reporter {
            r.info(format!("{} is writing {}", self.what(), location.display()));
        }
        self.folder = folder;
        self.started_at = SystemTime::now();
        self.current = Some(location.to_string_lossy().into_owned());
        self.pump = Some(pump);
        self.capture = Some(capture);
        Ok(())
    }
}

impl Output for FileRecorder {
    fn initialize(
        &mut self,
        ready: &Ready,
        reporter: Reporter,
    ) -> Result<InitializeResult, RpcError> {
        self.adopt(ready);
        self.reporter = Some(reporter);
        // `GMX_MEDIA` carries the FIFO for an output, and the handshake answer
        // carries it too when the transport needed an address. Either will do;
        // what matters is opening the read end before the core opens the
        // write end, which is why this is here and not in `start`.
        let address = if ready.media.is_empty() {
            PluginEnv::from_env().media
        } else {
            ready.media.clone()
        };
        if let Err(e) = self.open_fifo(&address) {
            if let Some(r) = &self.reporter {
                r.warn(format!("{e}"));
            }
        }
        Ok(InitializeResult {
            latency_ms: Some(0),
        })
    }

    fn start(&mut self, params: &StartParams) -> Result<StartResult, RpcError> {
        self.open(params)?;
        Ok(StartResult {
            latency_ms: Some(0),
        })
    }

    fn stop(&mut self) -> Result<(), RpcError> {
        // The pump first: it stops feeding the pipeline and lets go of the
        // FIFO, so the drain below sees the end of the programme rather than
        // racing whatever the core writes next.
        if let Some(pump) = self.pump.as_mut() {
            pump.stop();
        }
        self.pump = None;
        if let Some(capture) = self.capture.as_ref() {
            // This is the part that matters for a recording. An end of stream
            // is what makes the muxer finish the file; a pipeline taken
            // straight to NULL leaves an MP4 with no index, which is the one
            // thing fragmented mode exists to avoid having to care about and
            // which Matroska would not survive at all.
            capture.drain(Duration::from_secs(3));
        }
        self.capture = None;
        if let (Some(r), Some(file)) = (&self.reporter, &self.current) {
            r.info(format!("{} finished {file}", self.what()));
        }
        Ok(())
    }

    fn configure(&mut self, params: Value) -> Result<Configure, RpcError> {
        let next = Settings::from(&params);
        let restart = self.settings.needs_restart(&next);
        let recording = self.capture.is_some();
        self.settings = next;
        if restart && recording {
            // Not applied, and not pretended. Changing the folder or the
            // format mid recording would have to cut the file, and an operator
            // who did it by accident during a service would rather be told.
            return Ok(Configure::restart_required(
                "the folder, the name, the format and the split only change between \
                 recordings. Stop this output and start it again to use them.",
            ));
        }
        Ok(Configure::applied())
    }

    fn health(&mut self) -> Health {
        let Some(capture) = self.capture.as_ref() else {
            return Health::ok();
        };
        if let Some(detail) = capture.fault() {
            return Health::failing(format!("{}: {detail}", self.what()));
        }
        let written = pipeline::bytes_on_disk(&self.folder, self.started_at);
        if let (Some(pump), true) = (self.pump.as_ref(), written == 0) {
            if pump.bytes() == 0 {
                return Health::degraded(format!(
                    "{} has had no programme from the core yet. Check something is on air: a \
                     recorder records what is going out, and nothing is.",
                    self.what()
                ));
            }
        }
        match space::free_bytes(&self.folder) {
            Some(free) if free < self.settings.min_free_bytes => Health::degraded(format!(
                "{} left on the disk holding {}, and {} has written {}. Free some room or \
                 stop this output; recording carries on either way.",
                space::human(free),
                self.folder.display(),
                self.what(),
                space::human(written)
            )),
            _ => Health {
                state: HealthState::Ok,
                detail: Some(format!("{} written", space::human(written))),
                latency_ms: Some(0),
            },
        }
    }

    fn call(&mut self, method: &str, params: Value) -> Result<Value, RpcError> {
        tools::dispatch(method, params, &self.settings, self.current.as_deref())
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use serde_json::json;

    fn ready(params: Value) -> Ready {
        serde_json::from_value(json!({
            "core": "godwinmix", "version": "0.2.0", "api_level": 1, "api_compatible": 1,
            "canvas": {"width": 1280, "height": 720, "fps": 30},
            "transport": "container", "media": "", "instance": "archive",
            "provide": "output", "params": params
        }))
        .expect("the handshake answer parses")
    }

    #[test]
    fn the_handshake_answer_sets_the_params_and_the_instance() {
        let mut out = FileRecorder::new();
        out.adopt(&ready(json!({"format": "mkv", "label": "Archive"})));
        assert_eq!(out.settings.extension(), "mkv");
        assert_eq!(out.instance, "archive");
        assert!(out.what().contains("Archive"));
    }

    #[test]
    fn a_recorder_that_never_started_is_healthy_and_stops_cleanly() {
        let mut out = FileRecorder::new();
        assert_eq!(out.health().state, HealthState::Ok);
        out.stop().expect("stopping a stopped recorder is fine");
    }

    #[test]
    fn configure_before_recording_applies_everything() {
        let mut out = FileRecorder::new();
        let answer = out
            .configure(json!({"format": "mkv", "split_after_minutes": 30}))
            .expect("configure never fails on a valid object");
        assert!(answer.applied);
        assert_eq!(out.settings.split_after_minutes, 30);
    }

    #[test]
    fn a_start_with_no_media_address_says_what_an_output_receives() {
        let mut out = FileRecorder::new();
        let params: StartParams = serde_json::from_value(json!({
            "canvas": {"width": 1280, "height": 720, "fps": 30},
            "transport": "container", "media": ""
        }))
        .unwrap();
        let err = out
            .start(&params)
            .expect_err("there is no programme to record");
        assert!(err.message.contains("FIFO"), "{}", err.message);
    }

    #[test]
    fn an_unknown_method_is_refused_rather_than_ignored() {
        let mut out = FileRecorder::new();
        let err = out.call("teleport", json!({})).expect_err("no such method");
        assert_eq!(err.code, codes::METHOD_NOT_FOUND);
    }
}
