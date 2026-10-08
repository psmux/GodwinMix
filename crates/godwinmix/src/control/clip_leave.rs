//! A clip set to leave the scene, ending on air: the take that moves the
//! programme off it.
//!
//! The mixer holds the clip's last frame and says `event/source.ended` with
//! `at_end: "leave"` (`godwinmix_core::mixer::clip_act`). This hears it and
//! takes, through `program.take` like any other caller, so the safety rules,
//! the transition a scene has and the take history all apply. Nothing here
//! runs on the mixer thread or a streaming thread, and if the take is refused
//! the programme simply stays on the held frame and an alert says why.
//!
//! Where it goes, in order:
//!
//! 1. The scene armed in Preview, in Studio mode, if it is not the one on air
//!    and does not show the clip. Arming the next scene is how a person says
//!    what follows.
//! 2. Otherwise, what was on air before the clip's scene: a scene, or a source
//!    on its own, as long as it does not show the clip either.
//! 3. Otherwise nothing: the clip holds its last frame and an alert says so,
//!    with the way out. The programme is never cut to black for it.
//!
//! A clip that ends while it is not on air does nothing more than hold.

use crate::control::call::Call;
use crate::control::AppState;
use godwinmix_core::snapshot::Tracker;
use godwinmix_core::state::Event;
use godwinmix_protocol::method::Registry;
use godwinmix_protocol::scope::{Scope, Token};
use godwinmix_protocol::types::Severity;
use serde_json::{json, Value};
use std::sync::{Arc, OnceLock};
use tokio::sync::broadcast;
use tracing::{debug, info, warn};

/// What the programme shows, as a take names it.
#[derive(Debug, Clone, PartialEq)]
pub enum OnAir {
    Scene(String),
    Source(String),
    Black,
}

impl OnAir {
    fn from_take(source: Option<String>, scene: Option<String>) -> OnAir {
        match (scene, source) {
            (Some(scene), _) => OnAir::Scene(scene),
            (None, Some(source)) => OnAir::Source(source),
            (None, None) => OnAir::Black,
        }
    }
}

/// The `program.take` params that move the programme off `clip`, or `None`
/// when there is nowhere to go. `draws` answers which sources a scene shows.
pub fn choose(
    clip: &str,
    now: &OnAir,
    armed: Option<&str>,
    before: Option<&OnAir>,
    draws: impl Fn(&str) -> Vec<String>,
) -> Option<Value> {
    let shows_clip = |scene: &str| draws(scene).iter().any(|s| s == clip);
    if let Some(armed) = armed {
        if *now != OnAir::Scene(armed.to_string()) && !shows_clip(armed) {
            return Some(json!({ "scene": armed }));
        }
    }
    match before? {
        OnAir::Scene(scene) if scene_differs(now, scene) && !shows_clip(scene) => Some(json!({ "scene": scene })),
        OnAir::Source(source) if source != clip => Some(json!({ "source": source })),
        _ => None,
    }
}

fn scene_differs(now: &OnAir, scene: &str) -> bool {
    *now != OnAir::Scene(scene.to_string())
}

/// Listen for clips that leave, for as long as the mixer runs.
pub fn spawn(app: AppState, snapshots: Arc<Tracker>) {
    tokio::spawn(async move {
        let mut events = app.mixer.subscribe();
        let mut now = match app.mixer.status().await {
            Ok(s) => OnAir::from_take(s.program.clone(), s.scene.clone()),
            Err(_) => OnAir::Black,
        };
        let mut before: Option<OnAir> = None;
        loop {
            match events.recv().await {
                Ok(envelope) => match envelope.event {
                    Event::Took { source, scene, .. } => {
                        let next = OnAir::from_take(source, scene);
                        if next != now {
                            before = Some(std::mem::replace(&mut now, next));
                        }
                    }
                    Event::SourceEnded { source, at_end } if at_end == "leave" => {
                        let (app, snapshots, now, before) = (app.clone(), snapshots.clone(), now.clone(), before.clone());
                        tokio::spawn(async move { leave(&app, &snapshots, &source, &now, before.as_ref()).await });
                    }
                    _ => {}
                },
                Err(broadcast::error::RecvError::Closed) => return,
                Err(broadcast::error::RecvError::Lagged(missed)) => {
                    warn!(missed, "the clip watcher fell behind the event bus; a clip set to leave may have stayed on air");
                }
            }
        }
    });
}

/// Whether `clip` is what the programme shows now, alone or in its scene.
fn on_air(app: &AppState, clip: &str, now: &OnAir) -> bool {
    match now {
        OnAir::Source(source) => source == clip,
        OnAir::Scene(scene) => app.scenes.sources_in(scene).iter().any(|s| s == clip),
        OnAir::Black => false,
    }
}

async fn leave(app: &AppState, snapshots: &Arc<Tracker>, clip: &str, now: &OnAir, before: Option<&OnAir>) {
    if !on_air(app, clip, now) {
        debug!(source = %clip, "a clip set to leave the scene ended off air; it holds its last frame");
        return;
    }
    let armed = app.scenes.armed().and_then(|id| app.scenes.scene(&id.to_string()).ok()).map(|v| v.name);
    let Some(params) = choose(clip, now, armed.as_deref(), before, |s| app.scenes.sources_in(s)) else {
        alert(app, format!(
            "{clip} ended and is set to leave the scene, but no scene is armed in Preview and nothing else \
             was on air before it, so it holds its last frame. Arm the scene to go to next, or take one."
        ));
        return;
    };
    info!(source = %clip, to = %params, "the clip ended and is set to leave the scene; taking what comes next");
    if let Err(message) = take(app, snapshots, params).await {
        alert(app, format!("{clip} ended and is set to leave the scene, and the take was refused: {message}"));
    }
}

fn alert(app: &AppState, message: String) {
    warn!(%message, "a clip could not leave the scene");
    app.mixer.emit(Event::Alert { severity: Severity::Warning, message, action: None });
}

fn registry() -> &'static Registry<Call> {
    static REG: OnceLock<Registry<Call>> = OnceLock::new();
    REG.get_or_init(crate::control::methods::registry)
}

/// `program.take`, credited to whoever made the last take: the clip's end is
/// a consequence of theirs, and the operator watchdog must go on watching
/// them rather than a token that never calls.
async fn take(app: &AppState, snapshots: &Arc<Tracker>, params: Value) -> Result<Value, String> {
    let def = registry().get("program.take").ok_or("this core has no program.take")?;
    let id = app.safety.operator().unwrap_or_else(|| "clip-end".into());
    let call = Call {
        app: app.clone(),
        snapshots: snapshots.clone(),
        token: Token { id: id.clone(), scopes: vec![Scope::Read, Scope::Operate], ..Token::open() },
        client: id,
        trace_id: "clip-end-take".into(),
        dry_run: false,
        method: def.name,
    };
    (def.handler)(call, params).await.map_err(|e| e.message)
}

#[cfg(test)]
#[path = "clip_leave_tests.rs"]
mod tests;
