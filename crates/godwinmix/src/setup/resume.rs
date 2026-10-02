//! Sources that could not start for want of a piece, started once it is ready.
//!
//! A web page added before the renderer exists, or a camera from the config
//! before its plugin is installed, is kept by the mixer as unstarted with a
//! `setup` button naming the piece (`mixer::unstarted`). When the mixer
//! comes up those pieces are asked for; when one becomes ready, every source
//! waiting on it is added again under its own id.

use crate::control::AppState;
use godwinmix_core::setup::plain;
use godwinmix_protocol::types::{SourceState, SourceStatus};
use godwinmix_protocol::ActionKind;
use std::collections::BTreeSet;

/// The piece an unstarted source waits on.
fn piece_of(u: &godwinmix_core::mixer::unstarted::Unstarted) -> Option<&str> {
    let a = u.action.as_ref()?;
    (a.kind == ActionKind::Setup).then_some(a.piece.as_deref()).flatten()
}

/// At start: ask for every piece a source from the config is waiting on.
pub fn spawn(app: AppState) {
    tokio::spawn(async move {
        let Ok(configs) = app.mixer.configs().await else { return };
        let pieces: BTreeSet<String> =
            configs.unstarted.iter().filter_map(piece_of).map(String::from).collect();
        for piece in pieces {
            tracing::info!(%piece, "a configured source is waiting on this; setting it up");
            if let Some(rx) = super::start(&piece) {
                // Already ready: nothing will finish, so start them now.
                if rx.borrow().state == godwinmix_protocol::setup::SetupState::Ready {
                    ready(app.clone(), piece);
                }
            }
        }
    });
}

/// `piece` is ready: add every source waiting on it.
pub fn ready(app: AppState, piece: String) {
    tokio::spawn(async move {
        let Ok(configs) = app.mixer.configs().await else { return };
        let waiting: Vec<String> = configs
            .unstarted
            .iter()
            .filter(|u| piece_of(u) == Some(piece.as_str()))
            .map(|u| u.config.id.clone())
            .collect();
        for id in waiting {
            match crate::control::restore_source(&app, &id).await {
                Ok(_) => tracing::info!(source = %id, %piece, "started now that it is set up"),
                Err(e) => tracing::warn!(source = %id, %piece, error = %format!("{e:#}"), "still would not start"),
            }
        }
    });
}

/// What `source.add` answers for a source kept waiting on a piece: its id
/// and name, connecting, and the piece's status under `setup`.
pub async fn waiting_record(app: &AppState, id: &str) -> Option<SourceStatus> {
    let configs = app.mixer.configs().await.ok()?;
    let u = configs.unstarted.iter().find(|u| u.config.id == id)?;
    let piece = piece_of(u)?;
    let mut extra = serde_json::Map::new();
    let setup = super::status(piece);
    extra.insert("setup".into(), serde_json::to_value(&setup).ok()?);
    extra.insert("waiting".into(), serde_json::Value::String(plain::setting_up(piece)));
    Some(SourceStatus {
        id: id.to_string(),
        name: u.config.name.clone().unwrap_or_else(|| id.to_string()),
        uri: u.config.uri.clone(),
        state: SourceState::Connecting,
        has_video: false,
        has_audio: false,
        cell: None,
        video_idle_ms: None,
        audio_idle_ms: None,
        gain: 1.0,
        muted: false,
        seekable: false,
        position_ms: None,
        duration_ms: None,
        extra,
    })
}
