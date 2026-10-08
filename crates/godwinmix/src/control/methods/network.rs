//! `network.share`: let other devices on the network reach this mixer, or
//! keep it to this computer, from the page, with the restart done for the
//! person who asked rather than described to them.
//!
//! The address a mixer listens on is fixed for the life of the process, so a
//! change is a restart, and what restarts it decides how the change is made:
//!
//! * The desktop app starts its mixer with `--bind` and keeps the choice in
//!   its own `lan.json`, and says so with `GODWINMIX_SHELL=desktop`. The mixer
//!   exits with [`SHARE_EXIT_CODE`] or [`LOCAL_EXIT_CODE`]; the app keeps the
//!   choice and the port and starts it again, so the page reconnects where it
//!   is, from a browser or a phone as well as from the app's own window.
//! * A supervised mixer that reads its address from the config file has
//!   `control.bind` written there, the port kept and the host changed, and
//!   restarts as `core.restart` does.
//! * A mixer started by hand, or one whose address is on its command line,
//!   says why it cannot, and keeps running.
//!
//! Opening a mixer that has no control token is refused: anyone on the
//! network would be admin.

use std::path::{Path, PathBuf};
use std::sync::OnceLock;

use godwinmix_protocol::error::{ErrorCode, RpcError};
use godwinmix_protocol::method::{schema_of, MethodDef, Registry};
use godwinmix_protocol::scope::Scope;
use godwinmix_protocol::types::{RestartAnswer, RestartHow};
use serde::Deserialize;

use super::lifecycle::{self, RESTART_EXIT_CODE};
use super::{body, config, handler};
use crate::control::call::{dry_run_answer, Call};

/// The desktop app's mixer leaving so that it is started again for other
/// devices. Written again, as 76, in `tauri-app/src/restart.rs`.
pub const SHARE_EXIT_CODE: i32 = 76;
/// The same, started again for this computer only. 77 there.
pub const LOCAL_EXIT_CODE: i32 = 77;

/// The address this process listens on, whether `--bind` gave it, and the
/// config file `control.bind` is written to.
static BIND: OnceLock<(String, bool, PathBuf)> = OnceLock::new();

/// Called once at start, by the station or by a core on its own, with the
/// address the control port binds, whether it came from `--bind`, and the
/// config in force.
pub fn configure(bind: &str, from_flag: bool, config: &Path) {
    let _ = BIND.set((bind.to_string(), from_flag, config.to_path_buf()));
}

/// What the process answering the call can tell about itself. The station
/// answers `network.share` for the port it owns; a core on its own answers it
/// through its method table.
pub struct Here<'a> {
    /// No control token: anyone who reaches the port is admin.
    pub open: bool,
    /// What stops this process.
    pub quit: &'a tokio::sync::Notify,
}

#[derive(Debug, Deserialize, schemars::JsonSchema)]
pub struct NetworkShareRequest {
    /// True lets phones and other computers on the same network reach this
    /// mixer. False keeps it to this computer.
    pub enabled: bool,
}

/// What a call comes to, worked out before anything happens.
#[derive(Debug, PartialEq)]
enum Step {
    /// Nothing to do, or nothing that can be done: say so and keep running.
    Stay(String),
    /// Exit with this status for the desktop app.
    Desktop(i32),
    /// Write this `control.bind` and restart.
    Config(String),
}

/// The facts a decision rests on, gathered so it can be tested without a process.
struct Facts<'a> {
    bind: &'a str,
    desktop: bool,
    supervised: bool,
    bind_flag: bool,
    open: bool,
}

/// Whether an address answers other devices: anything but loopback.
fn shared(bind: &str) -> bool {
    let host = bind.rsplit_once(':').map_or(bind, |(host, _)| host);
    !(host.starts_with("127.") || host == "localhost" || host == "[::1]")
}

/// The same port, on every network or on this computer only.
fn moved(bind: &str, enabled: bool) -> String {
    let port = bind.rsplit_once(':').map_or("8080", |(_, port)| port);
    format!("{}:{port}", if enabled { "0.0.0.0" } else { "127.0.0.1" })
}

fn decide(enabled: bool, f: &Facts) -> Result<Step, RpcError> {
    if shared(f.bind) == enabled {
        return Ok(Step::Stay(match enabled {
            true => "Other devices on this network can already reach this mixer.".into(),
            false => "This mixer already answers on this computer only.".into(),
        }));
    }
    if enabled && f.open {
        return Err(RpcError::new(
            ErrorCode::NotInState,
            "this mixer has no control token, so opening it to the network would let anyone on \
             it run the show. Set one in Mixer settings, under Control token, then try again.",
        )
        .with("open", true));
    }
    if !f.supervised {
        return Ok(Step::Stay(lifecycle::answer(false).message));
    }
    if f.desktop {
        return Ok(Step::Desktop(if enabled { SHARE_EXIT_CODE } else { LOCAL_EXIT_CODE }));
    }
    if f.bind_flag {
        return Ok(Step::Stay(
            "This mixer's address is given where it is started (--bind), which wins over its \
             settings, so it cannot be changed from here. Change it there and restart the mixer."
                .into(),
        ));
    }
    Ok(Step::Config(moved(f.bind, enabled)))
}

fn restarting(enabled: bool) -> RestartAnswer {
    let reach = match enabled {
        true => "Phones and other computers on this network can reach the mixer once it is back.",
        false => "Only this computer can reach the mixer once it is back.",
    };
    RestartAnswer {
        restarting: true,
        how: RestartHow::Supervised,
        message: format!(
            "{reach} It is restarting now: the programme is off air for a few seconds, and this \
             page reconnects by itself."
        ),
    }
}

