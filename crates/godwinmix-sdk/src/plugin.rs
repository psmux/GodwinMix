//! The traits a Rust plugin implements.
//!
//! These mirror the Rust traits the core uses for tiers 0 and 1 (03 section
//! 10). The point of the mirror is that the same code compiles into the core
//! later without being rewritten: the core never knows which tier it is talking
//! to, and neither does the plugin author.
//!
//! Every method has a default except the ones a plugin of that kind cannot do
//! without. A source that does not seek says nothing about seeking and the SDK
//! answers -32601 with a message naming the capability to declare.

use serde_json::Value;

use crate::wire::{
    codes, AudioSet, AudioState, Candidate, Configure, Discovered, Health, InitializeResult,
    Position, Ready, Render, RpcError, StartParams, StartResult, ToolResult,
};

/// What a plugin uses to talk to the core outside a request.
///
/// Cheap to clone, safe to hold in a pacing thread. Every method is one line on
/// stderr and nothing else, so calling one from a media loop costs a syscall.
#[derive(Clone)]
pub struct Reporter {
    writer: std::sync::Arc<crate::framing::Writer>,
    health: std::sync::Arc<std::sync::Mutex<Health>>,
}

impl Reporter {
    pub(crate) fn new(
        writer: std::sync::Arc<crate::framing::Writer>,
        health: std::sync::Arc<std::sync::Mutex<Health>>,
    ) -> Reporter {
        Reporter { writer, health }
    }

    /// A structured log line, which lands in the core's log tagged with this
    /// instance and in the crash report's last fifty lines.
    pub fn log(&self, level: crate::wire::LogLevel, message: impl AsRef<str>) {
        let _ = self.writer.log(level, message.as_ref());
    }

    pub fn info(&self, message: impl AsRef<str>) {
        self.log(crate::wire::LogLevel::Info, message);
    }

    pub fn warn(&self, message: impl AsRef<str>) {
        self.log(crate::wire::LogLevel::Warn, message);
    }

    pub fn error(&self, message: impl AsRef<str>) {
        self.log(crate::wire::LogLevel::Error, message);
    }

    /// Publish a health state the core can read without waiting for the plugin
    /// to finish whatever it is doing. Call it from the media thread when the
    /// picture degrades; the answer to `health` changes at once.
    pub fn set_health(&self, health: Health) {
        let mut slot = self.health.lock().unwrap_or_else(|e| e.into_inner());
        *slot = health;
    }

    /// The `health.changed` notification, for a change the core should act on
    /// rather than wait to be asked about.
    pub fn health_changed(&self, health: Health) {
        self.set_health(health.clone());
        let _ = self.writer.notify(
            "health.changed",
            serde_json::to_value(&health).unwrap_or(Value::Null),
        );
    }

    /// A `media.report` notification: what the plugin is actually producing,
    /// which is how the browser sidecar's `[browser] media` line arrives today.
    pub fn media_report(&self, report: Value) {
        let _ = self.writer.notify("media.report", report);
    }

    /// An `event/<name>` notification.
    pub fn event(&self, name: &str, params: Value) {
        let _ = self.writer.notify(&format!("event/{name}"), params);
    }

    /// A reporter that writes to this process's stderr, for a plugin's own
    /// unit tests.
    ///
    /// The runtime builds the real one and hands it to `initialize`, so a
    /// plugin author never constructs a `Reporter`. A test that calls
    /// `initialize` directly needs one anyway, and the alternative is every
    /// plugin crate inventing its own trait to stand in for this.
    pub fn for_test() -> Reporter {
        Reporter::new(
            crate::framing::Writer::stderr(),
            std::sync::Arc::new(std::sync::Mutex::new(crate::wire::Health::ok())),
        )
    }
}

/// The error for a method a plugin did not implement.
pub fn not_implemented(method: &str, capability: &str) -> RpcError {
    RpcError::new(
        codes::METHOD_NOT_FOUND,
        format!(
            "this plugin does not implement '{method}'. Implement it and add '{capability}' \
             to capabilities in gmx-plugin.toml; the core only calls it when that is declared."
        ),
    )
    .with_data(serde_json::json!({"method": method, "capability": capability, "retryable": false}))
}

