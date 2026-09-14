//! JSON lines over a pipe: the 4 MiB limit, and ids in flight per direction.
//!
//! The core writes requests on the plugin's stdin and reads its stderr. Both
//! directions may have several requests outstanding at once, so a reply is
//! matched by id within its own direction: the core's ids and the plugin's ids
//! are separate spaces and may collide without ambiguity.
//!
//! Nothing here reads a file descriptor. The caller owns the child and its
//! pipes, because the core already has the process group teardown and the
//! Windows stdout reader that a sidecar needs. This is the part that decides
//! what a line means and who was waiting for it.

use godwinmix_protocol::error::ErrorCode;
use godwinmix_protocol::plugin::wire::{self, Frame, WireError, MAX_LINE_BYTES};
use serde_json::Value;
use std::collections::HashMap;
use std::io::Write;
use std::sync::mpsc;
use std::sync::Mutex;

/// What can go wrong reading one line.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum LineError {
    /// Over 4 MiB. Error -32011, the channel closes, the instance fails.
    TooLong { bytes: usize },
}

impl std::fmt::Display for LineError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            LineError::TooLong { bytes } => write!(
                f,
                "a protocol line was {bytes} bytes; the limit is {MAX_LINE_BYTES} \
                 ({} MiB). Send large data over a media transport or a file, not on the \
                 control channel.",
                MAX_LINE_BYTES / (1024 * 1024)
            ),
        }
    }
}

impl LineError {
    pub const fn code(&self) -> ErrorCode {
        ErrorCode::LineTooLong
    }
}

/// Read one line and say what it is, or refuse it for length.
pub fn read_line(line: &str) -> Result<Frame, LineError> {
    if line.len() > MAX_LINE_BYTES {
        return Err(LineError::TooLong { bytes: line.len() });
    }
    Ok(wire::parse_line(line))
}

/// The requests the core has sent and not yet had answered.
///
/// One per instance, per direction. A plugin that dies with calls outstanding
/// has every one of them answered `-32010 plugin died` rather than left to a
/// client's own timeout, which is the difference between an agent retrying and
/// an agent hanging.
#[derive(Default)]
pub struct Pending {
    next: i64,
    waiting: HashMap<i64, mpsc::Sender<Result<Value, WireError>>>,
}

impl Pending {
    pub fn new() -> Self {
        Self::default()
    }

    /// Claim an id and a place to put the answer.
    pub fn claim(&mut self) -> (i64, mpsc::Receiver<Result<Value, WireError>>) {
        let id = self.next;
        self.next += 1;
        let (tx, rx) = mpsc::channel();
        self.waiting.insert(id, tx);
        (id, rx)
    }

    /// Hand an answer to whoever was waiting for it. `false` when nobody was,
    /// which is a plugin answering an id it invented.
    pub fn settle(&mut self, id: &Value, result: Result<Value, WireError>) -> bool {
        let Some(id) = id.as_i64() else { return false };
        match self.waiting.remove(&id) {
            Some(tx) => tx.send(result).is_ok(),
            None => false,
        }
    }

    /// Fail everything outstanding. Called when the process exits.
    pub fn abandon(&mut self, why: &str) {
        let error = WireError::new(ErrorCode::PluginDied, why.to_string());
        for (_, tx) in self.waiting.drain() {
            let _ = tx.send(Err(error.clone()));
        }
    }

    pub fn len(&self) -> usize {
        self.waiting.len()
    }

    pub fn is_empty(&self) -> bool {
        self.waiting.is_empty()
    }
}

/// The writing half: the plugin's stdin, and the ids in flight behind it.
///
/// `Channel` is what a `SidecarSource` holds. It is `Send` and its lock is
/// never held across a blocking read, so the health poll, a `configure` and a
/// `tool.call` can all be in flight together.
pub struct Channel {
    stdin: Mutex<Option<Box<dyn Write + Send>>>,
    pending: Mutex<Pending>,
}

impl Channel {
    pub fn new(stdin: Box<dyn Write + Send>) -> Self {
        Self { stdin: Mutex::new(Some(stdin)), pending: Mutex::new(Pending::new()) }
    }

    /// Send a request and hand back the receiver its answer will arrive on.
    ///
    /// The caller waits with a timeout of its own: no call into a plugin
    /// blocks a control method for more than the 5 seconds the protocol
    /// allows, and the health poll waits far less.
    pub fn call(
        &self,
        method: &str,
        params: Value,
    ) -> anyhow::Result<mpsc::Receiver<Result<Value, WireError>>> {
        let (id, rx) = self.pending.lock().expect("the pending lock").claim();
        self.write_line(&wire::request_line(id, method, params))?;
        Ok(rx)
    }

