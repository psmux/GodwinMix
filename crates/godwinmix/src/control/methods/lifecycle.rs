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

use std::sync::atomic::{AtomicBool, Ordering};

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
static RESTART_ASKED: AtomicBool = AtomicBool::new(false);

/// Called once at start with what `--supervised` said.
pub fn set_supervised(on: bool) {
    SUPERVISED.store(on, Ordering::Relaxed);
}

pub fn supervised() -> bool {
    SUPERVISED.load(Ordering::Relaxed)
}

/// True once `core.restart` has been accepted, so the way out can use
/// [`RESTART_EXIT_CODE`] instead of a clean zero.
pub fn restart_asked() -> bool {
    RESTART_ASKED.load(Ordering::Relaxed)
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
                    RESTART_ASKED.store(true, Ordering::Relaxed);
                    call.app.quit.notify_one();
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
