//! Still pictures and a compact state document for agents.
//!
//! An AI agent steering the mixer cannot sit on the WebSocket and watch the
//! mosaic go by; every frame it looks at costs tokens. What it can afford is a
//! single JPEG when it decides it needs to see something, and a short JSON
//! document that tells it where to look. This module supplies both, and it
//! does so without asking the pipelines for anything new: the mosaic is
//! already encoded once per frame for the operator, so the freshest frame is
//! kept here and cut up on demand.
//!
//! The motion score is the part that saves the most. Two consecutive mosaics
//! are decoded to luma and compared cell by cell, which gives a number per
//! source for "how much is changing here" at the cost of one JPEG decode per
//! mosaic frame.
//!
//! # Nothing runs unless asked
//!
//! That decode is the most expensive thing the core does on behalf of a client
//! that may not be there, so the tracker follows the mosaic only while
//! something is asking: a snapshot request, an `agent.state` read, or a
//! subscriber watching agent thresholds. Each of those calls [`Tracker::want`],
//! which starts the follower if it is not running and keeps it running for
//! `[snapshot] idle_secs` afterwards. When that expires the follower drops its
//! multiview subscription (which lets the mosaic go too, if nothing else wants
//! it) and forgets the frame it was holding, so the tracker costs no CPU and
//! no memory until the next ask. With `[snapshot] enabled = false` there is no
//! follower at all and the routes answer 404 naming the switch.

use crate::config::SnapshotConfig;
use crate::mixer::MixerHandle;
use crate::multiview::{MultiviewHandle, MultiviewRequest};
use crate::state::{BackendInfo, CellAssignment, MixerStatus, OutputState, SourceId, SourceState};
use image::{GrayImage, ImageFormat, RgbImage};
use parking_lot::{Mutex, RwLock};
use serde::Serialize;
use std::collections::HashMap;
use std::sync::atomic::{AtomicBool, AtomicU64, Ordering};
use std::sync::Arc;
use std::time::{Duration, Instant};
use tokio::sync::broadcast;
use tracing::{debug, info, warn};

/// Quality for the JPEGs this module encodes itself. The mosaic arrives at
/// the operator's configured quality; cropping and re-encoding at a lower one
/// would throw away detail twice, so this sits a little above the default.
const JPEG_QUALITY: u8 = 80;

/// The smallest picture `?width=` will produce. Anything narrower is not a
/// picture anybody can read a scoreboard off, and a zero would be a panic in
/// the resizer.
const MIN_WIDTH: u32 = 16;

/// The newest mosaic frame together with the layout it was drawn with and
/// the motion measured against the frame before it.
#[derive(Debug, Clone)]
pub struct Latest {
    pub jpeg: Arc<[u8]>,
    /// The cells as the mixer reported them when this frame arrived. The
    /// layout is read once per frame rather than once per request so that a
    /// crop and the motion score for it describe the same rectangle.
    pub cells: Vec<CellAssignment>,
    /// Motion per cell, indexed like `cells`. `None` until a second frame has
    /// been seen, and again for one frame after the mosaic changes size.
    pub motion: Option<Vec<f64>>,
}

/// Why a snapshot request cannot be answered, in the words the client sees.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Refusal {
    /// Wider than `[snapshot] max_width` and no `allow_large` on the request.
    TooWide { asked: u32, max: u32 },
    /// Inside `[snapshot] min_interval_secs` of this client's last one.
    TooSoon { retry_after_secs: u64 },
}

impl Refusal {
    /// Every error names the next step, because an agent reads this and has to
    /// decide what to do without asking anybody.
    pub fn message(&self) -> String {
        match self {
            Refusal::TooWide { asked, max } => format!(
                "a {asked} pixel wide snapshot is above the {max} pixel ceiling in \
                 [snapshot] max_width. Ask for width={max} or less, or repeat the \
                 request with allow_large=true if you really want the big one."
            ),
            Refusal::TooSoon { retry_after_secs } => format!(
                "one snapshot per client per [snapshot] min_interval_secs. Wait \
                 {retry_after_secs}s and ask again, read /api/agent/state for motion \
                 in the meantime, or repeat the request with force=true."
            ),
        }
    }
}

/// What a client asked for, before the limits are applied.
#[derive(Debug, Clone, Copy, Default)]
pub struct Ask {
    /// `None` means the configured default (320). `Some(0)` means the cell at
    /// its own size, which is the old behaviour and still available.
    pub width: Option<u32>,
    /// Skip the rate limit.
    pub force: bool,
    /// Permit a width above `[snapshot] max_width`.
    pub allow_large: bool,
}

