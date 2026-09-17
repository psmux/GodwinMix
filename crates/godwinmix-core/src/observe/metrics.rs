//! A metrics registry small enough to read in one sitting, and `/metrics` in
//! Prometheus text format over it.
//!
//! No metrics crate. What Prometheus wants is counters, gauges and histograms
//! with fixed buckets, rendered as text, and that is about three hundred lines
//! including the tests. A dependency would cost more in binary size and build
//! time than it saves, and the exposition format has not changed since 2014.
//!
//! Every handle (`Counter`, `Gauge`, `Histogram`) is an `Arc` over atomics, so
//! the hot paths, a pad probe running thirty times a second, do one relaxed
//! atomic add and never take a lock. The lock is taken when a series is first
//! created and when `/metrics` is scraped.

use crate::state::{Event, MixerStatus, OutputState, SourceState};
use parking_lot::RwLock;
use std::collections::BTreeMap;
use std::fmt::Write as _;
use std::sync::atomic::{AtomicU64, Ordering};
use std::sync::{Arc, LazyLock};

#[derive(Clone, Copy, PartialEq, Eq, Debug)]
pub enum Kind {
    Counter,
    Gauge,
    Histogram,
}

impl Kind {
    fn as_str(self) -> &'static str {
        match self {
            Kind::Counter => "counter",
            Kind::Gauge => "gauge",
            Kind::Histogram => "histogram",
        }
    }
}

/// Frame intervals for a programme that is meant to be running at 25, 30, 50
/// or 60 frames a second. The buckets straddle every one of those periods so
/// that "most frames landed late" is visible without knowing the canvas rate.
const FRAME_MS: &[f64] = &[8.0, 16.0, 20.0, 25.0, 33.0, 40.0, 50.0, 66.0, 100.0, 250.0, 1000.0];
/// Call latency. A control call that takes a second is already a bug report.
const CALL_MS: &[f64] = &[1.0, 2.0, 5.0, 10.0, 25.0, 50.0, 100.0, 250.0, 500.0, 1000.0, 5000.0];

/// Every metric the observability contract names, declared up front so that a
/// scrape of a freshly started mixer lists them all with zero values. A
/// dashboard built against a running show then does not break on a restart,
/// and `curl /metrics | grep gmx_programme_frame_interval_ms` answers before
/// the first frame.
const DEFS: &[(&str, Kind, &str, &[f64])] = &[
    ("gmx_programme_frames_total", Kind::Counter, "Frames leaving the programme mixer.", &[]),
    (
        "gmx_programme_frame_interval_ms",
        Kind::Histogram,
        "Wall clock gap between two programme frames, in milliseconds.",
        FRAME_MS,
    ),
    (
        "gmx_programme_frame_stall_ms",
        Kind::Gauge,
        "Worst average frame interval over sixty consecutive programme frames in the last \
         thirty to sixty seconds, in milliseconds. The measure the 34 ms acceptance bar is \
         written against: a single late thread wake up averages out of it, a pipeline that \
         really stopped does not.",
        &[],
    ),
    ("gmx_source_buffers_total", Kind::Counter, "Buffers seen from a source.", &[]),
    (
        "gmx_source_video_behind_ms",
        Kind::Gauge,
        "Milliseconds since the last video buffer from a source.",
        &[],
    ),
    ("gmx_source_queue_buffers", Kind::Gauge, "Buffers waiting in a source's queues.", &[]),
    (
        "gmx_source_state",
        Kind::Gauge,
        "Source state: 0 connecting, 1 live, 2 stalled, 3 failed.",
        &[],
    ),
    ("gmx_output_bytes_total", Kind::Counter, "Bytes written by an output.", &[]),
    ("gmx_output_reconnects_total", Kind::Counter, "Times an output has reconnected.", &[]),
    ("gmx_output_queue_secs", Kind::Gauge, "Seconds of encoded data waiting for an output.", &[]),
    (
        "gmx_output_state",
        Kind::Gauge,
        "Output state: 0 connecting, 1 live, 2 reconnecting, 3 failed.",
        &[],
    ),
    ("gmx_rpc_calls_total", Kind::Counter, "Control calls answered.", &[]),
    ("gmx_rpc_duration_ms", Kind::Histogram, "Time to answer a control call.", CALL_MS),
    ("gmx_take_ack_ms", Kind::Histogram, "Time from a take being asked for to it landing.", CALL_MS),
    ("gmx_takes_total", Kind::Counter, "Takes that landed.", &[]),
    ("gmx_takes_refused_total", Kind::Counter, "Takes refused, by reason.", &[]),
    (
        "gmx_node_clock_offset_ms",
        Kind::Gauge,
        "How far one node's clock sits from the programme clock, in milliseconds.",
        &[],
    ),
    (
        "gmx_node_heartbeat_age_ms",
        Kind::Gauge,
        "Milliseconds since one node's last heartbeat. Above 3000 the node is treated as gone.",
        &[],
    ),
    ("gmx_multiview_fps", Kind::Gauge, "Configured mosaic frame rate. Zero when disabled.", &[]),
    ("gmx_multiview_subscribers", Kind::Gauge, "Clients receiving mosaic frames.", &[]),
    ("gmx_plugin_restarts_total", Kind::Counter, "Times a plugin instance was rebuilt.", &[]),
    (
        "gmx_stream_clients",
        Kind::Gauge,
        "Clients on a preview or monitoring stream, by kind.",
        &[],
    ),
    (
        "gmx_encoder_running",
        Kind::Gauge,
        "1 while the programme encode chain is attached and encoding, 0 when it is not.",
        &[],
    ),
    (
        "gmx_encoder_consumers",
        Kind::Gauge,
        "Things holding the programme encoder up, by kind: output, whep, record.",
        &[],
    ),
    (
        "gmx_encoder_starts_total",
        Kind::Counter,
        "Times the programme encode chain has been started since boot.",
        &[],
    ),
];

