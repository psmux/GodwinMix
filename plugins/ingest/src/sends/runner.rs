//! One destination: wait for its stream, send it while it is live, and wait
//! again when it leaves.

use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::{Arc, Mutex, MutexGuard};
use std::time::{Duration, Instant};

use godwinmix_protocol::destination::{DestinationLive, DestinationState, RecordingFile};

use super::Wanted;
use crate::hub::{Hub, Reader, Recv};
use crate::media_tag::MediaTag;
use crate::restream::{self, Target};

/// How often a waiting destination looks for its stream.
const LOOK: Duration = Duration::from_millis(250);

pub struct Runner {
    pub wanted: Wanted,
    stop: Arc<AtomicBool>,
    now: Mutex<Now>,
}

/// What the runner is doing.
struct Now {
    sending: Option<restream::Handle>,
    /// Reconnects counted by sessions that have ended, so the number a
    /// person sees keeps counting across a publisher coming and going.
    earlier_reconnects: u32,
    waiting_since: Instant,
    /// What a recording wrote last time the stream was live, so a person
    /// still sees the file once the encoder stops.
    last_file: Option<RecordingFile>,
}

impl Runner {
    pub fn start(wanted: Wanted, hub: Hub) -> Arc<Runner> {
        let now = Now { sending: None, earlier_reconnects: 0, waiting_since: Instant::now(), last_file: None };
        let runner = Arc::new(Runner { wanted, stop: Default::default(), now: Mutex::new(now) });
        let me = runner.clone();
        let name = format!("gmx-send-{}", runner.wanted.id);
        let _ = std::thread::Builder::new().name(name).spawn(move || me.run(&hub));
        runner
    }

    fn now(&self) -> MutexGuard<'_, Now> {
        self.now.lock().unwrap_or_else(|e| e.into_inner())
    }

    pub fn stop(&self) {
        self.stop.store(true, Ordering::Relaxed);
        if let Some(h) = self.now().sending.take() {
            h.stop();
        }
    }

    fn stopped(&self) -> bool {
        self.stop.load(Ordering::Relaxed)
    }

    fn run(&self, hub: &Hub) {
        while !self.stopped() {
            let Some(stream) = pick(hub, &self.wanted.app, &self.wanted.reads()) else {
                std::thread::sleep(LOOK);
                continue;
            };
            let ended = Arc::new(AtomicBool::new(false));
            let feed = Feed {
                reader: hub.subscribe(&self.wanted.app, &stream),
                stop: self.stop.clone(),
                ended: ended.clone(),
            };
            let w = &self.wanted;
            let handle = restream::start(Target::new(&w.id, &w.platform, &address(&w.url, &stream)), feed);
            self.now().sending = Some(handle);
            while !self.stopped() && !ended.load(Ordering::Relaxed) {
                std::thread::sleep(LOOK);
            }
            // The publisher left, or the destination was switched off.
            let mut now = self.now();
            if let Some(h) = now.sending.take() {
                let live = h.stats().live;
                now.earlier_reconnects += live.reconnects;
                if let Some(f) = live.file {
                    now.last_file = Some(RecordingFile { open: false, ..f });
                }
                h.stop();
            }
            now.waiting_since = Instant::now();
        }
    }

    /// What the destination is doing, in the shape the wire carries.
    pub fn stats(&self) -> DestinationLive {
        let now = self.now();
        if self.stopped() {
            return DestinationLive::default();
        }
        match &now.sending {
            Some(h) => {
                let mut live = h.stats().live;
                live.reconnects += now.earlier_reconnects;
                live
            }
            None => DestinationLive {
                state: DestinationState::Waiting,
                since_ms: now.waiting_since.elapsed().as_millis() as u64,
                reconnects: now.earlier_reconnects,
                file: now.last_file.clone(),
                ..Default::default()
            },
        }
    }
}

/// The address for one session: a recording's `{stream}` becomes the
/// stream it records, since `*` is only known once one is live.
fn address(url: &str, stream: &str) -> String {
    if url.starts_with("file://") {
        url.replace("{stream}", stream)
    } else {
        url.to_string()
    }
}

/// Which stream to send now: the one named if it is live, or for `*` the
/// channel's stream that has been live longest. A converting destination
/// with nothing planned yet names nothing, and waits.
fn pick(hub: &Hub, app: &str, wanted: &str) -> Option<String> {
    if wanted.is_empty() {
        return None;
    }
    if wanted != "*" {
        return hub.is_live(app, wanted).then(|| wanted.to_string());
    }
    hub.streams()
        .iter()
        .filter(|s| s["app"] == app)
        .filter_map(|s| Some((s["since_ms"].as_u64().unwrap_or(0), s["stream"].as_str()?.to_string())))
        .min()
        .map(|(_, name)| name)
}

/// A hub reader as the restreamer's input. It ends when the publisher leaves
/// or the destination is switched off, and says which through `ended`.
struct Feed {
    reader: Reader,
    stop: Arc<AtomicBool>,
    ended: Arc<AtomicBool>,
}

impl Iterator for Feed {
    type Item = MediaTag;

    fn next(&mut self) -> Option<MediaTag> {
        while !self.stop.load(Ordering::Relaxed) {
            match self.reader.recv_timeout(LOOK) {
                Recv::Tag(tag) => return Some(tag),
                Recv::Ended => break,
                Recv::Timeout => continue,
            }
        }
        self.ended.store(true, Ordering::Relaxed);
        None
    }
}
