//! One call, from the envelope to the answer.
//!
//! Everything that happens around a method rather than inside it lives here:
//! the scope check, the confirm round trip, the idempotency replay, the dry
//! run flag and the trace id. A handler in methods.rs sees none of it and
//! simply does the work.
//!
//! The order matters and is the same on `/rpc` and on `/api/v1`, because they
//! are the same call arriving by different doors.

use godwinmix_protocol::error::{ErrorCode, RpcError};
use godwinmix_protocol::idempotency::{Lookup, Reservation};
use godwinmix_protocol::method::Registry;
use godwinmix_protocol::rpc::CallEnvelope;
use godwinmix_protocol::scope::{ConfirmPolicy, Token};
use godwinmix_protocol::MutationMeta;
use crate::control::AppState;
use godwinmix_core::snapshot::Tracker;
use serde::de::DeserializeOwned;
use serde_json::{json, Value};
use std::sync::Arc;
use tracing::{info, warn};

/// What a handler is given.
///
/// Cloned once per call, so every field is either a handle or a short string.
#[derive(Clone)]
pub struct Call {
    pub app: AppState,
    pub snapshots: Arc<Tracker>,
    /// Who is calling. `open` when no token is configured.
    pub token: Token,
    /// Carried into the answer, the `X-Trace-Id` header and the log line.
    pub trace_id: String,
    /// True when the caller asked what would happen instead of asking for it
    /// to happen. Only ever true on a destructive method.
    pub dry_run: bool,
    /// The method being run, so a handler can name itself in an error.
    pub method: &'static str,
}

impl Call {
    /// Read the params into the method's own request type.
    ///
    /// A failure names the field and the method rather than quoting serde, so
    /// that "missing field `uri`" becomes something a caller can act on.
    pub fn params<T: DeserializeOwned>(&self, params: &Value) -> Result<T, RpcError> {
        serde_json::from_value(params.clone()).map_err(|e| {
            RpcError::invalid_params(format!(
                "{} could not read its params: {e}. Call core.api for the schema.",
                self.method
            ))
            .with("method", self.method)
        })
    }

    /// The answer to a `dry_run` call: what would change, and whether it would.
    pub fn dry_run_answer(&self, would_change: bool, diff: Vec<String>) -> Value {
        dry_run_answer(self.method, would_change, diff)
    }

    /// The ids that exist now, for a `-32004` that names the alternatives.
    pub async fn source_ids(&self) -> Vec<String> {
        match self.app.mixer.status().await {
            Ok(s) => s.sources.iter().map(|s| s.id.clone()).collect(),
            Err(_) => Vec::new(),
        }
    }

    pub async fn output_ids(&self) -> Vec<String> {
        match self.app.mixer.status().await {
            Ok(s) => s.outputs.iter().map(|o| o.id.clone()).collect(),
            Err(_) => Vec::new(),
        }
    }

    /// The mixer's own refusals already name the state and the next step, so
    /// they pass through as `-32001` rather than being rewritten.
    pub fn mixer_error(&self, e: anyhow::Error) -> RpcError {
        // A full command queue is a state, not a failure, and the one thing a
        // caller needs from it is how long to wait.
        if let Some(busy) = e.downcast_ref::<godwinmix_core::mixer::Busy>() {
            return RpcError::not_in_state(busy.to_string())
                .with("method", self.method)
                .with("retry_after_ms", busy.retry_after_ms)
                .with("retryable", true);
        }
        RpcError::not_in_state(e.to_string()).with("method", self.method)
    }

    /// A safety rule said no. `-32003` with the time left, which is the one
    /// thing the caller needs to try again (03 section 6's code table).
    pub fn safety_error(&self, refusal: godwinmix_core::safety::Refusal) -> RpcError {
        safety_error(self.method, refusal)
    }
}

/// The same, without a `Call` in hand.
pub fn safety_error(method: &str, refusal: godwinmix_core::safety::Refusal) -> RpcError {
    RpcError::new(ErrorCode::Safety, refusal.message)
        .with("rule", refusal.rule)
        .with("retry_after_ms", refusal.retry_after_ms)
        .with("method", method)
}

