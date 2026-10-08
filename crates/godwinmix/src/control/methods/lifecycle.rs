//! `core.restart`, and what `core.info` says about it.
//!
//! The core cannot start itself again. What it can do is exit and trust that
//! something is watching: systemd with `Restart=always`, a container with a
//! restart policy, or the desktop app, which starts its mixer again when it
//! exits with [`RESTART_EXIT_CODE`]. Whether anything is watching is not
//! something a process can find out reliably, so it is told: `--supervised`,
//! or `GODWINMIX_SUPERVISED=1`, set by the service file, the compose file and
//! the desktop app's sidecar launch. A mixer started by hand has neither, and
//! then `core.restart` says so and keeps running, because a Restart button
//! that turns the programme off and leaves it off is the worst thing it
//! could do.

use std::sync::atomic::{AtomicBool, AtomicI32, Ordering};

use godwinmix_protocol::method::{schema_of, MethodDef, Registry};
use godwinmix_protocol::scope::Scope;
use godwinmix_protocol::types::{RestartAnswer, RestartHow, RestartInfo};

use super::{body, handler};
use crate::control::call::Call;

/// The exit status of a core leaving because `core.restart` asked it to. Any
/// status restarts it under systemd and Docker; the desktop app starts its
/// mixer again only on this one, so Quit stays Quit. 75 is `EX_TEMPFAIL`:
/// try again.
pub const RESTART_EXIT_CODE: i32 = 75;

static SUPERVISED: AtomicBool = AtomicBool::new(false);
/// The status to exit with once the core stops, when a method asked for one.
/// Zero is none: a clean exit.
static EXIT_WITH: AtomicI32 = AtomicI32::new(0);

/// Called once at start with what `--supervised` said. Also when this
/// process started, for `core.info`.
pub fn set_supervised(on: bool) {
    SUPERVISED.store(on, Ordering::Relaxed);
    started_ms();
}

/// When this process started, in milliseconds since the Unix epoch.
pub fn started_ms() -> u64 {
    static STARTED: std::sync::OnceLock<u64> = std::sync::OnceLock::new();
    *STARTED.get_or_init(|| {
        std::time::SystemTime::now().duration_since(std::time::UNIX_EPOCH).map(|d| d.as_millis() as u64).unwrap_or(0)
    })
}

pub fn supervised() -> bool {
    SUPERVISED.load(Ordering::Relaxed)
}

/// The status the core leaves with: [`RESTART_EXIT_CODE`] once `core.restart`
/// was accepted, one of `network.share`'s for the desktop app, or `None` for a
/// clean zero.
pub fn exit_code() -> Option<i32> {
    match EXIT_WITH.load(Ordering::Relaxed) {
        0 => None,
        code => Some(code),
    }
}

/// Stop the core and leave with `code`, for whatever starts it again to read.
pub(super) fn leave_with(call: &Call, code: i32) {
    leave(code, &call.app.quit);
}

/// The same for a process that is not a core on its own: the station.
pub fn leave(code: i32, quit: &tokio::sync::Notify) {
    EXIT_WITH.store(code, Ordering::Relaxed);
    quit.notify_one();
}

/// Once everything has stopped: exit with the status a method asked for.
/// Not a clean zero, since the desktop app starts its mixer again on these
/// statuses only, and every supervisor reads one as "start me again".
pub fn exit_if_asked() {
    if let Some(code) = exit_code() {
        tracing::info!(code, "exiting to be restarted");
        std::process::exit(code);
    }
}

/// What `core.info` reports under `restart`.
pub fn restart_info() -> RestartInfo {
    info_for(supervised())
}

fn info_for(supervised: bool) -> RestartInfo {
    match supervised {
        true => RestartInfo { possible: true, how: RestartHow::Supervised },
        false => RestartInfo { possible: false, how: RestartHow::None },
    }
}

/// The answer, without doing anything. Split out so the shapes can be tested
/// without a process to exit.
pub fn answer(supervised: bool) -> RestartAnswer {
    if supervised {
        RestartAnswer {
            restarting: true,
            how: RestartHow::Supervised,
            message: "The mixer is restarting. The programme is off air until it is back, \
                      usually within a few seconds, and this page reconnects by itself."
                .into(),
        }
    } else {
        RestartAnswer {
            restarting: false,
            how: RestartHow::None,
            message: "Nothing would start this mixer again, because it was started by hand, \
                      so it is still running. Stop it where it was started (Ctrl+C in its \
                      terminal) and start it the same way. To make restarts possible from here, \
                      run it under the systemd unit or the container in deploy/, or start it \
                      with --supervised under something that starts it again when it exits."
                .into(),
        }
    }
}

pub(super) fn register(reg: &mut Registry<Call>) {
    reg.register(
        MethodDef::new(
            "core.restart",
            Scope::Admin,
            "Stop the mixer and have it started again, when something will start it again. \
             On a supervised core (core.info restart.possible) it answers restarting: true and \
             exits; the programme is off air until it is back. On a core started by hand it \
             answers restarting: false, says how to restart it, and keeps running.",
            handler(|call: Call, _| async move {
                let supervised = supervised();
                if call.dry_run {
                    let diff = match supervised {
                        true => vec!["stop the programme, exit, and be started again by the supervisor".into()],
                        false => Vec::new(),
                    };
                    return Ok(call.dry_run_answer(supervised, diff));
                }
                if supervised {
                    tracing::info!(trace_id = %call.trace_id, token = %call.token.id, "restart requested");
                    leave_with(&call, RESTART_EXIT_CODE);
                }
                body(answer(supervised))
            }),
        )
        .result(schema_of::<RestartAnswer>)
        .destructive()
        // A retried restart is a second outage.
        .not_idempotent(),
    );
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn a_supervised_core_says_it_is_restarting() {
        let a = serde_json::to_value(answer(true)).unwrap();
        assert_eq!(a["restarting"], true);
        assert_eq!(a["how"], "supervised");
        assert!(a["message"].as_str().unwrap().contains("reconnects"));
    }

    #[test]
    fn a_core_started_by_hand_says_how_and_stays() {
        let a = serde_json::to_value(answer(false)).unwrap();
        assert_eq!(a["restarting"], false);
        assert_eq!(a["how"], "none");
        let message = a["message"].as_str().unwrap();
        assert!(message.contains("started by hand") && message.contains("--supervised"), "{message}");
    }

    #[test]
    fn core_info_follows_the_flag() {
        let off = serde_json::to_value(info_for(false)).unwrap();
        assert_eq!(off, serde_json::json!({ "possible": false, "how": "none" }));
        let on = serde_json::to_value(info_for(true)).unwrap();
        assert_eq!(on, serde_json::json!({ "possible": true, "how": "supervised" }));
    }
}
