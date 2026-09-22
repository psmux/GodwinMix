//! One error shape, everywhere.
//!
//! `{"error": {"code", "message", "data"}}` on `/api/v1`, and the same object
//! as the JSON-RPC `error` member on `/rpc`. The code table is 03 section 6.
//!
//! The rule every constructor here follows: the message names the current
//! state and the next step. "no such source" is useless to an agent; "source
//! 'cam9' does not exist. Sources: cam1, cam2" is a repair instruction.

use schemars::JsonSchema;
use serde::{Deserialize, Serialize};
use serde_json::{json, Map, Value};

/// The codes in 03 section 6, plus the four JSON-RPC standard ones.
///
/// Held as an enum rather than as loose integers so that the table in
/// `protocol.json` and the codes the server actually returns are the same
/// list, and a new code cannot be invented at a call site.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize, JsonSchema)]
#[serde(rename_all = "snake_case")]
pub enum ErrorCode {
    ParseError,
    InvalidRequest,
    MethodNotFound,
    InvalidParams,
    InternalError,
    /// Not in a state that allows this: a source not live, a plugin not ready.
    NotInState,
    /// Refused by scope.
    Scope,
    /// Refused by safety: minimum hold, rate limit.
    Safety,
    /// No such id, plugin or node.
    NotFound,
    /// The plugin never declared that placement.
    Placement,
    /// The plugin died during the call.
    PluginDied,
    /// A line over 4 MiB arrived on a stdio transport.
    LineTooLong,
    /// `configure` says the instance has to be restarted.
    RestartRequired,
    /// The token's confirm policy is `required` and this call is destructive.
    ConfirmationRequired,
}

impl ErrorCode {
    pub const fn number(self) -> i64 {
        match self {
            Self::ParseError => -32700,
            Self::InvalidRequest => -32600,
            Self::MethodNotFound => -32601,
            Self::InvalidParams => -32602,
            Self::InternalError => -32603,
            Self::NotInState => -32001,
            Self::Scope => -32002,
            Self::Safety => -32003,
            Self::NotFound => -32004,
            Self::Placement => -32005,
            Self::PluginDied => -32010,
            Self::LineTooLong => -32011,
            Self::RestartRequired => -32012,
            Self::ConfirmationRequired => -32020,
        }
    }

    /// What a client should read the code as, one line, for `protocol.json`.
    pub const fn meaning(self) -> &'static str {
        match self {
            Self::ParseError => "the body was not JSON",
            Self::InvalidRequest => "the envelope was not a JSON-RPC request",
            Self::MethodNotFound => "no such method",
            Self::InvalidParams => "the params were wrong for this method",
            Self::InternalError => "the core failed while handling the call",
            Self::NotInState => "not in a state that allows this",
            Self::Scope => "refused by the token's scopes",
            Self::Safety => "refused by a safety rule",
            Self::NotFound => "no such id",
            Self::Placement => "the plugin did not declare that placement",
            Self::PluginDied => "the plugin died during the call",
            Self::LineTooLong => "a protocol line was over 4 MiB",
            Self::RestartRequired => "the change needs the instance restarted",
            Self::ConfirmationRequired => "a confirm token is needed first",
        }
    }

    /// Whether trying the same call again can ever work. `Sometimes` means it
    /// can once the condition the message names has changed.
    pub const fn retryable(self) -> bool {
        matches!(
            self,
            Self::NotInState
                | Self::Safety
                | Self::PluginDied
                | Self::ConfirmationRequired
                | Self::InternalError
        )
    }

    /// The HTTP status `/api/v1` answers this code with.
    ///
    /// A code an HTTP client can branch on without reading the body, which is
    /// what a shell script and a load balancer both want.
    pub const fn http_status(self) -> u16 {
        match self {
            Self::NotFound => 404,
            Self::Scope => 403,
            Self::Safety => 429,
            Self::NotInState => 409,
            Self::ConfirmationRequired => 428,
            Self::MethodNotFound => 404,
            Self::InternalError | Self::PluginDied => 500,
            _ => 400,
        }
    }

    pub const ALL: &'static [ErrorCode] = &[
        Self::ParseError,
        Self::InvalidRequest,
        Self::MethodNotFound,
        Self::InvalidParams,
        Self::InternalError,
        Self::NotInState,
        Self::Scope,
        Self::Safety,
        Self::NotFound,
        Self::Placement,
        Self::PluginDied,
        Self::LineTooLong,
        Self::RestartRequired,
        Self::ConfirmationRequired,
    ];
}

/// One refusal, ready to be written as JSON-RPC or as an HTTP body.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize, JsonSchema)]
pub struct RpcError {
    pub code: i64,
    /// Names the current state and the next step. Never "bad request".
    pub message: String,
    /// Whatever a client needs to act without parsing the message. Always an
    /// object, always carrying `retryable`.
    pub data: Value,
}

impl RpcError {
    pub fn new(code: ErrorCode, message: impl Into<String>) -> Self {
        Self {
            code: code.number(),
            message: message.into(),
            data: json!({ "retryable": code.retryable() }),
        }
    }

    /// Add one key to `data`. Chained at the call site so the interesting
    /// fields sit next to the message that mentions them.
    pub fn with(mut self, key: &str, value: impl Into<Value>) -> Self {
        if let Some(map) = self.data.as_object_mut() {
            map.insert(key.to_string(), value.into());
        }
        self
    }

    /// Put the button a person can press into `data.action`. See `action.rs`.
    pub fn with_action(self, action: crate::action::ErrorAction) -> Self {
        let value = action.to_value();
        self.with("action", value)
    }

