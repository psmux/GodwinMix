//! JSON lines on stdio: one JSON object per line, UTF-8, `\n` terminated.
//!
//! The rules come from 03 section 6 and they are short enough to state here:
//!
//! * One JSON object per line. No raw newlines inside a message, because JSON
//!   escapes them.
//! * At most 4 MiB per line. A longer line is error -32011 and the channel is
//!   closed.
//! * A line that is not a JSON object is a log line, not a protocol error.
//! * Both directions may have several requests in flight. Ids are per
//!   direction, so the core's id 1 and the plugin's id 1 are different things.

use std::io::{BufRead, Write};
use std::sync::atomic::{AtomicI64, Ordering};
use std::sync::{Arc, Mutex};

use serde::Serialize;
use serde_json::Value;

use crate::wire::{codes, Id, LogLevel, Message, Request, Response, RpcError};

/// The line limit. A line at exactly this length is fine; one byte more is not.
pub const MAX_LINE_BYTES: usize = 4 * 1024 * 1024;

/// What can go wrong reading a line.
#[derive(Debug)]
pub enum FramingError {
    /// The line was longer than [`MAX_LINE_BYTES`]. The channel must close.
    LineTooLong { bytes: usize },
    /// The byte stream was not UTF-8.
    NotUtf8,
    /// The underlying reader failed.
    Io(std::io::Error),
}

impl std::fmt::Display for FramingError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            FramingError::LineTooLong { bytes } => write!(
                f,
                "line too long: {bytes} bytes, the limit is {MAX_LINE_BYTES}. \
                 Split the payload or send a reference to it instead of the bytes."
            ),
            FramingError::NotUtf8 => write!(f, "the line was not valid UTF-8"),
            FramingError::Io(e) => write!(f, "read failed: {e}"),
        }
    }
}

impl std::error::Error for FramingError {}

impl From<FramingError> for RpcError {
    fn from(e: FramingError) -> RpcError {
        match e {
            FramingError::LineTooLong { bytes } => RpcError::new(
                codes::LINE_TOO_LONG,
                e.to_string(),
            )
            .with_data(
                serde_json::json!({"bytes": bytes, "limit": MAX_LINE_BYTES, "retryable": false}),
            ),
            _ => RpcError::new(codes::INTERNAL_ERROR, e.to_string()),
        }
    }
}

/// Reads one line, enforcing the limit without buffering past it.
///
/// Returns `Ok(None)` at end of input. The trailing newline is stripped, and so
/// is a `\r` before it, so a plugin driven from a Windows shell still parses.
pub fn read_line<R: BufRead>(reader: &mut R) -> Result<Option<String>, FramingError> {
    let mut buf: Vec<u8> = Vec::with_capacity(512);
    loop {
        let available = match reader.fill_buf() {
            Ok(b) => b,
            Err(ref e) if e.kind() == std::io::ErrorKind::Interrupted => continue,
            Err(e) => return Err(FramingError::Io(e)),
        };
        if available.is_empty() {
            if buf.is_empty() {
                return Ok(None);
            }
            break;
        }
        match available.iter().position(|b| *b == b'\n') {
            Some(at) => {
                let take = &available[..at];
                if buf.len() + take.len() > MAX_LINE_BYTES {
                    let bytes = buf.len() + take.len();
                    reader.consume(at + 1);
                    return Err(FramingError::LineTooLong { bytes });
                }
                buf.extend_from_slice(take);
                reader.consume(at + 1);
                break;
            }
            None => {
                let len = available.len();
                if buf.len() + len > MAX_LINE_BYTES {
                    // Do not grow the buffer past the limit just to report it.
                    let bytes = buf.len() + len;
                    reader.consume(len);
                    return Err(FramingError::LineTooLong { bytes });
                }
                buf.extend_from_slice(available);
                reader.consume(len);
            }
        }
    }
    if buf.last() == Some(&b'\r') {
        buf.pop();
    }
    match String::from_utf8(buf) {
        Ok(s) => Ok(Some(s)),
        Err(_) => Err(FramingError::NotUtf8),
    }
}

