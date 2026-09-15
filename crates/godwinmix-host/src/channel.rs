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

/// How many lines may be waiting on a plugin's stdin before the core stops
/// queueing them.
///
/// A pipe holds 64 KiB on macOS and Linux and most protocol lines are a few
/// hundred bytes, so the pipe itself absorbs far more than this. The queue is
/// what is left when the pipe is full, which only happens when the plugin has
/// stopped reading, and thirty two is enough to ride out a plugin that is busy
/// for a moment without pretending a wedged one is fine.
const OUTBOX: usize = 32;

/// The writing half: the plugin's stdin, and the ids in flight behind it.
///
/// `Channel` is what a `SidecarSource` holds. It is `Send` and its lock is
/// never held across a blocking read, so the health poll, a `configure` and a
/// `tool.call` can all be in flight together.
///
/// The write goes through a thread of its own, and that is the whole point of
/// the design rather than an implementation detail. `write_all` on a pipe
/// blocks when the pipe is full, a plugin that stops reading its stdin fills
/// it in about a hundred lines, and the caller is often the mixer's own loop:
/// a source removed during a show reaches `Sidecar::shutdown` from there.
/// Before this, one plugin that stopped reading stopped the mixer, and
/// `abandon` could not get in to fix it because it wanted the same lock.
/// Now nothing the core does waits on the pipe. The queue fills, the call is
/// refused with a message naming the plugin's state, and the process is killed
/// by the teardown, which is what unblocks the writer thread.
pub struct Channel {
    outbox: Mutex<Option<mpsc::SyncSender<Vec<u8>>>>,
    pending: Mutex<Pending>,
}

impl Channel {
    pub fn new(stdin: Box<dyn Write + Send>) -> Self {
        let (tx, rx) = mpsc::sync_channel::<Vec<u8>>(OUTBOX);
        let started = std::thread::Builder::new()
            .name("plugin-stdin".into())
            .spawn(move || write_lines(stdin, rx));
        if started.is_err() {
            // A machine too short of threads to take one. Nothing can be
            // written, and saying so at once beats queueing into a void.
            return Self { outbox: Mutex::new(None), pending: Mutex::new(Pending::new()) };
        }
        Self { outbox: Mutex::new(Some(tx)), pending: Mutex::new(Pending::new()) }
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
    ///
    /// Never blocks, whatever the plugin is doing with its stdin. Dropping the
    /// sender is what ends the writer thread and closes the pipe; a writer
    /// wedged inside `write_all` because the plugin stopped reading sees the
    /// pipe break when the process is killed, and nothing waits for it.
    pub fn abandon(&self, why: &str) {
        self.pending.lock().expect("the pending lock").abandon(why);
        *self.outbox.lock().expect("the stdin lock") = None;
    }

    pub fn in_flight(&self) -> usize {
        self.pending.lock().expect("the pending lock").len()
    }

    fn write_line(&self, line: &str) -> anyhow::Result<()> {
        let held = self.outbox.lock().expect("the stdin lock");
        let outbox = held.as_ref().ok_or_else(|| {
            anyhow::anyhow!("the plugin's control channel is closed; the process has gone")
        })?;
        match outbox.try_send(line.as_bytes().to_vec()) {
            Ok(()) => Ok(()),
            Err(mpsc::TrySendError::Full(_)) => anyhow::bail!(
                "the plugin is not reading its control channel: {OUTBOX} messages are queued \
                 behind a full pipe. It is wedged, or it never reads stdin. Restart it with \
                 `plugin.reload`, and check that it reads a line at a time from stdin as \
                 docs/reference/plugin-protocol.md describes."
            ),
            Err(mpsc::TrySendError::Disconnected(_)) => {
                anyhow::bail!("the plugin's control channel is closed; the process has gone")
            }
        }
    }
}

/// The writer thread. See the note on [`Channel`].
fn write_lines(mut stdin: Box<dyn Write + Send>, rx: mpsc::Receiver<Vec<u8>>) {
    while let Ok(line) = rx.recv() {
        if stdin.write_all(&line).is_err() || stdin.flush().is_err() {
            // The pipe is gone, which means the process is. Every outstanding
            // call is failed by `abandon` when the reader notices the same
            // thing; nothing here has anybody to tell.
            break;
        }
    }
    // Dropping the writer closes the pipe, which is the child's end of file
    // and the politest shutdown signal a plugin gets.
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

        /// The writing happens on a thread of its own, so a test that wants to
        /// read what was written waits for it rather than for a clock.
        fn wait_for_lines(&self, n: usize) -> String {
            let began = std::time::Instant::now();
            while began.elapsed() < std::time::Duration::from_secs(5) {
                let text = self.text();
                if text.lines().count() >= n {
                    return text;
                }
                std::thread::sleep(std::time::Duration::from_millis(2));
            }
            panic!("only {} line(s) were written: {}", self.text().lines().count(), self.text());
        }
    }

    /// A writer that never finishes a write, which is what a plugin that has
    /// stopped reading its stdin looks like from here.
    struct Wedged;

    impl Write for Wedged {
        fn write(&mut self, _buf: &[u8]) -> std::io::Result<usize> {
            loop {
                std::thread::sleep(std::time::Duration::from_millis(50));
            }
        }
        fn flush(&mut self) -> std::io::Result<()> {
            Ok(())
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
        let text = recorder.wait_for_lines(2);
        let lines: Vec<&str> = text.lines().collect();
        assert_eq!(lines.len(), 2, "{text}");
        let first = wire::parse_line(lines[0]);
        assert!(matches!(first, Frame::Request { ref method, .. } if method == "health"));
        let second = wire::parse_line(lines[1]);
        assert!(matches!(second, Frame::Notification { ref method, .. } if method == "log.set"));
        assert_eq!(channel.in_flight(), 1);
    }

    /// A plugin that stops reading its stdin must not stop the mixer.
    ///
    /// The caller here is often the mixer's own loop: removing a source during
    /// a show reaches `Sidecar::shutdown` from it. A `write_all` into a full
    /// pipe blocks for as long as the plugin feels like, and it used to do
    /// that with the stdin lock held, so `abandon` could not get in either.
    #[test]
    fn a_plugin_that_stops_reading_is_refused_rather_than_waited_for() {
        let channel = Channel::new(Box::new(Wedged));
        let began = std::time::Instant::now();
        let mut refused = None;
        // One goes to the writer thread and sticks there; `OUTBOX` more fill
        // the queue; the next one is refused.
        for _ in 0..OUTBOX + 8 {
            if let Err(e) = channel.notify("health", json!({})) {
                refused = Some(format!("{e}"));
                break;
            }
        }
        let refused = refused.expect("a wedged plugin refuses a write rather than taking it");
        assert!(refused.contains("not reading its control channel"), "{refused}");
        assert!(
            began.elapsed() < std::time::Duration::from_secs(1),
            "filling the queue took {:?}; nothing here may wait on the pipe",
            began.elapsed()
        );

        // And the teardown gets in, which is the half that used to deadlock.
        let began = std::time::Instant::now();
        channel.abandon("stopped");
        assert!(
            began.elapsed() < std::time::Duration::from_secs(1),
            "abandoning took {:?}",
            began.elapsed()
        );
    }

    #[test]
    fn writing_after_the_process_has_gone_names_the_reason() {
        let channel = Channel::new(Box::new(Recorder::default()));
        channel.abandon("stopped");
        let err = channel.notify("health", json!({})).expect_err("the channel is closed");
        assert!(format!("{err}").contains("closed"), "{err}");
    }
}
