//! One job a piece, however many callers ask.
//!
//! Each piece has a watch channel holding its `SetupStatus`. `start` looks at
//! the piece as it stands; a piece that is there is ready at once, one being
//! set up is joined, and anything else gets one job on the runtime. Every
//! change is also sent as `event/setup.changed`, so a page that did not ask
//! still sees the progress.

use super::Failure;
use crate::control::AppState;
use godwinmix_core::setup::{names, plain};
use godwinmix_protocol::setup::{SetupState, SetupStatus};
use godwinmix_protocol::types::Event;
use parking_lot::Mutex;
use std::collections::BTreeMap;
use std::sync::OnceLock;
use tokio::sync::watch;

struct Registry {
    app: AppState,
    runtime: tokio::runtime::Handle,
    pieces: Mutex<BTreeMap<String, watch::Sender<SetupStatus>>>,
}

static REGISTRY: OnceLock<Registry> = OnceLock::new();

/// Called once with the running core. Registers the engine's hook (a web
/// page from the config asks for the renderer through it) and looks at the
/// sources that could not start before this ran.
pub fn attach(app: AppState) {
    let runtime = tokio::runtime::Handle::current();
    let fresh = Registry { app: app.clone(), runtime, pieces: Mutex::new(BTreeMap::new()) };
    if REGISTRY.set(fresh).is_err() {
        return;
    }
    // From the mixer thread: `start` only spawns, so this returns at once.
    godwinmix_core::setup::starter::register(|piece| {
        let _ = start(piece);
    });
    super::resume::spawn(app);
}

/// Where `piece` stands now, without starting anything.
pub fn status(piece: &str) -> SetupStatus {
    let Some(reg) = REGISTRY.get() else { return look(piece, None) };
    if let Some(tx) = reg.pieces.lock().get(piece) {
        let now = tx.borrow().clone();
        if matches!(now.state, SetupState::Running | SetupState::Failed) {
            return now;
        }
    }
    look(piece, Some(&reg.app))
}

/// Every piece this mixer knows about: the renderer and the first party plugins.
pub fn list() -> Vec<SetupStatus> {
    let mut pieces = vec![names::WEB.to_string()];
    pieces.extend(super::plugin::shipped());
    pieces.iter().map(|p| status(p)).collect()
}

/// Set `piece` up, or join the job already doing it. `None` before `attach`.
pub fn start(piece: &str) -> Option<watch::Receiver<SetupStatus>> {
    let reg = REGISTRY.get()?;
    let mut pieces = reg.pieces.lock();
    let tx = pieces.entry(piece.to_string()).or_insert_with(|| watch::channel(look(piece, None)).0);
    if tx.borrow().state == SetupState::Running {
        return Some(tx.subscribe());
    }
    let now = look(piece, Some(&reg.app));
    if now.state != SetupState::Missing && now.state != SetupState::Failed {
        publish(tx, now);
        return Some(tx.subscribe());
    }
    let running = SetupStatus { state: SetupState::Running, message: plain::setting_up(piece), ..now };
    publish(tx, running);
    let progress = Progress { tx: tx.clone(), piece: piece.to_string() };
    let app = reg.app.clone();
    let name = piece.to_string();
    reg.runtime.spawn(async move {
        let outcome = work(&app, &name, &progress).await;
        progress.finish(outcome);
    });
    Some(tx.subscribe())
}

/// Wait for `piece` to stop running. Ok when it is ready.
pub async fn wait(mut rx: watch::Receiver<SetupStatus>) -> Result<(), SetupStatus> {
    loop {
        let now = rx.borrow_and_update().clone();
        match now.state {
            SetupState::Ready => return Ok(()),
            SetupState::Running => {}
            _ => return Err(now),
        }
        if rx.changed().await.is_err() {
            return Err(rx.borrow().clone());
        }
    }
}

async fn work(app: &AppState, piece: &str, progress: &Progress) -> Result<(), Failure> {
    if piece == names::WEB {
        super::web::run(app, progress).await
    } else {
        super::plugin::ensure(app, piece, progress).await
    }
}

/// The piece as it is on disk, with nothing running.
fn look(piece: &str, app: Option<&AppState>) -> SetupStatus {
    if piece == names::WEB {
        return super::web::look(app);
    }
    super::plugin::look(piece)
}

/// Send a status to everyone waiting and to every page.
fn publish(tx: &watch::Sender<SetupStatus>, status: SetupStatus) {
    let changed = *tx.borrow() != status;
    tx.send_replace(status.clone());
    if changed {
        if let Some(reg) = REGISTRY.get() {
            reg.app.mixer.emit(Event::SetupChanged { setup: Box::new(status) });
        }
    }
}

/// What a job says as it goes.
pub struct Progress {
    tx: watch::Sender<SetupStatus>,
    piece: String,
}

impl Progress {
    /// A new step, in a person's words, and how far through a download it
    /// is. Sent when the words change or the fraction moves a percent.
    pub fn say(&self, message: &str, fraction: Option<f64>) {
        let mut next = self.tx.borrow().clone();
        let moved = match (next.progress, fraction) {
            (Some(a), Some(b)) => (a - b).abs() >= 0.01,
            (a, b) => a.is_some() != b.is_some(),
        };
        if next.message == message && !moved {
            return;
        }
        next.message = message.to_string();
        next.progress = fraction;
        publish(&self.tx, next);
    }

    fn finish(&self, outcome: Result<(), Failure>) {
        let mut next = look(&self.piece, REGISTRY.get().map(|r| &r.app));
        match outcome {
            Ok(()) if next.state == SetupState::Ready => {
                tracing::info!(piece = %self.piece, "set up and ready");
            }
            Ok(()) => {
                tracing::warn!(piece = %self.piece, detail = %next.detail, "set up, but still not found");
                next.state = SetupState::Failed;
                next.message = format!("{} did not finish setting up. Press Try again.", names::title(&self.piece));
                next.action = Some(godwinmix_protocol::ErrorAction::setup("Try again", &self.piece));
            }
            Err(f) => {
                tracing::warn!(piece = %self.piece, why = %f.message, detail = %f.detail, "setting up did not finish");
                next.state = SetupState::Failed;
                next.message = f.message;
                next.action = f.action.or(Some(godwinmix_protocol::ErrorAction::setup("Try again", &self.piece)));
                next.detail = f.detail;
            }
        }
        publish(&self.tx, next);
    }
}
