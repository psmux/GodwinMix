//! What a stream is carrying right now: codec, size, frame rate, bit rate.
//!
//! Updated by the publisher's own thread as each tag passes, under the lock
//! it already holds to hand the tag on, so reading the numbers costs a caller
//! nothing and keeping them costs a clock read per tag. There is no timer.

use std::time::{Duration, Instant, SystemTime, UNIX_EPOCH};

use serde_json::{json, Value};

use crate::codec::{self, Audio, Video};
use crate::media_tag::{MediaTag, TagKind};

/// How long a rate is measured over before it is reported.
const WINDOW: Duration = Duration::from_secs(1);

/// Windows after which the rates count as measured even with a track silent.
const SETTLE_WINDOWS: u32 = 4;

/// One publisher's session, measured.
pub struct Meter {
    pub since_ms: u64,
    video: Option<Video>,
    audio: Option<Audio>,
    window_start: Instant,
    last_tag: Instant,
    video_bytes: u64,
    audio_bytes: u64,
    frames: u32,
    video_kbps: u32,
    audio_kbps: u32,
    fps: f64,
    /// The frame rate last worth an event: the first one measured, and any
    /// that moves by more than a tenth from it. A plan for a converting
    /// destination follows the frame rate, so the core has to hear of it.
    told_fps: f64,
    /// The frame rate the publisher's `onMetaData` states, which is exact
    /// where the measured one is only near: 29.97 and 30 look alike over a
    /// second of frames.
    declared_fps: Option<f64>,
    /// Whether the bit rates mean something yet (`settle`). A plan made before then took an unknown rate for one that
    /// fits, and copied 160 kbit/s sound to a destination that asked for 128.
    measured: bool,
    /// Windows measured before `measured`.
    windows: u32,
    pub total_bytes: u64,
    /// The sound tracks after the first, for an input that carries them.
    tracks: super::tracks::Tracks,
    /// What an MPEG-2 sequence says of its scan: `Some(true)` interlaced.
    interlaced: Option<bool>,
}

impl Meter {
    pub fn new() -> Meter {
        let now = Instant::now();
        let since_ms = SystemTime::now()
            .duration_since(UNIX_EPOCH)
            .map(|d| d.as_millis() as u64)
            .unwrap_or(0);
        Meter {
            since_ms,
            video: None,
            audio: None,
            window_start: now,
            last_tag: now,
            video_bytes: 0,
            audio_bytes: 0,
            frames: 0,
            video_kbps: 0,
            audio_kbps: 0,
            fps: 0.0,
            told_fps: 0.0,
            declared_fps: None,
            measured: false,
            windows: 0,
            total_bytes: 0,
            tracks: Default::default(),
            interlaced: None,
        }
    }

    /// Count one tag. Returns true when it told us something new about the
    /// codecs, which is worth an event.
    pub fn record(&mut self, tag: &MediaTag) -> bool {
        let now = Instant::now();
        self.last_tag = now;
        let len = tag.payload.len() as u64;
        self.total_bytes += len;
        if tag.track != 0 {
            return self.tracks.record(tag, now);
        }
        // The first window starts with the first frame: a publisher whose
        // media came more than a second after it connected measured an empty
        // first second, which read as 0 kbit/s measured.
        if !self.measured && self.windows == 0 && self.video_bytes + self.audio_bytes == 0 {
            self.window_start = now;
        }
        let mut news = false;
        match tag.kind {
            TagKind::Video if tag.sequence_header => {
                let read = codec::read_video(tag);
                news = self.video.as_ref() != Some(&read);
                self.video = Some(read);
            }
            TagKind::Audio if tag.sequence_header => {
                let read = codec::read_audio(tag);
                news = self.audio.as_ref() != Some(&read);
                self.audio = Some(read);
            }
            TagKind::Video => {
                self.video_bytes += len;
                self.frames += 1;
                // MPEG-2 has no sequence start tag: its keyframes say it.
                if let Some((read, fps, progressive)) = super::tracks::mpeg2(tag) {
                    news = self.video.as_ref() != Some(&read) || self.declared_fps != Some(fps);
                    (self.video, self.declared_fps, self.interlaced) = (Some(read), Some(fps), Some(!progressive));
                }
            }
            TagKind::Audio => {
                self.audio_bytes += len;
                // AC-3, E-AC-3 and MPEG audio come with no sequence header:
                // the first frame says what they are.
                let headerless = tag.payload.first().is_some_and(|b| b >> 4 != 10);
                if headerless && self.audio.as_ref().is_none_or(|a| a.codec == "aac") {
                    let read = codec::read_audio(tag);
                    news = self.audio.as_ref() != Some(&read);
                    self.audio = Some(read);
                }
            }
            TagKind::Script => news = self.declare(tag),
        }
        let elapsed = now.duration_since(self.window_start);
        if elapsed >= WINDOW {
            let secs = elapsed.as_secs_f64();
            self.video_kbps = (self.video_bytes as f64 * 8.0 / 1000.0 / secs).round() as u32;
            self.audio_kbps = (self.audio_bytes as f64 * 8.0 / 1000.0 / secs).round() as u32;
            self.fps = ((self.frames as f64 / secs) * 100.0).round() / 100.0;
            self.window_start = now;
            self.video_bytes = 0;
            self.audio_bytes = 0;
            self.frames = 0;
            news |= self.settle();
            news |= self.rate_moved();
        }
        news
    }

