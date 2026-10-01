//! The stream's own tags into the decoders.
//!
//! An FLV video body is a short prefix and an access unit (AVC, or HEVC and
//! AV1 as enhanced RTMP frames them; see `crate::eflv`), and an AAC body
//! two bytes and a raw AAC frame; the sequence headers carry the
//! configuration a decoder is given as `codec_data` in its caps. AC-3,
//! E-AC-3 and MPEG audio have no header: each frame says what it is, and
//! its caps are read off it (`direct::vitals::sound`). So each tag becomes
//! one buffer with its time on the pipeline's clock (the publisher's time
//! less the session's first), and nothing is parsed beyond that.
//!
//! A decoder that has just started waits for a keyframe, and the handle is
//! taken out of the lock before a push, which may wait for the decoder:
//! nothing that changes the graph ever waits behind it.

use std::sync::{Mutex, MutexGuard};

use gstreamer as gst;
use gstreamer_app::AppSrc;

pub(crate) use super::caps::caps_for;
use super::caps::headerless;
use crate::media_tag::{MediaTag, TagKind};

#[derive(Default)]
struct Feed {
    src: Option<AppSrc>,
    need_key: bool,
    header: Option<MediaTag>,
    /// Sound with no header: the caps its frames gave, last set.
    framed: Option<gst::Caps>,
}

#[derive(Default)]
pub struct Input {
    video: Mutex<Feed>,
    audio: Mutex<Feed>,
}

fn lock(m: &Mutex<Feed>) -> MutexGuard<'_, Feed> {
    m.lock().unwrap_or_else(|e| e.into_inner())
}

impl Input {
    fn feed(&self, kind: TagKind) -> Option<&Mutex<Feed>> {
        match kind {
            TagKind::Video => Some(&self.video),
            TagKind::Audio => Some(&self.audio),
            TagKind::Script => None,
        }
    }

    /// A decoder's appsrc for `track`, or none. A new one starts at the next
    /// keyframe with the configuration already known.
    pub fn attach(&self, track: TagKind, src: Option<AppSrc>) {
        let Some(feed) = self.feed(track) else { return };
        let mut f = lock(feed);
        if let (Some(s), Some(caps)) = (&src, f.header.as_ref().and_then(caps_for)) {
            s.set_caps(Some(&caps));
        }
        f.src = src;
        f.need_key = true;
        f.framed = None;
    }

    /// Forget the configuration: a new session brings its own.
    pub fn reset(&self) {
        for feed in [&self.video, &self.audio] {
            *lock(feed) = Feed::default();
        }
    }

    /// One tag of the stream, with the session's first time `base`.
    pub fn push(&self, tag: &MediaTag, base: u32) {
        let Some(feed) = self.feed(tag.kind) else { return };
        let (src, skip) = {
            let mut f = lock(feed);
            if tag.sequence_header {
                f.header = Some(tag.clone());
                if let (Some(s), Some(caps)) = (&f.src, caps_for(tag)) {
                    s.set_caps(Some(&caps));
                }
                return;
            }
            let framed = headerless(tag).then(|| crate::direct::vitals::sound::coded(tag, None)).flatten();
            if let (Some(s), Some(c)) = (&f.src, &framed) {
                if f.framed.as_ref() != Some(&c.caps) {
                    s.set_caps(Some(&c.caps));
                    f.framed = Some(c.caps.clone());
                }
            }
            let starts = tag.kind == TagKind::Audio || tag.keyframe;
            if f.src.is_none() || (f.header.is_none() && framed.is_none()) || (f.need_key && !starts) {
                return;
            }
            f.need_key = false;
            (f.src.clone(), framed.map(|c| c.skip))
        };
        let buffer = match skip {
            Some(skip) => framed(tag, base, skip, 0),
            None => buffer(tag, base),
        };
        if let (Some(src), Some(buffer)) = (src, buffer) {
            // Flushing means the decoder is being taken down; the tag is not
            // wanted, and that is all.
            let _ = src.push_buffer(buffer);
        }
    }
}

/// A tag's body past its prefix, shared with every other reader of the tag
/// rather than copied.
struct Tail(std::sync::Arc<[u8]>, usize);

impl AsRef<[u8]> for Tail {
    fn as_ref(&self) -> &[u8] {
        &self.0[self.1..]
    }
}

pub(crate) fn buffer(tag: &MediaTag, base: u32) -> Option<gst::Buffer> {
    let (skip, cts) = match tag.kind {
        TagKind::Video => crate::eflv::frame(&tag.payload)?,
        _ => (2, 0),
    };
    framed(tag, base, skip, cts)
}

/// One buffer of the body past `skip` bytes, at its time less `base`, its
/// picture `cts` milliseconds after its decode.
pub(crate) fn framed(tag: &MediaTag, base: u32, skip: usize, cts: i32) -> Option<gst::Buffer> {
    // A tag from before the session's first is from before the decoder's
    // zero, and is not wanted.
    let dts = i64::from(tag.timestamp_ms) - i64::from(base);
    if dts < 0 {
        return None;
    }
    let pts = (dts + i64::from(cts)).max(0);
    tag.payload.get(skip..)?;
    let mut buffer = gst::Buffer::from_slice(Tail(tag.payload.clone(), skip));
    let b = buffer.get_mut()?;
    b.set_dts(gst::ClockTime::from_mseconds(dts as u64));
    b.set_pts(gst::ClockTime::from_mseconds(pts as u64));
    if tag.kind == TagKind::Video && !tag.keyframe {
        b.set_flags(gst::BufferFlags::DELTA_UNIT);
    }
    Some(buffer)
}
