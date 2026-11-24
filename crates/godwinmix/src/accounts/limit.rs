//! How many wrong passwords a sign in page will take.
//!
//! Two windows, per address and per username, both counting only failures.
//! A person who mistypes twice is not slowed down at all; a script guessing
//! from one address, or spreading its guesses at one account over many, is
//! held to a handful a quarter of an hour. A success clears that username's
//! count. In memory on purpose: a restart forgetting the count costs an
//! attacker a restart of somebody else's mixer, which they cannot cause.

use parking_lot::Mutex;
use std::collections::{HashMap, VecDeque};
use std::time::{Duration, Instant};

/// The window both counts are kept over.
pub const WINDOW: Duration = Duration::from_secs(15 * 60);
/// Failures from one address inside the window before it is refused.
pub const PER_ADDRESS: usize = 10;
/// Failures against one username inside the window before it is refused.
pub const PER_USER: usize = 5;

#[derive(Debug, Default)]
pub struct Limiter {
    inner: Mutex<Counts>,
}

#[derive(Debug, Default)]
struct Counts {
    by_address: HashMap<String, VecDeque<Instant>>,
    by_user: HashMap<String, VecDeque<Instant>>,
}

fn trim(list: &mut VecDeque<Instant>, now: Instant) {
    while list.front().is_some_and(|at| now.duration_since(*at) >= WINDOW) {
        list.pop_front();
    }
}

/// How long until the oldest failure in a full list leaves the window.
fn wait(list: &VecDeque<Instant>, now: Instant) -> Duration {
    list.front().map(|at| WINDOW.saturating_sub(now.duration_since(*at))).unwrap_or_default()
}

impl Limiter {
    /// None when a try is allowed now, else how long to wait.
    pub fn check(&self, address: &str, user: &str) -> Option<Duration> {
        let now = Instant::now();
        let mut c = self.inner.lock();
        let mut longest = None::<Duration>;
        if let Some(list) = c.by_address.get_mut(address) {
            trim(list, now);
            if list.len() >= PER_ADDRESS {
                longest = Some(wait(list, now));
            }
        }
        if let Some(list) = c.by_user.get_mut(user) {
            trim(list, now);
            if list.len() >= PER_USER {
                longest = Some(longest.unwrap_or_default().max(wait(list, now)));
            }
        }
        longest
    }

    pub fn failed(&self, address: &str, user: &str) {
        let now = Instant::now();
        let mut c = self.inner.lock();
        // Bounded: a flood of made up usernames must not grow this forever.
        if c.by_user.len() > 10_000 {
            c.by_user.retain(|_, l| l.back().is_some_and(|at| now.duration_since(*at) < WINDOW));
        }
        if c.by_address.len() > 10_000 {
            c.by_address.retain(|_, l| l.back().is_some_and(|at| now.duration_since(*at) < WINDOW));
        }
        c.by_address.entry(address.to_string()).or_default().push_back(now);
        c.by_user.entry(user.to_string()).or_default().push_back(now);
    }

    pub fn succeeded(&self, user: &str) {
        self.inner.lock().by_user.remove(user);
    }
}
