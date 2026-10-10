//! The thread that watches a WHIP output and dials again when it is down.
//!
//! It redials on a pipeline error, on ICE `failed` or `closed`, on ICE
//! `disconnected` that lasts, and on an attempt that never comes up (see
//! `redial`), with the backoff in the settings and for as long as the output
//! exists.

use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::Arc;
use std::time::Instant;

use gmx_netkit::backoff::Backoff;
use godwinmix_sdk::wire::HealthState;

use super::{build, Shared, TICK_MS};
use crate::redial::Watch;

/// Watch the pipeline, and dial again with a backoff when it fails.
pub(super) fn spawn(
    shared: Arc<Shared>,
    stop: Arc<AtomicBool>,
) -> Option<std::thread::JoinHandle<()>> {
    std::thread::Builder::new()
        .name("gmx-whip-supervise".into())
        .spawn(move || {
            let mut backoff =
                Backoff::with(shared.settings.reconnect_first_ms, shared.settings.reconnect_max_ms);
            let mut last_state = HealthState::Degraded;
            let mut watch = Watch::default();
            watch.built(Instant::now());
            while !stop.load(Ordering::Relaxed) {
                std::thread::sleep(std::time::Duration::from_millis(TICK_MS));
                let health = shared.health();
                if health.state != last_state {
                    last_state = health.state;
                    if let Some(r) = &shared.reporter {
                        r.health_changed(health.clone());
                    }
                } else if let Some(r) = &shared.reporter {
                    r.set_health(health.clone());
                }
                if health.state == HealthState::Ok {
                    backoff.reset();
                    watch.redial(false, true, None, Instant::now());
                    continue;
                }
                let (broken, ice) = match shared.pipe.lock().unwrap_or_else(|e| e.into_inner()).as_ref() {
                    Some(p) => (p.failure().is_some(), shared.ice_state(p)),
                    None => (true, None),
                };
                let Some(why) = watch.redial(broken, false, ice.as_deref(), Instant::now()) else {
                    continue;
                };
                let wait = backoff.take();
                if let Some(r) = &shared.reporter {
                    r.warn(format!(
                        "the WHIP endpoint is not taking the programme ({why}). Dialling \
                         again in {} ms (attempt {}), and for as long as it takes.",
                        wait.as_millis(),
                        backoff.attempts()
                    ));
                }
                if sleep_unless_stopped(&stop, wait) {
                    return;
                }
                reconnect(&shared);
                watch.built(Instant::now());
            }
        })
        .ok()
}

/// Sleep in small pieces so `stop` is honoured inside a long backoff.
fn sleep_unless_stopped(stop: &Arc<AtomicBool>, wait: std::time::Duration) -> bool {
    let mut left = wait;
    let slice = std::time::Duration::from_millis(TICK_MS);
    while !left.is_zero() {
        if stop.load(Ordering::Relaxed) {
            return true;
        }
        let step = left.min(slice);
        std::thread::sleep(step);
        left -= step;
    }
    stop.load(Ordering::Relaxed)
}

/// Tear the pipeline down and build a fresh one.
fn reconnect(shared: &Arc<Shared>) {
    if let Some(mut old) = shared.pipe.lock().unwrap_or_else(|e| e.into_inner()).take() {
        old.stop();
    }
    shared.reconnects.fetch_add(1, Ordering::Relaxed);
    match build(shared) {
        Ok(fresh) => {
            *shared.pipe.lock().unwrap_or_else(|e| e.into_inner()) = Some(fresh);
        }
        Err(e) => {
            if let Some(r) = &shared.reporter {
                r.error(format!("the WHIP pipeline would not rebuild: {e}"));
            }
        }
    }
}
