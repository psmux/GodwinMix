//! Looking inside a running pipeline, on demand, without a restart.
//!
//! `GST_DEBUG_DUMP_DOT_DIR` answers the graph question at startup or never.
//! These answer it now, for one pipeline, over the control plane, which is
//! what an operator on the phone at 9 pm actually has.
//!
//! Every pipeline registers itself here as a weak reference when it is built.
//! Weak, so a source that is removed is collected exactly as before and this
//! module never keeps a pipeline alive; a name whose pipeline has gone answers
//! "not found", which is the truth.

use anyhow::{anyhow, Result};
use gstreamer as gst;
use gstreamer::prelude::*;
use parking_lot::Mutex;
use serde::Serialize;
use std::collections::BTreeMap;
use std::sync::LazyLock;

/// The programme pipeline's name in every introspection call. British spelling
/// throughout the plan; `program` is accepted too, because that is what the
/// pipeline is called inside GStreamer and what an operator who has read a dot
/// file will type.
pub const PROGRAMME: &str = "programme";

static PIPELINES: LazyLock<Mutex<BTreeMap<String, glib::WeakRef<gst::Pipeline>>>> =
    LazyLock::new(Default::default);

/// Make a pipeline reachable by name from `pipeline.dot` and friends.
///
/// Called once where each pipeline is built: the programme in `Mixer::build`,
/// a source in `InputPipeline::build_kind`, an output in `OutputSlot::attach`,
/// the mosaic in `Multiview::build`.
pub fn register_pipeline(name: &str, pipeline: &gst::Pipeline) {
    let weak = glib::object::ObjectExt::downgrade(pipeline);
    PIPELINES.lock().insert(name.to_string(), weak);
}

/// Forget a name. Not required for correctness, a dead weak reference answers
/// the same way, but it keeps `pipeline.list` honest.
pub fn unregister_pipeline(name: &str) {
    PIPELINES.lock().remove(name);
}

/// Look a pipeline up by the name a caller typed.
///
/// Tried in order: the exact name, `programme` against `program`, and the
/// `output-<id>` and `input-<id>` spellings, so `gmx dot cam1` works whether
/// cam1 is a source or an output and whoever typed it did not have to know.
pub fn pipeline(name: &str) -> Option<gst::Pipeline> {
    let map = PIPELINES.lock();
    let get = |n: &str| map.get(n).and_then(|w| w.upgrade());
    get(name)
        .or_else(|| match name {
            PROGRAMME => get("program"),
            "program" => get(PROGRAMME),
            _ => None,
        })
        .or_else(|| get(&format!("output-{name}")))
        .or_else(|| get(&format!("input-{name}")))
}

/// Every registered name whose pipeline is still alive.
pub fn names() -> Vec<String> {
    let mut map = PIPELINES.lock();
    map.retain(|_, w| w.upgrade().is_some());
    map.keys().cloned().collect()
}

fn found(name: &str) -> Result<gst::Pipeline> {
    pipeline(name).ok_or_else(|| {
        anyhow!(
            "no pipeline called '{name}' is running. Known: {}. \
             Try 'programme', 'multiview', or a source or output id",
            names().join(", ")
        )
    })
}

// --- pipeline.dot ------------------------------------------------------------

/// `pipeline.dot {instance | "programme" | "multiview"}`: the graph as
/// Graphviz, right now, for one pipeline.
pub fn dot(name: &str) -> Result<String> {
    let pipeline = found(name)?;
    Ok(pipeline.debug_to_dot_data(gst::DebugGraphDetails::ALL).to_string())
}

// --- pipeline.latency --------------------------------------------------------

#[derive(Debug, Clone, Serialize)]
pub struct LatencyReport {
    pub pipeline: String,
    /// Whether anything upstream is live, which is what makes latency matter.
    pub live: bool,
    /// The pipeline's own answer to a LATENCY query, in milliseconds.
    pub min_ms: Option<f64>,
    pub max_ms: Option<f64>,
    /// Per stage, for the elements that answered.
    pub stages: Vec<StageLatency>,
}

