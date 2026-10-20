//! The handle the control plane holds, and the demand it puts on the mixer.
//!
//! Every branch in this module is a GStreamer pipeline change, so it happens on
//! the mixer thread and nowhere else, exactly as the mosaic does. An HTTP
//! handler asks through [`PreviewHandle`], waits for the answer on a oneshot,
//! and gets back a receiver plus a lease. Dropping the lease is what eventually
//! removes the branch.
//!
//! Two clients asking for the same shape of the same target share one branch:
//! the key is the target plus the rate, channels and format, so `/pcm/program`
//! opened twice costs one branch and `/pcm/program?rate=16000` beside it costs
//! a second. The mixer keeps a count per key and removes the branch when the
//! count reaches zero.

use super::audio::{AudioRequest, Frame};
use super::feed::{FeedAsk, FeedShape, FeedSubscription, ProgrammeFeed};
use super::thumb::{ProgrammeThumb, Thumb};
use super::{ClientGuard, StreamClients};
use parking_lot::Mutex;
use std::collections::BTreeMap;
use std::sync::Arc;
use tokio::sync::{broadcast, oneshot};

/// What the control plane asks the mixer thread to do about a preview branch.
pub enum PreviewDemand {
    /// Open an audio monitoring branch, or join one that already exists.
    OpenAudio {
        target: String,
        request: AudioRequest,
        reply: oneshot::Sender<Result<broadcast::Receiver<Frame>, String>>,
    },
    /// One client has gone. The branch goes when the last one does.
    CloseAudio { key: String },
    /// Open a local raw preview socket, or join one that exists.
    OpenLocal { target: String, reply: oneshot::Sender<Result<String, String>> },
    CloseLocal { target: String },
    /// What the armed scene is, in canvas pixels, for the preview compositor
    /// to draw. Sent when a scene is armed or disarmed and when a client
    /// subscribes with `ext.preview`; the mixer holds it whether or not a
    /// preview exists, so arming one before anybody is watching costs a `Vec`.
    Scene { cells: Vec<crate::multiview::preview::Cell> },
    /// Put the programme thumbnail branch on the raw tee, or take it off.
    /// Sent by [`super::thumb::ProgrammeThumb`] on the first ask after a
    /// quiet spell and when asks stop.
    Thumb(bool),
    /// Put the full quality programme feed on the raw tee at this shape, or
    /// take it off with `None`. Sent by [`super::feed::ProgrammeFeed`] when
    /// the shape that serves every subscriber changes.
    Feed(Option<FeedShape>),
    /// Whether a source called `id` is here, so a stream on one that is not
    /// is refused with the names of those that are.
    HasSource { id: String, reply: oneshot::Sender<Result<(), String>> },
    /// Put one source's full size feed on that source's own tee at this
    /// shape, or take it off with `None`.
    SourceFeed { id: String, feed: Arc<ProgrammeFeed>, shape: Option<FeedShape> },
}

impl std::fmt::Debug for PreviewDemand {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            Self::OpenAudio { target, request, .. } => {
                write!(f, "OpenAudio({target}, {})", request.kind())
            }
            Self::CloseAudio { key } => write!(f, "CloseAudio({key})"),
            Self::OpenLocal { target, .. } => write!(f, "OpenLocal({target})"),
            Self::CloseLocal { target } => write!(f, "CloseLocal({target})"),
            Self::Scene { cells } => write!(f, "Scene({} items)", cells.len()),
            Self::Thumb(on) => write!(f, "Thumb({on})"),
            Self::Feed(shape) => write!(f, "Feed({shape:?})"),
            Self::HasSource { id, .. } => write!(f, "HasSource({id})"),
            Self::SourceFeed { id, shape, .. } => write!(f, "SourceFeed({id}, {shape:?})"),
        }
    }
}

type DemandSink = Arc<dyn Fn(PreviewDemand) + Send + Sync>;

/// The public face of the preview streams. Cloneable, cheap, and safe to hold
/// whether or not anything is open.
#[derive(Clone)]
pub struct PreviewHandle {
    clients: Arc<StreamClients>,
    demand: Option<DemandSink>,
    thumb: Arc<ProgrammeThumb>,
    feed: Arc<ProgrammeFeed>,
    /// One full size feed per source somebody has watched that way.
    sources: Arc<Mutex<BTreeMap<String, Arc<ProgrammeFeed>>>>,
}

