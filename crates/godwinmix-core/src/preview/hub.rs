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
use super::{ClientGuard, StreamClients};
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
    CloseAudio {
        key: String,
    },
    /// Open a local raw preview socket, or join one that exists.
    OpenLocal {
        target: String,
        reply: oneshot::Sender<Result<String, String>>,
    },
    CloseLocal {
        target: String,
    },
}

impl std::fmt::Debug for PreviewDemand {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            Self::OpenAudio {
                target, request, ..
            } => {
                write!(f, "OpenAudio({target}, {})", request.kind())
            }
            Self::CloseAudio { key } => write!(f, "CloseAudio({key})"),
            Self::OpenLocal { target, .. } => write!(f, "OpenLocal({target})"),
            Self::CloseLocal { target } => write!(f, "CloseLocal({target})"),
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
}

impl PreviewHandle {
    pub fn new(clients: Arc<StreamClients>, demand: DemandSink) -> Self {
        Self {
            clients,
            demand: Some(demand),
        }
    }

    /// A handle attached to nothing: it counts clients and refuses to open
    /// anything. For tests and for a core with no mixer thread.
    pub fn detached() -> Self {
        Self {
            clients: StreamClients::new(),
            demand: None,
        }
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
        sink(PreviewDemand::OpenLocal {
            target: target.to_string(),
            reply: tx,
        });
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
            sink(PreviewDemand::CloseAudio {
                key: self.key.clone(),
            });
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
            sink(PreviewDemand::CloseLocal {
                target: self.target.clone(),
            });
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
        assert_eq!(
            h.clients().total(),
            0,
            "a refusal must not leave a client counted"
        );
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
        assert!(
            seen.lock().iter().any(|s| s.starts_with("CloseAudio")),
            "{:?}",
            seen.lock()
        );
    }
}
