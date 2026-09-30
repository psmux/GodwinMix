//! The `Source` and `Device` methods.

use std::time::Duration;

use godwinmix_capture_common::{capture, Capture};
use godwinmix_sdk::prelude::*;
use serde_json::{json, Value};

use crate::card;
use crate::settings::Settings;

fn invalid(e: impl std::fmt::Display) -> RpcError {
    RpcError::new(codes::INVALID_PARAMS, e.to_string())
}

#[derive(Default)]
pub struct DeckLink {
    settings: Settings,
    reporter: Option<Reporter>,
    capture: Option<Capture>,
}

impl Source for DeckLink {
    fn initialize(&mut self, ready: &Ready, reporter: Reporter) -> Result<InitializeResult, RpcError> {
        self.settings = Settings::from_params(&ready.params).map_err(invalid)?;
        godwinmix_capture_common::init().map_err(|e| RpcError::new(codes::INTERNAL_ERROR, e))?;
        if let Some(missing) = card::missing_element() {
            return Err(RpcError::new(codes::INTERNAL_ERROR, missing));
        }
        reporter.info(format!("decklink/source '{}' will capture {}", ready.instance, self.settings.describe()));
        self.reporter = Some(reporter);
        Ok(InitializeResult { latency_ms: Some(0) })
    }

    fn start(&mut self, params: &StartParams) -> Result<StartResult, RpcError> {
        self.capture = None;
        let (s, p, r) = (self.settings.clone(), params.clone(), self.reporter.clone());
        let opened = capture::open_with_retry(2, Duration::from_millis(250), Duration::from_millis(500), r.as_ref(), || {
            let pipeline = card::build(&s, p.canvas, p.transport, &p.media)?;
            Capture::start(pipeline, Some("gmx-video-queue"), r.clone())
        });
        let capture = opened.map_err(|e| RpcError::new(codes::INTERNAL_ERROR, card::explain(&e, &s)))?;
        self.capture = Some(capture);
        Ok(StartResult { latency_ms: Some(0) })
    }

    fn stop(&mut self) -> Result<(), RpcError> {
        if let Some(mut c) = self.capture.take() {
            c.stop();
        }
        Ok(())
    }

    fn configure(&mut self, params: Value) -> Result<Configure, RpcError> {
        let wanted = Settings::from_params(&params).map_err(invalid)?;
        let (running, changed) = (self.capture.is_some(), wanted != self.settings);
        self.settings = wanted;
        if !running || !changed {
            return Ok(Configure::applied());
        }
        Ok(Configure::restart_required("a card input takes a new connector or mode by being opened again"))
    }

    fn health(&mut self) -> Health {
        match &self.capture {
            Some(c) => c.health(&self.settings.describe()),
            None => Health::degraded("not started yet"),
        }
    }

    fn call(&mut self, method: &str, _params: Value) -> Result<Value, RpcError> {
        match method {
            "stats" => Ok(json!({"input": self.settings.describe(), "frames": self.capture.as_ref().map(Capture::buffers)})),
            other => Err(RpcError::new(codes::METHOD_NOT_FOUND, format!("decklink/source has no method '{other}'. It answers 'stats'."))),
        }
    }
}

/// `decklink/devices`: the inputs the driver reports.
pub struct Devices;

impl Device for Devices {
    fn initialize(&mut self, _ready: &Ready, _reporter: Reporter) -> Result<InitializeResult, RpcError> {
        godwinmix_capture_common::init().map_err(|e| RpcError::new(codes::INTERNAL_ERROR, e))?;
        Ok(InitializeResult::default())
    }

    fn configure(&mut self, _params: Value) -> Result<Configure, RpcError> {
        Ok(Configure::applied())
    }

    /// Why the list is empty, when it is.
    fn health(&mut self) -> Health {
        if let Some(missing) = card::missing_element() {
            return Health::degraded(missing);
        }
        if card::inputs().is_empty() {
            return Health::degraded(
                "no DeckLink input is visible. Install Blackmagic Desktop Video and check the card in Desktop Video Setup; \
                 the list fills itself once the driver reports a card.",
            );
        }
        Health::ok()
    }

    fn discover(&mut self, _timeout_ms: u64) -> Result<Vec<Candidate>, RpcError> {
        Ok(if card::missing_element().is_some() { Vec::new() } else { card::inputs() })
    }
}