type Labels = Vec<(String, String)>;

struct Family {
    kind: Kind,
    help: &'static str,
    buckets: &'static [f64],
    series: BTreeMap<Labels, Arc<Series>>,
}

/// One labelled time series. A counter and a gauge use `value` alone; a
/// histogram uses `sum`, `count` and `buckets`, which are cumulative in the
/// Prometheus sense (each bucket counts everything at or below its bound).
struct Series {
    value: AtomicU64,
    sum: AtomicU64,
    count: AtomicU64,
    buckets: Vec<AtomicU64>,
}

impl Series {
    fn new(width: usize) -> Self {
        Self {
            value: AtomicU64::new(0),
            sum: AtomicU64::new(0),
            count: AtomicU64::new(0),
            buckets: (0..width).map(|_| AtomicU64::new(0)).collect(),
        }
    }
}

static REGISTRY: LazyLock<RwLock<BTreeMap<&'static str, Family>>> = LazyLock::new(|| {
    let mut m = BTreeMap::new();
    for (name, kind, help, buckets) in DEFS {
        let mut family =
            Family { kind: *kind, help, buckets, series: BTreeMap::new() };
        // An unlabelled family gets its one series now, so it renders at zero
        // rather than not at all.
        if !takes_labels(name) {
            family.series.insert(Vec::new(), Arc::new(Series::new(buckets.len())));
        }
        m.insert(*name, family);
    }
    RwLock::new(m)
});

/// Which families are per instance, per method or per reason. Everything else
/// is a single series and is pre-created.
fn takes_labels(name: &str) -> bool {
    name.starts_with("gmx_source_")
        || name.starts_with("gmx_output_")
        || name.starts_with("gmx_plugin_")
        || name.starts_with("gmx_node_")
        || name.starts_with("gmx_rpc_")
        || name == "gmx_takes_refused_total"
}

fn series(name: &'static str, kind: Kind, labels: &[(&str, &str)]) -> Arc<Series> {
    // Sorted, so that a caller who writes the labels in a different order
    // still reaches the same series rather than silently starting a second one
    // that counts half the events.
    let mut key: Labels =
        labels.iter().map(|(k, v)| ((*k).to_string(), (*v).to_string())).collect();
    key.sort();
    if let Some(s) = REGISTRY.read().get(name).and_then(|f| f.series.get(&key)).cloned() {
        return s;
    }
    let mut reg = REGISTRY.write();
    let family = reg.entry(name).or_insert_with(|| Family {
        kind,
        help: "",
        buckets: &[],
        series: BTreeMap::new(),
    });
    let width = family.buckets.len();
    family.series.entry(key).or_insert_with(|| Arc::new(Series::new(width))).clone()
}

#[derive(Clone)]
pub struct Counter(Arc<Series>);

impl Counter {
    pub fn inc(&self) {
        self.add(1);
    }
    pub fn add(&self, n: u64) {
        self.0.value.fetch_add(n, Ordering::Relaxed);
    }
    /// Counters only go up, so a value read from somewhere that already counts
    /// (an output's reconnect count) is set rather than added.
    pub fn set(&self, n: u64) {
        self.0.value.store(n, Ordering::Relaxed);
    }
    pub fn get(&self) -> u64 {
        self.0.value.load(Ordering::Relaxed)
    }
}

