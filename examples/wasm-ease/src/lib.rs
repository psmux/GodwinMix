//! A crossfade that starts and ends gently, answered once.
//!
//! The smallest interesting transition, and the one worth copying. It answers
//! `render` with a curve rather than with pad values, which means the core
//! asks it exactly once per take and then binds the curve to the compositor
//! pads as a control source. The component is not on the frame path and after
//! that one call it is not on any path at all: the aggregator samples the
//! control source per output frame with no plugin in the loop.
//!
//! A transition that answers per frame instead is asked up to 65 times inside
//! a 200 ms budget, still before the transition window opens. Either way the
//! component never runs while frames are going out, which is the whole of
//! `docs/explanation/why-wasm-is-not-on-the-frame-path.md`.

use godwinmix_sdk_wasm::{export_transition, Answer, Hello, Ready, Transition};
use serde_json::Value;

/// How many points the curve has.
///
/// 32 is finer than any frame rate a 300 ms transition will be sampled at, and
/// the core interpolates linearly between them, so the corners are below what
/// a viewer can see. More points cost bytes on the boundary and buy nothing.
const POINTS: usize = 32;

pub struct Ease {
    points: usize,
}

impl Transition for Ease {
    fn initialize(hello: &Hello) -> Result<(Self, Ready), String> {
        let points = hello.param_u64("points", POINTS as u64).clamp(2, 256) as usize;
        Ok((Ease { points }, Ready::new("wasm-ease", "0.2.0")))
    }

    fn render(&mut self, request: &Value) -> Result<Answer, String> {
        // The request carries `from`, `to`, `progress` and `running_time_ns`.
        // A curve answer ignores `progress`: it describes the whole window at
        // once, and the core samples it. Reading the pads is still worth it,
        // because a transition asked to move nothing should say so rather than
        // hand back a curve that drives nothing.
        let empty = |key: &str| {
            request.get(key).and_then(Value::as_array).is_none_or(|pads| pads.is_empty())
        };
        if empty("from") && empty("to") {
            return Err(
                "there are no pads on either side of this take, so there is nothing to ease"
                    .to_string(),
            );
        }
        Ok(Answer::smoothstep(self.points))
    }
}

export_transition!(Ease);
