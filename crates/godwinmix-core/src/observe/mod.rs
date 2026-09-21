//! Seeing inside a running mixer: logs, metrics, the session log and
//! introspection, behind one correlation id.
//!
//! The five surfaces of the observability contract, in the order a person
//! reaches for them:
//!
//! | Surface | Here | Over the wire |
//! |---|---|---|
//! | logs | [`logs`] | `log.set`, `log.gst`, `gmx logs` |
//! | events | the existing state broadcast | `/ws` |
//! | metrics | [`metrics`] | `GET /metrics` |
//! | session log | [`session`] | a file, append only, no RPC writes it |
//! | introspection | [`introspect`] | `pipeline.dot`, `.latency`, `.queues`, `.clock` |
//!
//! Nothing in here runs unless somebody asks for it (principle two). The
//! metrics registry is atomics that cost a scrape; the session log is one task
//! on an existing broadcast; introspection is a weak reference per pipeline
//! and a query when a caller makes one. The only permanent cost on a hot path
//! is the programme frame probe, which is two relaxed atomic adds.
//!
//! The `/metrics` route, the `log.*` and `pipeline.*` methods, the `gmx
//! doctor`, `gmx logs` and `gmx support-bundle` commands and the support
//! bundle itself are surfaces over this, and they live in the `godwinmix`
//! crate with the rest of the server. What is here is the instrumentation an
//! embedded engine gets whether or not anything is serving.
//!
//! ### What other modules call
//!
//! * `observe::start(...)` once from `run`, after the mixer is built.
//! * `observe::session().record(...)` for anything worth replaying.
//! * `observe::current_trace_id()` anywhere that wants the current call's id.
//! * `observe::source_span(id)` / `observe::output_span(id)` at the top of a
//!   build, which is what tags every line underneath with its instance.

pub mod doctor;
pub mod introspect;
pub mod logs;
pub mod metrics;
pub mod session;
pub mod trace;

pub use introspect::{register_pipeline, startup_report, unregister_pipeline, PROGRAMME};
pub use session::session;
pub use trace::{current_trace_id, with_trace_id, TraceId};

use std::path::{Path, PathBuf};

/// Where logs, the session log and anything else this process writes at
/// runtime live.
///
/// `GODWINMIX_RUNTIME_DIR` wins when it is set, which is what a container or a
/// systemd unit with `StateDirectory=` uses. Otherwise a `.godwinmix`
/// directory beside the config, which puts it next to the runtime store the
/// mixer already writes and needs no home directory, no XDG variable and no
/// platform special case. Works unchanged on Windows, macOS and Linux.
///
/// Always absolute. A plugin is a process with a working directory of its own,
/// and it is handed addresses under here: `--config godwinmix.toml`, which is
/// how anybody starts a mixer from the folder the config is in, made this
/// `.godwinmix`, and every camera then failed to bind a socket at a path that
/// only existed from where the mixer stood.
pub fn runtime_dir(config_path: &Path) -> PathBuf {
    let chosen = match crate::config::env_var("RUNTIME_DIR") {
        Some(dir) if !dir.trim().is_empty() => PathBuf::from(dir.trim()),
        _ => config_path.parent().unwrap_or(Path::new(".")).join(".godwinmix"),
    };
    std::path::absolute(&chosen).unwrap_or(chosen)
}

/// What `start` needs to know.
pub struct Options {
    /// The config in force, which decides where the runtime directory is.
    pub config_path: PathBuf,
    /// Print the startup report to stdout once everything is up.
    pub startup_report: bool,
}

/// Bring the observability surfaces up: files for the logs, the session log,
/// and the task that records every event.
///
/// One call from `run`, after the mixer is built and before the control server
/// starts. Returns the directory it settled on, for the log line that says so.
pub fn start(
    handle: &crate::mixer::MixerHandle,
    options: &Options,
) -> std::io::Result<PathBuf> {
    let dir = runtime_dir(&options.config_path);
    std::fs::create_dir_all(&dir)?;
    logs::attach_files(&dir)?;
    session::open_in(&dir)?;
    session::session().record(
        "start",
        serde_json::json!({
            "version": env!("CARGO_PKG_VERSION"),
            "config": options.config_path.display().to_string(),
            "runtime_dir": dir.display().to_string(),
        }),
    );
    session::spawn_recorder(handle.subscribe());
    Ok(dir)
}