/// Keeps `Latest` up to date from the mosaic, but only while something is
/// asking. One of these lives in the control server for as long as it runs.
pub struct Tracker {
    latest: RwLock<Option<Latest>>,
    cfg: SnapshotConfig,
    mv: MultiviewHandle,
    /// `None` in tests, where there is no mixer to read the layout from.
    mixer: Option<MixerHandle>,
    /// Whether a follower task exists. Changed only under `until`.
    running: AtomicBool,
    /// When the follower gives up, extended by every ask.
    until: Mutex<Instant>,
    /// Last snapshot per client, for the rate limit.
    last_served: Mutex<HashMap<String, Instant>>,
    /// How many times the follower has been started. A metric, and what the
    /// tests assert on to prove it is not running when nobody is asking.
    starts: AtomicU64,
}

impl Tracker {
    /// A tracker that will follow the mosaic when somebody asks it to.
    /// Nothing is started here: `want` does that.
    pub fn new(cfg: SnapshotConfig, mv: MultiviewHandle, mixer: MixerHandle) -> Arc<Self> {
        Self::build(cfg, mv, Some(mixer))
    }

    fn build(cfg: SnapshotConfig, mv: MultiviewHandle, mixer: Option<MixerHandle>) -> Arc<Self> {
        Arc::new(Self {
            latest: RwLock::new(None),
            cfg,
            mv,
            mixer,
            running: AtomicBool::new(false),
            until: Mutex::new(Instant::now()),
            last_served: Mutex::new(HashMap::new()),
            starts: AtomicU64::new(0),
        })
    }

    /// A tracker with nothing behind it, for tests.
    #[cfg(test)]
    pub fn disabled() -> Arc<Self> {
        Self::build(
            SnapshotConfig { enabled: false, ..Default::default() },
            MultiviewHandle::detached(
                crate::config::MultiviewConfig { enabled: false, ..Default::default() },
                tokio::runtime::Handle::current(),
            ),
            None,
        )
    }

    /// Whether a snapshot could ever be produced. False when either switch is
    /// off; `disabled_reason` says which.
    pub fn enabled(&self) -> bool {
        self.cfg.enabled && self.mv.enabled()
    }

    /// The 404 text for a switched off snapshot path, naming the switch that
    /// turned it off and what to do about it.
    pub fn disabled_reason(&self) -> Option<String> {
        if !self.cfg.enabled {
            return Some(
                "snapshots are switched off by [snapshot] enabled = false in the mixer's \
                 config. Set it to true and restart to get stills and motion back."
                    .into(),
            );
        }
        if !self.mv.enabled() {
            return Some(
                "stills are cut out of the mosaic, and the mosaic is switched off by \
                 [multiview] enabled = false in the mixer's config. Set it to true and \
                 restart, or read /api/agent/state, which works without pictures."
                    .into(),
            );
        }
        None
    }

    pub fn cfg(&self) -> &SnapshotConfig {
        &self.cfg
    }

    /// How many times the follower has been started since boot.
    pub fn starts(&self) -> u64 {
        self.starts.load(Ordering::Relaxed)
    }

    /// Whether the follower is running right now.
    pub fn following(&self) -> bool {
        self.running.load(Ordering::SeqCst)
    }

    /// The newest frame if one is being held. Does not ask for one: a caller
    /// that wants the tracker running calls `want` first, or uses
    /// `latest_wanted`.
    pub fn latest(&self) -> Option<Latest> {
        self.latest.read().clone()
    }

    /// Say that something wants motion and stills for the next little while.
    /// The first ask starts the follower, which takes a multiview subscription
    /// and so builds the mosaic if it is not up.
    pub fn want(self: &Arc<Self>) {
        if !self.enabled() {
            return;
        }
        let idle = Duration::from_secs(self.cfg.idle_secs.max(1));
        // Both the deadline and the running flag move under this lock, so an
        // ask that lands while the follower is giving up is never lost.
        let mut until = self.until.lock();
        *until = Instant::now() + idle;
        let was_running = self.running.swap(true, Ordering::SeqCst);
        drop(until);
        if !was_running {
            self.starts.fetch_add(1, Ordering::Relaxed);
            let me = self.clone();
            tokio::spawn(me.follow());
        }
    }

    /// The newest frame, having said that somebody wants one. Returns as soon
    /// as there is a frame, or when `wait` runs out: the first request after a
    /// quiet spell has to wait for the mosaic to be built and to produce a
    /// frame, which is why this is not simply `latest`.
    pub async fn latest_wanted(self: &Arc<Self>, wait: Duration) -> Option<Latest> {
        let deadline = Instant::now() + wait;
        loop {
            // Said again on every turn, not once at the top. `[snapshot]
            // idle_secs` is how long the follower keeps going with nobody
            // asking, and on a loaded machine the mosaic can take longer than
            // that to build and produce its first frame: the follower then
            // gave up while the one caller that wanted a frame was still
            // waiting for it, and the wait ran out against a tracker that had
            // stopped following. Wanting something for ten seconds is wanting
            // it at the end of the ten seconds.
            self.want();
            if let Some(l) = self.latest() {
                return Some(l);
            }
            if Instant::now() >= deadline || !self.enabled() {
                return None;
            }
            tokio::time::sleep(Duration::from_millis(40)).await;
        }
    }

