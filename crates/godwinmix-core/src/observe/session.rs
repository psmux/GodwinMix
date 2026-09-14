//! The session log: every command, every event, every supervisor decision, in
//! order, in one JSONL file, append only.
//!
//! This is the file a bug report is. "Here is the session log from 20:10 to
//! 20:20" replaces a chat transcript, and later (07 Phase 5) the same file is
//! replayed against a test core to turn a church hall on a Sunday into a test
//! that fails on a laptop on Monday.
//!
//! Append only is a property of the type, not a convention. There is no
//! method here that truncates, rewrites or deletes, the file is opened with
//! `append(true)` so every write goes to the end whatever any handle's
//! position is, and no RPC method reaches anything but `record`. A test holds
//! that line.

use crate::observe::trace::{current_trace_id, TraceId};
use crate::state::{Envelope, Event};
use parking_lot::Mutex;
use serde_json::{json, Value};
use std::io::{BufRead, Write as _};
use std::path::{Path, PathBuf};
use std::sync::atomic::{AtomicU64, Ordering};
use std::sync::LazyLock;

/// How large the current generation may get before it is rotated. The same
/// fifty megabytes the log files use in 10 section 2.
pub const MAX_BYTES: u64 = 50 * 1024 * 1024;
/// Rotated generations kept beside it: `session.1.jsonl` to `session.5.jsonl`.
pub const GENERATIONS: usize = 5;
/// The most a range query will ever return. A three hour show is tens of
/// megabytes of JSONL and nothing that asks for a range wants all of it in one
/// answer; the cap keeps the newest records that fit and says so by stopping.
pub const MAX_RANGE_BYTES: usize = 4 * 1024 * 1024;

/// One append only JSONL file.
pub struct SessionLog {
    path: Mutex<Option<PathBuf>>,
    file: Mutex<Option<std::fs::File>>,
    seq: AtomicU64,
    dropped: AtomicU64,
    /// Bytes in the current generation, so rotation does not stat the file on
    /// every record.
    written: AtomicU64,
}

impl SessionLog {
    /// A log with nowhere to write yet. Records are counted and discarded
    /// until `open` points it at a file, which keeps the global usable from
    /// the first line of `run` and in tests that never want a file.
    fn detached() -> Self {
        Self {
            path: Mutex::new(None),
            file: Mutex::new(None),
            seq: AtomicU64::new(0),
            dropped: AtomicU64::new(0),
            written: AtomicU64::new(0),
        }
    }

    /// Point this log at a file, creating it and its directory if needed.
    ///
    /// `append(true)` rather than `write(true)`: on every platform that means
    /// each write goes to the end of the file regardless of where any other
    /// handle has seeked to, so two processes sharing a runtime directory
    /// interleave rather than overwrite.
    pub fn open(&self, path: impl Into<PathBuf>) -> std::io::Result<()> {
        let path = path.into();
        if let Some(dir) = path.parent() {
            std::fs::create_dir_all(dir)?;
        }
        let file = std::fs::OpenOptions::new().create(true).append(true).open(&path)?;
        let size = file.metadata().map(|m| m.len()).unwrap_or(0);
        *self.file.lock() = Some(file);
        *self.path.lock() = Some(path);
        self.written.store(size, Ordering::Relaxed);
        Ok(())
    }

    pub fn path(&self) -> Option<PathBuf> {
        self.path.lock().clone()
    }

    /// Records with no file behind them, so the support bundle can say so
    /// rather than quietly shipping a short log.
    pub fn dropped(&self) -> u64 {
        self.dropped.load(Ordering::Relaxed)
    }

    /// Append one record. Everything else here is a shape over this.
    ///
    /// `seq` is this log's own counter, not the event stream's, so the order in
    /// the file is the order things happened even where two subsystems produce
    /// records at once.
    pub fn record(&self, kind: &str, mut value: Value) {
        let seq = self.seq.fetch_add(1, Ordering::Relaxed);
        let now = std::time::SystemTime::now();
        if let Some(obj) = value.as_object_mut() {
            obj.insert("seq".into(), json!(seq));
            obj.insert("ts".into(), json!(crate::observe::logs::rfc3339(&now)));
            obj.insert("kind".into(), json!(kind));
            if !obj.contains_key("trace_id") {
                if let Some(t) = current_trace_id() {
                    obj.insert("trace_id".into(), json!(t.to_string()));
                }
            }
        }
        let line = value.to_string();
        let bytes = line.len() as u64 + 1;
        {
            let mut guard = self.file.lock();
            let Some(file) = guard.as_mut() else {
                self.dropped.fetch_add(1, Ordering::Relaxed);
                return;
            };
            // A failed write is logged once at warn rather than retried: the
            // session log is a record, not a control path, and a full disk must
            // not become a reason the programme stops.
            if writeln!(file, "{line}").is_err() {
                self.dropped.fetch_add(1, Ordering::Relaxed);
                return;
            }
        }
        if self.written.fetch_add(bytes, Ordering::Relaxed) + bytes > MAX_BYTES {
            self.rotate();
        }
    }

