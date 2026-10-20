//! One source at full size, on the mixer thread.
//!
//! The multiview cell of a source is a fraction of a 1920 pixel sheet, which
//! is right for picking a camera and too small to read a web page by. Interact
//! asks for `/mjpeg/{source}?full=1` instead, and this puts the programme
//! feed's chain (`preview/feed_branch.rs`) on the source's own tee, at the
//! canvas size, for as long as somebody holds it.
//!
//! The branch lives in the source's pipeline, so it goes when the source
//! does. What was asked for stays: a source rebuilt under a viewer (a new
//! address, new params, a restart after a stall) gets the branch again on its
//! new tee, and the viewer's stream carries on from the same feed.

use super::Mixer;
use crate::preview::feed::{FeedShape, ProgrammeFeed};
use crate::preview::feed_branch::FeedBranch;
use std::sync::Arc;
use tracing::{debug, warn};

/// What one source's viewers asked for, and the branch serving it while the
/// source is up.
pub(super) struct SourceFeed {
    feed: Arc<ProgrammeFeed>,
    shape: FeedShape,
    branch: Option<FeedBranch>,
}

impl Mixer {
    /// The shape that serves this source's viewers changed: `None` when the
    /// last of them has gone.
    pub(super) fn source_feed(&mut self, id: String, feed: Arc<ProgrammeFeed>, shape: Option<FeedShape>) {
        let Some(shape) = shape else {
            if let Some(branch) = self.source_feeds.remove(&id).and_then(|f| f.branch) {
                branch.dismantle();
                debug!(source = %id, "full size source feed taken off: nobody is watching");
            }
            return;
        };
        self.source_feeds
            .entry(id.clone())
            .and_modify(|f| f.shape = shape)
            .or_insert(SourceFeed { feed, shape, branch: None });
        self.attach_source_feed(&id);
    }

    /// Put the branch on the source's tee if anybody wants it and it is not
    /// there, and give it the shape asked for. Called when a source joins too.
    pub(super) fn attach_source_feed(&mut self, id: &str) {
        let Some(want) = self.source_feeds.get_mut(id) else { return };
        if want.branch.is_none() {
            let Some(slot) = self.sources.iter().find(|s| s.input.id == id) else { return };
            let (pipeline, vtee, _atee) = slot.input.taps();
            let canvas = (self.canvas.width, self.canvas.height);
            match FeedBranch::build(&pipeline, &vtee, Vec::new(), canvas, &want.feed, &format!("{id}-f")) {
                Ok(branch) => want.branch = Some(branch),
                Err(e) => {
                    warn!(source = %id, ?e, "the full size source feed could not be built");
                    return;
                }
            }
        }
        if let Some(branch) = &mut want.branch {
            if let Err(e) = branch.apply(want.shape) {
                warn!(source = %id, ?e, "the full size source feed could not be attached");
            }
        }
    }

    /// The source is leaving the desk: its branch goes with its pipeline, and
    /// what was asked for stays for the source that replaces it.
    pub(super) fn detach_source_feed(&mut self, id: &str) {
        if let Some(branch) = self.source_feeds.get_mut(id).and_then(|f| f.branch.take()) {
            branch.dismantle();
        }
    }

    /// Every branch off, for shutdown.
    pub(super) fn drop_source_feeds(&mut self) {
        for (_, f) in std::mem::take(&mut self.source_feeds) {
            if let Some(branch) = f.branch {
                branch.dismantle();
            }
        }
    }
}