    /// Apply the size and rate limits to a request. `Ok(None)` means the cell
    /// at its own size.
    pub fn resolve(&self, client: &str, ask: &Ask) -> Result<Option<u32>, Refusal> {
        let width = match ask.width {
            None => Some(self.cfg.default_width),
            Some(0) => None,
            Some(w) => Some(w),
        };
        if let Some(w) = width {
            if w > self.cfg.max_width && !ask.allow_large {
                return Err(Refusal::TooWide { asked: w, max: self.cfg.max_width });
            }
        }
        let interval = Duration::from_secs(self.cfg.min_interval_secs);
        if interval.is_zero() || ask.force {
            return Ok(width);
        }
        let now = Instant::now();
        let mut last = self.last_served.lock();
        if let Some(prev) = last.get(client) {
            let waited = now.saturating_duration_since(*prev);
            if waited < interval {
                let left = interval - waited;
                return Err(Refusal::TooSoon {
                    retry_after_secs: left.as_secs().max(1),
                });
            }
        }
        // A client id is a peer address or a token, so the map is bounded by
        // the number of clients. Old entries still go, or a long running mixer
        // would remember every browser that ever connected.
        if last.len() > 256 {
            last.retain(|_, t| now.saturating_duration_since(*t) < interval * 4);
        }
        last.insert(client.to_string(), now);
        Ok(width)
    }

    async fn follow(self: Arc<Self>) {
        let Some(mixer) = self.mixer.clone() else { return };
        // The subscription is what holds the mosaic up. Dropping it at the end
        // of this function is what lets the mosaic go.
        let mut sub = self.mv.subscribe(MultiviewRequest::configured());
        info!("snapshot tracker following the mosaic");
        // Luma of the previous frame, kept only for the difference.
        let mut prev: Option<GrayImage> = None;
        loop {
            let left = {
                let until = self.until.lock();
                until.saturating_duration_since(Instant::now())
            };
            if left.is_zero() {
                // Nobody has asked for a while. Put it down, under the same
                // lock `want` takes, so an ask arriving now is not lost.
                let until = self.until.lock();
                if Instant::now() < *until {
                    continue;
                }
                self.running.store(false, Ordering::SeqCst);
                drop(until);
                *self.latest.write() = None;
                info!("snapshot tracker idle, letting the mosaic go");
                return;
            }
            let jpeg = match tokio::time::timeout(left, sub.recv()).await {
                Err(_) => continue,
                Ok(Ok(f)) => f,
                Ok(Err(broadcast::error::RecvError::Lagged(n))) => {
                    // Falling behind means the decode is slower than the
                    // mosaic. The next frame is still the one to score, only
                    // the difference now spans more than one interval.
                    debug!(skipped = n, "snapshot tracker fell behind on mosaic frames");
                    continue;
                }
                Ok(Err(broadcast::error::RecvError::Closed)) => break,
            };
            let cells = match mixer.status().await {
                Ok(s) => s.multiview.cells,
                Err(e) => {
                    warn!(?e, "snapshot tracker could not read the layout");
                    break;
                }
            };

            // Decoding a 960x540 JPEG takes a few milliseconds, which is too
            // long to hold an async worker for at eight frames a second.
            let bytes = jpeg.clone();
            let last = prev.take();
            let scored = tokio::task::spawn_blocking(move || {
                let cur = image::load_from_memory_with_format(&bytes, ImageFormat::Jpeg)
                    .map(|img| img.to_luma8())?;
                let motion = last
                    .as_ref()
                    .filter(|p| p.dimensions() == cur.dimensions())
                    .map(|p| cells.iter().map(|c| cell_motion(p, &cur, c)).collect());
                Ok::<_, image::ImageError>((cur, cells, motion))
            })
            .await;

            match scored {
                Ok(Ok((cur, cells, motion))) => {
                    prev = Some(cur);
                    *self.latest.write() = Some(Latest { jpeg, cells, motion });
                }
                Ok(Err(e)) => {
                    // A frame that does not decode is skipped. The raw bytes
                    // are still the newest picture, so keep serving them and
                    // wait for the next frame to score against.
                    warn!(?e, "mosaic frame did not decode");
                    if let Some(l) = self.latest.write().as_mut() {
                        l.jpeg = jpeg;
                        l.motion = None;
                    }
                }
                Err(e) => warn!(?e, "snapshot decode task failed"),
            }
        }
        // Only the error paths get here. Give the flag back so the next ask
        // starts a fresh follower rather than waiting on a dead one.
        self.running.store(false, Ordering::SeqCst);
        *self.latest.write() = None;
    }
}

