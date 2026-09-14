//! Numbers every tick, so an agent does not have to look at a picture.
//!
//! 09 section 5 item 11: a line under 50 tokens per tick carrying the shot
//! change score, the black ratio, a freeze flag, short term and integrated
//! loudness, a silence flag and per source liveness, computed by cheap probes
//! on the raw programme frames. It is about 75 times cheaper than a 1080p
//! still and it carries things a still cannot: that the picture has not moved
//! for two seconds, that the sound went away.
//!
//! Nothing here runs unless a client asks. The pad probe is attached once from
//! `Mixer::build` and its first statement is a relaxed atomic load: with no
//! subscriber it returns without mapping the buffer, which is a handful of
//! nanoseconds on a frame that was going to be copied anyway.
//!
//! The probe also feeds the flash guard in [`crate::safety`]: a luminance step
//! of 20 cd/m2 over more than a quarter of the frame is what ITU-R BT.1702-3
//! regulates, and this is the only place in the core that measures it.

use crate::safety::Guard;
use parking_lot::Mutex;
use serde::{Deserialize, Serialize};
use std::collections::VecDeque;
use std::sync::atomic::{AtomicUsize, Ordering};
use std::sync::{Arc, LazyLock};
use std::time::{Duration, Instant};

/// The subsampled grid the probes read. 16 by 9 is 144 samples, which is
/// enough to tell a black frame from a dark one and a cut from a pan, and
/// small enough that reading it costs less than the memcpy the frame is about
/// to have anyway.
pub const GRID_W: usize = 16;
pub const GRID_H: usize = 9;
pub const GRID: usize = GRID_W * GRID_H;

/// Luma at or below this counts as black. Studio swing black is 16.
const BLACK_LUMA: u8 = 24;
/// The picture has to be identical for this long before `freeze` is true.
pub const DEFAULT_FREEZE_MS: u64 = 500;
/// The programme has to be this quiet for `silence_ms` before `silence` is true.
const SILENCE_DBFS: f64 = -50.0;
pub const DEFAULT_SILENCE_MS: u64 = 500;
/// BS.1770's short term window.
const SHORT_TERM: Duration = Duration::from_secs(3);
/// BS.1770's absolute gate, below which a block does not count towards the
/// integrated figure.
const ABSOLUTE_GATE_LUFS: f64 = -70.0;
/// The name `Mixer::build` gives the programme's `level` element. The bus
/// watcher sees every meter in the process, and only this one is the
/// programme.
pub const PROGRAMME_METER: &str = "pgm-level";

/// One tick of `event/telemetry`.
///
/// Field names are short on purpose: this goes out up to ten times a second
/// and every byte is charged for in somebody's context window. `sources` is
/// `1` for a source that is live and `0` for one that is not, rather than
/// `true` and `false`, for the same reason.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct Tick {
    /// Milliseconds since the Unix epoch.
    pub ts: u64,
    /// How much the picture changed since the last frame, 0 to 1. A cut is
    /// near 1, a locked off camera is near 0.
    pub shot: f64,
    /// Fraction of the picture at or below black, 0 to 1.
    pub black: f64,
    /// The picture has not changed at all for `freeze_ms`.
    pub freeze: bool,
    /// Short term loudness over three seconds, and the running integrated
    /// figure. `null` until the programme has made a sound.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub lufs_s: Option<f64>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub lufs_i: Option<f64>,
    pub silence: bool,
    /// Source id to 1 (live) or 0.
    pub sources: std::collections::BTreeMap<String, u8>,
}

/// Everything the probes have measured, before it is trimmed into a [`Tick`].
#[derive(Debug, Clone, Copy, PartialEq, Default)]
pub struct Reading {
    pub shot: f64,
    pub black: f64,
    /// How long since the picture last changed, in milliseconds. `None` when
    /// no two frames have been compared yet.
    pub still_ms: Option<u64>,
    /// The last two frames were byte for byte the same on the grid.
    pub identical: bool,
    pub lufs_s: Option<f64>,
    pub lufs_i: Option<f64>,
    /// How long the programme has been below the silence floor.
    pub silent_ms: Option<u64>,
    /// Frames the probe has read since the first subscriber arrived.
    pub frames: u64,
}

impl Reading {
    pub fn freeze(&self, freeze_ms: u64) -> bool {
        self.identical && self.still_ms.is_some_and(|ms| ms >= freeze_ms)
    }

    pub fn silence(&self, silence_ms: u64) -> bool {
        self.silent_ms.is_some_and(|ms| ms >= silence_ms)
    }

