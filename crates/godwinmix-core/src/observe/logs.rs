//! Structured logs, tagged by instance, with levels that move at runtime.
//!
//! The interesting log is never on when the bug happens. That is the problem
//! this file exists to solve: a level per instance and a level per target,
//! both changeable over RPC with no restart, and GStreamer's own debug
//! thresholds on the same footing with a timer that turns the firehose off
//! again so nobody leaves it running.
//!
//! One `tracing` layer does the filtering, the formatting and the writing,
//! rather than the usual stack of an `EnvFilter` under an `fmt` layer. The
//! reason is the instance level: deciding whether a line is enabled needs the
//! span stack, and a reloadable `EnvFilter` cannot express "debug, but only
//! for lines under the span whose instance is cam1". Writing the layer is
//! about as much code as configuring one and it needs no extra crate feature.
//!
//! Cross platform: file rotation is `std::fs::rename` and `std::fs::remove_file`
//! only, both of which work on Windows, macOS and Linux. The one difference is
//! that Windows refuses to rename a file that is still open, so the handle is
//! dropped before the rename and reopened after it. Terminal detection is
//! `isatty` on Unix and `GetConsoleMode` on Windows, with "not a terminal", and
//! therefore JSON, as the fallback on anything else.

use crate::observe::trace::current_trace_id;
use parking_lot::{Mutex, RwLock};
use std::collections::BTreeMap;
use std::fmt::Write as _;
use std::io::Write as _;
use std::path::{Path, PathBuf};
use std::sync::atomic::{AtomicU8, AtomicUsize, Ordering};
use std::sync::{Arc, LazyLock, OnceLock};
use tracing::field::{Field, Visit};
use tracing::subscriber::Interest;
use tracing::{Event, Level, Metadata, Subscriber};
use tracing_subscriber::layer::{Context, Layer};
use tracing_subscriber::registry::LookupSpan;

/// 50 MB and five generations, from the observability contract.
const DEFAULT_MAX_BYTES: u64 = 50 * 1024 * 1024;
const DEFAULT_GENERATIONS: u32 = 5;

// --- levels -----------------------------------------------------------------

/// A level as a small number so the hot path compares two `u8`s.
///
/// Larger is more verbose, which is the opposite of `tracing::Level`'s ordering
/// and the same as everybody's intuition, so the comparison in `enabled` reads
/// the way it sounds: a line is on when its level is at or below the threshold.
#[derive(Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Debug)]
pub struct LevelCode(u8);

impl LevelCode {
    pub const OFF: LevelCode = LevelCode(0);
    pub const ERROR: LevelCode = LevelCode(1);
    pub const WARN: LevelCode = LevelCode(2);
    pub const INFO: LevelCode = LevelCode(3);
    pub const DEBUG: LevelCode = LevelCode(4);
    pub const TRACE: LevelCode = LevelCode(5);

    pub fn parse(s: &str) -> Option<Self> {
        match s.trim().to_ascii_lowercase().as_str() {
            "off" | "none" => Some(Self::OFF),
            "error" => Some(Self::ERROR),
            "warn" | "warning" => Some(Self::WARN),
            "info" => Some(Self::INFO),
            "debug" => Some(Self::DEBUG),
            "trace" => Some(Self::TRACE),
            _ => None,
        }
    }

    fn of(level: &Level) -> Self {
        match *level {
            Level::ERROR => Self::ERROR,
            Level::WARN => Self::WARN,
            Level::INFO => Self::INFO,
            Level::DEBUG => Self::DEBUG,
            Level::TRACE => Self::TRACE,
        }
    }

    pub fn as_str(self) -> &'static str {
        match self.0 {
            0 => "off",
            1 => "error",
            2 => "warn",
            3 => "info",
            4 => "debug",
            _ => "trace",
        }
    }
}

static DEFAULT_LEVEL: AtomicU8 = AtomicU8::new(3);
static TARGET_COUNT: AtomicUsize = AtomicUsize::new(0);
static INSTANCE_COUNT: AtomicUsize = AtomicUsize::new(0);
static TARGETS: LazyLock<RwLock<Vec<(String, LevelCode)>>> = LazyLock::new(Default::default);
static INSTANCES: LazyLock<RwLock<BTreeMap<String, LevelCode>>> = LazyLock::new(Default::default);

/// The level everything runs at unless a target or an instance says otherwise.
pub fn set_default_level(level: LevelCode) {
    DEFAULT_LEVEL.store(level.0, Ordering::Relaxed);
}

pub fn default_level() -> LevelCode {
    LevelCode(DEFAULT_LEVEL.load(Ordering::Relaxed))
}

/// `log.set {target, level}`. The target is matched as a module path prefix,
/// so `godwinmix::mixer` also turns up `godwinmix::mixer::supervisor`, and the
/// longest matching prefix wins. `None` for the level removes the override.
pub fn set_target_level(target: &str, level: Option<LevelCode>) {
    let mut t = TARGETS.write();
    t.retain(|(name, _)| name != target);
    if let Some(level) = level {
        t.push((target.to_string(), level));
        // Longest first, so the first match in `enabled` is the most specific.
        t.sort_by_key(|(name, _)| std::cmp::Reverse(name.len()));
    }
    TARGET_COUNT.store(t.len(), Ordering::Relaxed);
}

