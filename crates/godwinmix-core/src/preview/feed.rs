//! The programme at full quality, for a full screen output on a projector or
//! a second monitor.
//!
//! The programme monitor in the UI is a cell cut out of the mosaic: at most
//! 1920 pixels for the whole sheet, so a quarter or a ninth of that for the
//! programme, at the mosaic's rate. That is right for an operator's monitor
//! and wrong for a projector. This is the programme itself, off the raw
//! programme tee, at the canvas's own size (or the display's, when that is
//! smaller) and at the programme's own frame rate, one JPEG per frame.
//!
//! ```text
//!   vraw_tee ──► queue (leaky, 2) ──► videorate ──► [download]
//!          ──► videoscale ──► videoconvert ──► caps ──► jpegenc ──► appsink
//! ```
//!
//! Nothing runs unless asked: the branch is built with the programme and
//! linked to nothing. The first [`FeedSubscription`] puts it on the tee and
//! the last one dropped takes it off. Its queue is leaky, so a branch that
//! cannot keep up (a Raspberry Pi encoding 1080p JPEG) loses its own frames
//! and never pushes back on the tee the programme encoder is fed from.
//!
//! The branch is in `feed_branch.rs`; this file is the bookkeeping, which has
//! no GStreamer in it and is tested on its own.

use super::{ClientGuard, StreamClients};
use parking_lot::Mutex;
use std::collections::BTreeMap;
use std::sync::atomic::{AtomicU64, Ordering};
use std::sync::Arc;
use tokio::sync::broadcast;

/// What one client asked for. Zero on either means "the programme's own".
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub struct FeedAsk {
    /// The widest picture the client can show, in device pixels.
    pub width: u32,
    pub fps: u32,
}

/// What the branch is told to make: the widest ask and the fastest, where a
/// zero (the programme's own) beats any number.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub struct FeedShape {
    pub width: u32,
    pub fps: u32,
}

impl FeedShape {
    /// The shape that serves every ask in `asks`, or `None` for no asks.
    pub fn serving<'a>(asks: impl IntoIterator<Item = &'a FeedAsk>) -> Option<FeedShape> {
        let mut out: Option<FeedShape> = None;
        for ask in asks {
            let merged = match out {
                None => FeedShape { width: ask.width, fps: ask.fps },
                Some(s) => FeedShape { width: widest(s.width, ask.width), fps: widest(s.fps, ask.fps) },
            };
            out = Some(merged);
        }
        out
    }
}

/// The larger of two asks, where zero means unbounded.
fn widest(a: u32, b: u32) -> u32 {
    if a == 0 || b == 0 {
        0
    } else {
        a.max(b)
    }
}

/// Tells the mixer thread what the branch should make: `Some` to put it on
/// the tee at that shape, `None` to take it off. A send on the mixer's queue,
/// which never waits.
pub type Switch = Arc<dyn Fn(Option<FeedShape>) + Send + Sync>;

/// What the branch and its subscribers share. One source at full size has
/// one of these too, counted as `source` rather than `programme`.
pub struct ProgrammeFeed {
    tx: broadcast::Sender<Arc<[u8]>>,
    asks: Mutex<BTreeMap<u64, FeedAsk>>,
    told: Mutex<Option<FeedShape>>,
    next: AtomicU64,
    frames: AtomicU64,
    starts: AtomicU64,
    /// `gmx_stream_clients{kind="programme"}` is 1 while the branch is on.
    counted: Mutex<Option<ClientGuard>>,
    /// The `kind` that gauge is read under.
    kind: &'static str,
}

impl Default for ProgrammeFeed {
    fn default() -> Self {
        // Two frames: a reader more than one frame behind wants the newest
        // picture, not a queue of old ones.
        let (tx, _) = broadcast::channel(2);
        Self {
            tx,
            asks: Mutex::default(),
            told: Mutex::default(),
            next: AtomicU64::new(1),
            frames: AtomicU64::new(0),
            starts: AtomicU64::new(0),
            counted: Mutex::default(),
            kind: "programme",
        }
    }
}

impl ProgrammeFeed {
    pub fn new() -> Arc<Self> {
        Arc::new(Self::default())
    }

    /// The same, counted under another `kind`.
    pub fn counted_as(kind: &'static str) -> Arc<Self> {
        Arc::new(Self { kind, ..Self::default() })
    }

    /// Frames the branch has published since boot. Flat while nobody holds
    /// a subscription, which is what a test reads to know the work stopped.
    pub fn frames(&self) -> u64 {
        self.frames.load(Ordering::Relaxed)
    }

    /// How many times the branch has gone onto the tee since boot.
    pub fn starts(&self) -> u64 {
        self.starts.load(Ordering::Relaxed)
    }

    /// What the branch was last told to make, `None` while it is off.
    pub fn shape(&self) -> Option<FeedShape> {
        *self.told.lock()
    }

    /// One JPEG from the branch's streaming thread. A send on a broadcast
    /// channel, which never waits for a reader.
    pub fn publish(&self, jpeg: &[u8]) {
        self.frames.fetch_add(1, Ordering::Relaxed);
        let _ = self.tx.send(Arc::from(jpeg));
    }