impl PreviewHandle {
    pub fn new(clients: Arc<StreamClients>, demand: DemandSink) -> Self {
        Self { clients, demand: Some(demand), thumb: ProgrammeThumb::new(), feed: ProgrammeFeed::new(), sources: Arc::default() }
    }

    /// A handle attached to nothing: it counts clients and refuses to open
    /// anything. For tests and for a core with no mixer thread.
    pub fn detached() -> Self {
        Self {
            clients: StreamClients::new(),
            demand: None,
            thumb: ProgrammeThumb::new(),
            feed: ProgrammeFeed::new(),
            sources: Arc::default(),
        }
    }

    /// What the programme thumbnail branch keeps its frames in. The mixer
    /// hands it to the branch it builds; a test reads its counters.
    pub fn thumb(&self) -> Arc<ProgrammeThumb> {
        self.thumb.clone()
    }

    /// What the full quality programme feed publishes into. The mixer hands
    /// it to the branch it builds; a test reads its counters.
    pub fn feed(&self) -> Arc<ProgrammeFeed> {
        self.feed.clone()
    }

    /// Hold the full quality programme feed (`ext.programme`) for as long as
    /// the subscription lives. See `feed.rs`.
    pub fn programme_feed(&self, ask: FeedAsk) -> Result<FeedSubscription, String> {
        let Some(sink) = self.demand.clone() else {
            return Err("this core has no mixer, so there is no programme to show".into());
        };
        Ok(self.feed.subscribe(ask, Arc::new(move |shape| sink(PreviewDemand::Feed(shape))), &self.clients))
    }

    /// Hold one source at full size (`/mjpeg/{source}?full=1`) for as long
    /// as the subscription lives: the same branch as the programme feed, on
    /// the source's own tee. Refused, naming the sources, for one not here.
    pub async fn source_feed(&self, id: &str, ask: FeedAsk) -> Result<FeedSubscription, String> {
        let Some(sink) = self.demand.clone() else {
            return Err("this core has no mixer, so there are no sources to show".into());
        };
        let (tx, rx) = oneshot::channel();
        sink(PreviewDemand::HasSource { id: id.to_string(), reply: tx });
        rx.await.map_err(|_| "the mixer did not answer a source picture request".to_string())??;
        let feed = {
            let mut feeds = self.sources.lock();
            // A feed nobody holds is forgotten, so a removed source leaves nothing.
            feeds.retain(|k, f| k == id || f.shape().is_some());
            feeds.entry(id.to_string()).or_insert_with(|| ProgrammeFeed::counted_as("source")).clone()
        };
        let (key, held) = (id.to_string(), feed.clone());
        let switch = Arc::new(move |shape| sink(PreviewDemand::SourceFeed { id: key.clone(), feed: held.clone(), shape }));
        Ok(feed.subscribe(ask, switch, &self.clients))
    }

    /// The programme as a small picture, `width` pixels across (16 to 320),
    /// keeping pictures coming for the next ten seconds. `Ok(None)` while the
    /// first frame is on its way. See `thumb.rs`.
    pub async fn programme_thumbnail(&self, width: u32) -> Result<Option<Thumb>, String> {
        let Some(sink) = self.demand.clone() else {
            return Err("this core has no mixer, so there is no programme to take a picture of".into());
        };
        self.thumb.want(Arc::new(move |on| sink(PreviewDemand::Thumb(on))), &self.clients);
        Ok(self.thumb.picture(width).await)
    }

    /// The programme as a snapshot, `width` pixels across up to the canvas's
    /// own width, from the same branch. `Ok(None)` while no frame has come.
    pub async fn programme_picture(&self, width: u32) -> Result<Option<Thumb>, String> {
        let Some(sink) = self.demand.clone() else {
            return Err("this core has no mixer, so there is no programme to take a picture of".into());
        };
        self.thumb.want(Arc::new(move |on| sink(PreviewDemand::Thumb(on))), &self.clients);
        Ok(self.thumb.large(width).await)
    }

    pub fn clients(&self) -> Arc<StreamClients> {
        self.clients.clone()
    }

    /// Open an audio monitoring stream on `target`, which is `program` or a
    /// source id.
    ///
    /// The returned lease keeps the branch alive. Dropping it is what closes
    /// the stream, and the branch goes when the last lease on that shape does.
    pub async fn open_audio(
        &self,
        target: &str,
        request: AudioRequest,
    ) -> Result<(AudioStream, broadcast::Receiver<Frame>), String> {
        let request = request.clamped();
        let Some(sink) = &self.demand else {
            return Err("this core has no mixer, so there is no audio to monitor".into());
        };
        let (tx, rx) = oneshot::channel();
        sink(PreviewDemand::OpenAudio {
            target: target.to_string(),
            request,
            reply: tx,
        });
        let frames = rx
            .await
            .map_err(|_| "the mixer did not answer an audio monitoring request".to_string())??;
        let guard = self.clients.open(request.kind());
        Ok((
            AudioStream {
                key: request.key(target),
                demand: self.demand.clone(),
                _guard: guard,
            },
            frames,
        ))
    }