#[derive(Clone)]
pub struct Gauge(Arc<Series>);

impl Gauge {
    pub fn set(&self, v: f64) {
        self.0.value.store(v.to_bits(), Ordering::Relaxed);
    }
    pub fn get(&self) -> f64 {
        f64::from_bits(self.0.value.load(Ordering::Relaxed))
    }
}

#[derive(Clone)]
pub struct Histogram {
    series: Arc<Series>,
    bounds: &'static [f64],
}

impl Histogram {
    /// Record one observation. A linear walk of at most eleven bounds and one
    /// atomic add each; this runs on a streaming thread and must not allocate
    /// or lock.
    pub fn observe(&self, v: f64) {
        for (i, bound) in self.bounds.iter().enumerate() {
            if v <= *bound {
                self.series.buckets[i].fetch_add(1, Ordering::Relaxed);
            }
        }
        self.series.count.fetch_add(1, Ordering::Relaxed);
        // f64 addition under a compare and swap: a histogram sum is not on a
        // path where contention is plausible (one producer per series).
        let sum = &self.series.sum;
        let mut cur = sum.load(Ordering::Relaxed);
        loop {
            let next = (f64::from_bits(cur) + v).to_bits();
            match sum.compare_exchange_weak(cur, next, Ordering::Relaxed, Ordering::Relaxed) {
                Ok(_) => return,
                Err(seen) => cur = seen,
            }
        }
    }

    pub fn count(&self) -> u64 {
        self.series.count.load(Ordering::Relaxed)
    }
}

pub fn counter(name: &'static str, labels: &[(&str, &str)]) -> Counter {
    Counter(series(name, Kind::Counter, labels))
}

pub fn gauge(name: &'static str, labels: &[(&str, &str)]) -> Gauge {
    Gauge(series(name, Kind::Gauge, labels))
}

pub fn histogram(name: &'static str, labels: &[(&str, &str)]) -> Histogram {
    let bounds = REGISTRY.read().get(name).map(|f| f.buckets).unwrap_or(&[]);
    Histogram { series: series(name, Kind::Histogram, labels), bounds }
}

/// The whole registry in Prometheus text exposition format.
pub fn render() -> String {
    let reg = REGISTRY.read();
    let mut out = String::with_capacity(4096);
    for (name, family) in reg.iter() {
        if family.series.is_empty() {
            continue;
        }
        if !family.help.is_empty() {
            let _ = writeln!(out, "# HELP {name} {}", family.help);
        }
        let _ = writeln!(out, "# TYPE {name} {}", family.kind.as_str());
        for (labels, s) in &family.series {
            match family.kind {
                Kind::Counter | Kind::Gauge => {
                    let v = if family.kind == Kind::Counter {
                        s.value.load(Ordering::Relaxed) as f64
                    } else {
                        f64::from_bits(s.value.load(Ordering::Relaxed))
                    };
                    let _ = writeln!(out, "{name}{} {}", render_labels(labels, None), number(v));
                }
                Kind::Histogram => render_histogram(&mut out, name, family, labels, s),
            }
        }
    }
    out
}

fn render_histogram(out: &mut String, name: &str, family: &Family, labels: &Labels, s: &Series) {
    for (i, bound) in family.buckets.iter().enumerate() {
        let n = s.buckets[i].load(Ordering::Relaxed);
        let le = number(*bound);
        let _ = writeln!(out, "{name}_bucket{} {n}", render_labels(labels, Some(&le)));
    }
    let count = s.count.load(Ordering::Relaxed);
    let _ = writeln!(out, "{name}_bucket{} {count}", render_labels(labels, Some("+Inf")));
    let sum = f64::from_bits(s.sum.load(Ordering::Relaxed));
    let _ = writeln!(out, "{name}_sum{} {}", render_labels(labels, None), number(sum));
    let _ = writeln!(out, "{name}_count{} {count}", render_labels(labels, None));
}

fn render_labels(labels: &Labels, le: Option<&str>) -> String {
    if labels.is_empty() && le.is_none() {
        return String::new();
    }
    let mut out = String::from("{");
    for (k, v) in labels {
        let _ = write!(out, "{k}=\"{}\",", escape(v));
    }
    if let Some(le) = le {
        let _ = write!(out, "le=\"{le}\",");
    }
    out.pop();
    out.push('}');
    out
}