/// Attach the programme's metrics and make it reachable by name.
///
/// The one call inside `Mixer::build`. Both halves need the same moment, just
/// after the raw video tee is created and added to the pipeline, so they are
/// one function rather than two lines a later edit could separate.
pub fn attach_programme(pipeline: &gstreamer::Pipeline, raw_video_tee: &gstreamer::Element) {
    register_pipeline(PROGRAMME, pipeline);
    metrics::attach_programme_probe(raw_video_tee);
}

/// The span that tags a source's log lines with its instance, and times how
/// long it took to start.
///
/// One line at the top of the source build. Everything logged underneath,
/// including whatever a sidecar writes back, carries `instance` and obeys
/// `log.set {instance, level}`. Hold the returned guard for the scope you want
/// tagged; dropping it records the stage in the startup report.
pub fn source_span(instance: &str) -> InstanceGuard {
    InstanceGuard::new("source", instance)
}

/// The same for an output.
pub fn output_span(instance: &str) -> InstanceGuard {
    InstanceGuard::new("output", instance)
}

/// A `tracing` span plus a startup timer, as one value so an entry point is
/// one line rather than two.
pub struct InstanceGuard {
    // Field order is drop order: the span closes before the timer records, so
    // the "slow to start" line is logged with the instance tag still on.
    _entered: tracing::span::EnteredSpan,
    _timer: introspect::StageTimer,
}

impl InstanceGuard {
    fn new(kind: &'static str, instance: &str) -> Self {
        let span = tracing::info_span!("instance", instance = instance, kind = kind);
        Self { _entered: span.entered(), _timer: introspect::plugin_stage(kind, instance) }
    }
}

/// Run `f` with everything it logs tagged as this instance.
///
/// For a callback that runs on a thread of its own, where there is no build to
/// hang a span on: the thread draining a sidecar's stderr is the one that
/// matters today. Its lines then carry the instance tag and obey
/// `log.set {instance, level}`, which is what puts a Python traceback from a
/// sidecar in the same file as the core's decision about it, at the same
/// level the operator asked for.
///
/// The span is created per call rather than held, because these callbacks are
/// `FnMut` and a held `EnteredSpan` would have to cross the closure's
/// boundary. A sidecar writing enough stderr for that allocation to matter has
/// a problem the log is about to tell you about.
pub fn in_instance<R>(instance: &str, f: impl FnOnce() -> R) -> R {
    tracing::info_span!("instance", instance = instance).in_scope(f)
}

/// A temporary directory for a test, cleaned up by the test that made it.
#[cfg(test)]
pub(crate) fn tempdir(tag: &str) -> PathBuf {
    let dir = std::env::temp_dir().join(format!(
        "gmx-observe-{tag}-{}-{:?}",
        std::process::id(),
        std::thread::current().id()
    ));
    let _ = std::fs::remove_dir_all(&dir);
    std::fs::create_dir_all(&dir).expect("temp dir");
    dir
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn the_runtime_directory_sits_beside_the_config() {
        let dir = runtime_dir(Path::new("/etc/godwinmix/godwinmix.toml"));
        assert_eq!(dir, Path::new("/etc/godwinmix/.godwinmix"));
        // A bare file name still answers a usable relative path.
        assert_eq!(runtime_dir(Path::new("godwinmix.toml")), Path::new(".godwinmix"));
    }

    #[test]
    fn a_source_span_tags_everything_underneath_with_the_instance() {
        // The guard is the whole point of the entry point instrumentation, so
        // it is asserted rather than assumed: a line logged inside it carries
        // the instance, and a line after it does not.
        let (_, lines) = crate::observe::logs::tests::with_capture(|| {
            {
                let _guard = source_span("guard-cam");
                tracing::info!("building");
            }
            tracing::info!("after");
        });
        let first: serde_json::Value = serde_json::from_str(&lines[0]).unwrap();
        let second: serde_json::Value = serde_json::from_str(&lines[1]).unwrap();
        assert_eq!(first["instance"], "guard-cam");
        assert_eq!(second["instance"], serde_json::Value::Null);
    }

    #[test]
    fn a_slow_instance_build_reaches_the_startup_report() {
        introspect::begin();
        {
            let _guard = output_span("guard-slow-output");
            std::thread::sleep(std::time::Duration::from_millis(260));
        }
        let report = startup_report();
        let stage = report
            .stages
            .iter()
            .find(|s| s.name == "guard-slow-output")
            .expect("the output should be in the report");
        assert_eq!(stage.kind, "output");
        assert!(stage.slow, "{stage:?}");
    }
}