    /// The wire shape, with the source liveness the caller holds folded in.
    pub fn tick(
        &self,
        sources: std::collections::BTreeMap<String, u8>,
        freeze_ms: u64,
        silence_ms: u64,
    ) -> Tick {
        Tick {
            ts: now_ms(),
            shot: round3(self.shot),
            black: round3(self.black),
            freeze: self.freeze(freeze_ms),
            lufs_s: self.lufs_s.map(round1),
            lufs_i: self.lufs_i.map(round1),
            silence: self.silence(silence_ms),
            sources,
        }
    }
}

fn now_ms() -> u64 {
    std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .map(|d| d.as_millis() as u64)
        .unwrap_or_default()
}

fn round3(v: f64) -> f64 {
    (v * 1000.0).round() / 1000.0
}

fn round1(v: f64) -> f64 {
    (v * 10.0).round() / 10.0
}

/// The process's telemetry. One per core, like the session log.
pub struct Telemetry {
    /// How many clients have asked for telemetry. Read once per frame on the
    /// streaming thread, which is why it is an atomic and not a lock.
    subscribers: AtomicUsize,
    video: Mutex<Video>,
    audio: Mutex<Audio>,
    guard: Mutex<Option<Arc<Guard>>>,
}

#[derive(Default)]
struct Video {
    /// The last subsampled grid, for the difference against the next one.
    last: Option<[u8; GRID]>,
    shot: f64,
    black: f64,
    /// When the picture last changed.
    still_since: Option<Instant>,
    identical: bool,
    frames: u64,
}

#[derive(Default)]
struct Audio {
    /// Mean square per measurement, with the time it was taken, for the three
    /// second window.
    window: VecDeque<(Instant, f64)>,
    /// Running sum for the integrated figure, gated at -70 LUFS.
    sum: f64,
    blocks: u64,
    /// When the programme first went below the silence floor, cleared as soon
    /// as it comes back up.
    quiet_since: Option<Instant>,
    heard: bool,
}

static TELEMETRY: LazyLock<Telemetry> = LazyLock::new(Telemetry::new);

pub fn telemetry() -> &'static Telemetry {
    &TELEMETRY
}

/// Holding one of these is what keeps the probes measuring. Dropping the last
/// one stops them and forgets what they measured, so a later subscriber does
/// not read a freeze that happened while nobody was looking.
pub struct Lease<'a> {
    owner: &'a Telemetry,
}

impl Drop for Lease<'_> {
    fn drop(&mut self) {
        if self.owner.subscribers.fetch_sub(1, Ordering::Relaxed) == 1 {
            self.owner.reset();
        }
    }
}

impl Telemetry {
    /// A telemetry of its own. The process has one behind [`telemetry`]; the
    /// tests make their own so that two of them never share a reading.
    pub fn new() -> Self {
        Self {
            subscribers: AtomicUsize::new(0),
            video: Mutex::new(Video::default()),
            audio: Mutex::new(Audio::default()),
            guard: Mutex::new(None),
        }
    }