#[derive(Debug, Clone, Serialize)]
pub struct StageLatency {
    pub element: String,
    pub live: bool,
    pub min_ms: f64,
    pub max_ms: Option<f64>,
}

/// `pipeline.latency {instance}`: what each stage declares it needs.
///
/// A LATENCY query is answered from the element's own state and does not stop
/// the streaming thread, so this is safe to call on a live programme. It is
/// still a walk of every element, so it is on demand and never on a tick.
pub fn latency(name: &str) -> Result<LatencyReport> {
    let pipeline = found(name)?;
    let mut query = gst::query::Latency::new();
    let answered = pipeline.query(&mut query);
    let (live, min, max) = if answered { query.result() } else { (false, gst::ClockTime::ZERO, None) };

    let mut stages = Vec::new();
    for element in pipeline.iterate_recurse().into_iter().flatten() {
        let mut q = gst::query::Latency::new();
        if !element.query(&mut q) {
            continue;
        }
        let (elive, emin, emax) = q.result();
        // Only the stages that declare something are worth a line. An element
        // that passes through with no latency is noise in a report read at
        // speed by somebody whose stream is down.
        if emin.is_zero() && !elive {
            continue;
        }
        stages.push(StageLatency {
            element: element.name().to_string(),
            live: elive,
            min_ms: ms(emin),
            max_ms: emax.map(ms),
        });
    }
    stages.sort_by(|a, b| b.min_ms.total_cmp(&a.min_ms));
    Ok(LatencyReport {
        pipeline: pipeline.name().to_string(),
        live,
        min_ms: answered.then(|| ms(min)),
        max_ms: max.map(ms),
        stages,
    })
}

fn ms(t: gst::ClockTime) -> f64 {
    t.nseconds() as f64 / 1_000_000.0
}

// --- pipeline.queues ---------------------------------------------------------

#[derive(Debug, Clone, Serialize)]
pub struct QueueFill {
    pub element: String,
    pub buffers: u32,
    pub bytes: u32,
    pub time_ms: f64,
    pub max_buffers: u32,
    pub max_bytes: u32,
    pub max_time_ms: f64,
    /// The fullest of the three limits, 0.0 to 1.0. This is the number to read
    /// first: a queue at 1.0 is the one that is blocking.
    pub fullest: f64,
}

/// `pipeline.queues {instance}`: how full every queue on this pipeline is.
///
/// Reads properties, which is a lock free read on a GStreamer object, so it
/// costs nothing and can be polled by a UI.
pub fn queues(name: &str) -> Result<Vec<QueueFill>> {
    let pipeline = found(name)?;
    let mut out = Vec::new();
    for element in pipeline.iterate_recurse().into_iter().flatten() {
        let factory = element.factory().map(|f| f.name().to_string()).unwrap_or_default();
        if !factory.starts_with("queue") {
            continue;
        }
        let buffers: u32 = element.property("current-level-buffers");
        let bytes: u32 = element.property("current-level-bytes");
        let time: u64 = element.property("current-level-time");
        let max_buffers: u32 = element.property("max-size-buffers");
        let max_bytes: u32 = element.property("max-size-bytes");
        let max_time: u64 = element.property("max-size-time");
        let ratio = |cur: f64, max: f64| if max > 0.0 { cur / max } else { 0.0 };
        let fullest = ratio(buffers as f64, max_buffers as f64)
            .max(ratio(bytes as f64, max_bytes as f64))
            .max(ratio(time as f64, max_time as f64));
        out.push(QueueFill {
            element: element.name().to_string(),
            buffers,
            bytes,
            time_ms: time as f64 / 1_000_000.0,
            max_buffers,
            max_bytes,
            max_time_ms: max_time as f64 / 1_000_000.0,
            fullest,
        });
    }
    out.sort_by(|a, b| b.fullest.total_cmp(&a.fullest));
    Ok(out)
}

