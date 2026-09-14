//! The handshake and the legal call order, as a state machine.
//!
//! The plugin sends first. The core answers. Then the plugin says `initialized`
//! and the channel is open. From there the table in 03 section 6 decides which
//! calls are legal in which state; the SDK answers -32001 for the rest rather
//! than letting a plugin author discover the rule from a crash.

use crate::wire::{codes, Initialize, Ready, RpcError, Transport};

/// The one state enum, shared with `event/plugin.state` and `plugin.list`.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub enum State {
    Starting,
    Ready,
    Running,
    Stalled,
    Degraded,
    Stopped,
    Failed,
}

impl State {
    pub fn as_str(&self) -> &'static str {
        match self {
            State::Starting => "starting",
            State::Ready => "ready",
            State::Running => "running",
            State::Stalled => "stalled",
            State::Degraded => "degraded",
            State::Stopped => "stopped",
            State::Failed => "failed",
        }
    }

    /// Every method this protocol level defines for a plugin.
    ///
    /// A method outside this list is not a state error, it is a method the
    /// plugin has never heard of, and it is answered -32601 by the handler.
    /// Keeping the two apart matters: -32001 tells a caller to wait and try
    /// again, and waiting for a method that does not exist never ends.
    pub fn is_known(method: &str) -> bool {
        matches!(
            method,
            "initialize"
                | "initialized"
                | "shutdown"
                | "configure"
                | "health"
                | "start"
                | "stop"
                | "seek"
                | "position"
                | "keyframe"
                | "audio.set"
                | "render"
                | "discover"
                | "tool.call"
        )
    }

    /// Which methods the core may call in this state.
    ///
    /// `stalled` and `degraded` are the same as `running`: a plugin in trouble
    /// still answers everything, which is how the supervisor gets it back.
    pub fn allows(&self, method: &str) -> bool {
        // `shutdown` is legal from every state and leads to process exit.
        if method == "shutdown" {
            return *self != State::Failed;
        }
        match self {
            State::Starting => false,
            State::Ready | State::Stopped => matches!(
                method,
                "configure" | "start" | "health" | "discover" | "tool.call"
            ),
            State::Running | State::Stalled | State::Degraded => matches!(
                method,
                "configure"
                    | "stop"
                    | "health"
                    | "seek"
                    | "position"
                    | "keyframe"
                    | "audio.set"
                    | "render"
                    | "tool.call"
                    | "discover"
            ),
            State::Failed => false,
        }
    }

    /// The methods legal here, for an error message that names the next step.
    pub fn legal_methods(&self) -> &'static [&'static str] {
        match self {
            State::Starting => &[],
            State::Ready | State::Stopped => &[
                "configure",
                "start",
                "health",
                "discover",
                "tool.call",
                "shutdown",
            ],
            State::Running | State::Stalled | State::Degraded => &[
                "configure",
                "stop",
                "health",
                "seek",
                "position",
                "keyframe",
                "audio.set",
                "render",
                "tool.call",
                "shutdown",
            ],
            State::Failed => &[],
        }
    }

    /// The error for a call that arrived in the wrong state. It names the state
    /// and what to do next, because that is what an unattended caller needs.
    pub fn refuse(&self, method: &str) -> RpcError {
        let legal = self.legal_methods();
        let next = match self {
            State::Starting => "Wait for the initialize handshake to finish.",
            State::Ready | State::Stopped => "Call start first.",
            State::Running | State::Stalled | State::Degraded => "Call stop first.",
            State::Failed => "The supervisor decides; nothing is read from this process.",
        };
        RpcError::new(
            codes::WRONG_STATE,
            format!(
                "'{method}' is not legal in state '{}'. Legal now: {}. {next}",
                self.as_str(),
                if legal.is_empty() {
                    "nothing".to_string()
                } else {
                    legal.join(", ")
                }
            ),
        )
        .with_data(serde_json::json!({
            "state": self.as_str(),
            "method": method,
            "legal": legal,
            "retryable": matches!(self, State::Starting | State::Ready | State::Stopped | State::Running | State::Stalled | State::Degraded),
        }))
    }
}

