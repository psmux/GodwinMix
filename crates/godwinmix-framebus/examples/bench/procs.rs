//! Starting the bench's own processes and reading what they report.

use std::collections::HashMap;
use std::io::{BufRead, BufReader};
use std::process::{Child, Command, Stdio};

/// Kill whatever of `kids` is still alive at monotonic time `at`.
pub fn kill_after(kids: &[Child], at: u64) {
    // A process wedged on a stalled reader (unixfdsink does that) must not
    // hang the run: whatever is still alive well after the window is killed.
    let pids: Vec<i32> = kids.iter().map(|k| k.id() as i32).collect();
    std::thread::spawn(move || {
        crate::measure::sleep_until(at);
        for pid in pids {
            // SAFETY: a signal to a child we started; harmless if it exited.
            unsafe { libc::kill(pid, libc::SIGKILL) };
        }
    });
}

pub fn spawn(args: Vec<String>) -> Child {
    Command::new(std::env::current_exe().unwrap())
        .args(args)
        .stdout(Stdio::piped())
        .spawn()
        .expect("could not start a bench process")
}

pub fn collect(mut child: Child) -> Option<HashMap<String, String>> {
    let out = BufReader::new(child.stdout.take().unwrap());
    let mut found = None;
    for line in out.lines().map_while(Result::ok) {
        if let Some(rest) = line.strip_prefix("BENCH ") {
            found = Some(
                rest.split(' ')
                    .filter_map(|kv| kv.split_once('='))
                    .map(|(k, v)| (k.into(), v.into()))
                    .collect(),
            );
        }
    }
    let _ = child.wait();
    found
}
