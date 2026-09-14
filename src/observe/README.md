# `src/observe/`

Logs, metrics, the session log and pipeline introspection, behind one
correlation id. Phase 0 of the observability contract: everything here works on
today's architecture and none of it waits for the traits.

| File | What is in it |
|---|---|
| `logs.rs` | the `tracing` layer, the level tables, GStreamer debug, file rotation |
| `metrics.rs` | the registry, the Prometheus renderer, the programme frame probe |
| `session.rs` | the append only JSONL session log and the event recorder |
| `introspect.rs` | the pipeline registry, dot, latency, queues, clock, startup report |
| `doctor.rs` | the environment checks |
| `trace.rs` | `TraceId`, W3C `traceparent`, the task local |
| `bundle.rs` | the store only zip writer, config redaction, `gmx support-bundle` |
| `routes.rs` | the axum router the control plane merges |
| `cli.rs` | `gmx doctor`, `logs`, `trace`, `dot`, `support-bundle` |

Operator documentation is in `docs/how-to/debug-a-show.md`; the metric list is
`docs/reference/metrics.md`.

## What other modules call

Five entry points, and nothing else in this module is meant to be called from
outside it.

```rust
// Once from `run`, after the mixer is built. Opens the log files and the
// session log, and starts the task that records every event.
observe::start(&handle, &observe::Options { config_path, startup_report })?;

// Merged into the control plane's router. One line in `control::serve`.
router.merge(observe::router(observe::ObserveState { .. }))

// Around the api agent's `/rpc` router. Counts and times every call.
router.layer(rpc_layer!())

// Anything worth replaying later.
observe::session().record("command", json!({ .. }));

// The current call's id, wherever you are inside it.
let id: Option<observe::TraceId> = observe::current_trace_id();
```

## Introspection signatures

The api agent wires the RPC names; these are the functions behind them. All of
them take the name a caller typed, which may be a source id, an output id,
`programme` or `multiview`, and resolve it against the pipelines that are
actually running.

```rust
// pipeline.dot {instance | "programme" | "multiview"}
pub fn introspect::dot(name: &str) -> anyhow::Result<String>;

// pipeline.latency {instance}
pub fn introspect::latency(name: &str) -> anyhow::Result<LatencyReport>;
pub struct LatencyReport {
    pub pipeline: String,
    pub live: bool,
    pub min_ms: Option<f64>,
    pub max_ms: Option<f64>,
    pub stages: Vec<StageLatency>,       // { element, live, min_ms, max_ms }
}

// pipeline.queues {instance}
pub fn introspect::queues(name: &str) -> anyhow::Result<Vec<QueueFill>>;
pub struct QueueFill {
    pub element: String,
    pub buffers: u32, pub bytes: u32, pub time_ms: f64,
    pub max_buffers: u32, pub max_bytes: u32, pub max_time_ms: f64,
    pub fullest: f64,                    // 0.0 to 1.0, sorted fullest first
}

// pipeline.clock
pub fn introspect::clock() -> anyhow::Result<ClockReport>;
pub struct ClockReport {
    pub clock: Option<String>,
    pub clock_time_ms: Option<f64>,
    pub base_time_ms: Option<f64>,
    pub running_time_ms: Option<f64>,
    pub pipelines: Vec<PipelineClock>,   // { name, state, base_time_ms, running_time_ms }
    pub nodes: Vec<NodeClock>,           // empty until 04 lands
}

// core.startup_report
pub fn introspect::startup_report() -> StartupReport;
pub struct StartupReport {
    pub total_ms: f64,
    pub stages: Vec<Stage>,              // { name, kind, started_ms, took_ms, slow }
    pub slow: Vec<String>,               // the names over 250 ms
    pub threshold_ms: f64,               // 250
}

// Every pipeline that is running, by the name these calls accept.
pub fn introspect::names() -> Vec<String>;
```

Every one of them errors with the list of known names rather than a bare "not
found", because the caller's next question is always "well what is there".

## Log control signatures