/// Tracks the state a plugin is in, and the transitions the SDK drives.
#[derive(Debug, Clone)]
pub struct Machine {
    state: State,
}

impl Default for Machine {
    fn default() -> Self {
        Machine::new()
    }
}

impl Machine {
    pub fn new() -> Self {
        Machine {
            state: State::Starting,
        }
    }

    pub fn state(&self) -> State {
        self.state
    }

    /// The core answered `initialize`.
    pub fn initialized(&mut self) {
        if self.state == State::Starting {
            self.state = State::Ready;
        }
    }

    /// `start` returned.
    pub fn started(&mut self) {
        self.state = State::Running;
    }

    /// `stop` returned.
    pub fn stopped(&mut self) {
        self.state = State::Stopped;
    }

    /// Health moved while running. `ok` puts a degraded plugin back.
    pub fn degraded(&mut self, degraded: bool) {
        match (self.state, degraded) {
            (State::Running, true) => self.state = State::Degraded,
            (State::Degraded, false) => self.state = State::Running,
            _ => {}
        }
    }

    pub fn failed(&mut self) {
        self.state = State::Failed;
    }

    /// Check a call against the table. `Ok(())` means dispatch it.
    ///
    /// A method this protocol level does not define is dispatched whatever the
    /// state, so the handler answers -32601 rather than the state machine
    /// answering -32001 and inviting a retry that can never work.
    pub fn check(&self, method: &str) -> Result<(), RpcError> {
        if !State::is_known(method) || self.state.allows(method) {
            Ok(())
        } else {
            Err(self.state.refuse(method))
        }
    }
}

/// Build the `initialize` request the plugin sends first.
///
/// `provides` is the manifest's `[[provides]]` array, serialised verbatim, so
/// the core never has to read the file twice.
pub fn initialize_params(
    plugin: &str,
    version: &str,
    api: u32,
    transports: Vec<Transport>,
    provides: Vec<serde_json::Value>,
) -> Initialize {
    Initialize {
        plugin: plugin.to_string(),
        version: version.to_string(),
        api,
        transports,
        provides,
    }
}