/// Classify one line. Anything that is not a JSON object is a log line.
pub fn parse_line(line: &str) -> Message {
    let trimmed = line.trim();
    if trimmed.is_empty() {
        return Message::NonJson(line.to_string());
    }
    let value: Value = match serde_json::from_str(trimmed) {
        Ok(Value::Object(map)) => Value::Object(map),
        _ => return Message::NonJson(line.to_string()),
    };
    if value.get("method").and_then(Value::as_str).is_some() {
        match serde_json::from_value::<Request>(value) {
            Ok(r) if r.is_notification() => Message::Notification(r),
            Ok(r) => Message::Request(r),
            Err(_) => Message::NonJson(line.to_string()),
        }
    } else if value.get("result").is_some() || value.get("error").is_some() {
        match serde_json::from_value::<Response>(value) {
            Ok(r) => Message::Response(r),
            Err(_) => Message::NonJson(line.to_string()),
        }
    } else {
        Message::NonJson(line.to_string())
    }
}

/// Reads framed messages off a `BufRead`.
pub struct Reader<R: BufRead> {
    inner: R,
}

impl<R: BufRead> Reader<R> {
    pub fn new(inner: R) -> Self {
        Reader { inner }
    }

    /// The next message, or `None` at end of input.
    ///
    /// A `LineTooLong` error is fatal for the channel: the caller answers with
    /// -32011 and stops reading.
    pub fn next_message(&mut self) -> Result<Option<Message>, FramingError> {
        Ok(read_line(&mut self.inner)?.map(|line| parse_line(&line)))
    }
}

/// Writes framed messages to a `Write`, one object per line, from any thread.
///
/// The mutex is held only for the length of one `write_all`, so a pacing thread
/// sending `log` never blocks the reader thread for longer than a syscall.
pub struct Writer {
    out: Mutex<Box<dyn Write + Send>>,
    next_id: AtomicI64,
}

impl Writer {
    pub fn new(out: Box<dyn Write + Send>) -> Arc<Self> {
        Arc::new(Writer {
            out: Mutex::new(out),
            next_id: AtomicI64::new(0),
        })
    }

    /// Wrap this process's stderr, which is the plugin's side of the control
    /// channel. stdout is media in container mode and must never carry JSON.
    pub fn stderr() -> Arc<Self> {
        Writer::new(Box::new(std::io::stderr()))
    }

    /// The next id in this direction. The core has its own space.
    pub fn next_id(&self) -> Id {
        Id::Num(self.next_id.fetch_add(1, Ordering::Relaxed))
    }

    /// Serialise one value and write it as a line.
    pub fn send<T: Serialize>(&self, message: &T) -> std::io::Result<()> {
        let mut line = serde_json::to_string(message)
            .map_err(|e| std::io::Error::new(std::io::ErrorKind::InvalidData, e))?;
        if line.len() + 1 > MAX_LINE_BYTES {
            return Err(std::io::Error::new(
                std::io::ErrorKind::InvalidData,
                format!(
                    "refusing to send {} bytes on one line, the limit is {MAX_LINE_BYTES}. \
                     Send a reference instead of the bytes.",
                    line.len()
                ),
            ));
        }
        line.push('\n');
        let mut out = self.out.lock().unwrap_or_else(|e| e.into_inner());
        out.write_all(line.as_bytes())?;
        out.flush()
    }

    pub fn respond(&self, id: Id, result: Value) -> std::io::Result<()> {
        self.send(&Response::ok(id, result))
    }

    pub fn respond_error(&self, id: Option<Id>, error: RpcError) -> std::io::Result<()> {
        self.send(&Response::err(id, error))
    }

    pub fn notify(&self, method: &str, params: Value) -> std::io::Result<()> {
        self.send(&Request::notify(method, params))
    }