// --- pipeline.clock ----------------------------------------------------------

#[derive(Debug, Clone, Serialize)]
pub struct ClockReport {
    pub clock: Option<String>,
    pub clock_time_ms: Option<f64>,
    pub base_time_ms: Option<f64>,
    pub running_time_ms: Option<f64>,
    /// One row per pipeline in this process, so that a mosaic drifting from
    /// the programme is visible without asking twice.
    pub pipelines: Vec<PipelineClock>,
    /// Remote nodes and their offsets, one row per node the core has heard
    /// from. Empty on a core with no node bridge.
    pub nodes: Vec<NodeClock>,
}

#[derive(Debug, Clone, Serialize)]
pub struct PipelineClock {
    pub name: String,
    pub state: String,
    pub base_time_ms: Option<f64>,
    pub running_time_ms: Option<f64>,
}

#[derive(Debug, Clone, Serialize)]
pub struct NodeClock {
    pub node: String,
    pub offset_ms: f64,
    pub jitter_ms: f64,
}

/// `pipeline.clock`: the programme clock, its base time, and where every
/// pipeline in this process sits against it.
pub fn clock() -> Result<ClockReport> {
    let programme = pipeline(PROGRAMME).or_else(|| pipeline("program"));
    let clock = programme.as_ref().and_then(|p| p.clock());
    let clock_time = clock.as_ref().map(|c| c.time());
    let mut pipelines = Vec::new();
    for name in names() {
        let Some(p) = pipeline(&name) else { continue };
        let base = p.base_time();
        pipelines.push(PipelineClock {
            name,
            state: format!("{:?}", p.current_state()),
            base_time_ms: base.map(ms),
            running_time_ms: running_time(&p).map(ms),
        });
    }
    Ok(ClockReport {
        clock: clock.map(|c| c.name().to_string()),
        clock_time_ms: clock_time.map(ms),
        base_time_ms: programme.as_ref().and_then(|p| p.base_time()).map(ms),
        running_time_ms: programme.as_ref().and_then(running_time).map(ms),
        pipelines,
        // Whatever each node last said on its heartbeat. Read rather than
        // asked: the client clock on the node already knows its own offset,
        // and asking across the network to find out how far the network is
        // off would be a strange way round.
        nodes: crate::node::runtime::get()
            .map(|runtime| {
                runtime
                    .nodes
                    .views()
                    .into_iter()
                    .map(|view| NodeClock {
                        node: view.name,
                        offset_ms: view.clock_offset_ms,
                        jitter_ms: view.clock_jitter_ms,
                    })
                    .collect()
            })
            .unwrap_or_default(),
    })
}

fn running_time(p: &gst::Pipeline) -> Option<gst::ClockTime> {
    let clock = p.clock()?;
    let base = p.base_time()?;
    clock.time().checked_sub(base)
}

// --- core.startup_report -----------------------------------------------------

/// Anything slower than this gets named in the report. From 09 section 4
/// item 6: a plugin that takes a quarter of a second to start is a plugin the
/// author should be told about, before their users are.
pub const SLOW_MS: f64 = 250.0;

#[derive(Debug, Clone, Serialize)]
pub struct Stage {
    pub name: String,
    /// "stage" for a core step, "source" or "output" for a plugin instance.
    pub kind: String,
    pub started_ms: f64,
    pub took_ms: f64,
    pub slow: bool,
}

#[derive(Debug, Clone, Serialize)]
pub struct StartupReport {
    pub total_ms: f64,
    pub stages: Vec<Stage>,
    /// The names over 250 ms, already picked out.
    pub slow: Vec<String>,
    pub threshold_ms: f64,
}