/// The rectangle of a cell, clamped to the picture it is cut from. The
/// mixer's layout never overflows the mosaic, but a frame from just before a
/// relayout can be paired with cells from just after it.
fn cell_rect(width: u32, height: u32, c: &CellAssignment) -> (u32, u32, u32, u32) {
    let x = c.x.max(0) as u32;
    let y = c.y.max(0) as u32;
    let x = x.min(width);
    let y = y.min(height);
    let w = (c.w.max(0) as u32).min(width - x);
    let h = (c.h.max(0) as u32).min(height - y);
    (x, y, w, h)
}

/// Cut one cell out of a decoded mosaic.
pub fn crop_cell(mosaic: &RgbImage, c: &CellAssignment) -> RgbImage {
    let (x, y, w, h) = cell_rect(mosaic.width(), mosaic.height(), c);
    image::imageops::crop_imm(mosaic, x, y, w, h).to_image()
}

/// Mean absolute luma difference over a cell, scaled to 0.0 to 1.0. A still
/// picture scores about zero even through JPEG noise; full frame video sits
/// somewhere between 0.02 and 0.2; a cut to a different shot spikes higher.
pub fn cell_motion(prev: &GrayImage, cur: &GrayImage, c: &CellAssignment) -> f64 {
    let (x, y, w, h) = cell_rect(cur.width().min(prev.width()), cur.height().min(prev.height()), c);
    if w == 0 || h == 0 {
        return 0.0;
    }
    let mut sum: u64 = 0;
    for yy in y..y + h {
        for xx in x..x + w {
            let a = prev.get_pixel(xx, yy).0[0] as i32;
            let b = cur.get_pixel(xx, yy).0[0] as i32;
            sum += (a - b).unsigned_abs() as u64;
        }
    }
    sum as f64 / (w as f64 * h as f64 * 255.0)
}

/// Shrink to `width` pixels across, keeping the aspect, never enlarging.
pub fn fit_width(img: RgbImage, width: Option<u32>) -> RgbImage {
    let Some(target) = width else { return img };
    let target = target.max(MIN_WIDTH);
    if target >= img.width() {
        return img;
    }
    let h = ((img.height() as u64 * target as u64 + img.width() as u64 / 2) / img.width() as u64)
        .max(1) as u32;
    image::imageops::resize(&img, target, h, image::imageops::FilterType::Triangle)
}

pub fn encode_jpeg(img: &RgbImage) -> Result<Vec<u8>, image::ImageError> {
    let mut out = Vec::new();
    let enc = image::codecs::jpeg::JpegEncoder::new_with_quality(&mut out, JPEG_QUALITY);
    img.write_with_encoder(enc)?;
    Ok(out)
}

pub fn decode_jpeg(bytes: &[u8]) -> Result<RgbImage, image::ImageError> {
    Ok(image::load_from_memory_with_format(bytes, ImageFormat::Jpeg)?.into_rgb8())
}

/// Which picture a snapshot request is after.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum Pick {
    /// The whole mosaic.
    Sheet,
    /// The programme return cell.
    Program,
    Source(SourceId),
}

/// The file name of a snapshot request, `cam1.jpg`, `program.jpg` or
/// `sheet.jpg`, or `None` for anything else. The suffix is required so that
/// a source id without it stays a 404 rather than an accidental alias.
pub fn parse_pick(name: &str) -> Option<Pick> {
    let stem = name.strip_suffix(".jpg")?;
    match stem {
        "" => None,
        "sheet" => Some(Pick::Sheet),
        "program" => Some(Pick::Program),
        id => Some(Pick::Source(id.to_string())),
    }
}

/// The cell for a pick, or `None` when the layout has no such cell.
pub fn find_cell<'a>(cells: &'a [CellAssignment], pick: &Pick) -> Option<&'a CellAssignment> {
    match pick {
        Pick::Sheet => None,
        Pick::Program => cells.iter().find(|c| c.source.is_none()),
        Pick::Source(id) => cells.iter().find(|c| c.source.as_deref() == Some(id.as_str())),
    }
}

/// Everything an agent needs to decide what to do next, and nothing it does
/// not. Field names are the same ones `/api/status` uses so that an agent
/// reading both is not confused by two spellings, and URIs and queue depths
/// are left out because they are noise for the decision this is meant to
/// support.
#[derive(Debug, Clone, Serialize)]
pub struct AgentState {
    pub program: Option<SourceId>,
    /// Motion in the programme return cell, `None` when the mosaic does not
    /// carry one or no two frames have been compared yet.
    pub program_motion: Option<f64>,
    pub uptime_secs: u64,
    pub sources: Vec<AgentSource>,
    pub outputs: Vec<AgentOutput>,
    pub backend: BackendInfo,
    /// `None` while multiview is disabled, because then the URLs all 404.
    pub snapshots: Option<SnapshotUrls>,
}

