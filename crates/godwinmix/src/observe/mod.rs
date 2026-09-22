//! The observability surfaces that need a server or a person.
//!
//! The instrumentation itself is `godwinmix_core::observe`: the metrics
//! registry, the session log, the log layer, the trace id and the pipeline
//! introspection. It runs inside an embedded engine with nothing serving.
//!
//! What is here is what a client reaches:
//!
//! | Surface | Here | Over the wire |
//! |---|---|---|
//! | `/metrics`, `/debug/*` | [`routes`] | `GET /metrics` |
//! | `log.*`, `pipeline.*` | [`methods`] | the method table |
//!
//! `gmx doctor`, `gmx logs`, `gmx trace`, `gmx dot` and `gmx support-bundle`
//! are in `crate::cli::observe`, with the rest of the command line.

pub mod methods;
pub mod routes;

pub use methods::{doctor_config_path, register};
pub use routes::{router, ObserveState};

/// Middleware that counts and times every call, for the api agent's `/rpc`
/// router.
///
/// Used as `router.layer(observe::rpc_layer())`. It records
/// `gmx_rpc_calls_total{method, code}` and `gmx_rpc_duration_ms{method}`,
/// taking the method from the matched route rather than the URI so that a path
/// carrying an id does not become an unbounded label.
///
/// `axum::middleware::from_fn` names its own types, so this is a macro rather
/// than a function: `from_fn` returns a `FromFnLayer` over the closure's
/// anonymous future type, which cannot be written down in a signature. The
/// call site reads the same either way.
#[macro_export]
macro_rules! rpc_layer {
    () => {
        ::axum::middleware::from_fn($crate::observe::routes::rpc_metrics)
    };
}

/// A temporary directory for a test, cleaned up by the test that made it.
#[cfg(test)]
pub(crate) fn tempdir(tag: &str) -> std::path::PathBuf {
    let dir = std::env::temp_dir().join(format!(
        "gmx-observe-{tag}-{}-{:?}",
        std::process::id(),
        std::thread::current().id()
    ));
    let _ = std::fs::remove_dir_all(&dir);
    std::fs::create_dir_all(&dir).expect("temp dir");
    dir
}