    /// After a window: the rates count as measured from the second window
    /// on in which every track the stream has carried bytes, or after
    /// `SETTLE_WINDOWS` whatever they carried, so a track that never sends
    /// cannot hold a plan back for ever. The first window is a warm up: the
    /// programme's 160 kbit/s sound, starting part way into it, read 120.
    /// True the once it happens: the first rates are news of their own, for
    /// a stream with no video whose frame rate never moves.
    fn settle(&mut self) -> bool {
        if self.measured {
            return false;
        }
        self.windows += 1;
        let whole = (self.video.is_none() || self.video_kbps > 0) && (self.audio.is_none() || self.audio_kbps > 0);
        self.measured = (whole && self.windows >= 2) || self.windows >= SETTLE_WINDOWS;
        self.measured
    }

    /// Read the frame rate out of `onMetaData`. True when it is new.
    fn declare(&mut self, tag: &MediaTag) -> bool {
        let rate = crate::restream::meta::parse(&tag.payload).and_then(|m| m.video_frame_rate);
        let rate = rate.map(f64::from).filter(|r| r.is_finite() && *r > 0.0);
        let news = rate.is_some() && rate != self.declared_fps;
        if news {
            self.declared_fps = rate;
        }
        news
    }

    /// True once when the frame rate is first known, and again when it moves
    /// by more than a tenth.
    fn rate_moved(&mut self) -> bool {
        let moved = self.fps > 0.0 && (self.told_fps == 0.0 || (self.fps - self.told_fps).abs() > self.told_fps / 10.0);
        if moved {
            self.told_fps = self.fps;
        }
        moved
    }

    /// How long since the last tag, or since the session began if none came.
    pub fn quiet(&self) -> Duration {
        self.last_tag.elapsed()
    }

    /// Has any video arrived this session? An audio only publisher never
    /// sends a keyframe, and a reader must not wait for one.
    pub fn has_video(&self) -> bool {
        self.video.is_some() || self.frames > 0 || self.video_kbps > 0
    }

    /// Whether the bit rates have settled enough to plan against.
    pub fn measured(&self) -> bool {
        self.measured
    }

    /// The `video` and `audio` members of the contract's Stream shape.
    pub fn describe(&self) -> (Value, Value) {
        // A publisher that has gone quiet is not still sending at its last rate.
        let quiet = self.last_tag.elapsed() > WINDOW * 3;
        let rate = |kbps: u32| if quiet { 0 } else { kbps };
        let video = self.video.as_ref().map_or(Value::Null, |v| {
            let mut video = json!({
                "codec": v.codec, "width": v.width, "height": v.height,
                "fps": if quiet { 0.0 } else { self.fps }, "kbps": rate(self.video_kbps),
            });
            if let Some(declared) = self.declared_fps {
                video["frame_rate"] = json!(declared);
            }
            if let Some(i) = self.interlaced {
                video["interlaced"] = json!(i);
            }
            video
        });
        let audio = self.audio.as_ref().map_or(Value::Null, |a| {
            json!({
                "codec": a.codec, "channels": a.channels, "sample_rate": a.sample_rate,
                "kbps": rate(self.audio_kbps),
            })
        });
        (video, audio)
    }

    /// Every track carried: the video and the first sound as track 0, then
    /// each further sound, `{track, kind, codec, ...}` with the members of
    /// `describe` beside them.
    pub fn tracks(&self) -> Vec<Value> {
        let (video, audio) = self.describe();
        let mut out = Vec::new();
        for (kind, mut v) in [("video", video), ("audio", audio)] {
            if v.is_object() {
                v["track"] = json!(0);
                v["kind"] = json!(kind);
                out.push(v);
            }
        }
        out.extend(self.tracks.describe(self.last_tag.elapsed() > WINDOW * 3));
        out
    }
}
