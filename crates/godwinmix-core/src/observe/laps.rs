//! Where one mixer command spent its time, said once when it held the loop.
//!
//! The mixer loop serves Take and Cut, so a command that holds it for seconds
//! is a bug, and the watchdog can only say which command it was. This says
//! which step inside it. The loop opens a lap for each command; code deep in
//! the command marks the steps it passes; when the command ends and it held
//! the loop for longer than [`WORTH_A_LINE`], one line lists every step with
//! the milliseconds since the one before.
//!
//! A mark is a thread local lookup and, inside a lap, one `Instant` and a
//! push, so it costs nothing worth counting on any other thread or outside a
//! command. Nothing is kept between commands.

use std::cell::RefCell;
use std::time::{Duration, Instant};
use tracing::info;

/// A command that held the loop for longer than this gets its line. Four
/// frames at 25 fps: a Take behind it is late enough for a person to notice.
pub const WORTH_A_LINE: Duration = Duration::from_millis(100);

struct Lap {
    command: &'static str,
    start: Instant,
    last: Instant,
    steps: Vec<(&'static str, u64)>,
}

thread_local! {
    static LAP: RefCell<Option<Lap>> = const { RefCell::new(None) };
}

/// Open the lap for one command on this thread. A lap still open is closed
/// first, so a command that unwound without closing its own costs nothing.
pub fn open(command: &'static str) {
    close();
    let now = Instant::now();
    LAP.with(|l| *l.borrow_mut() = Some(Lap { command, start: now, last: now, steps: Vec::new() }));
}

/// Mark a step. Outside an open lap it does nothing.
pub fn mark(step: &'static str) {
    LAP.with(|l| {
        if let Some(lap) = l.borrow_mut().as_mut() {
            let now = Instant::now();
            lap.steps.push((step, now.duration_since(lap.last).as_millis() as u64));
            lap.last = now;
        }
    });
}

/// Close the lap and log it if it held the loop for long. Answers how long
/// the command took, or `None` when no lap was open.
pub fn close() -> Option<Duration> {
    let lap = LAP.with(|l| l.borrow_mut().take())?;
    let held = lap.start.elapsed();
    if held >= WORTH_A_LINE {
        let rest = lap.last.elapsed().as_millis() as u64;
        info!(
            command = lap.command,
            held_ms = held.as_millis() as u64,
            steps = %describe(&lap.steps, rest),
            "a command held the mixer loop; each step is the milliseconds since the one before"
        );
    }
    Some(held)
}

fn describe(steps: &[(&'static str, u64)], rest: u64) -> String {
    let mut out: Vec<String> = steps.iter().map(|(s, ms)| format!("{s} {ms}")).collect();
    out.push(format!("end {rest}"));
    out.join(", ")
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn marks_outside_a_lap_do_nothing() {
        close();
        mark("nowhere");
        assert!(close().is_none());
    }

    #[test]
    fn a_lap_measures_its_command() {
        open("test");
        mark("one");
        std::thread::sleep(Duration::from_millis(5));
        mark("two");
        let held = close().expect("a lap was open");
        assert!(held >= Duration::from_millis(5));
        assert!(close().is_none(), "closing takes the lap away");
    }

    #[test]
    fn the_line_lists_each_step_and_the_tail() {
        assert_eq!(describe(&[("build", 3), ("start", 40)], 7), "build 3, start 40, end 7");
    }
}
