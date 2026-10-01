//! The numbers read off the tags as they pass: rates, picture size, codecs,
//! the keyframe interval and how long since the last frame.
//!
//! Kept on the thread that hands the tag on, under the lock it already holds,
//! so it costs a clock read and a few additions per tag and has no timer.

use std::time::Instant;

use super::stats::InputStats;
use crate::codec;
use crate::media_tag::{MediaTag, TagKind};

#[derive(Debug)]
pub struct Meter {
    window: Instant,
    bytes: u64,
    frames: u32,
    video: Option<codec::Video>,
    audio: Option<codec::Audio>,
    last_video: Option<Instant>,
    last_audio: Option<Instant>,
    /// The timestamp of the last keyframe, and of the first and last frame
    /// this window: the stream's own clock, so a burst after a stall does
    /// not read as a fast frame rate.
    last_key: Option<u32>,
    keyframe_ms: Option<u64>,
    span: Option<(u32, u32)>,
    fps: f64,
    kbps: u32,
}

impl Default for Meter {
    fn default() -> Meter {
        Meter {
            window: Instant::now(),
            bytes: 0,
            frames: 0,
            video: None,
            audio: None,
            last_video: None,
            last_audio: None,
            last_key: None,
            keyframe_ms: None,
            span: None,
            fps: 0.0,
            kbps: 0,
        }
    }
}

impl Meter {
    pub fn record(&mut self, tag: &MediaTag) {
        let now = Instant::now();
        self.bytes += tag.payload.len() as u64;
        match tag.kind {
            TagKind::Video if tag.sequence_header => self.video = Some(codec::read_video(tag)),
            TagKind::Video => {
                self.frames += 1;
                self.last_video = Some(now);
                let ts = tag.timestamp_ms;
                self.span = Some(self.span.map_or((ts, ts), |(first, _)| (first, ts)));
                if tag.keyframe {
                    if let Some(prev) = self.last_key.replace(ts) {
                        self.keyframe_ms = Some(u64::from(ts.wrapping_sub(prev)));
                    }
                }
            }
            TagKind::Audio if tag.sequence_header => self.audio = Some(codec::read_audio(tag)),
            TagKind::Audio => {
                self.last_audio = Some(now);
                // AC-3 and MPEG audio carry no sequence header: the frame
                // says. An AAC frame does not, so it waits for its header.
                let framed = crate::exaudio::fourcc(&tag.payload).is_some();
                if framed || (self.audio.is_none() && codec::audio_codec(&tag.payload) != "aac") {
                    let read = codec::read_audio(tag);
                    if read.channels > 0 && self.audio.as_ref() != Some(&read) {
                        self.audio = Some(read);
                    }
                }
            }
            TagKind::Script => {}
        }
    }

    /// The video codec is known: a sequence header or a frame has come.
    pub fn has_video(&self) -> bool {
        self.video.is_some() || self.last_video.is_some()
    }

    /// Any frame at all has come.
    pub fn has_frames(&self) -> bool {
        self.last_video.is_some() || self.last_audio.is_some()
    }

    /// How long since the last frame, video first.
    pub fn quiet_ms(&self) -> Option<u64> {
        self.last_video.or(self.last_audio).map(|t| t.elapsed().as_millis() as u64)
    }

    /// Close the rate window when a second or more has passed, and write
    /// every number into `s`.
    pub fn fill(&mut self, s: &mut InputStats) {
        let secs = self.window.elapsed().as_secs_f64();
        if secs >= 1.0 {
            self.kbps = (self.bytes as f64 * 8.0 / 1000.0 / secs).round() as u32;
            self.fps = match self.span.take() {
                Some((first, last)) if last > first && self.frames > 1 => {
                    (f64::from(self.frames - 1) * 1000.0 / f64::from(last - first) * 100.0).round() / 100.0
                }
                _ => 0.0,
            };
            self.window = Instant::now();
            self.bytes = 0;
            self.frames = 0;
        }
        s.kbps = self.kbps;
        s.fps = self.fps;
        if let Some(v) = &self.video {
            (s.video_codec, s.width, s.height) = (v.codec.clone(), v.width, v.height);
        }
        if let Some(a) = &self.audio {
            (s.audio_codec, s.audio_channels) = (a.codec.clone(), a.channels);
        }
        s.keyframe_ms = self.keyframe_ms;
        s.last_frame_ms = self.quiet_ms();
    }
}

#[cfg(test)]
#[path = "meter_tests.rs"]
mod tests;
