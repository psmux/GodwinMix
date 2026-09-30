//! What to do when a show's process ends: pure, so the rules are tested
//! without starting anything.

use godwinmix_host::lifecycle::{Backoff, FREE_RESTARTS};
use std::time::Duration;

/// Failures in a row, past the free ones, before a show is left failed.
pub const PAID_RESTARTS: u32 = 3;
/// A show that ran this long before dying starts its count again.
pub const STABLE: Duration = Duration::from_secs(60);

/// How the process ended.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Exit {
    /// Status zero: it was shut down on purpose (`core.shutdown`).
    Clean,
    /// The restart code: it asked to be started again (`core.restart`).
    Restart,
    /// Anything else: a crash, a signal, a kill.
    Died,
}

impl Exit {
    pub fn of(status: &std::io::Result<std::process::ExitStatus>, restart_code: i32) -> Exit {
        match status {
            Ok(s) if s.success() => Exit::Clean,
            Ok(s) if s.code() == Some(restart_code) => Exit::Restart,
            _ => Exit::Died,
        }
    }
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum Next {
    /// Leave it stopped.
    Stop,
    /// Start it again after `wait`. `counted` is false for a restart it
    /// asked for, which is not a failure.
    Again { wait: Duration, counted: bool },
    /// It kept dying: leave it failed.
    Fail { times: u32 },
}

/// The count of failures in a row, with the plugin host's backoff.
#[derive(Debug, Default)]
pub struct Tally {
    backoff: Backoff,
}

impl Tally {
    pub fn after(&mut self, exit: Exit, ran: Duration) -> Next {
        if ran > STABLE {
            self.backoff.clear();
        }
        match exit {
            Exit::Clean => Next::Stop,
            Exit::Restart => Next::Again { wait: Duration::ZERO, counted: false },
            Exit::Died => {
                self.backoff.next_wait();
                let times = self.backoff.attempts();
                match times > FREE_RESTARTS + PAID_RESTARTS {
                    true => Next::Fail { times },
                    false => Next::Again { wait: self.backoff.wait(), counted: true },
                }
            }
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    const SOON: Duration = Duration::from_secs(1);

    #[test]
    fn three_deaths_are_restarted_at_once_then_it_waits_then_it_is_left_failed() {
        let mut t = Tally::default();
        for _ in 0..FREE_RESTARTS {
            assert_eq!(t.after(Exit::Died, SOON), Next::Again { wait: Duration::ZERO, counted: true });
        }
        assert_eq!(t.after(Exit::Died, SOON), Next::Again { wait: Duration::from_secs(30), counted: true });
        assert_eq!(t.after(Exit::Died, SOON), Next::Again { wait: Duration::from_secs(60), counted: true });
        assert_eq!(t.after(Exit::Died, SOON), Next::Again { wait: Duration::from_secs(120), counted: true });
        assert_eq!(t.after(Exit::Died, SOON), Next::Fail { times: FREE_RESTARTS + PAID_RESTARTS + 1 });
    }

    #[test]
    fn a_show_that_ran_a_while_before_dying_starts_its_count_again() {
        let mut t = Tally::default();
        for _ in 0..5 {
            t.after(Exit::Died, SOON);
        }
        assert_eq!(t.after(Exit::Died, STABLE * 2), Next::Again { wait: Duration::ZERO, counted: true });
    }

    #[test]
    fn a_restart_it_asked_for_is_not_a_failure_and_a_clean_exit_stops_it() {
        let mut t = Tally::default();
        for _ in 0..10 {
            assert_eq!(t.after(Exit::Restart, SOON), Next::Again { wait: Duration::ZERO, counted: false });
        }
        assert_eq!(t.after(Exit::Clean, SOON), Next::Stop);
        assert_eq!(t.after(Exit::Died, SOON), Next::Again { wait: Duration::ZERO, counted: true });
    }

    #[cfg(unix)]
    #[test]
    fn an_exit_status_is_read_as_clean_restart_or_died() {
        use std::os::unix::process::ExitStatusExt;
        let status = |raw: i32| Ok(std::process::ExitStatus::from_raw(raw));
        assert_eq!(Exit::of(&status(0), 75), Exit::Clean);
        assert_eq!(Exit::of(&status(75 << 8), 75), Exit::Restart);
        assert_eq!(Exit::of(&status(9), 75), Exit::Died, "killed by a signal");
        assert_eq!(Exit::of(&status(1 << 8), 75), Exit::Died);
    }
}