```rust
// log.set {instance, level} and log.set {target, level}.
// `None` for the level removes the override.
pub fn logs::set_instance_level(instance: &str, level: Option<LevelCode>);
pub fn logs::set_target_level(target: &str, level: Option<LevelCode>);
pub fn logs::set_default_level(level: LevelCode);
pub fn logs::levels() -> serde_json::Value;

// log.gst {instance, categories, duration_secs}. Answers the categories it
// raised; they go back down on their own after the duration.
pub fn logs::set_gst_debug(
    instance: Option<&str>, categories: &str, duration_secs: u64,
) -> anyhow::Result<Vec<String>>;
pub fn logs::gst_debug_in_force() -> Vec<(String, u64)>;

pub struct LevelCode;  // OFF, ERROR, WARN, INFO, DEBUG, TRACE; LevelCode::parse
```

A target is matched as a module path prefix and the longest match wins, so
`godwinmix::mixer` also reaches `godwinmix::mixer::supervisor` and a more
specific override still beats it.

## Metrics signatures

```rust
pub fn metrics::counter(name: &'static str, labels: &[(&str, &str)]) -> Counter;
pub fn metrics::gauge(name: &'static str, labels: &[(&str, &str)]) -> Gauge;
pub fn metrics::histogram(name: &'static str, labels: &[(&str, &str)]) -> Histogram;
pub fn metrics::render() -> String;      // Prometheus text exposition

// For the api and traits agents, so nothing else has to know the metric names.
pub fn metrics::record_rpc(method: &str, code: &str, millis: f64);
pub fn metrics::record_take_ack(millis: f64);
pub fn metrics::record_take_refused(reason: &str);
pub fn metrics::set_multiview_subscribers(n: usize);
```

Handles are `Arc`s over atomics. Take one once, outside the loop, and call
`inc`, `set` or `observe` on the hot path; the registry lock is only taken when
a series is first created and when `/metrics` is scraped.

## Where this module touches other people's files

Six lines, all of them a call into here.

| File | Function | Line |
|---|---|---|
| `mixer.rs` | `Mixer::build` | `crate::observe::attach_programme(&program, &vraw_tee);` |
| `input.rs` | `InputPipeline::build_kind` | `let _observe = crate::observe::source_span(&id);` |
| `input.rs` | `InputPipeline::build_kind` | `crate::observe::register_pipeline(&format!("input-{id}"), &pipeline);` |
| `output.rs` | `OutputSlot::attach` | `let _observe = crate::observe::output_span(id);` |
| `output.rs` | `OutputSlot::spin_up` | `crate::observe::register_pipeline(&format!("output-{id}"), &pipeline);` |
| `multiview.rs` | `Multiview::build` | `crate::observe::register_pipeline("multiview", &pipeline);` |

Plus one merge in `control::serve` and the module's own wiring in `lib.rs`.

`source_span` and `output_span` return a guard that does two things: it enters
a `tracing` span carrying `instance`, which is what makes `log.set {instance}`
reach that source's lines, and it times the build for `--startup-report`. Hold
it for the scope you want tagged.

## Cross platform

Three places differ between Windows, macOS and Linux, and each has an explicit
arm plus a fallback that reports "unknown" rather than guessing.

| What | Unix | Windows | Anything else |
|---|---|---|---|
| Is stderr a terminal (the log format default) | `isatty` | `GetConsoleMode` | false, so JSON |
| Free disk under the runtime directory | `statvfs` | `GetDiskFreeSpaceExW` | `None`, a warning |
| Physical memory (the gallery default) | `/proc/meminfo` on Linux, `sysctlbyname` on macOS | `GlobalMemoryStatusEx` | `None`, a warning |

Everything else is portable by construction. Log rotation is `std::fs::rename`
and `remove_file` only, with the file handle dropped before the rename because
Windows will not rename an open file. The runtime directory is a path relative
to the config, with no home directory, XDG variable or registry lookup. Zip
entry names use forward slashes, which is what the format requires everywhere,
and the default bundle name has no colon in it because Windows would refuse it.

The platform arms are checked against all three targets with
`cargo check --target`; they need no GStreamer, so the check runs anywhere.

## What is deliberately not here

* Per node metrics and clock offsets. The fields exist in `ClockReport` so the
  shape does not change when 04 lands, but nothing fills them.
* `configure_log` forwarding to sidecars. There is no sidecar host yet (P2).
  `set_gst_debug` already takes the instance argument it will need.
* `gmx session replay`, `gmx bisect`, `gmx chaos`, `gmx stats`, `gmx events`.
  P2 and P5.
* OpenTelemetry export. P6, optional, and `TraceId` is already a W3C trace id
  so it needs no translation when it comes.
