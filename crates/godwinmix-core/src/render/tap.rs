//! One rung's encoded output, and a consumer's feed off it.

use crate::gstutil::{self, make};
use anyhow::{Context, Result};
use godwinmix_protocol::rendition::{AudioShape, VideoShape};
use gstreamer as gst;
use gstreamer::prelude::*;
use tracing::warn;

/// One rung: the tees its encoded video and audio leave by.
#[derive(Debug, Clone)]
pub struct Tap {
    /// The request id: the output id, or `<output>-<rung>` on a ladder.
    pub request: String,
    /// 0 is the top of a ladder (the largest picture).
    pub rung: usize,
    pub video: Option<VideoShape>,
    pub audio: Option<AudioShape>,
    /// Shared by every rung of the ladder.
    pub keyframe_ms: u32,
    /// Encoded video, parsed, SPS and PPS before every keyframe.
    pub video_tee: Option<gst::Element>,
    pub audio_tee: Option<gst::Element>,
    pub program: gst::Pipeline,
}

/// A consumer's branch off a rung, inside the programme pipeline.
pub struct Feed {
    /// The `proxysink` a `proxysrc` in another pipeline reads video from.
    pub video: Option<gst::Element>,
    pub audio: Option<gst::Element>,
    parts: Vec<(gst::Element, gst::Pad, [gst::Element; 2])>,
    program: gst::Pipeline,
}

impl Tap {
    /// A leaky queue of `queue_secs` and a `proxysink` on each tee, named
    /// after `name`. Brought up before it is linked, as `OutputSlot` does,
    /// so a live tee is never handed a flushing pad.
    pub fn feed(&self, name: &str, queue_secs: f64) -> Result<Feed> {
        let mut feed = Feed {
            video: None,
            audio: None,
            parts: Vec::new(),
            program: self.program.clone(),
        };
        for (tee, tag) in [(&self.video_tee, "v"), (&self.audio_tee, "a")] {
            let Some(tee) = tee else { continue };
            let q = gstutil::queue_time(&format!("{name}-{tag}q"), queue_secs, true)?;
            let proxy = make("proxysink", &format!("{name}-{tag}proxy"))?;
            self.program.add_many([&q, &proxy]).context("adding a feed to the programme")?;
            q.link(&proxy).context("linking a feed")?;
            proxy.sync_state_with_parent().ok();
            q.sync_state_with_parent().ok();
            let pad = tee.request_pad_simple("src_%u").context("the rendition tee refused a pad")?;
            pad.link(&q.static_pad("sink").context("a queue with no sink")?)
                .context("linking a feed to its rendition")?;
            match tag {
                "v" => feed.video = Some(proxy.clone()),
                _ => feed.audio = Some(proxy.clone()),
            }
            feed.parts.push((tee.clone(), pad, [q, proxy]));
        }
        Ok(feed)
    }
}

impl Feed {
    /// Unlink from the tees and take the queue and proxy out.
    pub fn detach(self) {
        for (tee, pad, els) in self.parts {
            if let Some(peer) = pad.peer() {
                let _ = pad.unlink(&peer);
            }
            tee.release_request_pad(&pad);
            for el in els {
                el.set_locked_state(true);
                let _ = el.set_state(gst::State::Null);
                if let Err(e) = self.program.remove(&el) {
                    warn!(?e, "a rendition feed element would not come out");
                }
            }
        }
    }
}