#[derive(Debug, Clone, Serialize)]
pub struct AgentSource {
    pub id: SourceId,
    pub name: String,
    pub state: SourceState,
    pub superimposed: bool,
    pub has_video: bool,
    pub has_audio: bool,
    pub video_idle_ms: Option<u64>,
    /// 0.0 to 1.0, see `cell_motion`. `None` when the source has no cell or
    /// no score has been computed yet.
    pub motion: Option<f64>,
}

#[derive(Debug, Clone, Serialize)]
pub struct AgentOutput {
    pub id: String,
    pub state: OutputState,
    pub reconnects: u32,
}

#[derive(Debug, Clone, Serialize)]
pub struct SnapshotUrls {
    pub sheet: &'static str,
    pub program: &'static str,
    /// A pattern: substitute the source id for `{source_id}`.
    pub source: &'static str,
}

pub const SHEET_URL: &str = "/api/snapshot/sheet.jpg";
pub const PROGRAM_URL: &str = "/api/snapshot/program.jpg";
pub const SOURCE_URL: &str = "/api/snapshot/{source_id}.jpg";

/// Fold a status snapshot and the latest motion scores into one document.
///
/// `stills` is false when either switch is off, and then the URLs are `null`
/// rather than three addresses that would all answer 404.
pub fn agent_state(status: &MixerStatus, latest: Option<&Latest>, stills: bool) -> AgentState {
    // Scores are keyed by the layout the frame was drawn with, not by the
    // `cell` index in `status`, since the two can differ across a relayout.
    let score = |pick: Pick| -> Option<f64> {
        let l = latest?;
        let motion = l.motion.as_ref()?;
        let c = find_cell(&l.cells, &pick)?;
        motion.get(c.index as usize).copied().map(round3)
    };
    AgentState {
        program: status.program.clone(),
        program_motion: score(Pick::Program),
        uptime_secs: status.uptime_secs,
        sources: status
            .sources
            .iter()
            .map(|s| AgentSource {
                id: s.id.clone(),
                name: s.name.clone(),
                state: s.state,
                superimposed: s.superimposed(),
                has_video: s.has_video,
                has_audio: s.has_audio,
                video_idle_ms: s.video_idle_ms,
                motion: score(Pick::Source(s.id.clone())),
            })
            .collect(),
        outputs: status
            .outputs
            .iter()
            .map(|o| AgentOutput { id: o.id.clone(), state: o.state, reconnects: o.reconnects })
            .collect(),
        backend: status.backend.clone(),
        snapshots: (stills && status.multiview.enabled).then_some(SnapshotUrls {
            sheet: SHEET_URL,
            program: PROGRAM_URL,
            source: SOURCE_URL,
        }),
    }
}