/// Prometheus label values escape a backslash, a double quote and a newline
/// and nothing else.
fn escape(v: &str) -> String {
    let mut out = String::with_capacity(v.len());
    for c in v.chars() {
        match c {
            '\\' => out.push_str("\\\\"),
            '"' => out.push_str("\\\""),
            '\n' => out.push_str("\\n"),
            _ => out.push(c),
        }
    }
    out
}

/// Prometheus accepts Go's float spelling. A whole number is written without a
/// decimal point, which keeps a scrape of a mostly-integer registry short.
fn number(v: f64) -> String {
    if v.is_nan() {
        return "NaN".into();
    }
    if v.is_infinite() {
        return if v > 0.0 { "+Inf".into() } else { "-Inf".into() };
    }
    if v == v.trunc() && v.abs() < 1e15 {
        format!("{}", v as i64)
    } else {
        format!("{v}")
    }
}

// --- the places the numbers come from ---------------------------------------

/// Attach the programme frame counter and interval histogram to the raw video
/// tee, which is the one point every programme frame passes through before the
/// encoder and the multiview split apart.
///
/// The probe does two relaxed atomic adds and eleven compares in the worst
/// case. It allocates nothing, takes no lock and never touches the control
/// plane, because it runs on the compositor's streaming thread and principle
/// one says nothing there may block.
pub fn attach_programme_probe(tee: &gstreamer::Element) {
    use gstreamer::prelude::*;
    let Some(pad) = tee.static_pad("sink") else {
        tracing::warn!("programme tee has no sink pad, frame interval metrics are off");
        return;
    };
    let frames = counter("gmx_programme_frames_total", &[]);
    let intervals = histogram("gmx_programme_frame_interval_ms", &[]);
    let stall = gauge("gmx_programme_frame_stall_ms", &[]);
    let base = std::time::Instant::now();
    let last = AtomicU64::new(0);
    let ring: [AtomicU64; FRAME_WINDOW] = std::array::from_fn(|_| AtomicU64::new(0));
    // Frames counted into the ring since the last reset, and which reset that
    // was. A reset that lands mid window must not compare a frame after it
    // against one from before it.
    let filled = AtomicU64::new(0);
    let generation = AtomicU64::new(WINDOW_GENERATION.load(Ordering::Relaxed));
    // The gauge's own two buckets. See `GAUGE_SPAN`.
    let bucket_opened = AtomicU64::new(0);
    let this_bucket = AtomicU64::new(0);
    let last_bucket = AtomicU64::new(0);
    pad.add_probe(gstreamer::PadProbeType::BUFFER, move |_, _| {
        frames.inc();
        let now = base.elapsed().as_nanos() as u64;
        let prev = last.swap(now, Ordering::Relaxed);
        if prev != 0 {
            let gap = now - prev;
            intervals.observe(gap as f64 / 1_000_000.0);
            LONGEST_GAP_NS.fetch_max(gap, Ordering::Relaxed);
        }
        let current = WINDOW_GENERATION.load(Ordering::Relaxed);
        if generation.swap(current, Ordering::Relaxed) != current {
            filled.store(0, Ordering::Relaxed);
        }
        let n = filled.fetch_add(1, Ordering::Relaxed);
        let oldest = ring[(n % FRAME_WINDOW as u64) as usize].swap(now, Ordering::Relaxed);
        if n >= FRAME_WINDOW as u64 {
            let per_frame = now.saturating_sub(oldest) / FRAME_WINDOW as u64;
            LONGEST_WINDOW_NS.fetch_max(per_frame, Ordering::Relaxed);
            // The gauge ages. Two buckets, rolled over every `GAUGE_SPAN`, and
            // the gauge shows the worse of the one being filled and the one
            // before it, so it always answers for at least `GAUGE_SPAN` of
            // history and never for more than twice that.
            if now.saturating_sub(bucket_opened.load(Ordering::Relaxed)) >= GAUGE_SPAN_NS {
                last_bucket.store(this_bucket.swap(0, Ordering::Relaxed), Ordering::Relaxed);
                bucket_opened.store(now, Ordering::Relaxed);
            }
            let recent = this_bucket
                .fetch_max(per_frame, Ordering::Relaxed)
                .max(per_frame)
                .max(last_bucket.load(Ordering::Relaxed));
            stall.set(recent as f64 / 1_000_000.0);
        }
        gstreamer::PadProbeReturn::Ok
    });
}