static ORIGIN: LazyLock<std::time::Instant> = LazyLock::new(std::time::Instant::now);
static STAGES: LazyLock<Mutex<Vec<Stage>>> = LazyLock::new(Default::default);

/// Start the startup clock. Called on the first line of `run` so that every
/// stage's `started_ms` is measured from the same moment.
pub fn begin() {
    LazyLock::force(&ORIGIN);
}

/// Time one startup stage. The timer records when it is dropped, so a stage is
/// one line at the top of the block it measures.
pub struct StageTimer {
    name: String,
    kind: &'static str,
    started: f64,
    at: std::time::Instant,
}

impl StageTimer {
    fn start(name: String, kind: &'static str) -> Self {
        let at = std::time::Instant::now();
        Self { started: at.duration_since(*ORIGIN).as_secs_f64() * 1000.0, name, kind, at }
    }
}

impl Drop for StageTimer {
    fn drop(&mut self) {
        let took_ms = self.at.elapsed().as_secs_f64() * 1000.0;
        let slow = took_ms > SLOW_MS;
        if slow {
            tracing::info!(
                stage = %self.name, kind = self.kind, took_ms,
                "slow to start, over the 250 ms budget"
            );
        }
        STAGES.lock().push(Stage {
            name: std::mem::take(&mut self.name),
            kind: self.kind.to_string(),
            started_ms: self.started,
            took_ms,
            slow,
        });
    }
}

/// Time a core startup step: reading the config, building the mixer, binding
/// the control port.
pub fn stage(name: impl Into<String>) -> StageTimer {
    StageTimer::start(name.into(), "stage")
}

/// Time one plugin instance starting: a source or an output being built.
pub fn plugin_stage(kind: &'static str, instance: &str) -> StageTimer {
    StageTimer::start(instance.to_string(), kind)
}

/// `core.startup_report` and what `--startup-report` prints.
pub fn startup_report() -> StartupReport {
    let mut stages = STAGES.lock().clone();
    stages.sort_by(|a, b| a.started_ms.total_cmp(&b.started_ms));
    let slow = stages.iter().filter(|s| s.slow).map(|s| s.name.clone()).collect();
    let total_ms = stages
        .iter()
        .map(|s| s.started_ms + s.took_ms)
        .fold(0.0f64, f64::max);
    StartupReport { total_ms, stages, slow, threshold_ms: SLOW_MS }
}

/// The report as a person reads it, one line per stage with the slow ones
/// marked, then a verdict.
pub fn format_startup_report(report: &StartupReport) -> String {
    use std::fmt::Write as _;
    let mut out = String::new();
    let _ = writeln!(out, "startup: {:.0} ms in total", report.total_ms);
    let width = report.stages.iter().map(|s| s.name.len()).max().unwrap_or(4).clamp(4, 40);
    for s in &report.stages {
        let _ = writeln!(
            out,
            "  {:>8.1} ms  {:<width$}  {:<6} {}",
            s.took_ms,
            s.name,
            s.kind,
            if s.slow { "SLOW, over 250 ms" } else { "" },
            width = width
        );
    }
    if report.slow.is_empty() {
        let _ = writeln!(out, "nothing took longer than {:.0} ms", report.threshold_ms);
    } else {
        let _ = writeln!(out, "over {:.0} ms: {}", report.threshold_ms, report.slow.join(", "));
    }
    out
}

#[cfg(test)]
mod tests {
    use super::*;

    fn test_pipeline(name: &str) -> gst::Pipeline {
        gst::init().expect("gstreamer");
        let pipeline = gst::Pipeline::with_name(name);
        let src = gst::ElementFactory::make("videotestsrc")
            .property("is-live", true)
            .build()
            .expect("videotestsrc");
        let queue = gst::ElementFactory::make("queue")
            .name("pgm-vq")
            .property("max-size-buffers", 30u32)
            .build()
            .expect("queue");
        let sink = gst::ElementFactory::make("fakesink")
            .property("sync", false)
            .build()
            .expect("fakesink");
        pipeline.add_many([&src, &queue, &sink]).unwrap();
        gst::Element::link_many([&src, &queue, &sink]).unwrap();
        pipeline
    }

