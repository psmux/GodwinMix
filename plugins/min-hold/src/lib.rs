//! A minimum hold, enforced by a plugin rather than by the core.
//!
//! The core has `[safety] min_hold_ms` and it is on by default (03 section 6).
//! This is the same rule written outside the core, which is the point: a
//! station whose rule is not the core's rule (a different window per channel,
//! a window that only applies to cameras, a window that lifts during an ad
//! break) writes it here and ships it as a plugin, and nobody has to patch the
//! mixer.
//!
//! It answers one hook. `take.before` is the only hook that can change a
//! decision, and it does so by answering `{"allow": false, "reason": "..."}`
//! inside the window. `take.after` is how it learns when the last take landed.
//!
//! # Which clock
//!
//! The wall clock, through `now_ms`. A minimum hold is about what a viewer
//! sees, and a viewer counts seconds, not pipeline running time. The pipeline
//! clock is available as `running_time_ns` and is the right one for scheduling
//! a take; it is the wrong one here, because it restarts when the pipeline
//! does and a restart would silently lift the rule.

use godwinmix_sdk_wasm::{
    allow, export_service, log, now_ms, refuse, tool_text, Health, Hello, Level, Ready, Service,
};
use serde_json::{json, Value};

/// The default window, the same 8 seconds `[safety] min_hold_ms` uses.
const DEFAULT_MS: u64 = 8_000;

pub struct MinHold {
    /// The window, from `[plugins.min-hold] min_hold_ms`.
    window_ms: u64,
    /// When the last take landed, on the wall clock. `None` until the first.
    last_ms: Option<u64>,
    /// How many takes this instance has refused, for the tool and the log.
    refused: u64,
}

impl MinHold {
    /// How long is left of the window, or `None` when nothing is holding.
    fn left(&self, now: u64) -> Option<u64> {
        let last = self.last_ms?;
        let gone = now.saturating_sub(last);
        (gone < self.window_ms).then(|| self.window_ms - gone)
    }
}

impl Service for MinHold {
    fn initialize(hello: &Hello) -> Result<(Self, Ready), String> {
        let window_ms = hello.param_u64("min_hold_ms", DEFAULT_MS);
        if window_ms > 60_000 {
            return Err(format!(
                "min_hold_ms is {window_ms}, which is over a minute. A hold that long is a \
                 fault, not a policy; set it under 60000."
            ));
        }
        log(Level::Info, format!("minimum hold of {window_ms} ms, enforced by the plugin"));
        Ok((
            MinHold { window_ms, last_ms: None, refused: 0 },
            Ready::new("min-hold", "0.2.0")
                .hook("take.before")
                .hook("take.after")
                .tool("hold_state"),
        ))
    }

    fn configure(&mut self, params: &Value) -> Result<godwinmix_sdk_wasm::Configured, String> {
        self.window_ms = params.get("min_hold_ms").and_then(Value::as_u64).unwrap_or(DEFAULT_MS);
        Ok(godwinmix_sdk_wasm::Configured::applied())
    }

    fn health(&mut self) -> Health {
        Health::default()
    }

    fn hook(&mut self, name: &str, _payload: &Value) -> Result<Value, String> {
        let now = now_ms();
        match name {
            "take.before" => match self.left(now) {
                Some(left) => {
                    self.refused += 1;
                    Ok(refuse(format!(
                        "the last take was {} ms ago and this channel holds a shot for {} ms. \
                         Wait {} ms, or raise min_hold_ms under [plugins.min-hold].",
                        now.saturating_sub(self.last_ms.unwrap_or(now)),
                        self.window_ms,
                        left
                    )))
                }
                None => {
                    // Armed here rather than in `take.after`, so a take that
                    // is allowed starts the window even on a core that fires
                    // no `take.after`. `take.after` corrects it to the moment
                    // the take actually landed.
                    self.last_ms = Some(now);
                    Ok(allow())
                }
            },
            "take.after" => {
                self.last_ms = Some(now);
                Ok(json!({}))
            }
            other => Ok(json!({ "ignored": other })),
        }
    }

    fn tool_call(&mut self, name: &str, _arguments: &Value) -> Result<Value, String> {
        if name != "hold_state" {
            return Err(format!("this plugin has one tool, `hold_state`, not `{name}`"));
        }
        let now = now_ms();
        let left = self.left(now).unwrap_or(0);
        let mut answer = tool_text(match left {
            0 => "nothing is holding: the next take goes through".to_string(),
            ms => format!("{ms} ms left of a {} ms hold", self.window_ms),
        });
        answer["structuredContent"] = json!({
            "window_ms": self.window_ms,
            "remaining_ms": left,
            "refused": self.refused,
        });
        Ok(answer)
    }

    fn shutdown(&mut self, reason: &str) {
        log(Level::Info, format!("minimum hold stopping: {reason}; {} refused", self.refused));
    }
}

export_service!(MinHold);
