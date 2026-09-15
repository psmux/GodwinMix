//! `gmx plugin test` for a tier W plugin.
//!
//! The same eight checks of 03 section 11, minus the six that are about a
//! process and a pipeline. A component has no pid to kill, no media end to
//! count buffers on and no footprint a process sampler can read, so what is
//! left is the manifest, the load, `configure`, and the contract of whichever
//! kind it is. Every one of them runs in this process: there is nothing to
//! spawn, which is also why a tier W plugin's tests run on a CI box with no
//! GStreamer at all.
//!
//! ```text
//!   1 manifest     gmx-plugin.toml validates, and [run] wasm names a file
//!   2 component    it loads, it hand shakes, and it declares what it answers
//!   3 configure    every example in the settings schema is taken
//!   4 contract     a service answers its hooks; a transition drives a pad
//!   5 offline      the recorded transcript, replayed with no core at all
//! ```

use super::harness::{CheckResult, Report};
use super::wasm::{self, Grant, Spec};
use super::ProvideKind;
use anyhow::{Context, Result};
use godwinmix_protocol::plugin::manifest::Manifest as PluginManifest;
use godwinmix_protocol::plugin::transcript::{self, Step};
use serde_json::{json, Value};
use std::path::Path;
use std::sync::Arc;
use std::time::{Duration, Instant};

/// Everything `gmx plugin test` runs against a component.
pub fn check(root: &Path, manifest: &PluginManifest) -> Result<Report> {
    let provide = manifest
        .provides
        .iter()
        .find(|p| p.kind == "service" || p.kind == "transition")
        .or_else(|| manifest.provides.first())
        .context("the manifest declares no provides, so there is nothing to check")?;
    let type_id = format!("{}/{}", manifest.plugin.name, provide.id);
    let mut report = Report { type_id, checks: Vec::new() };
    report.checks.push(super::harness::check_manifest(root));
    let kind = ProvideKind::parse(&provide.kind);

    let started = Instant::now();
    let loaded = start(root, manifest, &provide.id, kind);
    match &loaded {
        Ok(instance) => report.checks.push(CheckResult::pass(
            "component",
            format!(
                "loaded and hand shook in {} ms; tools: {}; hooks: {}",
                started.elapsed().as_millis(),
                list(&instance.tools()),
                list(&instance.hooks())
            ),
        )),
        Err(e) => {
            report.checks.push(CheckResult::fail("component", format!("{e:#}")));
            return Ok(report);
        }
    }
    let instance = loaded.expect("checked above");
    report.checks.push(CheckResult::pass(
        "media",
        format!(
            "a {} provide at the `wasm` placement carries no media, so checks 2, 3 and 6 do \
             not apply",
            provide.kind
        ),
    ));
    report.checks.push(configure(&instance, root, provide.settings.as_deref()));
    report.checks.push(match kind {
        ProvideKind::Transition => transition(&instance),
        _ => service(&instance, manifest),
    });
    instance.shutdown("the harness is done");
    Ok(report)
}

/// Load one component, with the grants a test gives: no WASI, no core calls.
///
/// Deliberately less than a running core grants, because a plugin that only
/// works with a filesystem should fail the harness rather than fail on the
/// operator's machine when they have not written `allow_wasi`.
fn start(
    root: &Path,
    manifest: &PluginManifest,
    provide: &str,
    kind: ProvideKind,
) -> Result<Arc<dyn wasm::Instance>> {
    let file = manifest
        .run
        .as_ref()
        .and_then(|r| r.wasm.as_ref())
        .context("[run] wasm names no component file")?;
    let component = root.join(file);
    anyhow::ensure!(
        component.is_file(),
        "there is no `{file}` under `{}`. Build it with `cargo build --release --target \
         wasm32-wasip2` and copy the .wasm here, or run dev/build-wasm.sh.",
        root.display()
    );
    wasm::start(Spec {
        plugin: manifest.plugin.name.clone(),
        provide: provide.to_string(),
        instance: format!("{}-{provide}", manifest.plugin.name),
        kind,
        component,
        root: root.to_path_buf(),
        canvas: super::harness::test_canvas(),
        params: Value::Null,
        grant: Grant { core_call: false, ..Grant::default() },
    })
}

