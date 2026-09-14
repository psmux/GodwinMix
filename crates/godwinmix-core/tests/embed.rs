//! The embedding example is a test, so `cargo add godwinmix-core` cannot quietly
//! stop working.
//!
//! `examples/embed.rs` is included as a module rather than copied, so there is
//! one piece of code and the documentation cannot drift from what runs. CI
//! builds the example as well, which is what catches a missing dependency
//! rather than a missing behaviour.

#[path = "../examples/embed.rs"]
mod embed;

/// Real GStreamer elements, as every other test here uses: the engine starts,
/// colour bars reach programme, and the mixer thread joins when told to stop.
#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
async fn the_engine_runs_inside_another_program() {
    let status = embed::run()
        .await
        .expect("the embedded engine should come up");
    assert_eq!(
        status.program.as_deref(),
        Some("bars"),
        "colour bars should be on programme"
    );
    assert_eq!(status.sources.len(), 1, "the one source that was added");
}