/// How long the published gauge looks back.
///
/// The gauge answers "has the programme stalled lately", not "did it ever",
/// and the difference matters to everybody who reads it. A number that only
/// ever goes up is poisoned for the life of the process by one bad moment
/// during startup: an alert stays lit after the cause is gone, and a soak run
/// cannot say which of its rounds was the bad one. Thirty seconds, in two
/// buckets, so the answer covers between thirty and sixty seconds of history
/// and a scrape at any ordinary interval cannot miss a spike.
///
/// The worst since the process started is not lost. The histogram keeps the
/// whole distribution, `longest_frame_gap` keeps the single worst gap, and
/// `worst_frame_stall` keeps the worst window since the last reset, which is
/// what the tests assert on.
const GAUGE_SPAN_NS: u64 = 30_000_000_000;

/// How many consecutive frames the stall measure averages over.
///
/// Sixty, which is two seconds of a 30 fps programme, and the number is
/// measured rather than chosen. Against a 34 ms bar a window of sixty frames
/// allows (34 - 33.33) x 60 = 40 ms of accumulated lateness. An idle mixer with
/// two test sources, traced for forty seconds on a fourteen core Mac carrying a
/// load average of ten, never used more than 18 ms of that; the same trace put
/// one interval in three past 34 ms on its own. So the window has better than
/// twice the headroom it needs against ordinary scheduling noise, and it still
/// fails on any real stall longer than about 73 ms, which is two frames the
/// programme did not make. A wedged hook holding the pipeline for 200 ms
/// reports 36.6 ms and fails by a wide margin.
pub const FRAME_WINDOW: usize = 60;

/// The longest gap between two programme frames since the last reset.
///
/// The histogram above answers "how were the frame intervals distributed",
/// which is the question a dashboard asks. This answers "what was the very
/// worst one", which is the question a bug report asks. One relaxed
/// `fetch_max` on the probe, which allocates nothing and takes no lock; see
/// the note on the probe above.
static LONGEST_GAP_NS: AtomicU64 = AtomicU64::new(0);

/// The worst average interval across any [`FRAME_WINDOW`] consecutive frames.
///
/// This is the number the acceptance criteria are written against, and the
/// reason it is not `LONGEST_GAP_NS` is worth spelling out, because the raw
/// gap looks like the obvious measure and is not.
///
/// A programme frame arrives when the compositor's aggregator finishes waiting
/// on the pipeline clock and pushes. That wait is a `pthread_cond_timedwait`
/// on a general purpose operating system, which promises to wake no earlier
/// than asked and promises nothing about how much later. At 30 fps the period
/// is 33.3 ms and the acceptance bar is 34 ms, so the raw gap allows the
/// scheduler 0.7 ms of slop. No scheduler offers that. Measured on an idle
/// mixer with two test sources, on macOS in a debug build, a third of all
/// intervals land between 34 and 43 ms while the mean stays at exactly 33.3:
/// the aggregator is not late, it is jittery, and the frames it hands over
/// carry the right timestamps and arrive at the right average rate.
///
/// Averaging over a window of frames keeps what the criterion is about and
/// drops what it is not. A wake up 8 ms late followed by one 8 ms early
/// averages to nothing. A pipeline that really stopped, because a take blocked
/// a streaming thread or a plugin wedged it, does not average away: the
/// programme owes that time and every later frame in the window carries it.
/// See [`FRAME_WINDOW`] for how long the window is and what that buys. The raw
/// gap and the histogram are both still published, so nothing is hidden.
static LONGEST_WINDOW_NS: AtomicU64 = AtomicU64::new(0);

/// Bumped by every reset, so a probe mid window knows to start its ring again
/// rather than measure across the reset.
static WINDOW_GENERATION: AtomicU64 = AtomicU64::new(0);

/// The longest programme frame interval since [`reset_longest_frame_gap`].
pub fn longest_frame_gap() -> std::time::Duration {
    std::time::Duration::from_nanos(LONGEST_GAP_NS.load(Ordering::Relaxed))
}

/// The worst the programme stalled since [`reset_longest_frame_gap`], as an
/// average frame interval over [`FRAME_WINDOW`] frames. See
/// [`LONGEST_WINDOW_NS`] for why this and not [`longest_frame_gap`].
pub fn worst_frame_stall() -> std::time::Duration {
    std::time::Duration::from_nanos(LONGEST_WINDOW_NS.load(Ordering::Relaxed))
}