    /// Hold the feed at `ask` until the subscription is dropped.
    pub fn subscribe(self: &Arc<Self>, ask: FeedAsk, switch: Switch, clients: &Arc<StreamClients>) -> FeedSubscription {
        let id = self.next.fetch_add(1, Ordering::Relaxed);
        let frames = self.tx.subscribe();
        let mut asks = self.asks.lock();
        asks.insert(id, ask);
        self.retell(&asks, &switch, Some(clients));
        drop(asks);
        FeedSubscription { feed: self.clone(), id, switch, frames }
    }

    fn release(&self, id: u64, switch: &Switch) {
        let mut asks = self.asks.lock();
        asks.remove(&id);
        self.retell(&asks, switch, None);
    }

    /// Tell the mixer the shape that serves everyone, when it changed. Under
    /// the asks lock, so two changes reach the mixer in the order decided.
    fn retell(&self, asks: &BTreeMap<u64, FeedAsk>, switch: &Switch, clients: Option<&Arc<StreamClients>>) {
        let shape = FeedShape::serving(asks.values());
        let mut told = self.told.lock();
        if *told == shape {
            return;
        }
        match (told.is_some(), shape.is_some()) {
            (false, true) => {
                self.starts.fetch_add(1, Ordering::Relaxed);
                *self.counted.lock() = clients.map(|c| c.open(self.kind));
            }
            (true, false) => {
                self.counted.lock().take();
            }
            _ => {}
        }
        *told = shape;
        switch(shape);
    }
}

/// One client's hold on the feed. Dropping it is what lets the branch go.
pub struct FeedSubscription {
    feed: Arc<ProgrammeFeed>,
    id: u64,
    switch: Switch,
    frames: broadcast::Receiver<Arc<[u8]>>,
}

impl FeedSubscription {
    /// The next JPEG.
    pub async fn recv(&mut self) -> Result<Arc<[u8]>, broadcast::error::RecvError> {
        self.frames.recv().await
    }
}

impl Drop for FeedSubscription {
    fn drop(&mut self) {
        self.feed.release(self.id, &self.switch);
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn recorder() -> (Switch, Arc<Mutex<Vec<Option<FeedShape>>>>) {
        let said: Arc<Mutex<Vec<Option<FeedShape>>>> = Arc::default();
        let s = said.clone();
        (Arc::new(move |shape| s.lock().push(shape)), said)
    }

    #[test]
    fn the_shape_serves_the_widest_and_fastest_ask_and_zero_means_native() {
        let a = FeedAsk { width: 1280, fps: 25 };
        let b = FeedAsk { width: 1920, fps: 0 };
        assert_eq!(FeedShape::serving([&a]), Some(FeedShape { width: 1280, fps: 25 }));
        assert_eq!(FeedShape::serving([&a, &b]), Some(FeedShape { width: 1920, fps: 0 }));
        assert_eq!(FeedShape::serving([&FeedAsk::default(), &a]), Some(FeedShape { width: 0, fps: 0 }));
        assert_eq!(FeedShape::serving(std::iter::empty::<&FeedAsk>()), None);
    }

    #[tokio::test]
    async fn the_first_hold_starts_the_branch_and_the_last_release_stops_it() {
        let feed = ProgrammeFeed::new();
        let clients = StreamClients::new();
        let (switch, said) = recorder();
        let one = feed.subscribe(FeedAsk { width: 1280, fps: 30 }, switch.clone(), &clients);
        assert_eq!(clients.count("programme"), 1);
        let mut two = feed.subscribe(FeedAsk { width: 1920, fps: 30 }, switch.clone(), &clients);
        let same = feed.subscribe(FeedAsk { width: 640, fps: 30 }, switch.clone(), &clients);
        assert_eq!(clients.count("programme"), 1, "one branch however many hold it");
        feed.publish(&[0xff, 0xd8, 1]);
        assert_eq!(&*two.recv().await.unwrap(), &[0xff, 0xd8, 1]);
        drop(same);
        drop(two);
        assert_eq!(feed.shape(), Some(FeedShape { width: 1280, fps: 30 }), "back to what is still asked");
        drop(one);
        assert_eq!(clients.count("programme"), 0, "the gauge falls with the branch");
        assert_eq!(
            *said.lock(),
            vec![
                Some(FeedShape { width: 1280, fps: 30 }),
                Some(FeedShape { width: 1920, fps: 30 }),
                Some(FeedShape { width: 1280, fps: 30 }),
                None,
            ],
            "the 640 ask changed nothing, so the mixer was not told about it"
        );
        assert_eq!(feed.starts(), 1);
    }

    #[test]
    fn publishing_with_nobody_listening_costs_a_count_and_nothing_else() {
        let feed = ProgrammeFeed::new();
        feed.publish(&[1, 2, 3]);
        assert_eq!(feed.frames(), 1);
        assert_eq!(feed.shape(), None);
    }
}