/// Every example the settings schema carries, taken one at a time.
fn configure(instance: &Arc<dyn wasm::Instance>, root: &Path, settings: Option<&str>) -> CheckResult {
    let Some(settings) = settings else {
        return CheckResult::pass("configure", "the provide declares no settings schema");
    };
    let examples = super::harness::schema_examples(&root.join(settings));
    if examples.is_empty() {
        return CheckResult::pass(
            "configure",
            "the settings schema carries no examples, so only the defaults were tried",
        );
    }
    for params in &examples {
        match instance.call("configure", json!({ "params": params })) {
            Ok(answer) => {
                let applied = answer.get("applied").and_then(Value::as_bool).unwrap_or(false);
                let restart =
                    answer.get("restart_required").and_then(Value::as_bool).unwrap_or(false);
                if !applied && !restart {
                    return CheckResult::fail(
                        "configure",
                        format!(
                            "`configure` with {params} answered neither applied nor \
                             restart_required. One of the two has to be true, or the core \
                             cannot tell whether the change took."
                        ),
                    );
                }
            }
            Err(e) => {
                return CheckResult::fail(
                    "configure",
                    format!("`configure` with {params} was refused: {e:#}"),
                )
            }
        }
    }
    CheckResult::pass("configure", format!("{} settings examples were taken", examples.len()))
}

/// A transition drives something, and drives only what the core can write.
fn transition(instance: &Arc<dyn wasm::Instance>) -> CheckResult {
    let named = ["sink_0", "sink_1", "sink_2", "sink_3"];
    let mut seen: Vec<(f64, f64)> = Vec::new();
    for progress in [0.0f64, 0.5, 1.0] {
        let request = json!({
            "from": ["sink_0", "sink_1"],
            "to": ["sink_2", "sink_3"],
            "progress": progress,
            "running_time_ns": (progress * 1_000_000_000.0) as u64,
        });
        let answer = match instance.call_within("render", request, Duration::from_millis(200)) {
            Ok(v) => v,
            Err(e) => {
                return CheckResult::fail(
                    "transition",
                    format!("`render` at progress {progress} was refused: {e:#}"),
                )
            }
        };
        if let Some(curve) = answer.get("curve") {
            return match curve.as_array().map(Vec::len).unwrap_or(0) {
                n if n >= 2 => CheckResult::pass(
                    "transition",
                    format!("answered once with a curve of {n} points"),
                ),
                n => CheckResult::fail(
                    "transition",
                    format!(
                        "a `curve` answer needs at least two points, as [[t, progress], ...] \
                         with both in 0 to 1; this one has {n}"
                    ),
                ),
            };
        }
        match pads(&answer, &named, progress) {
            Ok(mut points) => seen.append(&mut points),
            Err(why) => return CheckResult::fail("transition", why),
        }
    }
    let first = seen.iter().find(|(at, _)| *at == 0.0).map(|(_, v)| *v);
    let last = seen.iter().rev().find(|(at, _)| *at == 1.0).map(|(_, v)| *v);
    if first.is_some() && first == last {
        return CheckResult::fail(
            "transition",
            "`render` answers the same thing at progress 0 and at progress 1, so nothing \
             would move. A transition has to go somewhere.",
        );
    }
    CheckResult::pass("transition", format!("rendered at 0, 0.5 and 1; {} values", seen.len()))
}

/// The pad values of one `render` answer, checked against what the core writes.
fn pads(answer: &Value, named: &[&str], progress: f64) -> Result<Vec<(f64, f64)>, String> {
    let Some(table) = answer.get("pads").and_then(Value::as_object) else {
        return Err(format!(
            "`render` at progress {progress} answered {answer}, and the shape is \
             {{pads: {{<pad>: {{alpha, xpos, ...}}}}}} or {{curve: [[t, progress], ...]}}"
        ));
    };
    let mut out = Vec::new();
    for (pad, values) in table {
        if !named.contains(&pad.as_str()) {
            return Err(format!(
                "`render` drove a pad called `{pad}`, and the pads it was offered are {}. A \
                 transition may only drive the pads in `from` and `to`.",
                named.join(", ")
            ));
        }
        let Some(values) = values.as_object() else {
            return Err(format!("the value for pad `{pad}` is not an object of properties"));
        };
        for (property, value) in values {
            if !crate::mixer::transition::DRIVEN.contains(&property.as_str()) {
                return Err(format!(
                    "`{property}` is not a property the core writes. It writes: {}.",
                    crate::mixer::transition::DRIVEN.join(", ")
                ));
            }
            match value.as_f64() {
                Some(v) if v.is_finite() => out.push((progress, v)),
                _ => {
                    return Err(format!("`{pad}.{property}` is {value}, which is not a number"))
                }
            }
        }
    }
    Ok(out)
}