/// `log.set {instance, level}`. Applies to every line logged inside a span
/// carrying that `instance` field, which is every line a source or an output
/// produces once the entry points are instrumented. `None` removes it.
pub fn set_instance_level(instance: &str, level: Option<LevelCode>) {
    let mut m = INSTANCES.write();
    match level {
        Some(level) => {
            m.insert(instance.to_string(), level);
        }
        None => {
            m.remove(instance);
        }
    }
    INSTANCE_COUNT.store(m.len(), Ordering::Relaxed);
}

/// Every override in force, for `log.set` with no arguments and for the
/// support bundle.
pub fn levels() -> serde_json::Value {
    let targets: BTreeMap<_, _> =
        TARGETS.read().iter().map(|(k, v)| (k.clone(), v.as_str())).collect();
    let instances: BTreeMap<_, _> =
        INSTANCES.read().iter().map(|(k, v)| (k.clone(), v.as_str())).collect();
    serde_json::json!({
        "default": default_level().as_str(),
        "targets": targets,
        "instances": instances,
    })
}

/// `try_read` rather than `read`, here and in `instance_level`.
///
/// These run inside `enabled`, which runs on whatever thread is logging,
/// including a GStreamer streaming thread. If an override is being written at
/// that moment the line falls back to the default level instead of waiting.
/// Losing the level on one line during the microsecond a `log.set` takes is a
/// trade nobody will notice; blocking a streaming thread on a lock is the kind
/// of thing that takes a programme off air, and principle one says it cannot
/// happen.
fn target_level(target: &str) -> Option<LevelCode> {
    TARGETS
        .try_read()?
        .iter()
        .find(|(name, _)| target.starts_with(name.as_str()))
        .map(|(_, level)| *level)
}

fn instance_level(instance: &str) -> Option<LevelCode> {
    INSTANCES.try_read()?.get(instance).copied()
}

// --- GStreamer debug ---------------------------------------------------------

/// Categories currently raised, with the moment each goes back down. Held so
/// that a second `log.gst` while the first is still running extends rather
/// than duplicates, and so the support bundle can say what was on.
static GST_RAISED: LazyLock<Mutex<BTreeMap<String, std::time::Instant>>> =
    LazyLock::new(Default::default);

/// `log.gst {categories, duration_secs}`.
///
/// `categories` is the `GST_DEBUG` spelling: `rtmp2src:6,rtpjitterbuffer:5`.
/// Every named category is raised through `gst::debug_set_threshold_for_name`
/// and put back to its previous level after `duration_secs`, because the one
/// certainty about a debug firehose is that whoever turned it on will forget.
///
/// The `instance` argument is accepted and recorded but does not narrow the
/// threshold today: GStreamer's debug system is per process, not per pipeline,
/// so a category raised for cam1 is raised for every pipeline in this process.
/// When a source runs as a sidecar (03 section 6) the same call is forwarded to
/// it as `configure_log` and then it does narrow, which is why the argument is
/// in the signature now.
pub fn set_gst_debug(
    instance: Option<&str>,
    categories: &str,
    duration_secs: u64,
) -> anyhow::Result<Vec<String>> {
    let mut applied = Vec::new();
    let deadline = std::time::Instant::now() + std::time::Duration::from_secs(duration_secs.max(1));
    for part in categories.split(',').map(str::trim).filter(|p| !p.is_empty()) {
        let (name, level) = part
            .rsplit_once(':')
            .ok_or_else(|| anyhow::anyhow!("'{part}' is not <category>:<level>, as GST_DEBUG is"))?;
        let level: u32 = level
            .trim()
            .parse()
            .map_err(|_| anyhow::anyhow!("'{level}' is not a GStreamer debug level, 0 to 9"))?;
        let level = gst_level(level);
        gstreamer::log::set_threshold_for_name(name, level);
        GST_RAISED.lock().insert(name.to_string(), deadline);
        applied.push(name.to_string());
    }
    // No placeholder for a missing instance: an `instance` field with a dash
    // in it would tag the line as belonging to a plugin called "-", and the
    // log writer would dutifully open `plugins/-.log`.
    match instance {
        Some(instance) => tracing::info!(
            instance,
            categories,
            duration_secs,
            "GStreamer debug raised, and it comes back down on its own"
        ),
        None => tracing::info!(
            categories,
            duration_secs,
            "GStreamer debug raised, and it comes back down on its own"
        ),
    }
    let names = applied.clone();
    tokio::spawn(async move {
        tokio::time::sleep(std::time::Duration::from_secs(duration_secs.max(1))).await;
        let now = std::time::Instant::now();
        let mut raised = GST_RAISED.lock();
        for name in names {
            // Somebody may have extended this category since. Only the last
            // deadline puts it back.
            if raised.get(&name).is_some_and(|d| *d > now) {
                continue;
            }
            raised.remove(&name);
            gstreamer::log::set_threshold_for_name(&name, gstreamer::DebugLevel::None);
            tracing::info!(category = %name, "GStreamer debug back to normal");
        }
    });
    Ok(applied)
}

