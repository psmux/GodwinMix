//! A stream's side of keeping its tracks in step (`ring_align.rs`): a break
//! marked on every track at once, and the picture the sound is cut beside.

use super::super::track::{Track, TrackKind};
use super::Stream;
use std::sync::Arc;

impl Stream {
    /// The feed behind this stream stopped and is starting again, so the
    /// timestamps begin anew. Call it once per new pipeline, before its
    /// tracks are attached, from the thread that builds the pipeline and
    /// never from a streaming thread. A stream with nothing cut yet has no
    /// break to mark.
    ///
    /// Every track is marked while the list of tracks is held for writing,
    /// so they all get the same discontinuity and the same next number: the
    /// picture and the sound cannot disagree about a break, whatever their
    /// muxers write as headers.
    pub fn restart(&self) {
        let tracks = self.tracks.write();
        let Some(last) = tracks.iter().filter_map(|t| t.newest()).min() else { return };
        // Two breaks with nothing cut between them are one discontinuity:
        // a player counts the tags it sees, and it would see one.
        let cut = tracks.iter().any(|t| t.cut_since_break());
        let disc = current_disc(&tracks) + u32::from(cut);
        for t in tracks.iter() {
            t.mark_break(disc, last);
        }
    }

    /// The picture the sound is cut beside: the first video track, which is
    /// a ladder's top rung.
    pub fn leader(&self) -> Option<Arc<Track>> {
        self.tracks.read().iter().find(|t| t.kind == TrackKind::Video).cloned()
    }
}

/// The discontinuity a stream's tracks are in now.
pub(super) fn current_disc(tracks: &[Arc<Track>]) -> u32 {
    tracks.iter().map(|t| t.disc()).max().unwrap_or(0)
}
