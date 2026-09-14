//! The failing test. It is meant to fail until you replace it.
//!
//! `./check` runs it last, and `./check --quick` skips it. It fails on a fresh
//! template on purpose, because a template that passes out of the box teaches
//! nothing and a green tick on an unfinished plugin is a lie.
//!
//! Replace it with a check of the picture your plugin actually draws. The unit
//! tests in `src/main.rs` already cover the frame size and the bar values; this
//! is the place for the thing only you know, such as "the clock reads the time
//! the settings asked for" or "the scoreboard shows the away team on the right".
//!
//! Delete `deliberately_failing` when you have.

#[test]
fn deliberately_failing() {
    panic!(
        "tests/your_picture.rs has not been written yet. Replace it with a check of \
         the picture your plugin draws, then delete this test. Run ./check --quick \
         to skip it while you work."
    );
}