fn gst_level(n: u32) -> gstreamer::DebugLevel {
    match n {
        0 => gstreamer::DebugLevel::None,
        1 => gstreamer::DebugLevel::Error,
        2 => gstreamer::DebugLevel::Warning,
        3 => gstreamer::DebugLevel::Fixme,
        4 => gstreamer::DebugLevel::Info,
        5 => gstreamer::DebugLevel::Debug,
        6 => gstreamer::DebugLevel::Log,
        7 => gstreamer::DebugLevel::Trace,
        _ => gstreamer::DebugLevel::Memdump,
    }
}

/// The categories raised right now and how much longer each has.
pub fn gst_debug_in_force() -> Vec<(String, u64)> {
    let now = std::time::Instant::now();
    GST_RAISED
        .lock()
        .iter()
        .map(|(k, d)| (k.clone(), d.saturating_duration_since(now).as_secs()))
        .collect()
}

// --- rotation ---------------------------------------------------------------

/// A log file that rotates itself. `godwinmix.log` becomes `godwinmix.log.1`,
/// `.1` becomes `.2`, and `.5` is deleted.
///
/// Implemented with `std::fs` rather than a rotation crate because it is forty
/// lines and the crate would be a build dependency for every platform we ship
/// on. The rename dance is the same on all three: on Windows the open handle
/// is dropped first, because Windows will not rename a file that is open.
struct Rotating {
    path: PathBuf,
    file: Option<std::fs::File>,
    size: u64,
    max_bytes: u64,
    generations: u32,
}

impl Rotating {
    fn open(path: PathBuf, max_bytes: u64, generations: u32) -> std::io::Result<Self> {
        if let Some(dir) = path.parent() {
            std::fs::create_dir_all(dir)?;
        }
        let file = std::fs::OpenOptions::new().create(true).append(true).open(&path)?;
        let size = file.metadata().map(|m| m.len()).unwrap_or(0);
        Ok(Self { path, file: Some(file), size, max_bytes, generations })
    }

    fn write_line(&mut self, line: &str) {
        if self.size + line.len() as u64 + 1 > self.max_bytes {
            self.rotate();
        }
        if let Some(f) = self.file.as_mut() {
            if writeln!(f, "{line}").is_ok() {
                self.size += line.len() as u64 + 1;
            }
        }
    }

    fn rotate(&mut self) {
        // Drop the handle before renaming: required on Windows, harmless
        // elsewhere.
        self.file = None;
        let gen_path = |n: u32| {
            let mut p = self.path.clone().into_os_string();
            p.push(format!(".{n}"));
            PathBuf::from(p)
        };
        let _ = std::fs::remove_file(gen_path(self.generations));
        for n in (1..self.generations).rev() {
            let _ = std::fs::rename(gen_path(n), gen_path(n + 1));
        }
        let _ = std::fs::rename(&self.path, gen_path(1));
        self.file =
            std::fs::OpenOptions::new().create(true).append(true).open(&self.path).ok();
        self.size = 0;
    }
}

// --- the layer ---------------------------------------------------------------

#[derive(Clone, Copy, PartialEq, Eq, Debug, Default, clap::ValueEnum)]
pub enum Format {
    /// Human form when stderr is a terminal, JSON when it is not.
    #[default]
    Auto,
    Json,
    Human,
}

/// What a span or an event carries that the line needs beyond its message.
#[derive(Default, Clone)]
struct Tags {
    instance: Option<String>,
    trace_id: Option<String>,
}

struct Files {
    core: Mutex<Rotating>,
    dir: PathBuf,
    max_bytes: u64,
    generations: u32,
    plugins: Mutex<BTreeMap<String, Rotating>>,
}

/// The one layer: filters, formats and writes.
pub struct ObserveLayer {
    human: bool,
    node: Option<String>,
    files: OnceLock<Arc<Files>>,
    /// Where the human or JSON copy for a person watching goes. Always stderr
    /// in the binary, a capture buffer in the tests.
    sink: Box<dyn Fn(&str) + Send + Sync>,
}

impl ObserveLayer {
    pub fn new(format: Format, node: Option<String>) -> Self {
        let human = match format {
            Format::Human => true,
            Format::Json => false,
            Format::Auto => stderr_is_terminal(),
        };
        Self {
            human,
            node,
            files: OnceLock::new(),
            sink: Box::new(|line| {
                let mut err = std::io::stderr().lock();
                let _ = writeln!(err, "{line}");
            }),
        }
    }

    /// Send the terminal copy somewhere else. Used by the tests to capture.
    pub fn with_sink(mut self, sink: Box<dyn Fn(&str) + Send + Sync>) -> Self {
        self.sink = sink;
        self
    }