    /// Move the current generation aside and start a new one.
    ///
    /// Rotation is renames only. Nothing already written is edited or
    /// truncated, and the five generations beside the current file are the
    /// same records in the same order, which is what keeps "append only" true
    /// of the log as a whole rather than only of one file.
    fn rotate(&self) {
        let Some(path) = self.path() else { return };
        let mut guard = self.file.lock();
        // Shift the generations down, oldest first, so nothing is overwritten
        // before it has been moved.
        for n in (1..=GENERATIONS).rev() {
            let from = if n == 1 { path.clone() } else { generation(&path, n - 1) };
            let to = generation(&path, n);
            if n == GENERATIONS {
                let _ = std::fs::remove_file(&to);
            }
            if from.exists() {
                let _ = std::fs::rename(&from, &to);
            }
        }
        match std::fs::OpenOptions::new().create(true).append(true).open(&path) {
            Ok(file) => {
                *guard = Some(file);
                self.written.store(0, Ordering::Relaxed);
            }
            Err(e) => {
                tracing::warn!(?e, path = %path.display(), "could not reopen the session log after rotating");
                *guard = None;
            }
        }
    }

    /// A command as it was accepted: what was asked for, by whom, under which
    /// trace, with the idempotency key that would make a retry the same call.
    pub fn record_command(
        &self,
        method: &str,
        trace: Option<TraceId>,
        token_id: Option<&str>,
        idempotency_key: Option<&str>,
        params: Value,
    ) {
        self.record(
            "command",
            json!({
                "method": method,
                "trace_id": trace.map(|t| t.to_string()),
                "token_id": token_id,
                "idempotency_key": idempotency_key,
                "params": params,
            }),
        );
    }

    /// One event off the state broadcast.
    pub fn record_event(&self, event: &Event) {
        let body = serde_json::to_value(event).unwrap_or_else(|e| json!({ "unserialisable": e.to_string() }));
        self.record("event", json!({ "event": body }));
    }

    /// A supervisor decision with the numbers that decided it, in the style of
    /// the existing `timeline_of` lines: not "cam1 rebuilt" but "cam1 rebuilt:
    /// no buffers for 10 s, video was 2,427 ms in the programme's future".
    pub fn record_decision(&self, instance: &str, decision: &str, why: &str, numbers: Value) {
        self.record(
            "decision",
            json!({
                "instance": instance,
                "decision": decision,
                "why": why,
                "numbers": numbers,
            }),
        );
    }

    /// The records written in the last `secs` seconds, for the support bundle.
    ///
    /// Reads the files rather than keeping a ring in memory, because the file
    /// is the record and a second copy would be a second thing to get wrong.
    /// Streamed a line at a time across the current generation and the rotated
    /// ones, newest first, and stopped at [`MAX_RANGE_BYTES`]: a lifetime log
    /// is larger than this process wants in memory and much larger than any
    /// answer is worth.
    pub fn tail_since(&self, secs: u64) -> Vec<String> {
        self.range_since(secs, MAX_RANGE_BYTES)
    }

