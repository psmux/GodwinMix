//! Noticing that a plugin process has gone without being asked to.
//!
//! Nothing used to. The lifecycle only ever left `running` when the core
//! stopped the plugin itself, so a plugin killed with SIGKILL, or one that
//! crashed, was still `running` to everything that asked. Its stderr reader
//! saw the end of the pipe and ended quietly, and the source went dark until
//! the mixer's stall timer gave up on it about twelve seconds later.
//!
//! The answer is one non blocking wait on the child we own, asked by whoever
//! already polls this instance: the mixer's tick for a source, the share
//! thread for a shared device. A wait is the only true answer: a process that
//! has exited and not been waited for is a zombie, its pid still allocated,
//! and `kill(pid, 0)` succeeds on it. The stderr pipe is not enough either,
//! because a helper the plugin started may hold the write end long after the
//! plugin itself is gone. The wait does not collect the child (see
//! `ExecChild::exited`), so the teardown that follows still signals the
//! plugin's whole process group and a helper left behind goes with it.

use super::Sidecar;
use godwinmix_protocol::plugin::wire::InstanceState;

impl Sidecar {
    /// Has the process exited since it was last asked? Answers `Some` once,
    /// with the reason, on the poll that first finds the process gone, and
    /// moves the lifecycle to `failed` so `plugin.list` and every caller see
    /// it. A plugin the core stopped itself is never reported here: `shutdown`
    /// lets go of the child first.
    pub fn exited(&mut self) -> Option<String> {
        let child = self.child.as_mut()?;
        if !child.exited() || matches!(self.life.state(), InstanceState::Failed) {
            return None;
        }
        let why = "the plugin process exited".to_string();
        self.life.failed(why.clone());
        // Every call still waiting is answered now rather than at its
        // deadline: nothing is left to answer it.
        self.shared.channel.abandon(&why);
        Some(why)
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::input::ExecSpec;
    #[cfg(unix)]
    use std::time::{Duration, Instant};

    fn sleeper() -> Sidecar {
        let argv = if cfg!(windows) {
            vec!["cmd".into(), "/C".into(), "ping -n 30 127.0.0.1 >NUL".into()]
        } else {
            vec!["sh".into(), "-c".into(), "sleep 30".into()]
        };
        let spec = ExecSpec { argv, env: Default::default(), pipe_stdin: true, cwd: None };
        Sidecar::spawn_spec("exit-test", spec).expect("a process starts")
    }

    #[cfg(unix)]
    #[test]
    fn a_plugin_killed_from_outside_is_reported_once_and_marked_failed() {
        let mut sidecar = sleeper();
        assert_eq!(sidecar.exited(), None, "a running process has not exited");
        let pid = sidecar.pid().expect("a pid") as i32;
        // SAFETY: a signal to the process this test just started.
        unsafe { libc::kill(pid, libc::SIGKILL) };
        let began = Instant::now();
        let mut why = None;
        while why.is_none() && began.elapsed() < Duration::from_secs(2) {
            why = sidecar.exited();
            std::thread::sleep(Duration::from_millis(10));
        }
        let why = why.expect("the exit was noticed");
        assert!(why.contains("exited"), "{why}");
        assert_eq!(sidecar.state(), InstanceState::Failed);
        assert_eq!(sidecar.exited(), None, "reported once, not on every poll");
    }

    #[test]
    fn a_plugin_the_core_stopped_is_not_reported_as_exited() {
        let mut sidecar = sleeper();
        sidecar.shutdown("the test is over");
        assert_eq!(sidecar.exited(), None);
    }
}