    /// Start writing `godwinmix.log` and `plugins/<instance>.log` under `dir`.
    /// Called once the config path is known, which is after the subscriber is
    /// installed, so that the lines logged while reading the config are not
    /// lost to a file that does not exist yet.
    pub fn attach_files(&self, dir: &Path, max_bytes: u64, generations: u32) -> std::io::Result<()> {
        let core = Rotating::open(dir.join("godwinmix.log"), max_bytes, generations)?;
        let files = Arc::new(Files {
            core: Mutex::new(core),
            dir: dir.to_path_buf(),
            max_bytes,
            generations,
            plugins: Mutex::new(BTreeMap::new()),
        });
        let _ = self.files.set(files);
        Ok(())
    }

    fn write(&self, tags: &Tags, json: &str, human: &str) {
        (self.sink)(if self.human { human } else { json });
        let Some(files) = self.files.get() else { return };
        files.core.lock().write_line(json);
        // A plugin author wants their instance's lines on their own, so an
        // instance tagged line is mirrored into `plugins/<instance>.log`. The
        // core file stays the complete timeline, which is what `gmx trace`
        // reads.
        if let Some(instance) = &tags.instance {
            if !instance_file_name_is_safe(instance) {
                return;
            }
            let mut plugins = files.plugins.lock();
            if let Some(f) = plugins.get_mut(instance) {
                f.write_line(json);
                return;
            }
            let path = files.dir.join("plugins").join(format!("{instance}.log"));
            if let Ok(mut f) = Rotating::open(path, files.max_bytes, files.generations) {
                f.write_line(json);
                plugins.insert(instance.clone(), f);
            }
        }
    }
}

/// An instance id becomes a file name, so a directory separator or a parent
/// reference in one would write outside the runtime directory. Ids are slugs
/// (principle five) and every real one passes; anything else logs to the core
/// file only.
fn instance_file_name_is_safe(id: &str) -> bool {
    !id.is_empty()
        && id.len() <= 64
        && id.chars().all(|c| c.is_ascii_alphanumeric() || c == '-' || c == '_' || c == '.')
        && id != "."
        && id != ".."
}

impl<S> Layer<S> for ObserveLayer
where
    S: Subscriber + for<'a> LookupSpan<'a>,
{
    /// Every callsite is asked each time rather than cached, because the answer
    /// changes when somebody sends `log.set`. That is the cost of runtime
    /// control and it is one `u8` comparison in the common case.
    fn register_callsite(&self, _: &'static Metadata<'static>) -> Interest {
        Interest::sometimes()
    }

    fn enabled(&self, meta: &Metadata<'_>, ctx: Context<'_, S>) -> bool {
        let level = LevelCode::of(meta.level());
        if TARGET_COUNT.load(Ordering::Relaxed) > 0 {
            if let Some(threshold) = target_level(meta.target()) {
                return level <= threshold;
            }
        }
        if INSTANCE_COUNT.load(Ordering::Relaxed) > 0 {
            if let Some(instance) = nearest_instance(&ctx) {
                if let Some(threshold) = instance_level(&instance) {
                    return level <= threshold;
                }
            }
        }
        level <= default_level()
    }

    fn on_new_span(
        &self,
        attrs: &tracing::span::Attributes<'_>,
        id: &tracing::span::Id,
        ctx: Context<'_, S>,
    ) {
        let Some(span) = ctx.span(id) else { return };
        let mut tags = Tags::default();
        attrs.record(&mut TagVisitor(&mut tags));
        // A span inside an instance span inherits the instance, so a line deep
        // in a source's build does not have to repeat it.
        if tags.instance.is_none() {
            tags.instance = span.parent().and_then(|p| {
                p.extensions().get::<Tags>().and_then(|t: &Tags| t.instance.clone())
            });
        }
        span.extensions_mut().insert(tags);
    }

    fn on_event(&self, event: &Event<'_>, ctx: Context<'_, S>) {
        let meta = event.metadata();
        let mut fields = Vec::new();
        let mut message = String::new();
        event.record(&mut EventVisitor { message: &mut message, fields: &mut fields });

        let mut tags = Tags {
            instance: nearest_instance(&ctx),
            trace_id: current_trace_id().map(|t| t.to_string()),
        };
        if tags.trace_id.is_none() {
            tags.trace_id = nearest_trace_id(&ctx);
        }
        // An event's own `instance` field beats the span it sits in.
        for (k, v) in &fields {
            if *k == "instance" {
                let name = v.trim_matches('"');
                tags.instance = (!name.is_empty()).then(|| name.to_string());
            }
        }

        let now = std::time::SystemTime::now();
        let json = render_json(&now, meta, &tags, self.node.as_deref(), &message, &fields);
        let human = render_human(&now, meta, &tags, &message, &fields);
        self.write(&tags, &json, &human);
    }
}