/// Three decimals is as fine as JPEG noise lets the score be, and it keeps
/// the document short.
fn round3(v: f64) -> f64 {
    (v * 1000.0).round() / 1000.0
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::state::{MultiviewStatus, OutputStatus, SourceStatus};
    use image::{Luma, Rgb};

    fn cell(index: u32, source: Option<&str>, x: i32, y: i32, w: i32, h: i32) -> CellAssignment {
        CellAssignment { index, source: source.map(str::to_string), x, y, w, h }
    }

    /// A mosaic with a distinct flat colour in each quadrant.
    fn quadrants() -> RgbImage {
        RgbImage::from_fn(8, 6, |x, y| match (x < 4, y < 3) {
            (true, true) => Rgb([255, 0, 0]),
            (false, true) => Rgb([0, 255, 0]),
            (true, false) => Rgb([0, 0, 255]),
            (false, false) => Rgb([255, 255, 255]),
        })
    }

    #[test]
    fn cropping_a_cell_returns_exactly_that_cell() {
        let m = quadrants();
        let green = crop_cell(&m, &cell(1, Some("cam1"), 4, 0, 4, 3));
        assert_eq!(green.dimensions(), (4, 3));
        assert!(green.pixels().all(|p| *p == Rgb([0, 255, 0])));
        let white = crop_cell(&m, &cell(3, Some("cam3"), 4, 3, 4, 3));
        assert!(white.pixels().all(|p| *p == Rgb([255, 255, 255])));
    }

    #[test]
    fn a_cell_that_overhangs_the_picture_is_clamped_not_panicked() {
        let m = quadrants();
        let c = crop_cell(&m, &cell(0, None, 6, 4, 10, 10));
        assert_eq!(c.dimensions(), (2, 2));
        // Entirely outside: an empty picture rather than a crash.
        let c = crop_cell(&m, &cell(0, None, 20, 20, 4, 4));
        assert_eq!(c.dimensions(), (0, 0));
        assert_eq!(cell_rect(8, 6, &cell(0, None, -3, -3, 4, 4)), (0, 0, 4, 4));
    }

    #[test]
    fn identical_frames_score_zero_and_a_flip_scores_one() {
        let dark = GrayImage::from_pixel(8, 6, Luma([0]));
        let light = GrayImage::from_pixel(8, 6, Luma([255]));
        let c = cell(0, None, 0, 0, 4, 3);
        assert_eq!(cell_motion(&dark, &dark, &c), 0.0);
        assert_eq!(cell_motion(&dark, &light, &c), 1.0);
        // Half the cell changing by the full range scores a half.
        let half = GrayImage::from_fn(8, 6, |x, _| Luma([if x < 2 { 255 } else { 0 }]));
        assert!((cell_motion(&dark, &half, &c) - 0.5).abs() < 1e-9);
        // Only the named cell is looked at.
        let other = cell(1, Some("cam1"), 4, 0, 4, 3);
        assert_eq!(cell_motion(&dark, &half, &other), 0.0);
    }

    #[test]
    fn motion_is_confined_to_the_overlap_and_an_empty_cell_scores_zero() {
        let a = GrayImage::from_pixel(8, 6, Luma([0]));
        let b = GrayImage::from_pixel(8, 6, Luma([255]));
        assert_eq!(cell_motion(&a, &b, &cell(0, None, 8, 6, 4, 4)), 0.0);
        assert_eq!(cell_motion(&a, &b, &cell(0, None, 0, 0, 0, 3)), 0.0);
    }

    #[test]
    fn fit_width_shrinks_with_aspect_and_never_grows() {
        let img = RgbImage::new(480, 270);
        assert_eq!(fit_width(img.clone(), None).dimensions(), (480, 270));
        assert_eq!(fit_width(img.clone(), Some(240)).dimensions(), (240, 135));
        assert_eq!(fit_width(img.clone(), Some(4000)).dimensions(), (480, 270));
        assert_eq!(fit_width(img.clone(), Some(1)).dimensions(), (MIN_WIDTH, 9));
    }

    #[test]
    fn a_crop_survives_a_trip_through_jpeg() {
        let m = quadrants();
        let big = image::imageops::resize(&m, 64, 48, image::imageops::FilterType::Nearest);
        let bytes = encode_jpeg(&big).unwrap();
        assert_eq!(&bytes[..2], &[0xFF, 0xD8], "not a JPEG");
        let back = decode_jpeg(&bytes).unwrap();
        assert_eq!(back.dimensions(), (64, 48));
        // Solid red should still be recognisably red after the codec.
        let p = back.get_pixel(8, 8);
        assert!(p[0] > 200 && p[1] < 60 && p[2] < 60, "got {p:?}");
    }

    #[test]
    fn snapshot_names_parse_only_with_the_suffix() {
        assert_eq!(parse_pick("sheet.jpg"), Some(Pick::Sheet));
        assert_eq!(parse_pick("program.jpg"), Some(Pick::Program));
        assert_eq!(parse_pick("cam1.jpg"), Some(Pick::Source("cam1".into())));
        assert_eq!(parse_pick("cam1"), None);
        assert_eq!(parse_pick(".jpg"), None);
        assert_eq!(parse_pick("cam1.png"), None);
    }

    fn status() -> MixerStatus {
        let source = |id: &str, cell: Option<u32>| SourceStatus {
            id: id.into(),
            name: id.to_uppercase(),
            uri: "rtmp://host/…".into(),
            state: SourceState::Live,
            has_video: true,
            has_audio: false,
            cell,
            video_idle_ms: Some(30),
            audio_idle_ms: None,
            gain: 1.0,
            muted: false,
            seekable: false,
            position_ms: None,
            duration_ms: None,
            extra: Default::default(),
        };
        MixerStatus {
            scene: None,
            program: Some("cam1".into()),
            sources: vec![source("cam1", Some(1)), source("cam2", Some(2))],
            outputs: vec![OutputStatus {
                id: "primary".into(),
                uri_host: "host".into(),
                state: OutputState::Live,
                reconnects: 2,
                queue_secs: 0.1,
                extra: Default::default(),
            }],
            multiview: MultiviewStatus {
                enabled: true,
                width: 960,
                height: 540,
                cols: 2,
                rows: 2,
                cells: vec![],
                fps: 8,
            },
            uptime_secs: 77,
            running_time_ms: 1000,
            backend: BackendInfo {
                video_decoder: "avdec_h264".into(),
                video_encoder: "x264enc".into(),
                audio_decoder: "avdec_aac".into(),
                audio_encoder: "avenc_aac".into(),
                hardware_accelerated: false,
            },
            ad: None,
        }
    }

    #[test]
    fn agent_state_has_the_shape_an_agent_is_told_to_expect() {
        let latest = Latest {
            jpeg: Arc::from(&[0xFF, 0xD8][..]),
            cells: vec![
                cell(0, None, 0, 0, 480, 270),
                cell(1, Some("cam1"), 480, 0, 480, 270),
                cell(2, Some("cam2"), 0, 270, 480, 270),
            ],
            motion: Some(vec![0.0512345, 0.1, 0.0]),
        };
        let v = serde_json::to_value(agent_state(&status(), Some(&latest), true)).unwrap();
        assert_eq!(v["program"], "cam1");
        assert_eq!(v["program_motion"], 0.051);
        assert_eq!(v["uptime_secs"], 77);
        assert_eq!(v["sources"].as_array().unwrap().len(), 2);
        let s0 = &v["sources"][0];
        assert_eq!(s0["id"], "cam1");
        assert_eq!(s0["name"], "CAM1");
        assert_eq!(s0["state"], "live");
        assert_eq!(s0["superimposed"], false);
        assert_eq!(s0["has_video"], true);
        assert_eq!(s0["has_audio"], false);
        assert_eq!(s0["video_idle_ms"], 30);
        assert_eq!(s0["motion"], 0.1);
        assert_eq!(v["sources"][1]["motion"], 0.0);
        // Nothing an agent does not need: no URIs, no queue depths.
        assert!(s0.get("uri").is_none());
        assert!(s0.get("cell").is_none());
        let o = &v["outputs"][0];
        assert_eq!(o["id"], "primary");
        assert_eq!(o["state"], "live");
        assert_eq!(o["reconnects"], 2);
        assert!(o.get("queue_secs").is_none());
        assert_eq!(v["backend"]["video_encoder"], "x264enc");
        assert_eq!(v["snapshots"]["sheet"], "/api/snapshot/sheet.jpg");
        assert_eq!(v["snapshots"]["program"], "/api/snapshot/program.jpg");
        assert_eq!(v["snapshots"]["source"], "/api/snapshot/{source_id}.jpg");
        // Exactly these top level keys and no others. Sorted, because
        // serde_json hands them back alphabetically.
        let keys: Vec<_> = v.as_object().unwrap().keys().cloned().collect();
        assert_eq!(
            keys,
            ["backend", "outputs", "program", "program_motion", "snapshots", "sources", "uptime_secs"]
        );
    }

    #[test]
    fn motion_is_null_until_it_exists_and_snapshots_null_without_multiview() {
        let mut st = status();
        let v = serde_json::to_value(agent_state(&st, None, true)).unwrap();
        assert!(v["program_motion"].is_null());
        assert!(v["sources"][0]["motion"].is_null());
        assert!(v["snapshots"].is_object());

        // A first frame has a layout but no score yet.
        let latest = Latest {
            jpeg: Arc::from(&[][..]),
            cells: vec![cell(1, Some("cam1"), 0, 0, 1, 1)],
            motion: None,
        };
        let v = serde_json::to_value(agent_state(&st, Some(&latest), true)).unwrap();
        assert!(v["sources"][0]["motion"].is_null());

        st.multiview.enabled = false;
        let v = serde_json::to_value(agent_state(&st, None, true)).unwrap();
        assert!(v["snapshots"].is_null());

        // And with the mosaic on but stills switched off, the same.
        st.multiview.enabled = true;
        let v = serde_json::to_value(agent_state(&st, None, false)).unwrap();
        assert!(v["snapshots"].is_null());
    }

    fn tracker_with(cfg: SnapshotConfig) -> Arc<Tracker> {
        Tracker::build(
            cfg,
            MultiviewHandle::detached(
                crate::config::MultiviewConfig::default(),
                tokio::runtime::Handle::current(),
            ),
            None,
        )
    }

    #[tokio::test]
    async fn the_default_snapshot_is_320_wide_and_a_big_one_needs_saying_so() {
        let t = tracker_with(SnapshotConfig::default());
        // Nothing asked for: the documented default, not the cell's own size.
        assert_eq!(t.resolve("a", &Ask::default()).unwrap(), Some(320));
        // Zero is how a client says "as it comes".
        assert_eq!(
            t.resolve("b", &Ask { width: Some(0), ..Default::default() }).unwrap(),
            None
        );
        assert_eq!(
            t.resolve("c", &Ask { width: Some(640), ..Default::default() }).unwrap(),
            Some(640)
        );
        // Above the ceiling without the flag, refused, and the message says
        // both ways out.
        let err = t
            .resolve("d", &Ask { width: Some(1920), ..Default::default() })
            .expect_err("1920 must be refused")
            .message();
        assert!(err.contains("1280"), "{err}");
        assert!(err.contains("allow_large"), "{err}");
        // With the flag, allowed.
        assert_eq!(
            t.resolve("e", &Ask { width: Some(1920), allow_large: true, ..Default::default() })
                .unwrap(),
            Some(1920)
        );
    }

    #[tokio::test]
    async fn one_snapshot_per_client_per_interval_unless_forced() {
        let t = tracker_with(SnapshotConfig::default());
        assert!(t.resolve("10.0.0.1", &Ask::default()).is_ok());
        let err = t.resolve("10.0.0.1", &Ask::default()).expect_err("the second is too soon");
        let msg = err.message();
        assert!(matches!(err, Refusal::TooSoon { .. }));
        assert!(msg.contains("force=true"), "{msg}");
        // Another client is another bucket.
        assert!(t.resolve("10.0.0.2", &Ask::default()).is_ok());
        // And force gets through.
        assert!(t.resolve("10.0.0.1", &Ask { force: true, ..Default::default() }).is_ok());

        // Zero turns the limit off for an operator who does not want it.
        let t = tracker_with(SnapshotConfig { min_interval_secs: 0, ..Default::default() });
        assert!(t.resolve("10.0.0.1", &Ask::default()).is_ok());
        assert!(t.resolve("10.0.0.1", &Ask::default()).is_ok());
    }

    #[tokio::test]
    async fn a_switched_off_snapshot_says_which_switch_did_it() {
        let off = tracker_with(SnapshotConfig { enabled: false, ..Default::default() });
        let why = off.disabled_reason().expect("must refuse");
        assert!(why.contains("[snapshot] enabled = false"), "{why}");
        assert!(!off.enabled());

        let no_mosaic = Tracker::build(
            SnapshotConfig::default(),
            MultiviewHandle::detached(
                crate::config::MultiviewConfig { enabled: false, ..Default::default() },
                tokio::runtime::Handle::current(),
            ),
            None,
        );
        let why = no_mosaic.disabled_reason().expect("must refuse");
        assert!(why.contains("[multiview] enabled = false"), "{why}");

        let on = tracker_with(SnapshotConfig::default());
        assert!(on.disabled_reason().is_none());
        assert!(on.enabled());
    }

    /// Asking a switched off tracker for anything starts nothing at all.
    #[tokio::test]
    async fn a_disabled_tracker_never_starts_a_follower() {
        let t = Tracker::disabled();
        t.want();
        assert!(!t.following());
        assert_eq!(t.starts(), 0);
        assert!(t.latest_wanted(Duration::from_millis(50)).await.is_none());
    }

    /// The whole chain, against a real mixer: no tracker and no mosaic until
    /// something asks, both when it does, and both gone again afterwards.
    #[tokio::test(flavor = "multi_thread")]
    async fn the_tracker_and_the_mosaic_come_and_go_with_the_asking() {
        let _ = gstreamer::init();
        let mut cfg: crate::config::Config = toml::from_str("").unwrap();
        cfg.canvas = crate::config::Canvas {
            width: 320,
            height: 180,
            fps: 15,
            sample_rate: 48000,
            channels: 2,
        };
        cfg.multiview = crate::config::MultiviewConfig {
            width: 320,
            height: 180,
            linger_secs: 1,
            ..Default::default()
        };
        let (mut mix, handle, cmd_rx, _bus_rx) = crate::mixer::Mixer::build(cfg).unwrap();
        mix.start().unwrap();
        let mv = mix.multiview_handle();
        let thread = crate::mixer::spawn(mix, cmd_rx, handle.clone());

        let tracker = Tracker::new(
            SnapshotConfig { idle_secs: 1, ..Default::default() },
            mv.clone(),
            handle.clone(),
        );
        assert!(!tracker.following(), "the tracker is running before anybody asked");
        assert_eq!(mv.live_pipelines(), 0, "a mosaic before anybody asked");
        assert!(tracker.latest().is_none());

        // Ten rather than four. On its own this takes 2.1 seconds; beside the
        // rest of the suite, with two dozen other pipelines going to PLAYING
        // on the same eight cores, four was a measurement of the build
        // machine's load. Ten is enough now that `latest_wanted` keeps saying
        // it wants one for the whole wait rather than once at the start: the
        // failure this used to show, about one run in six on a loaded machine,
        // was the follower giving up after `idle_secs` while the caller was
        // still waiting.
        let latest = tracker
            .latest_wanted(Duration::from_secs(10))
            .await
            .expect("no frame within ten seconds of asking");
        assert!(!latest.jpeg.is_empty());
        assert!(tracker.following());
        assert_eq!(mv.live_pipelines(), 1);
        assert_eq!(tracker.starts(), 1);

        // Stop asking. The tracker gives up after its idle window, and the
        // mosaic follows it down after the linger.
        for _ in 0..60 {
            if !tracker.following() && mv.live_pipelines() == 0 {
                break;
            }
            tokio::time::sleep(Duration::from_millis(100)).await;
        }
        assert!(!tracker.following(), "the tracker kept decoding with nobody asking");
        assert!(tracker.latest().is_none(), "a frame is still being held");
        assert_eq!(mv.live_pipelines(), 0, "the mosaic outlived the tracker");

        let _ = handle.send(crate::mixer::Command::Shutdown);
        tokio::task::spawn_blocking(move || thread.join()).await.unwrap().unwrap();
    }
}