    /// Ask for telemetry. The probes run for as long as the lease is held.
    pub fn lease(&self) -> Lease<'_> {
        if self.subscribers.fetch_add(1, Ordering::Relaxed) == 0 {
            self.reset();
            if let Some(guard) = self.guard.lock().as_ref() {
                guard.set_luma_observed(true);
            }
        }
        Lease { owner: self }
    }

    pub fn wanted(&self) -> bool {
        self.subscribers.load(Ordering::Relaxed) > 0
    }

    /// Tell the flash guard when the programme's brightness takes a step.
    pub fn bind_guard(&self, guard: Arc<Guard>) {
        guard.set_luma_observed(self.wanted());
        *self.guard.lock() = Some(guard);
    }

    fn reset(&self) {
        *self.video.lock() = Video::default();
        *self.audio.lock() = Audio::default();
        if let Some(guard) = self.guard.lock().as_ref() {
            guard.set_luma_observed(self.wanted());
        }
    }

    /// What the probes have measured, right now.
    pub fn read(&self) -> Reading {
        let video = self.video.lock();
        let audio = self.audio.lock();
        let (lufs_s, lufs_i) = audio.loudness();
        Reading {
            shot: video.shot,
            black: video.black,
            still_ms: video.still_since.map(|t| t.elapsed().as_millis() as u64),
            identical: video.identical,
            lufs_s,
            lufs_i,
            silent_ms: audio
                .quiet_since
                .filter(|_| audio.heard)
                .map(|t| t.elapsed().as_millis() as u64),
            frames: video.frames,
        }
    }

    /// One subsampled grid off a programme frame. Public so the probe and the
    /// tests feed it the same way.
    pub fn absorb_grid(&self, grid: [u8; GRID]) {
        let flash = {
            let mut video = self.video.lock();
            video.frames += 1;
            let flash = match video.last.take() {
                Some(last) => {
                    let diff: u32 =
                        last.iter().zip(&grid).map(|(a, b)| a.abs_diff(*b) as u32).sum();
                    video.shot = diff as f64 / (GRID as f64 * 255.0);
                    video.identical = diff == 0;
                    if diff == 0 {
                        video.still_since.get_or_insert_with(Instant::now);
                    } else {
                        video.still_since = Some(Instant::now());
                    }
                    crate::safety::is_flash(&last, &grid)
                }
                None => {
                    video.shot = 0.0;
                    video.identical = false;
                    video.still_since = None;
                    false
                }
            };
            video.black =
                grid.iter().filter(|l| **l <= BLACK_LUMA).count() as f64 / GRID as f64;
            video.last = Some(grid);
            flash
        };
        if flash {
            if let Some(guard) = self.guard.lock().as_ref() {
                guard.note_flash();
            }
        }
    }

    /// One `level` measurement off the programme meter, in dBFS per channel.
    pub fn absorb_rms(&self, rms_db: &[f64]) {
        if rms_db.is_empty() {
            return;
        }
        let now = Instant::now();
        let mut audio = self.audio.lock();
        // Mean square summed across channels, which is what BS.1770 does
        // before weighting. No K weighting here: that wants a filter on the
        // audio path and this reads a meter that is already running.
        let power: f64 = rms_db.iter().map(|db| 10f64.powf(db / 10.0)).sum();
        audio.heard = true;
        audio.window.push_back((now, power));
        while audio.window.front().is_some_and(|(t, _)| now.duration_since(*t) > SHORT_TERM) {
            audio.window.pop_front();
        }
        // Bounded whatever the meter's interval turns out to be.
        while audio.window.len() > 256 {
            audio.window.pop_front();
        }
        let loudness = lufs(power);
        if loudness > ABSOLUTE_GATE_LUFS {
            audio.sum += power;
            audio.blocks += 1;
        }
        let dbfs = 10.0 * power.max(f64::MIN_POSITIVE).log10();
        if dbfs > SILENCE_DBFS {
            audio.quiet_since = None;
        } else {
            audio.quiet_since.get_or_insert(now);
        }
    }
}

impl Audio {
    fn loudness(&self) -> (Option<f64>, Option<f64>) {
        if !self.heard {
            return (None, None);
        }
        let short = if self.window.is_empty() {
            None
        } else {
            let mean: f64 = self.window.iter().map(|(_, p)| *p).sum::<f64>()
                / self.window.len() as f64;
            Some(lufs(mean))
        };
        let integrated = (self.blocks > 0).then(|| lufs(self.sum / self.blocks as f64));
        (short, integrated)
    }
}

/// BS.1770's loudness from a mean square, without the K weighting filter.
fn lufs(mean_square: f64) -> f64 {
    -0.691 + 10.0 * mean_square.max(f64::MIN_POSITIVE).log10()
}

// --- the GStreamer side ----------------------------------------------------

/// Read a subsampled luma grid out of one programme frame.
///
/// Attached from `Mixer::build` with `crate::telemetry::attach(&vraw_tee)`,
/// on the same pad the frame counter uses, so every programme frame passes it
/// exactly once before the encoder and the multiview split apart.
pub fn attach(tee: &gstreamer::Element) {
    use gstreamer::prelude::*;
    let Some(pad) = tee.static_pad("sink") else {
        tracing::warn!("programme tee has no sink pad, telemetry is off");
        return;
    };
    let cached: Mutex<Option<(gstreamer::Caps, gstreamer_video::VideoInfo)>> = Mutex::new(None);
    pad.add_probe(gstreamer::PadProbeType::BUFFER, move |pad, info| {
        // Nothing runs unless asked: one relaxed load and out.
        if !telemetry().wanted() {
            return gstreamer::PadProbeReturn::Ok;
        }
        let Some(gstreamer::PadProbeData::Buffer(buffer)) = &info.data else {
            return gstreamer::PadProbeReturn::Ok;
        };
        let Some(caps) = pad.current_caps() else { return gstreamer::PadProbeReturn::Ok };
        let mut cache = cached.lock();
        if !cache.as_ref().is_some_and(|(c, _)| c.is_strictly_equal(&caps)) {
            match gstreamer_video::VideoInfo::from_caps(&caps) {
                Ok(vi) => *cache = Some((caps.clone(), vi)),
                Err(_) => return gstreamer::PadProbeReturn::Ok,
            }
        }
        let Some((_, video_info)) = cache.as_ref() else {
            return gstreamer::PadProbeReturn::Ok;
        };
        if let Some(grid) = grid_of(buffer, video_info) {
            drop(cache);
            telemetry().absorb_grid(grid);
        }
        gstreamer::PadProbeReturn::Ok
    });
}

