//! The `Transition` trait and the macro that exports it.
//!
//! A transition component decides what the pads do, never what the pixels are.
//! It answers `render` with pad values for one sample, or once with a curve
//! the core binds to the compositor as a control source. The curve is the
//! answer worth giving: it is asked for once per take and costs the frame path
//! nothing at all.

use crate::Hello;
use serde_json::Value;

/// What `render` answers with.
pub enum Answer {
    /// `{"<pad id>": {"alpha": .., "xpos": ..}}` for this one sample. The host
    /// puts the `pads` key on.
    Pads(Value),
    /// `[[t, progress], ...]` with both in 0 to 1, the whole transition
    /// described once. The host puts the `curve` key on. The core stops asking
    /// after an answer of this kind.
    Curve(Value),
}

impl Answer {
    /// An ease in and out curve over `steps` points, which is what most
    /// transitions want and nobody should have to write twice.
    ///
    /// Smoothstep: `3t^2 - 2t^3`. It starts and ends with zero slope, so the
    /// movement has no corner at either end, and it needs no state.
    pub fn smoothstep(steps: usize) -> Answer {
        Answer::Curve(Value::Array(
            (0..=steps.max(2))
                .map(|i| {
                    let t = i as f64 / steps.max(2) as f64;
                    let p = t * t * (3.0 - 2.0 * t);
                    Value::Array(vec![number(t), number(p)])
                })
                .collect(),
        ))
    }
}

fn number(v: f64) -> Value {
    serde_json::Number::from_f64(v).map(Value::Number).unwrap_or(Value::Null)
}

/// A transition's logic, as a component.
pub trait Transition: Sized {
    fn initialize(hello: &Hello) -> Result<(Self, crate::Ready), String>;

    /// `request` is `{from, to, progress, running_time_ns, duration_ms}`.
    fn render(&mut self, request: &Value) -> Result<Answer, String>;
}

/// Export a [`Transition`] as a component.
///
/// The component it produces also exports the `service` interface, answering
/// -32601 there, so the same binary satisfies either of the host's two worlds.
#[macro_export]
macro_rules! export_transition {
    ($t:ty) => {
        const _: () = {
            $crate::__glue!($t, transition);
        };
    };
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn smoothstep_starts_at_nothing_ends_at_everything_and_is_flat_at_both_ends() {
        let Answer::Curve(Value::Array(points)) = Answer::smoothstep(10) else {
            panic!("smoothstep is a curve");
        };
        assert_eq!(points.len(), 11);
        assert_eq!(points[0], serde_json::json!([0.0, 0.0]));
        assert_eq!(points[10], serde_json::json!([1.0, 1.0]));
        let at = |i: usize| points[i][1].as_f64().expect("a number");
        assert!(at(1) < 0.1, "it should leave slowly, not jump: {}", at(1));
        assert!(at(9) > 0.9, "and arrive slowly: {}", at(9));
        assert!((at(5) - 0.5).abs() < 1e-9, "and be symmetric: {}", at(5));
    }
}
