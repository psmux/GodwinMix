//! A first party plugin, set up the first time somebody needs it.
//!
//! Installed from the copy shipped beside the mixer or in the checkout it was
//! built in (`plugin::first_party`), through the same path `plugin.add`
//! takes, so it arrives exactly as one installed by hand. One that is
//! installed and switched off is switched on instead.

use super::registry::Progress;
use super::Failure;
use crate::control::AppState;
use godwinmix_core::plugin::{first_party, loader};
use godwinmix_core::setup::{names, plain};
use godwinmix_protocol::setup::{SetupState, SetupStatus};
use serde_json::json;

/// The plugins this mixer can set up by itself: every first party one it
/// has a copy of.
pub fn shipped() -> Vec<String> {
    names::plugins().filter(|n| first_party::find(n).is_some()).map(String::from).collect()
}

/// Where a plugin stands, with nothing running.
pub fn look(piece: &str) -> SetupStatus {
    let base = SetupStatus { piece: piece.into(), title: names::title(piece).into(), ..Default::default() };
    match loader::get(piece) {
        Some(p) if p.enabled => SetupStatus {
            state: SetupState::Ready,
            message: format!("{} {} ready.", names::sentence_start(piece), names::be(piece)),
            ..base
        },
        Some(_) => {
            let off = plain::plugin_off(piece);
            SetupStatus { state: SetupState::Missing, message: off.message, detail: off.detail.unwrap_or_default(), ..base }
        }
        None => match first_party::find(piece) {
            Some(dir) => SetupStatus {
                state: SetupState::Missing,
                message: format!(
                    "{} {} not set up yet. {} up the first time you pick one.",
                    names::sentence_start(piece),
                    names::be(piece),
                    names::sets_itself(piece)
                ),
                detail: json!({ "plugin": piece, "from": dir }),
                ..base
            },
            None => SetupStatus {
                state: SetupState::Unavailable,
                message: format!("{} {} not part of this copy of the mixer.", names::sentence_start(piece), names::be(piece)),
                detail: json!({ "plugin": piece, "looked": first_party::places(piece) }),
                ..base
            },
        },
    }
}

/// Switch it on, or install it and start what it brings.
pub async fn ensure(app: &AppState, piece: &str, progress: &Progress) -> Result<(), Failure> {
    if loader::get(piece).is_some_and(|p| !p.enabled) {
        progress.say(&format!("Turning {} back on.", names::noun(piece)), None);
        let (name, supervisor) = (piece.to_string(), app.plugins.clone());
        let on = tokio::task::spawn_blocking(move || {
            let on = loader::set_enabled(&name, true).is_some();
            crate::start_singletons(&supervisor);
            on
        })
        .await
        .unwrap_or(false);
        return match on {
            true => Ok(()),
            false => Err(did_not_finish(piece, json!({ "plugin": piece, "step": "enable" }))),
        };
    }
    let Some(dir) = first_party::find(piece) else {
        let status = look(piece);
        return Err(Failure::new(status.message, status.detail));
    };
    tracing::info!(plugin = piece, from = %dir.display(), "installing a first party plugin on first use");
    let started = std::time::Instant::now();
    match crate::control::methods::plugins::install_and_start(app, piece.to_string()).await {
        Ok((_, failures)) => {
            for (provide, why) in failures {
                tracing::warn!(%provide, %why, "a plugin singleton would not start");
            }
            tracing::info!(plugin = piece, secs = started.elapsed().as_secs(), "installed");
            Ok(())
        }
        Err(e) => Err(did_not_finish(piece, json!({ "plugin": piece, "from": dir, "error": e }))),
    }
}

fn did_not_finish(piece: &str, detail: serde_json::Value) -> Failure {
    Failure::new(
        format!(
            "{} could not be set up. Press Try again; if it fails again, the mixer's log says why.",
            names::sentence_start(piece)
        ),
        detail,
    )
}