    /// Send a notification. Nothing comes back and nothing waits.
    pub fn notify(&self, method: &str, params: Value) -> anyhow::Result<()> {
        self.write_line(&wire::notification_line(method, params))
    }

    /// Answer a request the plugin made of the core.
    pub fn answer(&self, id: &Value, result: Result<Value, WireError>) -> anyhow::Result<()> {
        self.write_line(&wire::response_line(id, result))
    }

    /// Route one answer from the reader thread to whoever asked.
    pub fn settle(&self, id: &Value, result: Result<Value, WireError>) -> bool {
        self.pending.lock().expect("the pending lock").settle(id, result)
    }

    /// The process is gone. Fail every outstanding call and close stdin.
    pub fn abandon(&self, why: &str) {
        self.pending.lock().expect("the pending lock").abandon(why);
        *self.stdin.lock().expect("the stdin lock") = None;
    }

    pub fn in_flight(&self) -> usize {
        self.pending.lock().expect("the pending lock").len()
    }

    fn write_line(&self, line: &str) -> anyhow::Result<()> {
        let mut held = self.stdin.lock().expect("the stdin lock");
        let stdin = held.as_mut().ok_or_else(|| {
            anyhow::anyhow!("the plugin's control channel is closed; the process has gone")
        })?;
        stdin.write_all(line.as_bytes())?;
        stdin.flush()?;
        Ok(())
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use serde_json::json;
    use std::sync::{Arc, Mutex as StdMutex};

    /// A pipe that keeps what was written, so a test can read the protocol.
    #[derive(Clone, Default)]
    struct Recorder(Arc<StdMutex<Vec<u8>>>);

    impl Write for Recorder {
        fn write(&mut self, buf: &[u8]) -> std::io::Result<usize> {
            self.0.lock().expect("the recorder").extend_from_slice(buf);
            Ok(buf.len())
        }
        fn flush(&mut self) -> std::io::Result<()> {
            Ok(())
        }
    }

    impl Recorder {
        fn text(&self) -> String {
            String::from_utf8(self.0.lock().expect("the recorder").clone()).expect("utf-8")
        }
    }

    #[test]
    fn a_line_over_four_mebibytes_is_refused_by_length_not_by_parsing() {
        let long = "x".repeat(MAX_LINE_BYTES + 1);
        let err = read_line(&long).expect_err("too long");
        assert_eq!(err.code(), ErrorCode::LineTooLong);
        assert!(format!("{err}").contains("4 MiB"), "{err}");
        // One byte under the limit is a line like any other, not an error.
        assert!(read_line(&"y".repeat(MAX_LINE_BYTES)).is_ok());
    }

    #[test]
    fn several_requests_are_in_flight_and_answered_out_of_order() {
        let mut pending = Pending::new();
        let (first, a) = pending.claim();
        let (second, b) = pending.claim();
        assert_ne!(first, second);
        assert!(pending.settle(&json!(second), Ok(json!({"state": "ok"}))));
        assert!(pending.settle(&json!(first), Ok(json!({"applied": true}))));
        assert_eq!(b.recv().expect("the second answer").expect("ok")["state"], "ok");
        assert_eq!(a.recv().expect("the first answer").expect("ok")["applied"], true);
        assert!(pending.is_empty());
    }

    #[test]
    fn a_death_answers_every_outstanding_call_rather_than_hanging_it() {
        let mut pending = Pending::new();
        let (_, rx) = pending.claim();
        pending.abandon("the plugin exited with signal 9");
        let err = rx.recv().expect("an answer arrived").expect_err("it is a failure");
        assert_eq!(err.code, ErrorCode::PluginDied.number());
        assert!(err.message.contains("signal 9"), "{err}");
    }

    #[test]
    fn a_request_goes_out_as_one_line_with_an_id() {
        let recorder = Recorder::default();
        let channel = Channel::new(Box::new(recorder.clone()));
        channel.call("health", json!({})).expect("the channel writes");
        channel.notify("log.set", json!({"level": "debug"})).expect("the channel writes");
        let text = recorder.text();
        let lines: Vec<&str> = text.lines().collect();
        assert_eq!(lines.len(), 2, "{text}");
        let first = wire::parse_line(lines[0]);
        assert!(matches!(first, Frame::Request { ref method, .. } if method == "health"));
        let second = wire::parse_line(lines[1]);
        assert!(matches!(second, Frame::Notification { ref method, .. } if method == "log.set"));
        assert_eq!(channel.in_flight(), 1);
    }

    #[test]
    fn writing_after_the_process_has_gone_names_the_reason() {
        let channel = Channel::new(Box::new(Recorder::default()));
        channel.abandon("stopped");
        let err = channel.notify("health", json!({})).expect_err("the channel is closed");
        assert!(format!("{err}").contains("closed"), "{err}");
    }
}