/// `network.share`, for whichever process owns the port.
pub async fn share(here: Here<'_>, params: serde_json::Value, dry_run: bool) -> Result<serde_json::Value, RpcError> {
    let req: NetworkShareRequest = serde_json::from_value(params)
        .map_err(|e| RpcError::invalid_params(format!("network.share takes {{\"enabled\": true}} or false: {e}")))?;
    let (bind, bind_flag, config) = BIND.get().cloned().unwrap_or_default();
    let facts = Facts {
        bind: &bind,
        desktop: std::env::var("GODWINMIX_SHELL").is_ok_and(|v| v == "desktop"),
        supervised: lifecycle::supervised(),
        bind_flag,
        open: here.open,
    };
    let step = decide(req.enabled, &facts)?;
    if dry_run {
        let diff = match &step {
            Step::Stay(_) => Vec::new(),
            Step::Desktop(_) => vec!["exit for the desktop app to start the mixer again on the new address".into()],
            Step::Config(to) => vec![format!("write control.bind = {to}, then restart")],
        };
        return Ok(dry_run_answer("network.share", !diff.is_empty(), diff));
    }
    match step {
        Step::Stay(message) => body(RestartAnswer { restarting: false, how: lifecycle::restart_info().how, message }),
        Step::Desktop(code) => {
            tracing::info!(enabled = req.enabled, code, "network change asked of the desktop app");
            lifecycle::leave(code, here.quit);
            body(restarting(req.enabled))
        }
        Step::Config(to) => {
            config::write_for_restart(&config, "control.bind", serde_json::Value::String(to.clone())).await?;
            tracing::info!(bind = %to, "network change written, restarting");
            lifecycle::leave(RESTART_EXIT_CODE, here.quit);
            body(restarting(req.enabled))
        }
    }
}

pub(super) fn register(reg: &mut Registry<Call>) {
    reg.register(
        MethodDef::new(
            "network.share",
            Scope::Admin,
            "Let phones and other computers on the same network reach this mixer (enabled: \
             true), or keep it to this computer (false). The address is fixed while the mixer \
             runs, so this restarts it on the same port and the programme is off air for a few \
             seconds: under the desktop app, or under a supervisor when the address comes from \
             the config file. Otherwise it answers restarting: false and says why. Refused on a \
             mixer with no control token.",
            handler(|call: Call, params| async move {
                let here = Here { open: call.app.tokens.is_open(), quit: &call.app.quit };
                share(here, params, call.dry_run).await
            }),
        )
        .params(schema_of::<NetworkShareRequest>)
        .result(schema_of::<RestartAnswer>)
        .destructive()
        // A retried change is a second outage.
        .not_idempotent(),
    );
}

#[cfg(test)]
mod tests {
    use super::*;

    fn facts(bind: &str) -> Facts<'_> {
        Facts { bind, desktop: false, supervised: true, bind_flag: false, open: false }
    }

    #[test]
    fn loopback_is_this_computer_and_anything_else_is_shared() {
        assert!(!shared("127.0.0.1:8080") && !shared("localhost:8080") && !shared("[::1]:8080"));
        assert!(shared("0.0.0.0:8080") && shared("192.168.1.20:8080") && shared("[::]:8080"));
        assert_eq!(moved("127.0.0.1:54576", true), "0.0.0.0:54576");
        assert_eq!(moved("0.0.0.0:18801", false), "127.0.0.1:18801");
    }

    #[test]
    fn the_desktop_app_is_asked_with_an_exit_status() {
        let f = Facts { desktop: true, bind_flag: true, ..facts("127.0.0.1:54576") };
        assert_eq!(decide(true, &f).unwrap(), Step::Desktop(SHARE_EXIT_CODE));
        let f = Facts { desktop: true, bind_flag: true, ..facts("0.0.0.0:54576") };
        assert_eq!(decide(false, &f).unwrap(), Step::Desktop(LOCAL_EXIT_CODE));
    }

    #[test]
    fn a_supervised_server_has_its_config_moved_on_the_same_port() {
        assert_eq!(decide(true, &facts("127.0.0.1:8080")).unwrap(), Step::Config("0.0.0.0:8080".into()));
    }

    #[test]
    fn what_is_already_so_changes_nothing() {
        assert!(matches!(decide(true, &facts("0.0.0.0:8080")).unwrap(), Step::Stay(m) if m.contains("already")));
        assert!(matches!(decide(false, &facts("127.0.0.1:8080")).unwrap(), Step::Stay(m) if m.contains("already")));
    }

    #[test]
    fn what_cannot_be_restarted_from_here_says_why_and_stays() {
        let by_hand = Facts { supervised: false, ..facts("127.0.0.1:8080") };
        assert!(matches!(decide(true, &by_hand).unwrap(), Step::Stay(m) if m.contains("started by hand")));
        let flag = Facts { bind_flag: true, ..facts("127.0.0.1:8080") };
        assert!(matches!(decide(true, &flag).unwrap(), Step::Stay(m) if m.contains("--bind")));
    }

    #[test]
    fn an_open_mixer_is_never_put_on_the_network() {
        let open = Facts { open: true, desktop: true, ..facts("127.0.0.1:8080") };
        let refused = decide(true, &open).unwrap_err();
        assert!(refused.message.contains("Control token"), "{}", refused.message);
        // Taking it off the network is always allowed.
        assert_eq!(decide(false, &Facts { open: true, ..facts("0.0.0.0:8080") }).unwrap(), Step::Config("127.0.0.1:8080".into()));
    }
}