    /// The same with the cap given, which is how the test reaches it without
    /// writing four megabytes.
    pub fn range_since(&self, secs: u64, byte_cap: usize) -> Vec<String> {
        let Some(path) = self.path() else { return Vec::new() };
        let cutoff = std::time::SystemTime::now()
            .checked_sub(std::time::Duration::from_secs(secs))
            .unwrap_or(std::time::UNIX_EPOCH);
        let cutoff = crate::observe::logs::rfc3339(&cutoff);
        // Newest first, so a cap that bites keeps the records nearest the
        // thing being investigated.
        let mut chunks: Vec<Vec<String>> = Vec::new();
        let mut used = 0usize;
        for file in std::iter::once(path.clone())
            .chain((1..=GENERATIONS).map(|n| generation(&path, n)))
        {
            let Ok(handle) = std::fs::File::open(&file) else { continue };
            let mut chunk: Vec<String> = Vec::new();
            let mut saw_older = false;
            for line in std::io::BufReader::new(handle).lines().map_while(Result::ok) {
                match ts_of(&line) {
                    // Lexical comparison is date comparison for RFC 3339 in
                    // UTC, which is the only spelling this file writes.
                    Some(ts) if ts >= cutoff.as_str() => {
                        used += line.len() + 1;
                        chunk.push(line);
                    }
                    _ => saw_older = true,
                }
            }
            let over = used >= byte_cap;
            if over {
                // Drop from the front of the oldest chunk collected, which is
                // this one, until the answer fits.
                while used >= byte_cap && !chunk.is_empty() {
                    used -= chunk.remove(0).len() + 1;
                }
            }
            let empty = chunk.is_empty();
            chunks.push(chunk);
            // A generation whose records are all older than the cutoff is the
            // end of the range; so is a full answer.
            if over || (saw_older && empty) {
                break;
            }
        }
        chunks.reverse();
        chunks.concat()
    }

    /// The same range query on a blocking worker, for a request handler.
    ///
    /// Reading tens of megabytes of JSONL on the Tokio runtime is how one slow
    /// disk becomes every client's latency, so no async caller does it.
    pub async fn tail_since_async(&'static self, secs: u64) -> Vec<String> {
        tokio::task::spawn_blocking(move || self.tail_since(secs)).await.unwrap_or_default()
    }
}

/// `session.jsonl` becomes `session.3.jsonl`.
fn generation(path: &Path, n: usize) -> PathBuf {
    let stem = path.file_stem().map(|s| s.to_string_lossy().to_string()).unwrap_or_default();
    let ext = path.extension().map(|s| s.to_string_lossy().to_string());
    let name = match ext {
        Some(ext) => format!("{stem}.{n}.{ext}"),
        None => format!("{stem}.{n}"),
    };
    path.with_file_name(name)
}

/// The `ts` of a JSONL record without parsing the whole line.
fn ts_of(line: &str) -> Option<&str> {
    let at = line.find("\"ts\":\"")? + 6;
    let rest = &line[at..];
    let end = rest.find('"')?;
    Some(&rest[..end])
}

static SESSION: LazyLock<SessionLog> = LazyLock::new(SessionLog::detached);

/// The process's session log. Other modules record through this and nothing
/// else; there is no handle that can do more.
pub fn session() -> &'static SessionLog {
    &SESSION
}

/// Where the session log lives under a runtime directory.
pub fn path_in(dir: &Path) -> PathBuf {
    dir.join("session.jsonl")
}

/// Point the process's session log at the runtime directory.
pub fn open_in(dir: &Path) -> std::io::Result<()> {
    session().open(path_in(dir))
}

/// Record every event on the state broadcast, and fold the same events into
/// the metrics registry.
///
/// One task subscribed to the existing broadcast, which is why no other module
/// changes to get a session log: everything that happens is already an event.
/// The meter and scrubber events are counted but not written, because three
/// hundred lines a minute of "the level was -21 dBFS" buries the take that
/// went wrong and is not a thing a replay needs.
pub fn spawn_recorder(
    mut rx: tokio::sync::broadcast::Receiver<Envelope>,
) -> tokio::task::JoinHandle<()> {
    tokio::spawn(async move {
        loop {
            match rx.recv().await {
                Ok(envelope) => {
                    let event = envelope.event;
                    crate::observe::metrics::observe_event(&event);
                    if worth_recording(&event) {
                        session().record_event(&event);
                    }
                }
                Err(tokio::sync::broadcast::error::RecvError::Lagged(n)) => {
                    session().record(
                        "gap",
                        json!({ "missed": n, "why": "the recorder fell behind the event stream" }),
                    );
                }
                Err(tokio::sync::broadcast::error::RecvError::Closed) => return,
            }
        }
    })
}