/// Start measuring again. A test calls this before the thing it is measuring
/// so that the pipeline coming up does not count against it.
pub fn reset_longest_frame_gap() {
    LONGEST_GAP_NS.store(0, Ordering::Relaxed);
    LONGEST_WINDOW_NS.store(0, Ordering::Relaxed);
    WINDOW_GENERATION.fetch_add(1, Ordering::Relaxed);
    gauge("gmx_programme_frame_stall_ms", &[]).set(0.0);
}

/// Record a control call. The api agent's router gets this through
/// `observe::rpc_layer()`; anything calling an RPC method by another route
/// (the MCP server, the CLI against an in-process core) calls it directly.
pub fn record_rpc(method: &str, code: &str, millis: f64) {
    counter("gmx_rpc_calls_total", &[("method", method), ("code", code)]).inc();
    histogram("gmx_rpc_duration_ms", &[("method", method)]).observe(millis);
}

/// Record how long a take took from being asked for to landing.
pub fn record_take_ack(millis: f64) {
    histogram("gmx_take_ack_ms", &[]).observe(millis);
}

/// Record a take that was refused, and why. The reason is a short slug, not a
/// sentence: it becomes a label and a label with unbounded values is how a
/// Prometheus server runs out of memory.
pub fn record_take_refused(reason: &str) {
    counter("gmx_takes_refused_total", &[("reason", reason)]).inc();
}

/// Fold one event into the registry. Called from the recorder task that also
/// writes the session log, so no other module has to know metrics exist.
pub fn observe_event(event: &Event) {
    match event {
        Event::Status(status) => observe_status(status),
        Event::Took { .. } => counter("gmx_takes_total", &[]).inc(),
        Event::SourceStateChanged { source, state } => {
            gauge("gmx_source_state", &[("instance", source)]).set(source_state_code(*state));
        }
        Event::OutputStateChanged { output, state, reconnects } => {
            gauge("gmx_output_state", &[("instance", output)]).set(output_state_code(*state));
            counter("gmx_output_reconnects_total", &[("instance", output)])
                .set(*reconnects as u64);
        }
        _ => {}
    }
}

/// Every gauge a status snapshot carries. A snapshot arrives on every
/// structural change and on every supervisor tick, which is twice a second,
/// so these are as fresh as a Prometheus scrape can use.
pub fn observe_status(status: &MixerStatus) {
    // A source or output that has gone takes its series with it. A show
    // that adds and removes a source every few seconds otherwise leaves a
    // dead series per gauge per source for the life of the process, and a
    // metrics page that only ever grows.
    let sources: Vec<&str> = status.sources.iter().map(|s| s.id.as_str()).collect();
    let outputs: Vec<&str> = status.outputs.iter().map(|o| o.id.as_str()).collect();
    for name in ["gmx_source_state", "gmx_source_video_behind_ms", "gmx_source_queue_buffers"] {
        retain_instances(name, &sources);
    }
    for name in ["gmx_output_state", "gmx_output_queue_secs", "gmx_output_reconnects_total"] {
        retain_instances(name, &outputs);
    }
    for s in &status.sources {
        let labels = [("instance", s.id.as_str())];
        gauge("gmx_source_state", &labels).set(source_state_code(s.state));
        if let Some(ms) = s.video_idle_ms {
            gauge("gmx_source_video_behind_ms", &labels).set(ms as f64);
        }
    }
    for o in &status.outputs {
        let labels = [("instance", o.id.as_str())];
        gauge("gmx_output_state", &labels).set(output_state_code(o.state));
        gauge("gmx_output_queue_secs", &labels).set(o.queue_secs);
        counter("gmx_output_reconnects_total", &labels).set(o.reconnects as u64);
    }
    let fps = if status.multiview.enabled { status.multiview.fps as f64 } else { 0.0 };
    gauge("gmx_multiview_fps", &[]).set(fps);
}

/// Drop every series of one family whose `instance` label is not in `keep`.
fn retain_instances(name: &str, keep: &[&str]) {
    let mut reg = REGISTRY.write();
    let Some(family) = reg.get_mut(name) else { return };
    family.series.retain(|labels, _| {
        labels
            .iter()
            .find(|(k, _)| k == "instance")
            .is_none_or(|(_, v)| keep.contains(&v.as_str()))
    });
}

/// How many clients are taking mosaic frames. Sampled rather than counted,
/// because the broadcast channel already knows and a second counter kept in
/// step with it would be a second thing to get wrong.
pub fn set_multiview_subscribers(n: usize) {
    gauge("gmx_multiview_subscribers", &[]).set(n as f64);
}