/// Run one method, with everything that has to happen around it.
///
/// Long because the sequence is the contract and splitting it into six
/// functions that each do one check would hide the order that matters.
pub async fn dispatch(
    registry: &Registry<Call>,
    app: &AppState,
    snapshots: &Arc<Tracker>,
    token: &Token,
    trace_id: &str,
    method: &str,
    params: Value,
) -> Result<Value, RpcError> {
    let Some(def) = registry.get(method) else {
        let near = registry.nearest(method);
        return Err(RpcError::new(
            ErrorCode::MethodNotFound,
            format!(
                "there is no method '{method}'. {} Call core.api for the whole list.",
                if near.is_empty() {
                    String::new()
                } else {
                    format!("Nearest: {}.", near.join(", "))
                }
            ),
        )
        .with("method", method)
        .with("nearest", near.iter().map(|s| s.to_string()).collect::<Vec<_>>()));
    };

    if !token.has(def.scope) {
        return Err(RpcError::scope(method, def.scope.as_str(), &token.scope_names()));
    }
    if let Some(refusal) = rehearsal_refusal(app, method) {
        return Err(refusal);
    }

    let envelope = CallEnvelope::read(&params);
    if envelope.dry_run && !def.destructive {
        return Err(RpcError::invalid_params(format!(
            "{method} is not destructive, so dry_run has nothing to describe. \
             Call it without dry_run."
        )));
    }
    let dry_run = envelope.dry_run && def.destructive;
    // A dry run changes nothing, so there is nothing to confirm. Asking for a
    // confirm token before answering "here is what would happen" would make
    // the safer call the more awkward one.
    if def.destructive && !dry_run && token.confirm == ConfirmPolicy::Required {
        match &envelope.confirm {
            None => return Err(app.confirmations.require(method, token)),
            Some(confirm) => app.confirmations.redeem(confirm, method, token)?,
        }
    }

    // Whoever is calling is still there, which is what the operator watchdog
    // in `safety` watches for.
    app.safety.note_call(&token.id);

    // The key is claimed before the work, not after it. Two clients racing on
    // one key: the first owns it and the second waits on the same slot and
    // gets the first answer, rather than both running the work.
    let key = envelope.idempotency_key.filter(|_| def.mutating && !dry_run);
    let reservation = match &key {
        None => None,
        Some(key) => match claim(app, key, method, &params).await? {
            Claim::Replayed(body) => {
                info!(%trace_id, method, key, "replayed from the idempotency cache");
                return Ok(body);
            }
            Claim::Mine(reservation) => Some(reservation),
        },
    };

    let call = Call {
        app: app.clone(),
        snapshots: snapshots.clone(),
        token: token.clone(),
        trace_id: trace_id.to_string(),
        dry_run,
        method: def.name,
    };
    // The session log's command records, from the one place every surface's
    // calls pass through. Only the mutating ones: a log with a UI's status
    // polls in it buries the take that went wrong, and a read is not a
    // command. This is what `gmx session replay` re-issues.
    if def.mutating && !dry_run {
        godwinmix_core::observe::session::session().record_command(
            method,
            None,
            Some(&token.id),
            key.as_deref(),
            params.clone(),
        );
    }
    let started = std::time::Instant::now();
    let result = (def.handler)(call, params.clone()).await;
    let elapsed_ms = started.elapsed().as_millis() as u64;

    match result {
        Ok(mut body) => {
            if def.mutating && !dry_run {
                stamp(&mut body, def.idempotent);
                if let Some(reservation) = reservation {
                    reservation.commit(&body);
                }
            }
            info!(%trace_id, method, elapsed_ms, token = %token.id, "ok");
            Ok(body)
        }
        Err(e) => {
            warn!(%trace_id, method, elapsed_ms, code = e.code, message = %e.message, "refused");
            Err(e)
        }
    }
}

/// What claiming an idempotency key ended in.
enum Claim {
    /// This call owns the key and commits the answer when it has one.
    Mine(Reservation),
    /// Somebody else already answered this exact call.
    Replayed(Value),
}

/// Take the key, or wait for whoever has it.
///
/// A waiter never waits longer than a call is allowed to take. If the first
/// caller is still going after that, the waiter takes the key itself rather
/// than hanging: the alternative is a client that gets nothing at all, and
/// the reservation's own TTL has released the key by then.
async fn claim(
    app: &AppState,
    key: &str,
    method: &str,
    params: &Value,
) -> Result<Claim, RpcError> {
    let deadline = std::time::Instant::now()
        + std::time::Duration::from_secs(godwinmix_protocol::MAX_CALL_SECS);
    loop {
        match app.idempotency.reserve(key, method, params)? {
            Lookup::Fresh(reservation) => return Ok(Claim::Mine(reservation)),
            Lookup::Replay(body) => return Ok(Claim::Replayed(body)),
            Lookup::InFlight(wake) => {
                let left = deadline.saturating_duration_since(std::time::Instant::now());
                if left.is_zero() {
                    warn!(method, key, "the call holding this idempotency key is still running");
                    return Err(RpcError::new(
                        ErrorCode::NotInState,
                        format!(
                            "another call is still running under idempotency_key '{key}'.                              Wait and send the identical call again to get its answer."
                        ),
                    )
                    .with("idempotency", "in_flight")
                    .with("key", key)
                    .with("retry_after_ms", 500));
                }
                let _ = tokio::time::timeout(left, wake.notified()).await;
            }
        }
    }
}

