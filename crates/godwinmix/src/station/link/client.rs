//! The show's half of the link: plain threads and a blocking socket, because
//! the governor asks from whatever thread starts a node and a release comes
//! from a ticket's drop, which can be anywhere.
//!
//! An ask waits at most [`ASK_WAIT`] for its answer; past that the show's own
//! governor decides, as it would with no station. A release and an on air
//! note are queued and never wait.

use super::{Hello, Line};
use godwinmix_govern::{Answer, Ask, Remote, ShedStep};
use parking_lot::Mutex;
use serde_json::json;
use std::collections::HashMap;
use std::io::{BufRead, BufReader, Write};
use std::net::{SocketAddr, TcpStream};
use std::sync::atomic::{AtomicU64, Ordering};
use std::sync::mpsc::{sync_channel, Sender, SyncSender};
use std::sync::Arc;
use std::time::Duration;
use tracing::{info, warn};

/// How long a show waits for its station to admit something.
pub const ASK_WAIT: Duration = Duration::from_millis(500);

type Pending = Arc<Mutex<HashMap<u64, SyncSender<Answer>>>>;
/// What the station last said to give up, until the mixer thread takes it.
type Shed = Arc<Mutex<Vec<ShedStep>>>;

/// A connected link. Cheap to share: every clone writes to the same socket.
pub struct Link {
    out: Mutex<Sender<String>>,
    socket: TcpStream,
    pending: Pending,
    shed: Shed,
    report: Arc<super::report::Reporter>,
    next: AtomicU64,
}

impl Link {
    /// Connect, say hello, and start the two threads. `lost` is called once,
    /// from the reader, when the station goes away.
    pub fn connect(station: SocketAddr, hello: &Hello, lost: Box<dyn FnOnce() + Send>) -> std::io::Result<Arc<Link>> {
        let stream = TcpStream::connect_timeout(&station, Duration::from_secs(5))?;
        stream.set_nodelay(true)?;
        let mut writer = stream.try_clone()?;
        let socket = stream.try_clone()?;
        let (tx, rx) = std::sync::mpsc::channel::<String>();
        let report = super::report::Reporter::spawn(tx.clone())?;
        let first = Line::call(None, "show.hello", serde_json::to_value(hello).unwrap_or_default());
        writer.write_all(first.text().as_bytes())?;
        std::thread::Builder::new().name("station-link-out".into()).spawn(move || {
            for line in rx {
                if writer.write_all(line.as_bytes()).is_err() {
                    return;
                }
            }
        })?;
        let pending: Pending = Arc::new(Mutex::new(HashMap::new()));
        let shed: Shed = Arc::new(Mutex::new(Vec::new()));
        let (answers, told) = (pending.clone(), shed.clone());
        std::thread::Builder::new().name("station-link-in".into()).spawn(move || {
            read_answers(stream, &answers, &told);
            warn!("the link to the station closed; this show stops, because nothing can reach it now");
            lost();
        })?;
        info!(%station, "linked to the station");
        Ok(Arc::new(Link { out: Mutex::new(tx), socket, pending, shed, report, next: AtomicU64::new(1) }))
    }

    fn send(&self, line: Line) {
        let _ = self.out.lock().send(line.text());
    }

    /// Close the link, as a show's exit would.
    pub fn close(&self) {
        let _ = self.socket.shutdown(std::net::Shutdown::Both);
    }

    /// Something started or stopped going out.
    pub fn on_air(&self, on: bool) {
        self.send(Line::call(None, "show.on_air", json!({ "on": on })));
    }

    /// The programme's health moved: its state or its set of alarm kinds.
    pub fn health(&self, health: &godwinmix_protocol::health::Health) {
        self.send(Line::call(None, "show.health", json!({ "health": health })));
    }
}

fn read_answers(stream: TcpStream, pending: &Pending, shed: &Shed) {
    for line in BufReader::new(stream).lines() {
        let Ok(line) = line else { return };
        let Ok(parsed) = serde_json::from_str::<Line>(&line) else { continue };
        if parsed.method.as_deref() == Some("governor.shed") {
            if let Ok(steps) = serde_json::from_value(parsed.params["steps"].clone()) {
                *shed.lock() = steps;
            }
            continue;
        }
        let (Some(id), None) = (parsed.id, parsed.method) else { continue };
        let Some(waiter) = pending.lock().remove(&id) else { continue };
        if let Ok(answer) = serde_json::from_value::<Answer>(parsed.result) {
            let _ = waiter.try_send(answer);
        }
    }
}

impl Remote for Link {
    fn ask(&self, ask: &Ask) -> Option<Answer> {
        let id = self.next.fetch_add(1, Ordering::Relaxed);
        let (tx, rx) = sync_channel(1);
        self.pending.lock().insert(id, tx);
        self.send(Line::call(Some(id), "governor.admit", serde_json::to_value(ask).ok()?));
        let answer = rx.recv_timeout(ASK_WAIT).ok();
        if matches!(answer, Some(Answer::Granted { .. })) {
            self.report.granted();
        }
        if answer.is_none() {
            self.pending.lock().remove(&id);
            warn!(what = %ask.what, "the station did not answer in time; this show decided on its own");
        }
        answer
    }

    fn release(&self, ticket: u64) {
        self.report.released();
        self.send(Line::call(None, "governor.release", json!({ "ticket": ticket })));
    }

    fn shed(&self) -> Vec<ShedStep> {
        std::mem::take(&mut *self.shed.lock())
    }
}