/// The mosaic's measured rate, zero when no mosaic exists.
pub fn set_multiview_fps(fps: f64) {
    gauge("gmx_multiview_fps", &[]).set(fps);
}

/// Clients on each preview or monitoring stream.
///
/// Every known kind is written, including the ones with nobody on them, so a
/// scrape of an idle core lists them all at zero rather than leaving a
/// dashboard to tell "none" from "not yet scraped". A kind whose last client
/// left is written as zero for the same reason.
pub fn set_stream_clients(counts: &std::collections::BTreeMap<String, u64>) {
    for kind in crate::preview::STREAM_KINDS {
        let n = counts.get(*kind).copied().unwrap_or(0);
        gauge("gmx_stream_clients", &[("kind", kind)]).set(n as f64);
    }
    // A kind the core did not declare, which a plugin could add later.
    for (kind, n) in counts {
        if !crate::preview::STREAM_KINDS.contains(&kind.as_str()) {
            gauge("gmx_stream_clients", &[("kind", kind.as_str())]).set(*n as f64);
        }
    }
}

/// Whether the programme encoder is running, and what is keeping it up.
///
/// With `[program] encoder = "on-demand"` and nothing attached, `running` is 0
/// and every consumer kind is 0, which is how an operator checks that an idle
/// core really is idle.
pub fn set_encoder(stats: &crate::encoder::EncoderStats) {
    gauge("gmx_encoder_running", &[]).set(if stats.running { 1.0 } else { 0.0 });
    for kind in ["output", "whep", "record"] {
        let n = stats.consumers.get(kind).copied().unwrap_or(0);
        gauge("gmx_encoder_consumers", &[("kind", kind)]).set(n as f64);
    }
    counter("gmx_encoder_starts_total", &[]).set(stats.starts);
}

/// How full each source's queues are, read off the pipelines at scrape time.
///
/// Reading a queue's level is a property read on a GStreamer object, so this
/// costs nothing when nobody is scraping, which is the rule. A source whose
/// queues are filling is the shape of every "it went to slate" report, and
/// this is the number that shows it before the supervisor acts.
pub fn sample_source_queues() {
    for name in crate::observe::introspect::names() {
        let Some(instance) = name.strip_prefix("input-") else { continue };
        let Ok(queues) = crate::observe::introspect::queues(&name) else { continue };
        let buffers: u32 = queues.iter().map(|q| q.buffers).sum();
        gauge("gmx_source_queue_buffers", &[("instance", instance)]).set(buffers as f64);
    }
}

fn source_state_code(s: SourceState) -> f64 {
    match s {
        SourceState::Connecting => 0.0,
        SourceState::Live => 1.0,
        SourceState::Stalled => 2.0,
        SourceState::Failed => 3.0,
    }
}

