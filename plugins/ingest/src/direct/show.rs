//! One direct show: its input on a thread of its own, published on the hub,
//! and its outputs.
//!
//! The publication is made here, before the input starts, and held where
//! the show can take it back: stopping a show ends the stream for every
//! reader at once, whatever the input's thread is still doing, so a new
//! input for the same show can publish the moment the old one is told to
//! stop.

use std::sync::{Arc, Mutex, MutexGuard};
use std::time::{Duration, Instant};

use super::input::{self, Context};
use super::output::Output;
use super::table::{Row, STREAM};
use super::{InputStats, StopSignal, TagSink};
use crate::hub::{Hub, Publication};
use crate::media_tag::{MediaTag, TagKind};
use crate::sends::{Feed, Wanted};

/// How long without a tag before an input counts as idle.
pub const IDLE: Duration = Duration::from_secs(3);

fn lock<T>(m: &Mutex<T>) -> MutexGuard<'_, T> {
    m.lock().unwrap_or_else(|e| e.into_inner())
}

/// What the show's input has said, for the events and the stats.
#[derive(Default)]
pub struct Seen {
    pub stats: InputStats,
    pub last_tag: Option<Instant>,
    /// Why the input could not be opened, when it could not.
    pub refused: Option<String>,
}

impl Seen {
    pub fn live(&self) -> bool {
        self.last_tag.is_some_and(|t| t.elapsed() < IDLE)
    }
}

/// The input's sink: every tag straight onto the hub.
struct ToHub {
    publication: Arc<Mutex<Option<Publication>>>,
    seen: Arc<Mutex<Seen>>,
}

impl TagSink for ToHub {
    fn tag(&mut self, tag: MediaTag) {
        if tag.kind != TagKind::Script {
            lock(&self.seen).last_tag = Some(Instant::now());
        }
        if let Some(p) = lock(&self.publication).as_ref() {
            p.push(tag);
        }
    }

    fn stats(&mut self, stats: &InputStats) {
        lock(&self.seen).stats = stats.clone();
    }
}

pub struct Show {
    pub row: Row,
    pub seen: Arc<Mutex<Seen>>,
    pub outputs: Vec<Output>,
    publication: Arc<Mutex<Option<Publication>>>,
    stop: StopSignal,
}

impl Show {
    /// Publish, open the input, start the outputs.
    pub fn start(row: Row, hub: &Hub, renditions: &Hub, ctx: &Context) -> Show {
        let seen = Arc::new(Mutex::new(Seen::default()));
        let publication = hub.publish_via(&row.app(), STREAM, &row.input.uri, None, "direct");
        if let Err(why) = &publication {
            lock(&seen).refused = Some(why.clone());
        }
        let publication = Arc::new(Mutex::new(publication.ok()));
        let stop = StopSignal::default();
        let opened = match &row.refused {
            Some(why) => Err(why.clone()),
            None => input::open(&row.input, ctx).map_err(|e| e.message),
        };
        match opened {
            Ok(input) => {
                let sink = Box::new(ToHub { publication: publication.clone(), seen: seen.clone() });
                let halt = stop.clone();
                let _ = std::thread::Builder::new().name(format!("gmx-in-{}", row.id)).spawn(move || input.run(sink, halt));
            }
            Err(why) => lock(&seen).refused = Some(why),
        }
        let outputs = row.outputs.iter().filter(|w| sends(w)).map(|w| start_output(w, &row, hub, renditions)).collect();
        Show { row, seen, outputs, publication, stop }
    }

    /// Make the outputs match `row`'s, touching only those that changed.
    pub fn set_outputs(&mut self, row: &Row, hub: &Hub, renditions: &Hub) {
        self.outputs.retain(|o| row.outputs.contains(&o.wanted));
        for w in row.outputs.iter().filter(|w| sends(w)) {
            if !self.outputs.iter().any(|o| &o.wanted == w) {
                self.outputs.push(start_output(w, row, hub, renditions));
            }
        }
        self.row.outputs = row.outputs.clone();
        self.row.transcode = row.transcode.clone();
        self.row.monitor = row.monitor.clone();
        self.row.name = row.name.clone();
    }

    /// Stop the input and end the stream for every reader, now.
    pub fn stop(&mut self) {
        self.stop.stop();
        lock(&self.publication).take();
        for o in &self.outputs {
            o.stop();
        }
    }
}

impl Drop for Show {
    fn drop(&mut self) {
        self.stop();
    }
}

/// Whether this host sends the output. An HLS output is packaged and
/// served by the station, which reads the show's stream or the output's
/// rendition off the relay; its row is here so the rendition is built.
fn sends(w: &Wanted) -> bool {
    w.platform != "hls"
}

fn start_output(w: &Wanted, row: &Row, hub: &Hub, renditions: &Hub) -> Output {
    match &w.feed {
        Feed::Copy => Output::start(w.clone(), hub.clone(), None),
        Feed::Rendition { video, .. } => Output::start(w.clone(), renditions.clone(), encoder(row, video.as_deref())),
    }
}

/// The element the plan chose for a video node, for the output's stats.
fn encoder(row: &Row, node: Option<&str>) -> Option<String> {
    let node = node?;
    row.transcode.iter().flat_map(|s| &s.nodes).find(|n| n.id == node).and_then(|n| n.text("element")).map(str::to_string)
}