fn nearest_instance<S>(ctx: &Context<'_, S>) -> Option<String>
where
    S: Subscriber + for<'a> LookupSpan<'a>,
{
    let span = ctx.lookup_current()?;
    span.scope().find_map(|s| s.extensions().get::<Tags>().and_then(|t: &Tags| t.instance.clone()))
}

fn nearest_trace_id<S>(ctx: &Context<'_, S>) -> Option<String>
where
    S: Subscriber + for<'a> LookupSpan<'a>,
{
    let span = ctx.lookup_current()?;
    span.scope().find_map(|s| s.extensions().get::<Tags>().and_then(|t: &Tags| t.trace_id.clone()))
}

struct TagVisitor<'a>(&'a mut Tags);

impl Visit for TagVisitor<'_> {
    fn record_str(&mut self, field: &Field, value: &str) {
        match field.name() {
            "instance" => self.0.instance = Some(value.to_string()),
            "trace_id" => self.0.trace_id = Some(value.to_string()),
            _ => {}
        }
    }
    fn record_debug(&mut self, field: &Field, value: &dyn std::fmt::Debug) {
        let text = format!("{value:?}");
        self.record_str(field, text.trim_matches('"'));
    }
}

struct EventVisitor<'a> {
    message: &'a mut String,
    fields: &'a mut Vec<(&'static str, String)>,
}

impl Visit for EventVisitor<'_> {
    fn record_str(&mut self, field: &Field, value: &str) {
        if field.name() == "message" {
            self.message.push_str(value);
        } else {
            self.fields.push((field.name(), json_string(value)));
        }
    }
    fn record_i64(&mut self, field: &Field, value: i64) {
        self.push_raw(field, value.to_string());
    }
    fn record_u64(&mut self, field: &Field, value: u64) {
        self.push_raw(field, value.to_string());
    }
    fn record_f64(&mut self, field: &Field, value: f64) {
        self.push_raw(field, value.to_string());
    }
    fn record_bool(&mut self, field: &Field, value: bool) {
        self.push_raw(field, value.to_string());
    }
    fn record_debug(&mut self, field: &Field, value: &dyn std::fmt::Debug) {
        let text = format!("{value:?}");
        if field.name() == "message" {
            self.message.push_str(&text);
        } else {
            self.fields.push((field.name(), json_string(&text)));
        }
    }
}

impl EventVisitor<'_> {
    fn push_raw(&mut self, field: &Field, value: String) {
        if field.name() == "message" {
            self.message.push_str(&value);
        } else {
            self.fields.push((field.name(), value));
        }
    }
}

/// A JSON string, quotes included. Hand rolled rather than `serde_json` so a
/// log line costs one allocation on a path that runs on every event.
fn json_string(s: &str) -> String {
    let mut out = String::with_capacity(s.len() + 2);
    out.push('"');
    for c in s.chars() {
        match c {
            '"' => out.push_str("\\\""),
            '\\' => out.push_str("\\\\"),
            '\n' => out.push_str("\\n"),
            '\r' => out.push_str("\\r"),
            '\t' => out.push_str("\\t"),
            c if (c as u32) < 0x20 => {
                let _ = write!(out, "\\u{:04x}", c as u32);
            }
            c => out.push(c),
        }
    }
    out.push('"');
    out
}