/// A source: something that produces pictures, sound, or both.
pub trait Source: Send {
    /// The handshake landed. Keep what you need from `ready`: the canvas, the
    /// instance id, and the params already validated against your schema.
    fn initialize(
        &mut self,
        ready: &Ready,
        reporter: Reporter,
    ) -> Result<InitializeResult, RpcError>;

    /// Open the media transport and start producing. Return as soon as the
    /// producer is running; do not block until the first frame.
    fn start(&mut self, params: &StartParams) -> Result<StartResult, RpcError>;

    /// Stop producing and release the transport. `start` may follow.
    fn stop(&mut self) -> Result<(), RpcError>;

    /// New params, the full validated object rather than a diff.
    fn configure(&mut self, params: Value) -> Result<Configure, RpcError>;

    /// How the plugin thinks it is doing. Answer quickly; the core asks this
    /// while other calls are in flight.
    fn health(&mut self) -> Health {
        Health::ok()
    }

    /// Declare `seek` to get this.
    fn seek(&mut self, _position_ms: u64) -> Result<Position, RpcError> {
        Err(not_implemented("seek", "seek"))
    }

    /// Declare `seek` to get this.
    fn position(&mut self) -> Result<Position, RpcError> {
        Err(not_implemented("position", "seek"))
    }

    /// Gain in decibels, 0 is unity. Return the full state read back.
    fn audio_set(&mut self, _set: AudioSet) -> Result<AudioState, RpcError> {
        Err(not_implemented("audio.set", "audio-layers"))
    }

    /// Declare `keyframe-request` to get this.
    fn keyframe(&mut self) -> Result<(), RpcError> {
        Ok(())
    }

    /// Anything else: `tool.call`, or a method a later api level adds.
    fn call(&mut self, method: &str, _params: Value) -> Result<Value, RpcError> {
        Err(unknown_method(method))
    }
}

/// An output: something that takes the programme somewhere.
///
/// `start` carries the address the core is publishing the programme at. In
/// container mode that is a pipe the core opened; the plugin reads it rather
/// than writing, which is the one place the media flows the other way.
pub trait Output: Send {
    fn initialize(
        &mut self,
        ready: &Ready,
        reporter: Reporter,
    ) -> Result<InitializeResult, RpcError>;
    fn start(&mut self, params: &StartParams) -> Result<StartResult, RpcError>;
    fn stop(&mut self) -> Result<(), RpcError>;
    fn configure(&mut self, params: Value) -> Result<Configure, RpcError>;
    fn health(&mut self) -> Health {
        Health::ok()
    }
    /// A downstream client asked for a keyframe. Declare `keyframe-request`.
    fn keyframe(&mut self) -> Result<(), RpcError> {
        Ok(())
    }
    fn call(&mut self, method: &str, _params: Value) -> Result<Value, RpcError> {
        Err(unknown_method(method))
    }
}

/// A filter: frames in at canvas caps, frames out at canvas caps.
///
/// The declared latency must match what the filter actually adds within one
/// frame, because the aligner trusts it. The harness measures it.
pub trait Filter: Send {
    fn initialize(
        &mut self,
        ready: &Ready,
        reporter: Reporter,
    ) -> Result<InitializeResult, RpcError>;
    fn start(&mut self, params: &StartParams) -> Result<StartResult, RpcError>;
    fn stop(&mut self) -> Result<(), RpcError>;
    fn configure(&mut self, params: Value) -> Result<Configure, RpcError>;
    fn health(&mut self) -> Health {
        Health::ok()
    }
    fn call(&mut self, method: &str, _params: Value) -> Result<Value, RpcError> {
        Err(unknown_method(method))
    }
}

/// A service: no media, one singleton instance, usually tools and hooks.
pub trait Service: Send {
    fn initialize(
        &mut self,
        ready: &Ready,
        reporter: Reporter,
    ) -> Result<InitializeResult, RpcError>;
    fn configure(&mut self, params: Value) -> Result<Configure, RpcError>;
    fn health(&mut self) -> Health {
        Health::ok()
    }
    /// An MCP tool call, in MCP's shape.
    fn tool_call(&mut self, _name: &str, _arguments: Value) -> Result<ToolResult, RpcError> {
        Err(not_implemented("tool.call", "tools"))
    }
    fn call(&mut self, method: &str, _params: Value) -> Result<Value, RpcError> {
        Err(unknown_method(method))
    }
}