/// Sample a 16 by 9 grid of luma out of a mapped frame.
///
/// Planar and semi-planar YUV, which is what the canvas is (I420), read plane
/// zero directly. Packed RGB is weighted with the BT.709 coefficients. A
/// format on GPU memory, or one with no readable luma, answers `None` and the
/// telemetry tick simply carries the numbers it does have.
fn grid_of(
    buffer: &gstreamer::BufferRef,
    info: &gstreamer_video::VideoInfo,
) -> Option<[u8; GRID]> {
    use gstreamer_video::prelude::VideoFrameExt;
    let frame = gstreamer_video::VideoFrameRef::from_buffer_ref_readable(buffer, info).ok()?;
    let format = info.format_info();
    let width = info.width() as usize;
    let height = info.height() as usize;
    if width < GRID_W || height < GRID_H {
        return None;
    }
    let plane = frame.plane_data(0).ok()?;
    let stride = frame.plane_stride().first().copied()? as usize;
    if stride == 0 {
        return None;
    }
    let pixel_stride = format.pixel_stride()[0].max(1) as usize;
    let rgb = format.is_rgb();
    if rgb && pixel_stride < 3 {
        return None;
    }
    let mut grid = [0u8; GRID];
    for gy in 0..GRID_H {
        // The centre of each cell rather than its corner, so a grid over a
        // letterboxed picture is not all bars.
        let y = (gy * 2 + 1) * height / (GRID_H * 2);
        let row = y * stride;
        for gx in 0..GRID_W {
            let x = (gx * 2 + 1) * width / (GRID_W * 2);
            let at = row + x * pixel_stride;
            grid[gy * GRID_W + gx] = if rgb {
                let (r, g, b) =
                    (*plane.get(at)? as f64, *plane.get(at + 1)? as f64, *plane.get(at + 2)? as f64);
                // BT.709 luma, into studio swing so it matches the I420 path.
                (16.0 + (0.2126 * r + 0.7152 * g + 0.0722 * b) * 219.0 / 255.0) as u8
            } else {
                *plane.get(at)?
            };
        }
    }
    Some(grid)
}

/// One `level` message off the bus, handed straight to the audio probe.
///
/// Called from the bus watcher in `gstutil`, which is the only place a `level`
/// message is seen. Returns at once unless a client has asked for telemetry
/// and the message came from the programme meter.
pub fn note_level(src: Option<&str>, structure: &gstreamer::StructureRef) {
    if src != Some(PROGRAMME_METER) || !telemetry().wanted() {
        return;
    }
    let Ok(array) = structure.get::<glib::ValueArray>("rms") else { return };
    let rms: Vec<f64> = array.iter().filter_map(|v| v.get::<f64>().ok()).collect();
    telemetry().absorb_rms(&rms);
}

#[cfg(test)]
mod tests {
    use super::*;

    fn grid(value: u8) -> [u8; GRID] {
        [value; GRID]
    }

    /// The acceptance number from 09 section 5 item 11 and 07 Phase 2: a tick
    /// at eight sources is under 50 tokens, which this measures in bytes.
    #[test]
    fn a_tick_at_eight_sources_is_under_two_hundred_bytes() {
        let sources: std::collections::BTreeMap<String, u8> = (1..=8)
            .map(|i| (format!("cam{i}"), u8::from(i % 2 == 0)))
            .collect();
        let reading = Reading {
            shot: 0.0312345,
            black: 0.0,
            still_ms: Some(20),
            identical: false,
            lufs_s: Some(-21.34),
            lufs_i: Some(-22.07),
            silent_ms: Some(0),
            frames: 900,
        };
        let tick = reading.tick(sources, DEFAULT_FREEZE_MS, DEFAULT_SILENCE_MS);
        let text = serde_json::to_string(&tick).unwrap();
        assert!(text.len() < 200, "{} bytes: {text}", text.len());
        assert_eq!(tick.shot, 0.031);
        assert_eq!(tick.lufs_s, Some(-21.3));
        assert!(!tick.freeze);
        assert!(!tick.silence);
        assert_eq!(tick.sources["cam2"], 1);
        assert_eq!(tick.sources["cam1"], 0);
    }