fn output_state_code(s: OutputState) -> f64 {
    match s {
        OutputState::Connecting => 0.0,
        OutputState::Live => 1.0,
        OutputState::Reconnecting => 2.0,
        OutputState::Failed => 3.0,
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    /// A gone source takes its series with it, and a present one keeps its.
    #[test]
    fn a_departed_instance_loses_its_series() {
        gauge("gmx_source_state", &[("instance", "retain-a")]).set(1.0);
        gauge("gmx_source_state", &[("instance", "retain-b")]).set(1.0);
        retain_instances("gmx_source_state", &["retain-a"]);
        let page = render();
        assert!(page.contains("instance=\"retain-a\""), "the present source kept its series");
        assert!(!page.contains("instance=\"retain-b\""), "the gone source lost its series");
    }

    #[test]
    fn a_fresh_registry_already_lists_the_programme_frame_interval_histogram() {
        let text = render();
        assert!(text.contains("# TYPE gmx_programme_frame_interval_ms histogram"), "{text}");
        assert!(text.contains("gmx_programme_frame_interval_ms_bucket{le=\"33\"}"), "{text}");
        assert!(text.contains("gmx_programme_frame_interval_ms_count"), "{text}");
        assert!(text.contains("gmx_programme_frames_total"), "{text}");
    }

    #[test]
    fn histogram_buckets_are_cumulative_and_the_sum_is_kept() {
        let h = histogram("gmx_take_ack_ms", &[]);
        let before = h.count();
        h.observe(3.0);
        h.observe(30.0);
        assert_eq!(h.count(), before + 2);
        let text = render();
        let line = text
            .lines()
            .find(|l| l.starts_with("gmx_take_ack_ms_bucket{le=\"5\"}"))
            .expect("a five millisecond bucket");
        let n: u64 = line.rsplit(' ').next().unwrap().parse().unwrap();
        assert!(n >= 1, "{line}");
        let inf = text
            .lines()
            .find(|l| l.starts_with("gmx_take_ack_ms_bucket{le=\"+Inf\"}"))
            .expect("an overflow bucket");
        let total: u64 = inf.rsplit(' ').next().unwrap().parse().unwrap();
        assert!(total >= n, "a cumulative histogram never shrinks: {inf} against {line}");
    }

    #[test]
    fn labels_are_escaped_and_ordered_so_a_scrape_parses() {
        record_rpc("program.take", "ok", 4.0);
        record_rpc("source.add\"odd", "-32602", 9.0);
        let text = render();
        assert!(text.contains("gmx_rpc_calls_total{code=\"ok\",method=\"program.take\"}"), "{text}");
        assert!(text.contains("method=\"source.add\\\"odd\""), "{text}");
    }

    /// The queue gauge is sampled off the real pipelines, so it is tested
    /// against one.
    #[test]
    fn source_queue_buffers_are_sampled_off_the_registered_pipelines() {
        use gstreamer::prelude::*;
        gstreamer::init().expect("gstreamer");
        let pipeline = gstreamer::Pipeline::with_name("input-metrics-queue");
        let src = gstreamer::ElementFactory::make("videotestsrc")
            .property("is-live", true)
            .build()
            .expect("videotestsrc");
        let queue = gstreamer::ElementFactory::make("queue")
            .name("metrics-queue-vq")
            .build()
            .expect("queue");
        let sink = gstreamer::ElementFactory::make("fakesink")
            .property("sync", false)
            .build()
            .expect("fakesink");
        pipeline.add_many([&src, &queue, &sink]).unwrap();
        gstreamer::Element::link_many([&src, &queue, &sink]).unwrap();
        crate::observe::register_pipeline("input-metrics-queue", &pipeline);
        pipeline.set_state(gstreamer::State::Paused).unwrap();

        sample_source_queues();
        assert!(
            render().contains("gmx_source_queue_buffers{instance=\"metrics-queue\"}"),
            "{}",
            render()
        );

        pipeline.set_state(gstreamer::State::Null).unwrap();
        crate::observe::unregister_pipeline("input-metrics-queue");
    }

    #[test]
    fn whole_numbers_render_without_a_decimal_point() {
        assert_eq!(number(33.0), "33");
        assert_eq!(number(0.5), "0.5");
        assert_eq!(number(f64::INFINITY), "+Inf");
    }

    #[test]
    fn a_source_going_stalled_moves_its_state_gauge() {
        let ev = Event::SourceStateChanged {
            source: "metrics-test-cam".into(),
            state: SourceState::Stalled,
        };
        observe_event(&ev);
        assert!(
            render().contains("gmx_source_state{instance=\"metrics-test-cam\"} 2"),
            "{}",
            render()
        );
    }

    /// The probe is the acceptance criterion for the frame interval metric, so
    /// it is tested against a real pipeline rather than by calling `observe`.
    #[test]
    fn the_programme_probe_counts_real_frames() {
        use gstreamer::prelude::*;
        gstreamer::init().expect("gstreamer");
        let pipeline = gstreamer::Pipeline::with_name("metrics-probe-test");
        let src = gstreamer::ElementFactory::make("videotestsrc")
            .property("num-buffers", 10i32)
            .property_from_str("pattern", "black")
            .build()
            .expect("videotestsrc");
        let tee = gstreamer::ElementFactory::make("tee").build().expect("tee");
        let sink = gstreamer::ElementFactory::make("fakesink")
            .property("sync", false)
            .build()
            .expect("fakesink");
        pipeline.add_many([&src, &tee, &sink]).unwrap();
        gstreamer::Element::link_many([&src, &tee, &sink]).unwrap();

        let before = counter("gmx_programme_frames_total", &[]).get();
        attach_programme_probe(&tee);
        pipeline.set_state(gstreamer::State::Playing).unwrap();
        let bus = pipeline.bus().unwrap();
        let _ = bus.timed_pop_filtered(
            gstreamer::ClockTime::from_seconds(10),
            &[gstreamer::MessageType::Eos, gstreamer::MessageType::Error],
        );
        pipeline.set_state(gstreamer::State::Null).unwrap();

        assert!(
            counter("gmx_programme_frames_total", &[]).get() >= before + 10,
            "the probe should have counted ten frames"
        );
        assert!(histogram("gmx_programme_frame_interval_ms", &[]).count() >= 9);
    }
}