fn render_json(
    now: &std::time::SystemTime,
    meta: &Metadata<'_>,
    tags: &Tags,
    node: Option<&str>,
    message: &str,
    fields: &[(&'static str, String)],
) -> String {
    let mut out = String::with_capacity(160);
    out.push('{');
    let _ = write!(out, "\"ts\":\"{}\"", rfc3339(now));
    let _ = write!(out, ",\"level\":\"{}\"", LevelCode::of(meta.level()).as_str());
    let _ = write!(out, ",\"target\":{}", json_string(meta.target()));
    match &tags.instance {
        Some(i) => {
            let _ = write!(out, ",\"instance\":{}", json_string(i));
        }
        None => out.push_str(",\"instance\":null"),
    }
    if let Some(node) = node {
        let _ = write!(out, ",\"node\":{}", json_string(node));
    }
    if let Some(trace) = &tags.trace_id {
        let _ = write!(out, ",\"trace_id\":{}", json_string(trace));
    }
    let _ = write!(out, ",\"message\":{}", json_string(message));
    for (k, v) in fields {
        if *k == "instance" || *k == "trace_id" {
            continue;
        }
        let _ = write!(out, ",{}:{}", json_string(k), v);
    }
    out.push('}');
    out
}

fn render_human(
    now: &std::time::SystemTime,
    meta: &Metadata<'_>,
    tags: &Tags,
    message: &str,
    fields: &[(&'static str, String)],
) -> String {
    let mut out = String::with_capacity(120);
    let _ = write!(out, "{} {:>5} {}", rfc3339(now), LevelCode::of(meta.level()).as_str(), meta.target());
    if let Some(i) = &tags.instance {
        let _ = write!(out, " [{i}]");
    }
    let _ = write!(out, ": {message}");
    for (k, v) in fields {
        if *k == "instance" {
            continue;
        }
        let _ = write!(out, " {k}={v}");
    }
    if let Some(trace) = &tags.trace_id {
        let _ = write!(out, " trace_id={trace}");
    }
    out
}

/// RFC 3339 in UTC to the millisecond, with no date crate.
///
/// The civil-from-days algorithm is Howard Hinnant's, which is the one every
/// date library uses. It is correct for every date the Gregorian calendar
/// covers, and a log timestamp is the only date this program formats.
pub fn rfc3339(t: &std::time::SystemTime) -> String {
    let d = t.duration_since(std::time::UNIX_EPOCH).unwrap_or_default();
    let secs = d.as_secs() as i64;
    let millis = d.subsec_millis();
    let days = secs.div_euclid(86_400);
    let rem = secs.rem_euclid(86_400);
    let (h, mi, s) = (rem / 3600, (rem % 3600) / 60, rem % 60);
    let (y, mo, dy) = civil_from_days(days);
    format!("{y:04}-{mo:02}-{dy:02}T{h:02}:{mi:02}:{s:02}.{millis:03}Z")
}

fn civil_from_days(z: i64) -> (i64, u32, u32) {
    let z = z + 719_468;
    let era = z.div_euclid(146_097);
    let doe = z.rem_euclid(146_097);
    let yoe = (doe - doe / 1460 + doe / 36_524 - doe / 146_096) / 365;
    let y = yoe + era * 400;
    let doy = doe - (365 * yoe + yoe / 4 - yoe / 100);
    let mp = (5 * doy + 2) / 153;
    let d = (doy - (153 * mp + 2) / 5 + 1) as u32;
    let m = if mp < 10 { mp + 3 } else { mp - 9 } as u32;
    (if m <= 2 { y + 1 } else { y }, m, d)
}

/// Whether stderr is a terminal, which decides the default log format.
///
/// Unix asks `isatty`. Windows asks `GetConsoleMode` on the standard error
/// handle, which succeeds only for a console. Anything else, a target we do
/// not ship on today, answers false and gets JSON, which is the safe default:
/// a machine can read human lines badly, but a person can read JSON.
#[cfg(unix)]
fn stderr_is_terminal() -> bool {
    // SAFETY: `isatty` reads a file descriptor's kind and touches no memory we
    // own. Fd 2 is always valid in a hosted process.
    unsafe { libc::isatty(libc::STDERR_FILENO) == 1 }
}

#[cfg(windows)]
fn stderr_is_terminal() -> bool {
    #[link(name = "kernel32")]
    extern "system" {
        fn GetStdHandle(n: u32) -> isize;
        fn GetConsoleMode(h: isize, mode: *mut u32) -> i32;
    }
    const STD_ERROR_HANDLE: u32 = -12i32 as u32;
    // SAFETY: both calls take a handle by value and write at most one u32 into
    // a stack local we own.
    unsafe {
        let h = GetStdHandle(STD_ERROR_HANDLE);
        let mut mode = 0u32;
        h != 0 && h != -1 && GetConsoleMode(h, &mut mode) != 0
    }
}

#[cfg(not(any(unix, windows)))]
fn stderr_is_terminal() -> bool {
    false
}

// --- installation ------------------------------------------------------------

/// The installed layer, so `attach_files` can reach it after the config has
/// been read and told us where the runtime directory is.
static INSTALLED: OnceLock<Arc<ObserveLayer>> = OnceLock::new();

pub struct Options {
    pub format: Format,
    pub node: Option<String>,
    /// The starting level. `RUST_LOG` still works: a bare level sets the
    /// default, and `target=level` pairs become target overrides, so an
    /// operator's existing environment keeps behaving.
    pub env_filter: Option<String>,
}

/// Install the subscriber. Call once, first thing in `run`.
pub fn init(options: Options) {
    use tracing_subscriber::prelude::*;
    apply_env_filter(options.env_filter.as_deref());
    let layer = Arc::new(ObserveLayer::new(options.format, options.node));
    let _ = INSTALLED.set(layer.clone());
    let _ = tracing_subscriber::registry().with(SharedLayer(layer)).try_init();
}

/// `Layer` is implemented for the value, and the installed copy is shared with
/// `attach_files`, so the registry gets a thin wrapper over the `Arc`.
struct SharedLayer(Arc<ObserveLayer>);

impl<S> Layer<S> for SharedLayer
where
    S: Subscriber + for<'a> LookupSpan<'a>,
{
    fn register_callsite(&self, m: &'static Metadata<'static>) -> Interest {
        Layer::<S>::register_callsite(&*self.0, m)
    }
    fn enabled(&self, m: &Metadata<'_>, ctx: Context<'_, S>) -> bool {
        Layer::<S>::enabled(&*self.0, m, ctx)
    }
    fn on_new_span(
        &self,
        attrs: &tracing::span::Attributes<'_>,
        id: &tracing::span::Id,
        ctx: Context<'_, S>,
    ) {
        Layer::<S>::on_new_span(&*self.0, attrs, id, ctx)
    }
    fn on_event(&self, event: &Event<'_>, ctx: Context<'_, S>) {
        Layer::<S>::on_event(&*self.0, event, ctx)
    }
}

/// Read `RUST_LOG` into the level tables. Only the two shapes an operator
/// actually types are honoured: a bare level, and `target=level` pairs.
fn apply_env_filter(filter: Option<&str>) {
    let Some(filter) = filter else { return };
    for part in filter.split(',').map(str::trim).filter(|p| !p.is_empty()) {
        match part.split_once('=') {
            None => {
                if let Some(level) = LevelCode::parse(part) {
                    set_default_level(level);
                }
            }
            Some((target, level)) => {
                if let Some(level) = LevelCode::parse(level) {
                    set_target_level(target.trim(), Some(level));
                }
            }
        }
    }
}

/// Start writing files under `dir`. Safe to call once the config is loaded.
pub fn attach_files(dir: &Path) -> std::io::Result<()> {
    match INSTALLED.get() {
        Some(layer) => layer.attach_files(dir, DEFAULT_MAX_BYTES, DEFAULT_GENERATIONS),
        None => Ok(()),
    }
}

/// Where the core log lives under a runtime directory.
pub fn core_log_path(dir: &Path) -> PathBuf {
    dir.join("godwinmix.log")
}

/// Every log file under a runtime directory, core first then plugins, which is
/// the order `gmx logs` and `gmx trace` read them in.
pub fn log_files(dir: &Path) -> Vec<PathBuf> {
    let mut files = vec![core_log_path(dir)];
    if let Ok(entries) = std::fs::read_dir(dir.join("plugins")) {
        let mut plugins: Vec<_> = entries
            .filter_map(Result::ok)
            .map(|e| e.path())
            .filter(|p| p.extension().is_some_and(|e| e == "log"))
            .collect();
        plugins.sort();
        files.extend(plugins);
    }
    files.into_iter().filter(|p| p.exists()).collect()
}

#[cfg(test)]
pub(crate) mod tests {
    use super::*;
    use std::sync::Mutex as StdMutex;

    /// The level tables are global, so the tests that move them run one at a
    /// time. Everything else here is independent.
    static SERIAL: StdMutex<()> = StdMutex::new(());

    fn capture() -> (Arc<ObserveLayer>, Arc<Mutex<Vec<String>>>) {
        let lines = Arc::new(Mutex::new(Vec::new()));
        let sink = lines.clone();
        let layer = ObserveLayer::new(Format::Json, None)
            .with_sink(Box::new(move |l| sink.lock().push(l.to_string())));
        (Arc::new(layer), lines)
    }

    pub(crate) fn with_capture<R>(f: impl FnOnce() -> R) -> (R, Vec<String>) {
        use tracing_subscriber::prelude::*;
        let (layer, lines) = capture();
        let subscriber = tracing_subscriber::registry().with(SharedLayer(layer));
        let out = tracing::subscriber::with_default(subscriber, f);
        let captured = lines.lock().clone();
        (out, captured)
    }

    fn reset_levels() {
        set_default_level(LevelCode::INFO);
        // Collected into a local first. A guard in a `for` loop's iterator
        // expression lives for the whole loop, so reading it there and writing
        // inside the body is a deadlock with itself.
        let instances: Vec<String> = INSTANCES.read().keys().cloned().collect();
        for name in instances {
            set_instance_level(&name, None);
        }
        let targets: Vec<String> = TARGETS.read().iter().map(|(n, _)| n.clone()).collect();
        for name in targets {
            set_target_level(&name, None);
        }
    }

    #[test]
    fn every_line_carries_ts_level_target_and_instance() {
        let (_, lines) = with_capture(|| {
            let span = tracing::info_span!("source", instance = "cam1");
            let _enter = span.enter();
            tracing::info!(queue = 3, "first frame");
        });
        let line = lines.last().expect("a line");
        let v: serde_json::Value = serde_json::from_str(line).expect("valid JSON");
        assert!(v["ts"].as_str().unwrap().ends_with('Z'));
        assert_eq!(v["level"], "info");
        assert_eq!(v["instance"], "cam1");
        assert_eq!(v["message"], "first frame");
        assert_eq!(v["queue"], 3);
        assert!(v["target"].as_str().unwrap().contains("godwinmix"));
    }

    /// The acceptance criterion: a running source's level moves with no
    /// restart. The source here is a span, which is what a running source is
    /// as far as the log is concerned.
    #[test]
    fn log_set_changes_a_running_instances_level_with_no_restart() {
        let _guard = SERIAL.lock().unwrap();
        reset_levels();
        let (_, lines) = with_capture(|| {
            let span = tracing::info_span!("source", instance = "cam1");
            let _enter = span.enter();
            tracing::debug!("before: not logged at info");
            set_instance_level("cam1", Some(LevelCode::DEBUG));
            tracing::debug!("after: logged");
            set_instance_level("cam1", None);
            tracing::debug!("back off again");
        });
        let messages: Vec<_> = lines
            .iter()
            .map(|l| serde_json::from_str::<serde_json::Value>(l).unwrap())
            .map(|v| v["message"].as_str().unwrap().to_string())
            .collect();
        assert_eq!(messages, vec!["after: logged".to_string()], "{lines:?}");
        reset_levels();
    }

    #[test]
    fn an_instance_level_does_not_raise_every_other_instance() {
        let _guard = SERIAL.lock().unwrap();
        reset_levels();
        set_instance_level("cam1", Some(LevelCode::DEBUG));
        let (_, lines) = with_capture(|| {
            let one = tracing::info_span!("source", instance = "cam1");
            one.in_scope(|| tracing::debug!("cam1 detail"));
            let two = tracing::info_span!("source", instance = "cam2");
            two.in_scope(|| tracing::debug!("cam2 detail"));
            tracing::debug!("core detail");
        });
        assert_eq!(lines.len(), 1, "{lines:?}");
        assert!(lines[0].contains("cam1 detail"));
        reset_levels();
    }

    #[test]
    fn a_target_level_matches_on_module_prefix_longest_first() {
        let _guard = SERIAL.lock().unwrap();
        reset_levels();
        set_target_level("godwinmix", Some(LevelCode::OFF));
        set_target_level(module_path!(), Some(LevelCode::TRACE));
        let (_, lines) = with_capture(|| tracing::trace!("deep detail"));
        assert_eq!(lines.len(), 1, "the longer prefix should win: {lines:?}");
        reset_levels();
    }

    #[test]
    fn the_instance_is_inherited_by_a_nested_span() {
        let (_, lines) = with_capture(|| {
            let outer = tracing::info_span!("source", instance = "cam1");
            let _o = outer.enter();
            let inner = tracing::info_span!("build");
            let _i = inner.enter();
            tracing::info!("inside two spans");
        });
        let v: serde_json::Value = serde_json::from_str(&lines[0]).unwrap();
        assert_eq!(v["instance"], "cam1");
    }

    #[tokio::test]
    async fn a_line_inside_a_call_carries_the_trace_id() {
        use crate::observe::trace::{with_trace_id, TraceId};
        use tracing_subscriber::prelude::*;
        let (layer, lines) = capture();
        let subscriber = tracing_subscriber::registry().with(SharedLayer(layer));
        let id = TraceId::new();
        let _default = tracing::subscriber::set_default(subscriber);
        with_trace_id(id, async {
            tracing::info!("inside the call");
        })
        .await;
        let v: serde_json::Value = serde_json::from_str(&lines.lock()[0]).unwrap();
        assert_eq!(v["trace_id"], id.to_string());
    }

    #[test]
    fn a_line_outside_a_call_has_no_trace_id() {
        let (_, lines) = with_capture(|| tracing::info!("a supervisor decision"));
        let v: serde_json::Value = serde_json::from_str(&lines[0]).unwrap();
        assert!(v.get("trace_id").is_none(), "{}", lines[0]);
    }

    #[test]
    fn messages_with_quotes_and_newlines_stay_parseable() {
        let (_, lines) = with_capture(|| {
            tracing::info!(detail = "a \"quoted\" thing\nover two lines", "odd");
        });
        let v: serde_json::Value = serde_json::from_str(&lines[0]).expect("valid JSON");
        assert_eq!(v["detail"], "a \"quoted\" thing\nover two lines");
    }

    #[test]
    fn rotation_keeps_five_generations_and_no_more() {
        let dir = crate::observe::tempdir("rotation");
        let path = dir.join("godwinmix.log");
        let mut f = Rotating::open(path.clone(), 64, 5).expect("open");
        for i in 0..200 {
            f.write_line(&format!("line {i} padded out to force a rotation soon"));
        }
        drop(f);
        assert!(path.exists());
        for n in 1..=5 {
            assert!(dir.join(format!("godwinmix.log.{n}")).exists(), "generation {n} missing");
        }
        assert!(!dir.join("godwinmix.log.6").exists(), "a sixth generation was kept");
        let _ = std::fs::remove_dir_all(&dir);
    }

    #[test]
    fn an_instance_id_that_would_escape_the_directory_is_refused() {
        assert!(instance_file_name_is_safe("cam1"));
        assert!(instance_file_name_is_safe("cam-1_a.2"));
        assert!(!instance_file_name_is_safe("../etc/passwd"));
        assert!(!instance_file_name_is_safe("a/b"));
        assert!(!instance_file_name_is_safe(".."));
        assert!(!instance_file_name_is_safe(""));
    }

    #[test]
    fn timestamps_are_rfc3339_in_utc() {
        let t = std::time::UNIX_EPOCH + std::time::Duration::from_millis(1_757_851_234_567);
        assert_eq!(rfc3339(&t), "2025-09-14T12:00:34.567Z");
        assert_eq!(rfc3339(&std::time::UNIX_EPOCH), "1970-01-01T00:00:00.000Z");
    }

    #[test]
    fn rust_log_still_sets_levels() {
        let _guard = SERIAL.lock().unwrap();
        reset_levels();
        apply_env_filter(Some("warn,godwinmix::observe=trace"));
        assert_eq!(default_level(), LevelCode::WARN);
        assert_eq!(target_level("godwinmix::observe::logs"), Some(LevelCode::TRACE));
        reset_levels();
    }

    #[test]
    fn gst_debug_rejects_a_spelling_that_is_not_gst_debugs() {
        gstreamer::init().unwrap();
        assert!(set_gst_debug(None, "rtmp2src", 1).is_err());
        assert!(set_gst_debug(None, "rtmp2src:high", 1).is_err());
    }

}