/// A device: finds things on the network or the machine and offers them as
/// candidates ready for `source.add`.
pub trait Device: Send {
    fn initialize(
        &mut self,
        ready: &Ready,
        reporter: Reporter,
    ) -> Result<InitializeResult, RpcError>;
    fn configure(&mut self, params: Value) -> Result<Configure, RpcError>;
    fn health(&mut self) -> Health {
        Health::ok()
    }
    /// Look for `timeout_ms` and answer with what was found. Answer within the
    /// timeout; the core will not wait longer.
    fn discover(&mut self, timeout_ms: u64) -> Result<Vec<Candidate>, RpcError>;
    fn call(&mut self, method: &str, _params: Value) -> Result<Value, RpcError> {
        Err(unknown_method(method))
    }
}

/// A transition: asked for pad properties at the compositor's frame rate.
pub trait Transition: Send {
    fn initialize(
        &mut self,
        ready: &Ready,
        reporter: Reporter,
    ) -> Result<InitializeResult, RpcError>;
    fn configure(&mut self, params: Value) -> Result<Configure, RpcError>;
    fn health(&mut self) -> Health {
        Health::ok()
    }
    /// Answer with `{pads: {...}}`, or once with `{curve: ...}` for the core to
    /// bind as a control source. Answer fast: this runs per frame.
    fn render(&mut self, render: &Render) -> Result<Value, RpcError>;
    fn call(&mut self, method: &str, _params: Value) -> Result<Value, RpcError> {
        Err(unknown_method(method))
    }
}

fn unknown_method(method: &str) -> RpcError {
    RpcError::new(
        codes::METHOD_NOT_FOUND,
        format!(
            "this plugin has no method '{method}'. Check the spelling against \
             docs/reference/plugin-protocol.md, or implement `call` to handle it."
        ),
    )
    .with_data(serde_json::json!({"method": method, "retryable": false}))
}

// ---------------------------------------------------------------------------
// The object safe shape the runtime drives
// ---------------------------------------------------------------------------

/// One plugin, whatever kind, as the runtime sees it.
///
/// The adapters below turn each trait into this, so the main loop is written
/// once. A plugin author never implements `Handler` directly.
pub trait Handler: Send {
    fn on_initialize(
        &mut self,
        ready: &Ready,
        reporter: Reporter,
    ) -> Result<InitializeResult, RpcError>;
    fn on_health(&mut self) -> Health;
    /// Everything else, already checked against the state machine.
    fn on_call(&mut self, method: &str, params: Value) -> Result<Value, RpcError>;
    /// Called before the process exits, however that came about.
    fn on_shutdown(&mut self, _reason: &str) {}
    /// Whether this method puts the plugin into `running` or back out of it.
    fn starts(&self, method: &str) -> bool {
        method == "start"
    }
}

fn parse<T: serde::de::DeserializeOwned>(method: &str, params: Value) -> Result<T, RpcError> {
    serde_json::from_value(params).map_err(|e| {
        RpcError::new(
            codes::INVALID_PARAMS,
            format!("the params of '{method}' did not parse: {e}"),
        )
    })
}

fn json<T: serde::Serialize>(value: T) -> Result<Value, RpcError> {
    serde_json::to_value(value).map_err(|e| {
        RpcError::new(
            codes::INTERNAL_ERROR,
            format!("could not encode the result: {e}"),
        )
    })
}

/// Wraps a [`Source`] for the runtime.
pub struct SourceHandler<S: Source>(pub S);

impl<S: Source> Handler for SourceHandler<S> {
    fn on_initialize(
        &mut self,
        ready: &Ready,
        reporter: Reporter,
    ) -> Result<InitializeResult, RpcError> {
        self.0.initialize(ready, reporter)
    }

    fn on_health(&mut self) -> Health {
        self.0.health()
    }

