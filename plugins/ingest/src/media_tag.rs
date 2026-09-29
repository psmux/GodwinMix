//! One FLV tag as it came off the wire, shared by every reader of a stream.

use std::sync::Arc;

/// What a tag carries.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum TagKind {
    Audio,
    Video,
    /// `onMetaData` and the other script data a publisher sends.
    Script,
}

/// One tag. Cloning it clones a pointer, never the payload.
#[derive(Debug, Clone)]
pub struct MediaTag {
    pub kind: TagKind,
    /// Milliseconds on the publisher's own timeline.
    pub timestamp_ms: u32,
    /// A video tag that starts a GOP. A reader that fell behind waits for one.
    pub keyframe: bool,
    /// The AVC or AAC sequence header, which a reader joining late must be
    /// given before any other tag of its kind.
    pub sequence_header: bool,
    pub payload: Arc<[u8]>,
}