    /// A structured log line. Plain `eprintln!` also reaches the core's log,
    /// but at `info` and without the level.
    pub fn log(&self, level: LogLevel, message: &str) -> std::io::Result<()> {
        crate::crash::record_log(level, message);
        self.notify(
            "log",
            serde_json::json!({"level": level.as_str(), "message": message}),
        )
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::io::Cursor;

    #[test]
    fn reads_lines_and_stops_at_end() {
        let input = "{\"jsonrpc\":\"2.0\",\"id\":1,\"method\":\"health\"}\nplain text\n";
        let mut r = Reader::new(Cursor::new(input));
        match r.next_message().unwrap().unwrap() {
            Message::Request(req) => assert_eq!(req.method, "health"),
            other => panic!("expected a request, got {other:?}"),
        }
        match r.next_message().unwrap().unwrap() {
            Message::NonJson(s) => assert_eq!(s, "plain text"),
            other => panic!("expected a log line, got {other:?}"),
        }
        assert!(r.next_message().unwrap().is_none());
    }

    #[test]
    fn a_last_line_without_a_newline_still_arrives() {
        let mut r = Reader::new(Cursor::new(
            "{\"jsonrpc\":\"2.0\",\"method\":\"initialized\"}",
        ));
        match r.next_message().unwrap().unwrap() {
            Message::Notification(req) => assert_eq!(req.method, "initialized"),
            other => panic!("got {other:?}"),
        }
    }

    #[test]
    fn carriage_returns_are_stripped() {
        let mut r = Reader::new(Cursor::new(
            "{\"jsonrpc\":\"2.0\",\"method\":\"initialized\"}\r\n",
        ));
        assert!(matches!(
            r.next_message().unwrap().unwrap(),
            Message::Notification(_)
        ));
    }

    #[test]
    fn a_line_at_the_limit_is_accepted() {
        // A JSON object whose serialised form is exactly MAX_LINE_BYTES.
        let prefix = r#"{"jsonrpc":"2.0","id":1,"method":"configure","params":{"x":""#;
        let suffix = r#""}}"#;
        let pad = MAX_LINE_BYTES - prefix.len() - suffix.len();
        let line = format!("{prefix}{}{suffix}\n", "a".repeat(pad));
        let mut r = Reader::new(Cursor::new(line));
        match r.next_message().unwrap().unwrap() {
            Message::Request(req) => assert_eq!(req.method, "configure"),
            other => panic!("got {other:?}"),
        }
    }

    #[test]
    fn a_line_over_the_limit_is_minus_32011() {
        let line = format!("{}\n", "a".repeat(MAX_LINE_BYTES + 1));
        let mut r = Reader::new(Cursor::new(line));
        let err = r.next_message().unwrap_err();
        assert!(matches!(err, FramingError::LineTooLong { .. }));
        let rpc: RpcError = err.into();
        assert_eq!(rpc.code, codes::LINE_TOO_LONG);
        assert!(rpc.message.contains("limit"));
    }

    #[test]
    fn responses_are_told_apart_from_requests() {
        assert!(matches!(
            parse_line(r#"{"jsonrpc":"2.0","id":3,"result":{}}"#),
            Message::Response(_)
        ));
        assert!(matches!(
            parse_line(r#"{"jsonrpc":"2.0","id":3,"error":{"code":-32601,"message":"no"}}"#),
            Message::Response(_)
        ));
        assert!(matches!(parse_line("[1,2,3]"), Message::NonJson(_)));
        assert!(matches!(
            parse_line("Traceback (most recent call last):"),
            Message::NonJson(_)
        ));
    }

    /// Shared so a test can read what the writer produced.
    #[derive(Clone, Default)]
    struct Sink(Arc<Mutex<Vec<u8>>>);

    impl Write for Sink {
        fn write(&mut self, buf: &[u8]) -> std::io::Result<usize> {
            self.0.lock().unwrap().extend_from_slice(buf);
            Ok(buf.len())
        }
        fn flush(&mut self) -> std::io::Result<()> {
            Ok(())
        }
    }

    #[test]
    fn the_writer_puts_one_object_on_one_line() {
        let sink = Sink::default();
        let w = Writer::new(Box::new(sink.clone()));
        w.log(LogLevel::Warn, "a message\nwith a newline in it")
            .unwrap();
        w.respond(Id::Num(1), serde_json::json!({"state": "ok"}))
            .unwrap();
        let bytes = sink.0.lock().unwrap().clone();
        let text = String::from_utf8(bytes).unwrap();
        let lines: Vec<&str> = text.lines().collect();
        assert_eq!(lines.len(), 2, "{text}");
        assert!(lines[0].contains("\\n"), "the newline must be escaped");
        for line in lines {
            let _: Value = serde_json::from_str(line).unwrap();
        }
    }

    #[test]
    fn ids_walk_up_in_our_own_space() {
        let w = Writer::new(Box::new(std::io::sink()));
        assert_eq!(w.next_id(), Id::Num(0));
        assert_eq!(w.next_id(), Id::Num(1));
    }
}
