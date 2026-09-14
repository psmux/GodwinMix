//! The regression corpus: every session log in `tests/sessions/` is replayed
//! against a test core and graded on what it did to the world.
//!
//! This is step 5 of the loop in 10 section 5. A field bug becomes a file, the
//! file becomes a test, and the test fails until the bug is fixed. See
//! `tests/sessions/README.md` for how to add one.

use std::path::PathBuf;

fn corpus_dir() -> PathBuf {
    PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("../../tests/sessions")
}

/// Every recorded session replays to the state changes recorded beside it.
///
/// One test rather than one per file: the cases run one after another anyway
/// (each builds a pipeline) and a single failure message that names the case
/// is easier to act on than a list of test names to look up.
#[tokio::test(flavor = "multi_thread", worker_threads = 4)]
async fn every_recorded_session_replays_to_the_same_state_changes() {
    let dir = corpus_dir();
    let corpus = godwinmix::cli::session::corpus(&dir).expect("the corpus directory");
    assert!(
        corpus.len() >= 3,
        "the corpus should hold at least the take sequence, the ad break and the source \
         stall; it holds {}",
        corpus.len()
    );

    let mut failures = Vec::new();
    for (name, log) in &corpus {
        let expect = godwinmix::cli::session::expectations_for(log);
        assert!(
            expect.is_some(),
            "{name} has no expect_changes.json beside it. Write one with \
             `gmx session replay {} --against test-core --write-expectations`.",
            log.display()
        );
        let options = godwinmix::cli::session::Options {
            expect,
            // The recorded gaps are what put the events in the order the night
            // put them in, so the corpus runs at the speed it was recorded at.
            fast: false,
            ..Default::default()
        };
        match godwinmix::cli::session::replay(log, &options).await {
            Ok(outcome) if outcome.passed() => {}
            Ok(outcome) => failures.push(format!("{name}:\n{}", outcome.report())),
            Err(e) => failures.push(format!("{name}: the replay would not run: {e:#}")),
        }
    }
    assert!(failures.is_empty(), "\n{}", failures.join("\n"));
}

/// The corpus is only worth having if a difference actually fails it.
///
/// A regression test that passes whatever the code does is the failure mode
/// worth guarding against, so this one checks the grader by feeding it a
/// session whose expectations say something else happened.
#[tokio::test(flavor = "multi_thread", worker_threads = 4)]
async fn a_session_whose_expectations_are_wrong_fails() {
    let log = corpus_dir().join("takes.jsonl");
    let wrong = std::env::temp_dir().join("gmx-corpus-wrong-expectations.json");
    std::fs::write(
        &wrong,
        r#"{"changes": [{"what": "program", "to": "a-source-that-was-never-taken"}]}"#,
    )
    .expect("writing the wrong expectations");
    let options = godwinmix::cli::session::Options {
        expect: Some(wrong.clone()),
        fast: true,
        ..Default::default()
    };
    let outcome = godwinmix::cli::session::replay(&log, &options).await.expect("it ran");
    assert!(!outcome.passed(), "a wrong expectation has to fail:\n{}", outcome.report());
    assert!(
        outcome.report().contains("a-source-that-was-never-taken"),
        "the failure names what was expected:\n{}",
        outcome.report()
    );
    let _ = std::fs::remove_file(&wrong);
}