/// 09 section 5 item 14: a rehearsal core refuses to start a real output, so
/// an agent rehearsing cannot put anything on a real destination by accident.
fn rehearsal_refusal(app: &AppState, method: &str) -> Option<RpcError> {
    if !app.rehearsal || method != "output.add" {
        return None;
    }
    Some(
        RpcError::new(
            ErrorCode::Safety,
            "this core was started with --rehearsal and will not add an output, so nothing \
             here reaches a real destination. Everything else works. Start a core without \
             --rehearsal to go on air.",
        )
        .with("rehearsal", true)
        .with("method", method),
    )
}

/// What a destructive method answers when it was asked what it would do.
///
/// Validated against the live state by the handler that builds `diff`, not
/// simulated: 09 section 5 item 15 is explicit that a dry run reads the real
/// thing or it is worth nothing.
pub fn dry_run_answer(method: &str, would_change: bool, diff: Vec<String>) -> Value {
    json!({
        "would_change": would_change,
        "diff": diff,
        "method": method,
        "dry_run": true,
    })
}

/// Every mutating answer says whether retrying is safe, so a client never has
/// to guess (03 section 6).
fn stamp(body: &mut Value, idempotent: bool) {
    let meta = MutationMeta { replayed: false, should_retry: idempotent };
    if let Some(map) = body.as_object_mut() {
        map.insert("should_retry".into(), json!(meta.should_retry));
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use godwinmix_protocol::method::MethodDef;
    use godwinmix_protocol::scope::Scope;

    fn registry() -> Registry<Call> {
        let mut r: Registry<Call> = Registry::new();
        r.register(MethodDef::new(
            "source.list",
            Scope::Read,
            "list",
            Arc::new(|_, _| Box::pin(async { Ok(json!([])) })),
        ));
        r.register(
            MethodDef::new(
                "source.remove",
                Scope::Operate,
                "remove",
                Arc::new(|call: Call, _| {
                    Box::pin(async move {
                        if call.dry_run {
                            return Ok(call.dry_run_answer(true, vec!["remove cam1".into()]));
                        }
                        Ok(json!({ "removed": "cam1" }))
                    })
                }),
            )
            .destructive(),
        );
        r
    }

    /// A misspelling is answered with the methods on the same noun, because
    /// an agent that gets only "no" spends its next call guessing again.
    #[test]
    fn an_unknown_method_names_the_nearest_ones() {
        let r = registry();
        let near = r.nearest("source.destroy");
        assert!(near.contains(&"source.remove"), "{near:?}");
    }

    #[test]
    fn a_mutating_answer_always_says_whether_a_retry_is_safe() {
        let mut body = json!({ "removed": "cam1" });
        stamp(&mut body, true);
        assert_eq!(body["should_retry"], true);
        let mut body = json!({ "name": "clip.mp4" });
        stamp(&mut body, false);
        assert_eq!(body["should_retry"], false);
        // A body that is not an object, such as a list, is left alone rather
        // than being wrapped into something a client did not expect.
        let mut list = json!([1, 2]);
        stamp(&mut list, true);
        assert_eq!(list, json!([1, 2]));
    }

    #[test]
    fn a_dry_run_answer_says_what_would_change_and_does_nothing() {
        let answer = dry_run_answer("source.remove", true, vec!["remove source cam1".into()]);
        assert_eq!(answer["would_change"], true);
        assert_eq!(answer["diff"][0], "remove source cam1");
        assert_eq!(answer["method"], "source.remove");
        assert_eq!(answer["dry_run"], true);

        // Nothing to do is an answer, not an error: a client asking "would
        // this change anything" gets false rather than a refusal.
        let nothing = dry_run_answer("output.remove", false, vec![]);
        assert_eq!(nothing["would_change"], false);
        assert!(nothing["diff"].as_array().unwrap().is_empty());
    }
}
