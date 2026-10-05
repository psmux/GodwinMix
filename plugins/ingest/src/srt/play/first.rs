//! The first tags a player is sent, held until the stream's kinds are known.

use crate::media_tag::{MediaTag, TagKind};

/// Nothing goes in until both the picture and the sound have arrived, and
/// then the picture goes first.
///
/// `flvdemux` makes a pad for each kind the first time it sees one, and the
/// muxer writes its first PMT with the pads it has. With one kind in alone,
/// the first PMT named only that one and the next named both, and a
/// player's `tsdemux` takes a changed PMT as a new program: on a Windows
/// runner it offered no picture at all, and elsewhere it dropped the picture
/// it had after one frame. A stream that has only one kind is let through
/// after `HOLD_AT_MOST` tags. What was held also says which kinds the
/// stream has, so the pipeline is built for those and no others.
#[derive(Default)]
pub struct VideoFirst {
    open: bool,
    held: Vec<MediaTag>,
}

const HOLD_AT_MOST: usize = 100;

impl VideoFirst {
    pub fn take(&mut self, tag: MediaTag) -> Vec<MediaTag> {
        if self.open {
            return vec![tag];
        }
        self.held.push(tag);
        let has = |kind: TagKind| self.held.iter().any(|t| t.kind == kind);
        if !(has(TagKind::Video) && has(TagKind::Audio)) && self.held.len() < HOLD_AT_MOST {
            return Vec::new();
        }
        self.open = true;
        // Stable, so each kind keeps its own order: the pictures, then the sound.
        let mut out = std::mem::take(&mut self.held);
        out.sort_by_key(|t| t.kind != TagKind::Video);
        out
    }
}