    #[test]
    fn a_cut_scores_near_one_and_a_still_camera_near_zero() {
        let t = Telemetry::new();
        let _lease = t.lease();
        t.absorb_grid(grid(16));
        assert_eq!(t.read().shot, 0.0, "the first frame has nothing to compare to");
        t.absorb_grid(grid(235));
        let reading = t.read();
        assert!(reading.shot > 0.8, "a cut from black to white: {}", reading.shot);
        t.absorb_grid(grid(235));
        assert_eq!(t.read().shot, 0.0);
        t.absorb_grid(grid(236));
        assert!(t.read().shot < 0.01, "one step of luma is not a shot change");
    }

    #[test]
    fn black_is_the_fraction_of_the_picture_at_or_below_black() {
        let t = Telemetry::new();
        let _lease = t.lease();
        t.absorb_grid(grid(16));
        assert_eq!(t.read().black, 1.0);
        let mut half = grid(16);
        half[..GRID / 2].fill(200);
        t.absorb_grid(half);
        assert!((t.read().black - 0.5).abs() < 1e-9);
        t.absorb_grid(grid(200));
        assert_eq!(t.read().black, 0.0);
    }

    #[test]
    fn a_picture_that_stops_changing_freezes_and_one_that_moves_does_not() {
        let t = Telemetry::new();
        let _lease = t.lease();
        t.absorb_grid(grid(100));
        assert!(!t.read().freeze(0), "one frame is not a freeze");
        t.absorb_grid(grid(100));
        let reading = t.read();
        assert!(reading.identical);
        assert!(reading.freeze(0));
        assert!(!reading.freeze(500), "it has only just stopped");

        // One changed sample is enough to say the picture is alive.
        let mut moved = grid(100);
        moved[0] = 101;
        t.absorb_grid(moved);
        assert!(!t.read().freeze(0));
    }

    #[test]
    fn loudness_is_none_until_the_programme_makes_a_sound() {
        let t = Telemetry::new();
        let _lease = t.lease();
        let reading = t.read();
        assert_eq!(reading.lufs_s, None);
        assert_eq!(reading.lufs_i, None);
        assert!(!reading.silence(500), "silence before any audio is not silence");

        // Full scale on two channels reads near 0 LUFS.
        t.absorb_rms(&[-3.0, -3.0]);
        let reading = t.read();
        let short = reading.lufs_s.expect("short term");
        assert!(short > -1.5 && short < 0.5, "{short}");
        assert!(reading.lufs_i.is_some());
        assert!(!reading.silence(0));

        // Nothing at all is below the floor, and the integrated figure does
        // not fall towards it, because a gated block does not count.
        let before = t.read().lufs_i.unwrap();
        t.absorb_rms(&[-120.0, -120.0]);
        assert!((t.read().lufs_i.unwrap() - before).abs() < 1e-9, "a silent block is gated out");
        assert!(t.read().silence(0), "and the silence flag follows the level");
    }

    /// Nothing runs unless asked, and what was measured while somebody was
    /// looking is forgotten when they stop.
    #[test]
    fn the_probes_stop_and_forget_when_the_last_client_goes() {
        let t = Telemetry::new();
        assert!(!t.wanted());
        let lease = t.lease();
        assert!(t.wanted());
        t.absorb_grid(grid(50));
        t.absorb_grid(grid(50));
        assert_eq!(t.read().frames, 2);
        drop(lease);
        assert!(!t.wanted());
        assert_eq!(t.read().frames, 0, "a later subscriber starts from nothing");
        assert_eq!(t.read().still_ms, None);
    }

    /// The flash guard's only source of truth. A cut from black to white over
    /// the whole frame is a flash; a step of one is not.
    #[test]
    fn a_luminance_step_over_the_frame_tells_the_flash_guard() {
        let t = Telemetry::new();
        let guard = crate::safety::Guard::new(
            crate::safety::SafetyConfig { min_hold_ms: 0, ..Default::default() },
            30,
        );
        t.bind_guard(guard.clone());
        let _lease = t.lease();
        t.absorb_grid(grid(16));
        t.absorb_grid(grid(235));
        let refusal = guard.check(&godwinmix_protocol::scope::Token::open()).unwrap_err();
        assert_eq!(refusal.rule, "flash_guard");

        // A gentle change is not a flash, so a second core stays open.
        let quiet = Telemetry::new();
        let other = crate::safety::Guard::new(
            crate::safety::SafetyConfig { min_hold_ms: 0, ..Default::default() },
            30,
        );
        quiet.bind_guard(other.clone());
        let _lease = quiet.lease();
        quiet.absorb_grid(grid(120));
        quiet.absorb_grid(grid(124));
        assert!(other.check(&godwinmix_protocol::scope::Token::open()).is_ok());
    }
}
