//! The current call's trace id, in a task local.
//!
//! The id itself is `godwinmix_protocol::trace::TraceId`: sixteen bytes,
//! written as thirty two lowercase hex characters, so a caller who already
//! speaks OpenTelemetry can hand us a `traceparent` header and get its id back
//! out of our logs unchanged.
//!
//! What is here is where the current one lives. Every async handler that wants
//! its log lines correlated runs its body inside `with_trace_id`, and anything
//! deeper in the call, including synchronous code, reads it back with
//! `current_trace_id` without being passed an argument. A task local rather
//! than a thread local because a handler can move between worker threads
//! between two awaits, and a thread local would be read by the wrong task.

pub use godwinmix_protocol::trace::{incoming, TraceId};

tokio::task_local! {
    static CURRENT: TraceId;
}

/// The trace id of the call this code is running inside, if there is one.
///
/// `None` outside a call: the supervisor's own decisions, a bus message, a
/// watchdog tick. Those lines carry no `trace_id` and that is correct, they
/// were not caused by anybody.
pub fn current_trace_id() -> Option<TraceId> {
    CURRENT.try_with(|t| *t).ok()
}

/// Run a future with `id` as the current trace id.
pub async fn with_trace_id<F: std::future::Future>(id: TraceId, fut: F) -> F::Output {
    CURRENT.scope(id, fut).await
}

#[cfg(test)]
mod tests {
    use super::*;

    #[tokio::test]
    async fn the_current_id_is_visible_inside_the_call_and_not_outside_it() {
        assert_eq!(current_trace_id(), None);
        let id = TraceId::new();
        let seen = with_trace_id(id, async {
            tokio::task::yield_now().await;
            current_trace_id()
        })
        .await;
        assert_eq!(seen, Some(id));
        assert_eq!(current_trace_id(), None);
    }
}