    /// Open a local raw preview socket for `target` and answer with its path.
    pub async fn open_local(&self, target: &str) -> Result<(LocalStream, String), String> {
        let Some(sink) = &self.demand else {
            return Err("this core has no mixer, so there is nothing to preview".into());
        };
        let (tx, rx) = oneshot::channel();
        sink(PreviewDemand::OpenLocal { target: target.to_string(), reply: tx });
        let path = rx
            .await
            .map_err(|_| "the mixer did not answer a preview.open request".to_string())??;
        let guard = self.clients.open("unixfd");
        Ok((
            LocalStream {
                target: target.to_string(),
                demand: self.demand.clone(),
                _guard: guard,
            },
            path,
        ))
    }

    /// Count an MJPEG or WHEP client, which own no branch of their own: the
    /// mosaic subscription and the encoder lease respectively are what those
    /// hold.
    pub fn count(&self, kind: &str) -> ClientGuard {
        self.clients.open(kind)
    }

    /// Tell the mixer what the armed scene is.
    ///
    /// Not a request and not awaited: the caller has the document in hand and
    /// the mixer is the only thread that may touch a pipeline, so this is a
    /// one way message. A core with no mixer drops it, which is right: there
    /// is nothing to draw it on.
    pub fn set_scene(&self, cells: Vec<crate::multiview::preview::Cell>) {
        if let Some(sink) = &self.demand {
            sink(PreviewDemand::Scene { cells });
        }
    }
}

/// One client's hold on an audio monitoring branch.
pub struct AudioStream {
    key: String,
    demand: Option<DemandSink>,
    _guard: ClientGuard,
}

impl AudioStream {
    pub fn key(&self) -> &str {
        &self.key
    }
}

impl Drop for AudioStream {
    fn drop(&mut self) {
        if let Some(sink) = &self.demand {
            sink(PreviewDemand::CloseAudio { key: self.key.clone() });
        }
    }
}

/// One client's hold on a local raw preview socket.
pub struct LocalStream {
    target: String,
    demand: Option<DemandSink>,
    _guard: ClientGuard,
}

impl Drop for LocalStream {
    fn drop(&mut self) {
        if let Some(sink) = &self.demand {
            sink(PreviewDemand::CloseLocal { target: self.target.clone() });
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[tokio::test]
    async fn a_detached_handle_refuses_and_names_why() {
        let h = PreviewHandle::detached();
        let e = match h.open_audio("program", AudioRequest::default()).await {
            Ok(_) => panic!("a detached handle must refuse"),
            Err(e) => e,
        };
        assert!(e.contains("no mixer"), "{e}");
        assert_eq!(h.clients().total(), 0, "a refusal must not leave a client counted");
    }

    #[tokio::test]
    async fn a_lease_counts_a_client_and_asks_for_the_branch_to_go_when_it_drops() {
        let seen: Arc<parking_lot::Mutex<Vec<String>>> =
            Arc::new(parking_lot::Mutex::new(Vec::new()));
        let sink = {
            let seen = seen.clone();
            Arc::new(move |d: PreviewDemand| {
                if let PreviewDemand::OpenAudio { reply, .. } = d {
                    let (tx, _rx) = broadcast::channel(4);
                    let _ = reply.send(Ok(tx.subscribe()));
                    // Keep the sender alive for the life of the test.
                    std::mem::forget(tx);
                } else {
                    seen.lock().push(format!("{d:?}"));
                }
            }) as DemandSink
        };
        let h = PreviewHandle::new(StreamClients::new(), sink);
        let (stream, _rx) = match h.open_audio("program", AudioRequest::default()).await {
            Ok(pair) => pair,
            Err(e) => panic!("open_audio refused: {e}"),
        };
        assert_eq!(h.clients().count("pcm"), 1);
        assert!(stream.key().starts_with("program/pcm/"));
        drop(stream);
        assert_eq!(h.clients().count("pcm"), 0);
        assert!(seen.lock().iter().any(|s| s.starts_with("CloseAudio")), "{:?}", seen.lock());
    }
}