    fn on_call(&mut self, method: &str, params: Value) -> Result<Value, RpcError> {
        match method {
            "start" => json(self.0.start(&parse::<StartParams>(method, params)?)?),
            "stop" => {
                self.0.stop()?;
                Ok(serde_json::json!({}))
            }
            "configure" => json(self.0.configure(config_params(params))?),
            "seek" => json(
                self.0
                    .seek(parse::<crate::wire::Seek>(method, params)?.position_ms)?,
            ),
            "position" => json(self.0.position()?),
            "audio.set" => json(self.0.audio_set(parse::<AudioSet>(method, params)?)?),
            "keyframe" => {
                self.0.keyframe()?;
                Ok(serde_json::json!({}))
            }
            other => self.0.call(other, params),
        }
    }

    fn on_shutdown(&mut self, _reason: &str) {
        let _ = self.0.stop();
    }
}

/// Wraps an [`Output`] for the runtime.
pub struct OutputHandler<O: Output>(pub O);

impl<O: Output> Handler for OutputHandler<O> {
    fn on_initialize(
        &mut self,
        ready: &Ready,
        reporter: Reporter,
    ) -> Result<InitializeResult, RpcError> {
        self.0.initialize(ready, reporter)
    }

    fn on_health(&mut self) -> Health {
        self.0.health()
    }

    fn on_call(&mut self, method: &str, params: Value) -> Result<Value, RpcError> {
        match method {
            "start" => json(self.0.start(&parse::<StartParams>(method, params)?)?),
            "stop" => {
                self.0.stop()?;
                Ok(serde_json::json!({}))
            }
            "configure" => json(self.0.configure(config_params(params))?),
            "keyframe" => {
                self.0.keyframe()?;
                Ok(serde_json::json!({}))
            }
            other => self.0.call(other, params),
        }
    }

    fn on_shutdown(&mut self, _reason: &str) {
        let _ = self.0.stop();
    }
}

/// Wraps a [`Filter`] for the runtime.
pub struct FilterHandler<F: Filter>(pub F);

impl<F: Filter> Handler for FilterHandler<F> {
    fn on_initialize(
        &mut self,
        ready: &Ready,
        reporter: Reporter,
    ) -> Result<InitializeResult, RpcError> {
        self.0.initialize(ready, reporter)
    }

    fn on_health(&mut self) -> Health {
        self.0.health()
    }

    fn on_call(&mut self, method: &str, params: Value) -> Result<Value, RpcError> {
        match method {
            "start" => json(self.0.start(&parse::<StartParams>(method, params)?)?),
            "stop" => {
                self.0.stop()?;
                Ok(serde_json::json!({}))
            }
            "configure" => json(self.0.configure(config_params(params))?),
            other => self.0.call(other, params),
        }
    }

    fn on_shutdown(&mut self, _reason: &str) {
        let _ = self.0.stop();
    }
}

/// Wraps a [`Service`] for the runtime.
pub struct ServiceHandler<S: Service>(pub S);

impl<S: Service> Handler for ServiceHandler<S> {
    fn on_initialize(
        &mut self,
        ready: &Ready,
        reporter: Reporter,
    ) -> Result<InitializeResult, RpcError> {
        self.0.initialize(ready, reporter)
    }

    fn on_health(&mut self) -> Health {
        self.0.health()
    }

    fn on_call(&mut self, method: &str, params: Value) -> Result<Value, RpcError> {
        match method {
            "configure" => json(self.0.configure(config_params(params))?),
            "tool.call" => {
                let call = parse::<crate::wire::ToolCall>(method, params)?;
                json(self.0.tool_call(&call.name, call.arguments)?)
            }
            other => self.0.call(other, params),
        }
    }

    /// A service never runs media, so it stays in `ready` for its whole life.
    fn starts(&self, _method: &str) -> bool {
        false
    }
}

/// Wraps a [`Device`] for the runtime.
pub struct DeviceHandler<D: Device>(pub D);

impl<D: Device> Handler for DeviceHandler<D> {
    fn on_initialize(
        &mut self,
        ready: &Ready,
        reporter: Reporter,
    ) -> Result<InitializeResult, RpcError> {
        self.0.initialize(ready, reporter)
    }

    fn on_health(&mut self) -> Health {
        self.0.health()
    }

    fn on_call(&mut self, method: &str, params: Value) -> Result<Value, RpcError> {
        match method {
            "configure" => json(self.0.configure(config_params(params))?),
            "discover" => {
                let d = parse::<crate::wire::Discover>(method, params)?;
                json(Discovered {
                    candidates: self.0.discover(d.timeout_ms)?,
                })
            }
            other => self.0.call(other, params),
        }
    }