    #[test]
    fn dot_answers_graphviz_for_a_registered_pipeline() {
        let p = test_pipeline("introspect-dot");
        register_pipeline("introspect-dot", &p);
        p.set_state(gst::State::Paused).unwrap();
        let text = dot("introspect-dot").expect("dot data");
        assert!(text.contains("digraph"), "{text}");
        assert!(text.contains("videotestsrc"), "{text}");
        p.set_state(gst::State::Null).unwrap();
        unregister_pipeline("introspect-dot");
    }

    #[test]
    fn an_unknown_name_says_what_is_known_rather_than_just_no() {
        let e = dot("no-such-source").unwrap_err().to_string();
        assert!(e.contains("no-such-source"), "{e}");
        assert!(e.contains("Known:"), "{e}");
    }

    #[test]
    fn queues_reports_fill_against_the_limit() {
        let p = test_pipeline("introspect-queues");
        register_pipeline("introspect-queues", &p);
        p.set_state(gst::State::Paused).unwrap();
        let fills = queues("introspect-queues").expect("queues");
        let q = fills.iter().find(|q| q.element == "pgm-vq").expect("the queue");
        assert_eq!(q.max_buffers, 30);
        assert!((0.0..=1.0).contains(&q.fullest));
        p.set_state(gst::State::Null).unwrap();
        unregister_pipeline("introspect-queues");
    }

    #[test]
    fn latency_answers_for_a_live_pipeline() {
        let p = test_pipeline("introspect-latency");
        register_pipeline("introspect-latency", &p);
        p.set_state(gst::State::Playing).unwrap();
        // Let it preroll so the latency query has something to answer with.
        let _ = p.state(gst::ClockTime::from_seconds(5));
        let report = latency("introspect-latency").expect("latency");
        assert_eq!(report.pipeline, "introspect-latency");
        assert!(report.min_ms.is_some(), "a running pipeline should answer a latency query");
        p.set_state(gst::State::Null).unwrap();
        unregister_pipeline("introspect-latency");
    }

    #[test]
    fn the_clock_report_names_every_live_pipeline() {
        let p = test_pipeline("introspect-clock");
        register_pipeline("introspect-clock", &p);
        p.set_state(gst::State::Playing).unwrap();
        let _ = p.state(gst::ClockTime::from_seconds(5));
        let report = clock().expect("clock");
        assert!(report.pipelines.iter().any(|c| c.name == "introspect-clock"), "{report:?}");
        p.set_state(gst::State::Null).unwrap();
        unregister_pipeline("introspect-clock");
    }

    #[test]
    fn a_dropped_pipeline_stops_being_registered() {
        {
            let p = test_pipeline("introspect-gone");
            register_pipeline("introspect-gone", &p);
            assert!(pipeline("introspect-gone").is_some());
            p.set_state(gst::State::Null).unwrap();
        }
        assert!(pipeline("introspect-gone").is_none(), "a weak reference kept a pipeline alive");
        assert!(!names().contains(&"introspect-gone".to_string()));
    }

    #[test]
    fn the_startup_report_names_what_took_longer_than_the_budget() {
        begin();
        {
            let _quick = stage("observe-test-quick");
        }
        {
            let _slow = plugin_stage("source", "observe-test-slow");
            std::thread::sleep(std::time::Duration::from_millis(260));
        }
        let report = startup_report();
        assert!(report.slow.contains(&"observe-test-slow".to_string()), "{report:?}");
        assert!(!report.slow.contains(&"observe-test-quick".to_string()), "{report:?}");
        let text = format_startup_report(&report);
        assert!(text.contains("observe-test-slow"), "{text}");
        assert!(text.contains("SLOW"), "{text}");
    }
}
