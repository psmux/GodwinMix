//! The observability methods as rows in the control plane's table.
//!
//! Pipeline introspection and log control were HTTP routes and nothing else,
//! which meant an agent on `/rpc` could not reach them and `protocol.json`
//! did not know they existed. They are methods now, so they answer on `/rpc`,
//! on `/api/v1`, in the reference and in the MCP tool list from one
//! declaration.
//!
//! The REST paths those methods generate are the paths `routes.rs` already
//! serves, and it goes on serving them: two of the answers are not JSON (a
//! dot graph is graphviz text, the session log is ndjson) and a client that
//! pipes them wants the bytes, not a string inside an object. A test in this
//! file fails if the two ever name different paths.

use godwinmix_protocol::error::RpcError;
use godwinmix_protocol::method::{any_object, schema_of, MethodDef, Registry, Tier};
use godwinmix_protocol::scope::Scope;
use crate::control::call::Call;
use crate::control::methods::{body, handler};
use schemars::JsonSchema;
use serde::{Deserialize, Serialize};
use serde_json::json;

/// Every path this module's methods sit on, which `routes.rs` also serves.
/// The test at the bottom keeps the two lists the same.
pub const PATHS: &[(&str, &str, &str)] = &[
    ("POST", "/api/v1/log/set", "log.set"),
    ("POST", "/api/v1/log/gst", "log.gst"),
    ("GET", "/api/v1/log/levels", "log.levels"),
    ("GET", "/api/v1/pipeline/dot", "pipeline.dot"),
    ("GET", "/api/v1/pipeline/latency", "pipeline.latency"),
    ("GET", "/api/v1/pipeline/queues", "pipeline.queues"),
    ("GET", "/api/v1/pipeline/clock", "pipeline.clock"),
    ("GET", "/api/v1/pipeline/list", "pipeline.list"),
    ("GET", "/api/v1/core/startup_report", "core.startup_report"),
    ("GET", "/api/v1/core/doctor", "core.doctor"),
    ("GET", "/api/v1/core/session_log", "core.session_log"),
];

/// One line in `control::methods::registry`.
pub fn register(reg: &mut Registry<Call>) {
    register_logs(reg);
    register_pipeline(reg);
    register_core(reg);
}

// --- log control -----------------------------------------------------------

/// `log.set`. Name an instance or a target, not both.
#[derive(Debug, Clone, Serialize, Deserialize, JsonSchema)]
pub struct LogSetRequest {
    /// A plugin instance: a source or an output id. Its lines carry the id,
    /// so raising this one raises only that camera.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub instance: Option<String>,
    /// A module path prefix such as `godwinmix::mixer`. The longest match
    /// wins, so a more specific override still beats a broader one.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub target: Option<String>,
    /// `off`, `error`, `warn`, `info`, `debug`, `trace`, or `default` to stop
    /// overriding this one.
    pub level: String,
}

/// `log.gst`.
#[derive(Debug, Clone, Serialize, Deserialize, JsonSchema)]
pub struct LogGstRequest {
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub instance: Option<String>,
    /// `GST_DEBUG` spelling: `rtmp2src:6,rtpjitterbuffer:5`.
    pub categories: String,
    /// How long before it goes back down. A minute by default, which is long
    /// enough to reproduce a fault and short enough that a forgotten firehose
    /// stops on its own.
    #[serde(default = "default_gst_secs")]
    pub duration_secs: u64,
}

fn default_gst_secs() -> u64 {
    60
}

/// What `log.gst` answers with.
#[derive(Debug, Clone, Serialize, Deserialize, JsonSchema)]
pub struct LogGstResult {
    /// The categories actually raised, which is what the caller asked for
    /// with anything GStreamer does not know dropped.
    pub categories: Vec<String>,
    pub duration_secs: u64,
}

