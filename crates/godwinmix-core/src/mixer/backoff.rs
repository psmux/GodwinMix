//! How long to wait before trying a source again.
//!
//! Two curves, both pure so their shape can be checked without a pipeline.
//! A restart backs off from half a second to ten. A source that could not be
//! started at all starts on that same curve and, once it has failed
//! `stall.rebuild_attempts` times, moves to the rebuild backoff, which doubles
//! from `rebuild_backoff_secs` to `rebuild_backoff_max_secs`. A camera that
//! was slow or busy at boot is back within seconds; one that is unplugged for
//! the night costs a try every five minutes.

use crate::config::StallConfig;
use std::time::Duration;

/// The wait before restart number `attempt`, counting from nothing.
pub fn restart_delay(attempt: u32) -> Duration {
    Duration::from_millis((500.0 * 1.8f64.powi(attempt.min(8) as i32)).min(10_000.0) as u64)
}

/// The wait before trying again a source that has failed to start
/// `failures` times in a row.
pub fn unstarted_delay(stall: &StallConfig, failures: u32) -> Duration {
    stall
        .rebuild_delay(failures)
        .unwrap_or_else(|| restart_delay(failures))
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn a_restart_backs_off_from_half_a_second_to_ten() {
        assert_eq!(restart_delay(0), Duration::from_millis(500));
        assert!(restart_delay(1) > restart_delay(0));
        assert_eq!(restart_delay(20), Duration::from_secs(10));
    }

    #[test]
    fn a_source_that_never_started_is_tried_soon_and_then_less_often() {
        let stall: StallConfig = toml::from_str("").expect("the defaults");
        assert_eq!(unstarted_delay(&stall, 0), Duration::from_millis(500));
        assert!(unstarted_delay(&stall, 2) < Duration::from_secs(5));
        let later = unstarted_delay(&stall, stall.rebuild_attempts);
        assert_eq!(later, Duration::from_secs(stall.rebuild_backoff_secs));
        let overnight = unstarted_delay(&stall, 40);
        assert_eq!(
            overnight,
            Duration::from_secs(stall.rebuild_backoff_max_secs)
        );
    }
}