    /// An unknown id, with the ids that would have worked.
    ///
    /// Listing them is the whole point: 09 section 5 item 6 asks that an
    /// unknown id answer with the valid ids, because an agent that has to
    /// guess spends a call finding out and often guesses again.
    pub fn not_found(kind: &str, id: &str, valid: &[String]) -> Self {
        let message = if valid.is_empty() {
            format!("there is no {kind} '{id}', and no {kind} is configured. Add one first.")
        } else {
            format!(
                "there is no {kind} '{id}'. {} {}: {}. Use one of those.",
                if valid.len() == 1 { "The only" } else { "The" },
                if valid.len() == 1 { kind.to_string() } else { format!("{kind}s") },
                valid.join(", ")
            )
        };
        Self::new(ErrorCode::NotFound, message)
            .with("id", id)
            .with("kind", kind)
            .with("valid", valid.to_vec())
    }

    /// Refused because the token does not carry the scope.
    pub fn scope(method: &str, needed: &str, held: &[String]) -> Self {
        Self::new(
            ErrorCode::Scope,
            format!(
                "{method} needs the '{needed}' scope and this token holds {}. \
                 Ask for a token with '{needed}' in its scopes.",
                if held.is_empty() { "none".to_string() } else { held.join(", ") }
            ),
        )
        .with("method", method)
        .with("needed", needed)
        .with("held", held.to_vec())
    }

    /// Refused because the params were wrong for the method.
    pub fn invalid_params(message: impl Into<String>) -> Self {
        Self::new(ErrorCode::InvalidParams, message)
    }

    pub fn internal(message: impl Into<String>) -> Self {
        Self::new(ErrorCode::InternalError, message)
    }

    /// The mixer refused for a reason of its own. Those messages already name
    /// the state and the next step, so they pass through verbatim.
    pub fn not_in_state(message: impl Into<String>) -> Self {
        Self::new(ErrorCode::NotInState, message)
    }

    /// The body `/api/v1` writes, and the shape the `error` member on `/rpc`
    /// is built from.
    pub fn body(&self, trace_id: &str) -> Value {
        json!({
            "error": { "code": self.code, "message": self.message, "data": self.data },
            "trace_id": trace_id,
        })
    }

    pub fn http_status(&self) -> u16 {
        ErrorCode::ALL
            .iter()
            .find(|c| c.number() == self.code)
            .map_or(400, |c| c.http_status())
    }

    /// Merge extra keys into `data` without losing what is there.
    pub fn with_data(mut self, extra: Map<String, Value>) -> Self {
        if let Some(map) = self.data.as_object_mut() {
            for (k, v) in extra {
                map.insert(k, v);
            }
        }
        self
    }
}

impl std::fmt::Display for RpcError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        write!(f, "{} ({})", self.message, self.code)
    }
}

impl std::error::Error for RpcError {}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn the_code_table_matches_the_protocol_document() {
        assert_eq!(ErrorCode::NotInState.number(), -32001);
        assert_eq!(ErrorCode::Scope.number(), -32002);
        assert_eq!(ErrorCode::Safety.number(), -32003);
        assert_eq!(ErrorCode::NotFound.number(), -32004);
        assert_eq!(ErrorCode::ConfirmationRequired.number(), -32020);
        assert_eq!(ErrorCode::MethodNotFound.number(), -32601);
        assert_eq!(ErrorCode::InvalidParams.number(), -32602);
        // Every code is in ALL exactly once, because ALL is what generates the
        // table clients read.
        let mut numbers: Vec<i64> = ErrorCode::ALL.iter().map(|c| c.number()).collect();
        let count = numbers.len();
        numbers.sort_unstable();
        numbers.dedup();
        assert_eq!(numbers.len(), count);
    }

    /// 09 section 5 item 7 asks that every error name a concrete next step.
    /// An unknown id is the case that comes up most, so it lists the ids that
    /// would have worked.
    #[test]
    fn an_unknown_id_answers_with_the_ids_that_exist() {
        let e = RpcError::not_found("source", "cam9", &["cam1".into(), "cam2".into()]);
        assert_eq!(e.code, -32004);
        assert!(e.message.contains("cam9"), "{}", e.message);
        assert!(e.message.contains("cam1, cam2"), "{}", e.message);
        assert_eq!(e.data["valid"][1], "cam2");
        assert_eq!(e.data["retryable"], false);

        // One id reads as English rather than as a template.
        let one = RpcError::not_found("output", "yt", &["primary".into()]);
        assert!(one.message.contains("The only output: primary"), "{}", one.message);

        // None at all says what to do instead of listing nothing.
        let none = RpcError::not_found("source", "cam1", &[]);
        assert!(none.message.contains("Add one first"), "{}", none.message);
    }

    #[test]
    fn a_scope_refusal_names_the_scope_to_ask_for() {
        let e = RpcError::scope("source.remove", "operate", &["read".into()]);
        assert_eq!(e.code, -32002);
        assert!(e.message.contains("operate"), "{}", e.message);
        assert!(e.message.contains("read"), "{}", e.message);
        assert_eq!(e.http_status(), 403);
    }

    #[test]
    fn the_body_is_the_one_shape_everywhere() {
        let e = RpcError::new(ErrorCode::Safety, "held for 3200 ms more").with("retry_after_ms", 3200);
        let b = e.body("0af7651916cd43dd8448eb211c80319c");
        assert_eq!(b["error"]["code"], -32003);
        assert_eq!(b["error"]["data"]["retry_after_ms"], 3200);
        assert_eq!(b["error"]["data"]["retryable"], true);
        assert_eq!(b["trace_id"], "0af7651916cd43dd8448eb211c80319c");
        assert_eq!(e.http_status(), 429);
    }
}