fn register_logs(reg: &mut Registry<Call>) {
    reg.register(
        MethodDef::new(
            "log.set",
            Scope::Admin,
            "Change one instance's or one module's log level while the mixer runs.",
            handler(|call: Call, params| async move {
                let req: LogSetRequest = call.params(&params)?;
                let level = if req.level.eq_ignore_ascii_case("default") {
                    None
                } else {
                    Some(godwinmix_core::observe::logs::LevelCode::parse(&req.level).ok_or_else(|| {
                        RpcError::invalid_params(format!(
                            "'{}' is not a level. Use off, error, warn, info, debug, trace, \
                             or default to stop overriding.",
                            req.level
                        ))
                    })?)
                };
                match (&req.instance, &req.target) {
                    (None, None) => match level {
                        Some(level) => godwinmix_core::observe::logs::set_default_level(level),
                        None => {
                            return Err(RpcError::invalid_params(
                                "name an `instance` or a `target`, or give a `level` for the \
                                 default to move to.",
                            ))
                        }
                    },
                    (Some(instance), None) => godwinmix_core::observe::logs::set_instance_level(instance, level),
                    (None, Some(target)) => godwinmix_core::observe::logs::set_target_level(target, level),
                    (Some(_), Some(_)) => {
                        return Err(RpcError::invalid_params(
                            "set an `instance` or a `target`, not both: the two would \
                             contradict each other.",
                        ))
                    }
                }
                godwinmix_core::observe::session::session().record(
                    "log.set",
                    json!({ "instance": req.instance, "target": req.target, "level": req.level }),
                );
                Ok(godwinmix_core::observe::logs::levels())
            }),
        )
        .params(schema_of::<LogSetRequest>)
        .result(any_object),
    );

    reg.register(
        MethodDef::new(
            "log.gst",
            Scope::Admin,
            "Raise GStreamer's own debug categories for a while, then let them fall back \
             on their own.",
            handler(|call: Call, params| async move {
                let req: LogGstRequest = call.params(&params)?;
                let applied = godwinmix_core::observe::logs::set_gst_debug(
                    req.instance.as_deref(),
                    &req.categories,
                    req.duration_secs,
                )
                .map_err(|e| RpcError::invalid_params(format!("{e:#}")))?;
                godwinmix_core::observe::session::session().record(
                    "log.gst",
                    json!({
                        "instance": req.instance,
                        "categories": req.categories,
                        "duration_secs": req.duration_secs,
                    }),
                );
                body(LogGstResult { categories: applied, duration_secs: req.duration_secs })
            }),
        )
        .params(schema_of::<LogGstRequest>)
        .result(schema_of::<LogGstResult>),
    );

    reg.register(
        MethodDef::new(
            "log.levels",
            Scope::Read,
            "Every log level override in force, and the GStreamer categories still raised.",
            handler(|_call: Call, _| async move {
                Ok(json!({
                    "levels": godwinmix_core::observe::logs::levels(),
                    "gst": godwinmix_core::observe::logs::gst_debug_in_force()
                        .into_iter()
                        .map(|(name, secs)| json!({ "category": name, "secs_left": secs }))
                        .collect::<Vec<_>>(),
                }))
            }),
        )
        .result(any_object),
    );
}

// --- pipeline introspection -------------------------------------------------

/// Which pipeline to look at. A source id, an output id, `programme` or
/// `multiview`. `pipeline.list` says what is running.
#[derive(Debug, Clone, Serialize, Deserialize, JsonSchema)]
pub struct PipelineRequest {
    #[serde(default = "default_pipeline")]
    pub name: String,
}

fn default_pipeline() -> String {
    godwinmix_core::observe::introspect::PROGRAMME.to_string()
}

/// What `pipeline.dot` answers with on `/rpc`. The REST route serves the same
/// graph as `text/vnd.graphviz`, so `gmx dot | dot -Tsvg` needs no unwrapping.
#[derive(Debug, Clone, Serialize, Deserialize, JsonSchema)]
pub struct PipelineDot {
    pub pipeline: String,
    /// The graph itself, in the dot language.
    pub dot: String,
}

/// Anything that could not find the pipeline it was asked about. The message
/// from `introspect` already lists the names that would have worked.
fn no_such_pipeline(e: anyhow::Error) -> RpcError {
    RpcError::not_in_state(format!("{e:#}"))
}

fn register_pipeline(reg: &mut Registry<Call>) {
    reg.register(
        MethodDef::new(
            "pipeline.list",
            Scope::Read,
            "Every pipeline running right now, by the name the other pipeline methods \
             accept.",
            handler(|_call: Call, _| async move {
                Ok(json!({ "pipelines": godwinmix_core::observe::introspect::names() }))
            }),
        )
        .result(any_object),
    );

    reg.register(
        MethodDef::new(
            "pipeline.dot",
            Scope::Read,
            "One pipeline as a graphviz graph: every element, every pad and the caps \
             negotiated between them.",
            handler(|call: Call, params| async move {
                let req: PipelineRequest = call.params(&params)?;
                let dot = godwinmix_core::observe::introspect::dot(&req.name).map_err(no_such_pipeline)?;
                body(PipelineDot { pipeline: req.name, dot })
            }),
        )
        .params(schema_of::<PipelineRequest>)
        .result(schema_of::<PipelineDot>),
    );

    reg.register(
        MethodDef::new(
            "pipeline.latency",
            Scope::Read,
            "How much delay one pipeline is carrying, and which stage put it there.",
            handler(|call: Call, params| async move {
                let req: PipelineRequest = call.params(&params)?;
                body(godwinmix_core::observe::introspect::latency(&req.name).map_err(no_such_pipeline)?)
            }),
        )
        .params(schema_of::<PipelineRequest>)
        .result(any_object)
        .tool(
            "pipeline_latency",
            Tier::Search,
            "How much delay one pipeline is carrying and which element is responsible. \
             `name` is a source id, an output id, \"programme\" or \"multiview\". Reach \
             for it when audio and video have drifted apart or the output is behind.",
        ),
    );

    reg.register(
        MethodDef::new(
            "pipeline.queues",
            Scope::Read,
            "Every queue in one pipeline with how full it is, fullest first. A queue \
             that stays full is where the trouble is.",
            handler(|call: Call, params| async move {
                let req: PipelineRequest = call.params(&params)?;
                let queues = godwinmix_core::observe::introspect::queues(&req.name).map_err(no_such_pipeline)?;
                Ok(json!({ "pipeline": req.name, "queues": body(queues)? }))
            }),
        )
        .params(schema_of::<PipelineRequest>)
        .result(any_object),
    );

    reg.register(
        MethodDef::new(
            "pipeline.clock",
            Scope::Read,
            "The clock every pipeline is running against, and how far each one has got.",
            handler(|_call: Call, _| async move {
                body(godwinmix_core::observe::introspect::clock().map_err(no_such_pipeline)?)
            }),
        )
        .result(any_object),
    );
}