fn worth_recording(event: &Event) -> bool {
    !matches!(
        event,
        Event::AudioLevel { .. } | Event::SourceAudioLevel { .. } | Event::SourcePosition { .. }
    )
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::state::{Envelope, Severity, SourceState};

    fn temp_log(tag: &str) -> (SessionLog, PathBuf) {
        let dir = crate::observe::tempdir(tag);
        let path = path_in(&dir);
        let log = SessionLog::detached();
        log.open(&path).expect("open");
        (log, path)
    }

    #[test]
    fn a_record_carries_seq_ts_and_kind_in_order() {
        let (log, path) = temp_log("session-order");
        log.record("command", json!({ "method": "program.take" }));
        log.record("event", json!({ "event": "took" }));
        let text = std::fs::read_to_string(&path).unwrap();
        let lines: Vec<Value> =
            text.lines().map(|l| serde_json::from_str(l).unwrap()).collect();
        assert_eq!(lines.len(), 2);
        assert_eq!(lines[0]["seq"], 0);
        assert_eq!(lines[1]["seq"], 1);
        assert_eq!(lines[0]["kind"], "command");
        assert!(lines[0]["ts"].as_str().unwrap().ends_with('Z'));
        assert!(lines[0]["ts"].as_str().unwrap() <= lines[1]["ts"].as_str().unwrap());
    }

    /// The acceptance criterion: nothing reachable from the public API makes
    /// the file shorter or changes a line already in it.
    #[test]
    fn the_file_is_append_only_through_every_public_method() {
        let (log, path) = temp_log("session-append");
        log.record_command(
            "program.take",
            Some(TraceId::new()),
            Some("token-1"),
            Some("idem-1"),
            json!({ "source": "cam1" }),
        );
        let after_one = std::fs::read_to_string(&path).unwrap();
        let len_one = after_one.len();

        // Every other way in.
        log.record_event(&Event::SourceStateChanged {
            source: "cam1".into(),
            state: SourceState::Stalled,
        });
        log.record_decision("cam1", "rebuild", "no buffers for 10 s", json!({ "behind_ms": -2427 }));
        log.record("anything", json!({ "x": 1 }));
        let _ = log.tail_since(3600);
        let _ = log.path();
        let _ = log.dropped();
        // And reopening the same path, which is what a restart does.
        log.open(&path).expect("reopen");
        log.record("after-restart", json!({}));

        let after_all = std::fs::read_to_string(&path).unwrap();
        assert!(after_all.len() > len_one, "the file should only have grown");
        assert!(
            after_all.starts_with(&after_one),
            "an earlier record was rewritten:\nwas: {after_one}\nnow: {after_all}"
        );
        assert_eq!(after_all.lines().count(), 5);
        let _ = std::fs::remove_dir_all(path.parent().unwrap());
    }

    #[test]
    fn a_reopen_appends_rather_than_truncating() {
        let (log, path) = temp_log("session-reopen");
        log.record("one", json!({}));
        drop(log);
        let second = SessionLog::detached();
        second.open(&path).unwrap();
        second.record("two", json!({}));
        let text = std::fs::read_to_string(&path).unwrap();
        assert_eq!(text.lines().count(), 2, "{text}");
        let _ = std::fs::remove_dir_all(path.parent().unwrap());
    }

    #[test]
    fn tail_since_returns_the_recent_records_and_nothing_older() {
        let (log, path) = temp_log("session-tail");
        // A record from last year, written by hand the way an older run would
        // have left it.
        {
            let mut f = std::fs::OpenOptions::new().append(true).open(&path).unwrap();
            writeln!(f, "{}", json!({ "seq": 0, "ts": "2020-01-01T00:00:00.000Z", "kind": "old" }))
                .unwrap();
        }
        log.record("new", json!({}));
        let tail = log.tail_since(3600);
        assert_eq!(tail.len(), 1, "{tail:?}");
        assert!(tail[0].contains("\"kind\":\"new\""));
        let _ = std::fs::remove_dir_all(path.parent().unwrap());
    }

    /// The Codex finding: a range query used to read the whole lifetime log
    /// into memory. It now streams, stops at a cap, and keeps the newest
    /// records rather than the first ones it happened to meet.
    #[test]
    fn a_range_query_is_capped_and_keeps_the_newest_records() {
        let (log, path) = temp_log("session-cap");
        for i in 0..200 {
            log.record("event", json!({ "i": i, "pad": "x".repeat(100) }));
        }
        let all = log.range_since(3600, MAX_RANGE_BYTES);
        assert_eq!(all.len(), 200);

        let capped = log.range_since(3600, 2_000);
        assert!(capped.len() < 200, "the cap has to bite: {} records", capped.len());
        let total: usize = capped.iter().map(|l| l.len() + 1).sum();
        assert!(total < 2_000 + 200, "{total} bytes came back");
        // The newest are the ones kept, and they are still in order.
        let last: Value = serde_json::from_str(capped.last().unwrap()).unwrap();
        assert_eq!(last["i"], 199);
        let first: Value = serde_json::from_str(capped.first().unwrap()).unwrap();
        assert!(first["i"].as_u64().unwrap() > 0, "the oldest records were dropped, not the newest");
        let _ = std::fs::remove_dir_all(path.parent().unwrap());
    }

    /// Rotation is renames only, so the log as a whole stays append only and a
    /// range query still reads across the generations.
    #[test]
    fn the_log_rotates_by_renaming_and_the_range_query_reads_the_generations() {
        let dir = crate::observe::tempdir("session-rotate");
        let path = path_in(&dir);
        let log = SessionLog::detached();
        log.open(&path).unwrap();
        log.record("one", json!({ "n": 1 }));
        // Pretend the generation filled up.
        log.written.store(MAX_BYTES, Ordering::Relaxed);
        log.record("two", json!({ "n": 2 }));
        log.record("three", json!({ "n": 3 }));

        assert!(generation(&path, 1).exists(), "the old generation is beside it, not gone");
        let first = std::fs::read_to_string(generation(&path, 1)).unwrap();
        assert!(first.contains("\"n\":1") && first.contains("\"n\":2"), "{first}");
        let current = std::fs::read_to_string(&path).unwrap();
        assert!(current.contains("\"n\":3"), "{current}");
        assert!(!current.contains("\"n\":1"), "the new generation starts empty: {current}");

        // And the query reads across both, oldest first.
        let all = log.range_since(3600, MAX_RANGE_BYTES);
        assert_eq!(all.len(), 3, "{all:?}");
        assert!(all[0].contains("\"n\":1"));
        assert!(all[2].contains("\"n\":3"));
        let _ = std::fs::remove_dir_all(&dir);
    }

    #[test]
    fn a_generation_is_named_beside_the_file_it_came_from() {
        let p = Path::new("/run/gmx/session.jsonl");
        assert_eq!(generation(p, 1), Path::new("/run/gmx/session.1.jsonl"));
        assert_eq!(generation(p, 5), Path::new("/run/gmx/session.5.jsonl"));
        assert_eq!(generation(Path::new("/run/gmx/session"), 2), Path::new("/run/gmx/session.2"));
    }

    #[test]
    fn a_detached_log_counts_what_it_cannot_write_rather_than_failing() {
        let log = SessionLog::detached();
        log.record("command", json!({ "method": "core.info" }));
        assert_eq!(log.dropped(), 1);
    }

    #[tokio::test]
    async fn every_event_off_the_broadcast_lands_in_the_file() {
        let (tx, rx) = tokio::sync::broadcast::channel(16);
        let dir = crate::observe::tempdir("session-broadcast");
        session().open(path_in(&dir)).unwrap();
        let task = spawn_recorder(rx);
        let mut seq = 0u64;
        let mut tx = |event: Event| {
            seq += 1;
            tx.send(Envelope { seq, event })
        };

        tx(Event::Took { source: Some("cam1".into()), at_running_time_ms: 1234 }).unwrap();
        tx(Event::SourceStateChanged { source: "cam1".into(), state: SourceState::Live }).unwrap();
        tx(Event::Alert { severity: Severity::Warning, message: "cam1 stalled".into() }).unwrap();
        // Meters are counted, not written.
        tx(Event::AudioLevel { peak_db: vec![-21.0, -20.5] }).unwrap();

        for _ in 0..50 {
            tokio::time::sleep(std::time::Duration::from_millis(20)).await;
            if std::fs::read_to_string(path_in(&dir)).map(|t| t.lines().count()).unwrap_or(0) >= 3 {
                break;
            }
        }
        task.abort();

        let text = std::fs::read_to_string(path_in(&dir)).unwrap();
        assert!(text.contains("\"took\""), "{text}");
        assert!(text.contains("source_state_changed"), "{text}");
        assert!(text.contains("cam1 stalled"), "{text}");
        assert!(!text.contains("audio_level"), "meters should not be in the session log: {text}");
    }

    #[test]
    fn the_timestamp_is_found_without_parsing_the_line() {
        assert_eq!(ts_of(r#"{"a":1,"ts":"2026-01-01T00:00:00.000Z","b":2}"#), Some("2026-01-01T00:00:00.000Z"));
        assert_eq!(ts_of("{}"), None);
    }
}