/// A service answers every hook it declared, and refuses only where it may.
fn service(instance: &Arc<dyn wasm::Instance>, manifest: &PluginManifest) -> CheckResult {
    let health = match instance.call("health", json!({})) {
        Ok(h) => h,
        Err(e) => return CheckResult::fail("service", format!("`health` was refused: {e:#}")),
    };
    let state = health.get("state").and_then(Value::as_str).unwrap_or("");
    if !["ok", "degraded", "failing"].contains(&state) {
        return CheckResult::fail(
            "service",
            format!("`health` answered state `{state}`; it is ok, degraded or failing"),
        );
    }
    let declared: Vec<String> = instance.hooks();
    for hook in manifest.hooks.keys() {
        if !declared.iter().any(|h| h == hook) {
            return CheckResult::fail(
                "service",
                format!(
                    "the manifest asks for the `{hook}` hook and `initialize` did not name it, \
                     so the core would never call it. Add it to the `Ready` the handshake \
                     answers with."
                ),
            );
        }
    }
    for hook in &declared {
        let payload = json!({"hook": hook, "ts": "1970-01-01T00:00:00Z", "payload": {}});
        match instance.call_within("hook", payload, Duration::from_millis(100)) {
            Ok(answer) if answer.is_object() => {}
            Ok(answer) => {
                return CheckResult::fail(
                    "service",
                    format!("the `{hook}` hook answered {answer}, and a hook answers an object"),
                )
            }
            Err(e) => {
                return CheckResult::fail(
                    "service",
                    format!("the `{hook}` hook was refused: {e:#}"),
                )
            }
        }
    }
    CheckResult::pass(
        "service",
        format!("health is {state}; {} hooks answered: {}", declared.len(), list(&declared)),
    )
}

// ---------------------------------------------------------------------------
// The offline transcript
// ---------------------------------------------------------------------------

/// Replay a recorded transcript against a component, with no core at all.
///
/// The same file a process plugin is replayed from, read the same way. Two
/// differences, both because a component is not a process: `initialize` and
/// `initialized` happen at load and are skipped where they appear, and there
/// is no `start` or `stop`, because nothing at this placement carries media.
pub fn replay(root: &Path, manifest: &PluginManifest, text: &str) -> Result<String> {
    let provide = manifest
        .provides
        .iter()
        .find(|p| p.kind == "service" || p.kind == "transition")
        .or_else(|| manifest.provides.first())
        .context("the manifest declares no provides")?;
    let kind = ProvideKind::parse(&provide.kind);
    let instance = start(root, manifest, &provide.id, kind)?;
    let steps = transcript::steps(text).map_err(|e| anyhow::anyhow!("{e}"))?;
    let mut pending: Option<(usize, Value, Value)> = None;
    let mut matched = 0usize;
    for (line, step) in steps {
        match step {
            Step::Core(call) => {
                let method = call.get("method").and_then(Value::as_str).unwrap_or("");
                if method.is_empty() || method == "initialize" || method == "initialized" {
                    continue;
                }
                let params = call.get("params").cloned().unwrap_or(json!({}));
                let id = call.get("id").cloned().unwrap_or(Value::Null);
                let answer = match instance.call(method, params) {
                    Ok(result) => json!({"jsonrpc": "2.0", "id": id, "result": result}),
                    Err(e) => json!({
                        "jsonrpc": "2.0", "id": id,
                        "error": {"code": -32001, "message": format!("{e:#}")}
                    }),
                };
                pending = Some((line, call, answer));
            }
            Step::Plugin(want) => {
                let Some((at, _, got)) = pending.take() else {
                    anyhow::bail!(
                        "line {line} expects an answer and no call came before it. A transcript \
                         alternates {{\"core\": ...}} and {{\"plugin\": ...}}."
                    );
                };
                if !transcript::matches(&want, &got) {
                    instance.shutdown("the transcript did not match");
                    anyhow::bail!(
                        "line {line} (answering the call on line {at}): {}",
                        transcript::explain(&want, &got)
                    );
                }
                matched += 1;
            }
            Step::Ignored => {}
        }
    }
    instance.shutdown("the transcript is done");
    Ok(format!("{matched} answers matched, with no core and no process"))
}

fn list(items: &[String]) -> String {
    if items.is_empty() {
        return "none".to_string();
    }
    items.join(", ")
}