// --- core reports ------------------------------------------------------------

/// `core.session_log`.
#[derive(Debug, Clone, Serialize, Deserialize, JsonSchema)]
pub struct SessionLogRequest {
    /// How far back to read, in seconds. An hour by default, a day at most.
    #[serde(default = "default_session_secs")]
    pub secs: u64,
}

fn default_session_secs() -> u64 {
    3600
}

fn register_core(reg: &mut Registry<Call>) {
    reg.register(
        MethodDef::new(
            "core.startup_report",
            Scope::Read,
            "How long each stage of the start took, and what was over the 250 ms mark.",
            handler(|_call: Call, _| async move { body(godwinmix_core::observe::introspect::startup_report()) }),
        )
        .result(any_object),
    );

    reg.register(
        MethodDef::new(
            "core.doctor",
            Scope::Read,
            "The environment checks: GStreamer, the elements, the config, the disk and \
             the ports. The same list `gmx doctor` prints.",
            handler(|call: Call, _| async move {
                // Every check is a syscall or a registry lookup, none of it
                // long, but none of it belongs on a runtime worker either.
                let path = godwinmix_core::config::path_in_force(std::path::Path::new("godwinmix.toml"));
                let checks = tokio::task::spawn_blocking(move || godwinmix_core::observe::doctor::run(&path))
                    .await
                    .map_err(|e| {
                        RpcError::internal(format!("the doctor could not run: {e}"))
                            .with("method", call.method)
                    })?;
                let ok = godwinmix_core::observe::doctor::exit_code(&checks) == 0;
                Ok(json!({ "checks": body(checks)?, "ok": ok }))
            }),
        )
        .result(any_object)
        .tool(
            "doctor",
            Tier::Search,
            "Check this machine: is GStreamer there, are the elements the config asks for \
             installed, is the config readable, is there disk left, is the control port \
             free. Run it first when something will not start at all.",
        ),
    );

    reg.register(
        MethodDef::new(
            "core.session_log",
            Scope::Admin,
            "The append only record of everything that happened, back as far as you ask.",
            handler(|call: Call, params| async move {
                let req: SessionLogRequest = call.params(&params)?;
                let lines = godwinmix_core::observe::session::session().tail_since(req.secs.min(86_400));
                Ok(json!({ "secs": req.secs, "lines": lines }))
            }),
        )
        .params(schema_of::<SessionLogRequest>)
        .result(any_object),
    );
}

#[cfg(test)]
mod tests {
    use super::*;

    /// The REST paths in `routes.rs` and the ones the transform rule gives
    /// these methods have to be the same paths, or the core answers a method
    /// at one address and documents it at another.
    #[test]
    fn every_method_sits_where_its_route_already_is() {
        let reg = crate::control::methods::registry();
        for (http, path, method) in PATHS {
            let def = reg
                .get(method)
                .unwrap_or_else(|| panic!("{method} is not in the table"));
            let rest = def
                .rest
                .as_ref()
                .unwrap_or_else(|| panic!("{method} has no REST binding"));
            assert_eq!(
                (rest.http, rest.path.as_str()),
                (*http, *path),
                "{method} is documented at a different address from the one it answers on"
            );
        }
    }

    /// And the router serves exactly those paths, so a method added here
    /// without a route, or a route left behind after a rename, fails rather
    /// than quietly 404ing.
    #[test]
    fn the_router_and_the_table_name_the_same_paths() {
        let served = super::super::routes::served_paths();
        let mut declared: Vec<String> =
            PATHS.iter().map(|(http, path, _)| format!("{http} {path}")).collect();
        declared.sort();
        let mut served: Vec<String> = served.iter().map(|s| s.to_string()).collect();
        served.sort();
        assert_eq!(declared, served);
    }
}