    fn starts(&self, _method: &str) -> bool {
        false
    }
}

/// Wraps a [`Transition`] for the runtime.
pub struct TransitionHandler<T: Transition>(pub T);

impl<T: Transition> Handler for TransitionHandler<T> {
    fn on_initialize(
        &mut self,
        ready: &Ready,
        reporter: Reporter,
    ) -> Result<InitializeResult, RpcError> {
        self.0.initialize(ready, reporter)
    }

    fn on_health(&mut self) -> Health {
        self.0.health()
    }

    fn on_call(&mut self, method: &str, params: Value) -> Result<Value, RpcError> {
        match method {
            "configure" => json(self.0.configure(config_params(params))?),
            "render" => self.0.render(&parse::<Render>(method, params)?),
            other => self.0.call(other, params),
        }
    }

    fn starts(&self, _method: &str) -> bool {
        false
    }
}

/// `configure` carries `{params: {...}}`. A core that sends the object bare is
/// accepted too, because an offline transcript written by hand often does.
fn config_params(params: Value) -> Value {
    match params {
        Value::Object(mut map) => match map.remove("params") {
            Some(inner) => inner,
            None => Value::Object(map),
        },
        other => other,
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn configure_accepts_both_shapes() {
        let wrapped = serde_json::json!({"params": {"timezone": "UTC"}});
        assert_eq!(config_params(wrapped)["timezone"], "UTC");
        let bare = serde_json::json!({"timezone": "UTC"});
        assert_eq!(config_params(bare)["timezone"], "UTC");
    }

    #[test]
    fn an_unimplemented_method_names_the_capability_to_declare() {
        let err = not_implemented("seek", "seek");
        assert_eq!(err.code, codes::METHOD_NOT_FOUND);
        assert!(err.message.contains("gmx-plugin.toml"), "{}", err.message);
    }

    struct Nothing;

    impl Source for Nothing {
        fn initialize(&mut self, _: &Ready, _: Reporter) -> Result<InitializeResult, RpcError> {
            Ok(InitializeResult::default())
        }
        fn start(&mut self, _: &StartParams) -> Result<StartResult, RpcError> {
            Ok(StartResult::default())
        }
        fn stop(&mut self) -> Result<(), RpcError> {
            Ok(())
        }
        fn configure(&mut self, _: Value) -> Result<Configure, RpcError> {
            Ok(Configure::applied())
        }
    }

    #[test]
    fn the_defaults_refuse_politely() {
        let mut s = Nothing;
        assert!(s.seek(0).is_err());
        assert!(s.position().is_err());
        assert!(s.audio_set(AudioSet::default()).is_err());
        assert!(s.keyframe().is_ok());
        let err = s.call("teleport", Value::Null).unwrap_err();
        assert!(err.message.contains("teleport"), "{}", err.message);
    }

    #[test]
    fn the_source_handler_routes_every_documented_method() {
        let mut h = SourceHandler(Nothing);
        assert!(h
            .on_call(
                "start",
                serde_json::json!({"canvas": {"width": 64, "height": 64, "fps": 30},
                                   "transport": "container", "media": ""})
            )
            .is_ok());
        assert!(h.on_call("stop", serde_json::json!({})).is_ok());
        assert_eq!(
            h.on_call("configure", serde_json::json!({"params": {}}))
                .unwrap()["applied"],
            true
        );
        assert!(h
            .on_call("seek", serde_json::json!({"position_ms": 0}))
            .is_err());
        assert!(h.on_call("keyframe", serde_json::json!({})).is_ok());
        assert_eq!(h.on_health().state, crate::wire::HealthState::Ok);
        assert!(h.starts("start"));
    }

    #[test]
    fn bad_params_are_minus_32602_and_name_the_method() {
        let mut h = SourceHandler(Nothing);
        let err = h
            .on_call("start", serde_json::json!({"canvas": "wrong"}))
            .unwrap_err();
        assert_eq!(err.code, codes::INVALID_PARAMS);
        assert!(err.message.contains("start"), "{}", err.message);
    }
}