/// Check the core's answer before trusting it.
///
/// The core kills a plugin whose `api` it cannot serve. The plugin checks the
/// other direction: a core older than the plugin expects is worth a clear
/// message rather than a confusing failure three calls later.
pub fn check_ready(ready: &Ready, our_api: u32) -> Result<(), RpcError> {
    if our_api < ready.api_compatible || our_api > ready.api_level {
        return Err(RpcError::new(
            codes::WRONG_STATE,
            format!(
                "this plugin is written against api {our_api}, and core {} serves {} to {}. \
                 Upgrade the core, or publish a build at an api it serves.",
                ready.version, ready.api_compatible, ready.api_level
            ),
        )
        .with_data(serde_json::json!({
            "api": our_api,
            "api_level": ready.api_level,
            "api_compatible": ready.api_compatible,
            "retryable": false,
        })));
    }
    if ready.canvas.width == 0 || ready.canvas.height == 0 || ready.canvas.fps == 0 {
        return Err(RpcError::new(
            codes::INVALID_PARAMS,
            format!(
                "the core sent a canvas of {}x{}@{}, which no source can fill. \
                 Check [canvas] in the core's config.",
                ready.canvas.width, ready.canvas.height, ready.canvas.fps
            ),
        ));
    }
    if ready.transport != Transport::Container && ready.media.is_empty() {
        return Err(RpcError::new(
            codes::INVALID_PARAMS,
            format!(
                "the core chose the '{}' transport but sent no media address. \
                 Ask for 'container' in the manifest's transports, which needs no address.",
                ready.transport.as_str()
            ),
        ));
    }
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::wire::Canvas;

    #[test]
    fn nothing_is_legal_before_the_handshake() {
        let m = Machine::new();
        assert_eq!(m.state(), State::Starting);
        for method in ["start", "health", "configure", "stop"] {
            assert!(m.check(method).is_err(), "{method} must be refused");
        }
    }

    #[test]
    fn ready_takes_configure_start_and_health_but_not_stop() {
        let mut m = Machine::new();
        m.initialized();
        assert_eq!(m.state(), State::Ready);
        for method in ["configure", "start", "health", "discover", "tool.call"] {
            assert!(m.check(method).is_ok(), "{method} must be legal when ready");
        }
        for method in ["stop", "seek", "position", "keyframe", "audio.set"] {
            assert!(
                m.check(method).is_err(),
                "{method} must be refused when ready"
            );
        }
    }

    #[test]
    fn running_takes_the_media_methods() {
        let mut m = Machine::new();
        m.initialized();
        m.started();
        for method in [
            "configure",
            "stop",
            "health",
            "seek",
            "position",
            "keyframe",
            "audio.set",
            "render",
        ] {
            assert!(
                m.check(method).is_ok(),
                "{method} must be legal when running"
            );
        }
        assert!(m.check("start").is_err(), "start twice is refused");
    }

    #[test]
    fn degraded_and_stalled_answer_everything_running_does() {
        for state in [State::Degraded, State::Stalled] {
            for method in ["configure", "stop", "health", "seek", "shutdown"] {
                assert!(state.allows(method), "{state:?} must allow {method}");
            }
        }
    }

    #[test]
    fn stopped_goes_back_to_the_ready_rules() {
        let mut m = Machine::new();
        m.initialized();
        m.started();
        m.stopped();
        assert_eq!(m.state(), State::Stopped);
        assert!(m.check("start").is_ok());
        assert!(m.check("stop").is_err());
    }

    #[test]
    fn shutdown_is_legal_from_every_live_state() {
        for state in [
            State::Ready,
            State::Running,
            State::Stalled,
            State::Degraded,
            State::Stopped,
        ] {
            assert!(state.allows("shutdown"), "{state:?}");
        }
        assert!(!State::Failed.allows("shutdown"));
    }

    #[test]
    fn health_moves_running_to_degraded_and_back() {
        let mut m = Machine::new();
        m.initialized();
        m.started();
        m.degraded(true);
        assert_eq!(m.state(), State::Degraded);
        m.degraded(false);
        assert_eq!(m.state(), State::Running);
        // A ready plugin is not moved by a health report.
        let mut r = Machine::new();
        r.initialized();
        r.degraded(true);
        assert_eq!(r.state(), State::Ready);
    }

    #[test]
    fn a_method_this_level_does_not_define_reaches_the_handler() {
        // The handler answers -32601. The state machine must not turn it into
        // -32001, which would tell the caller to wait for a method that will
        // never exist.
        let mut m = Machine::new();
        m.initialized();
        m.started();
        assert!(m.check("teleport").is_ok());
        assert!(!State::is_known("teleport"));
        assert!(State::is_known("audio.set"));
    }

    #[test]
    fn a_refusal_names_the_state_and_the_next_step() {
        let m = Machine::new();
        let err = m.check("start").unwrap_err();
        assert_eq!(err.code, codes::WRONG_STATE);
        assert!(err.message.contains("starting"), "{}", err.message);
        assert!(err.message.contains("Wait for"), "{}", err.message);
        assert!(err.data.is_some());
    }

    #[test]
    fn an_api_outside_the_range_is_refused_with_the_numbers() {
        let mut ready = Ready::for_test(Canvas::new(1280, 720, 30));
        ready.api_level = 1;
        ready.api_compatible = 1;
        assert!(check_ready(&ready, 1).is_ok());
        let err = check_ready(&ready, 2).unwrap_err();
        assert!(err.message.contains("api 2"), "{}", err.message);
    }

    #[test]
    fn a_transport_with_no_address_is_refused() {
        let mut ready = Ready::for_test(Canvas::new(1280, 720, 30));
        ready.transport = Transport::Shm;
        ready.media = String::new();
        let err = check_ready(&ready, 1).unwrap_err();
        assert!(err.message.contains("no media address"), "{}", err.message);
    }

    #[test]
    fn a_zero_canvas_is_refused() {
        let mut ready = Ready::for_test(Canvas::new(1280, 720, 0));
        ready.transport = Transport::Container;
        assert!(check_ready(&ready, 1).is_err());
    }
}
