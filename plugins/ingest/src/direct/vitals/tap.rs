//! A show's input as the vitals see it: a hub reader that keeps the time of
//! the last packet and picks out what the workers decode.
//!
//! Only a keyframe is ever handed to a decoder, at most one per `PACE`, and
//! every inter frame is dropped here, before anything is copied. Sound is a
//! burst of three frames once per `PACE`, and only while the silence check
//! is on, decoded as what its first frame says it is (`sound.rs`). A show that already decodes its input hands its frames in through
//! `Vitals::offer_frame` instead, and then no keyframe is decoded for it.
//!
//! When the directhost's decoded frame tap exists this reader stays for the
//! packet clock and the sound; the keyframes it decodes are the same ones.

use std::sync::atomic::Ordering;
use std::sync::Arc;
use std::time::Duration;

use crate::hub::{Hub, Reader, Recv};
use crate::media_tag::{MediaTag, TagKind};
use crate::transcode::input::{buffer, caps_for, framed};

use super::show::Show;
use super::sound;
use super::pool::Pool;
use super::work::Job;

/// How often a picture and a burst of sound are taken: about once a second.
pub const PACE: u64 = 1000;
/// Sound frames per burst. The first only primes the decoder.
const BURST: usize = 3;

pub struct Tap {
    pub show: Arc<Show>,
    pub app: String,
    pub stream: String,
    reader: Option<Reader>,
    video_header: Option<MediaTag>,
    audio_header: Option<MediaTag>,
    last_pick: u64,
    last_sound: u64,
    burst: Vec<MediaTag>,
    /// Unix ms of the last frame offered from a decode that already exists.
    pub offered_at: u64,
    /// A channel stream somebody asked a picture of, not a show: never
    /// judged, and let go when the asks stop (`peek.rs`).
    pub peek: bool,
}

impl Tap {
    pub fn new(show: Arc<Show>, app: &str, stream: &str) -> Tap {
        Tap {
            show,
            app: app.to_string(),
            stream: stream.to_string(),
            reader: None,
            video_header: None,
            audio_header: None,
            last_pick: 0,
            last_sound: 0,
            burst: Vec::new(),
            offered_at: 0,
            peek: false,
        }
    }

    /// Take whatever has arrived, without waiting.
    pub fn drain(&mut self, hub: &Hub, pool: &Pool, now: u64) {
        let reader = self.reader.take().unwrap_or_else(|| hub.subscribe(&self.app, &self.stream));
        loop {
            match reader.recv_timeout(Duration::ZERO) {
                Recv::Tag(tag) => {
                    self.show.last_packet.store(now, Ordering::Relaxed);
                    self.take(tag, pool, now);
                }
                Recv::Ended => {
                    // The next session needs a reader of its own, and brings
                    // its own headers.
                    self.video_header = None;
                    self.audio_header = None;
                    self.burst.clear();
                    return;
                }
                Recv::Timeout => {
                    self.reader = Some(reader);
                    return;
                }
            }
        }
    }

    fn take(&mut self, tag: MediaTag, pool: &Pool, now: u64) {
        match tag.kind {
            TagKind::Video if tag.sequence_header => self.video_header = Some(tag),
            TagKind::Audio if tag.sequence_header => self.audio_header = Some(tag),
            TagKind::Video if tag.keyframe => self.keyframe(&tag, pool, now),
            TagKind::Audio => self.sound(tag, pool, now),
            _ => {}
        }
    }

    fn keyframe(&mut self, tag: &MediaTag, pool: &Pool, now: u64) {
        let decoded_elsewhere = now.saturating_sub(self.offered_at) < 3 * PACE;
        let due = now.saturating_sub(self.last_pick) + 100 >= PACE;
        if decoded_elsewhere || !due || !self.show.wants_pictures(now) {
            return;
        }
        let Some(caps) = self.video_header.as_ref().and_then(caps_for) else { return };
        let Some(buffer) = buffer(tag, tag.timestamp_ms) else { return };
        self.last_pick = now;
        pool.offer(Job::Picture { show: self.show.clone(), caps, buffer, jpeg: self.show.wants_jpeg(now) });
    }

    fn sound(&mut self, tag: MediaTag, pool: &Pool, now: u64) {
        if !self.show.wants_sound() || (self.burst.is_empty() && now.saturating_sub(self.last_sound) < PACE) {
            return;
        }
        self.burst.push(tag);
        if self.burst.len() < BURST {
            return;
        }
        let burst = std::mem::take(&mut self.burst);
        self.last_sound = now;
        // What the frames say they are: the AAC header is for AAC alone.
        let Some(coded) = sound::coded(&burst[0], self.audio_header.as_ref()) else { return };
        let base = burst[0].timestamp_ms;
        let buffers = burst.iter().filter_map(|t| framed(t, base, coded.skip, 0)).collect();
        pool.offer(Job::Sound { show: self.show.clone(), caps: coded.caps, buffers });
    }
}
