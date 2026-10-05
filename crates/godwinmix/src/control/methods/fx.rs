//! Transitions and effects from packs: list, import, change, remove, fire,
//! preview. A take uses one by name (`program.take {transition: "<name>"}`),
//! which `program::resolve_transition` looks up here through `plan`.
//!
//! Everything that reads or writes the library runs on a blocking worker:
//! an import decodes every clip it is given, and the first list writes the
//! starter set and asks once whether GStreamer GL runs here.

use super::{body, handler};
use crate::control::call::Call;
use crate::control::AppState;
use godwinmix_core::fx::{self, library, Plan};
use godwinmix_protocol::error::{ErrorCode, RpcError};
use godwinmix_protocol::fx::*;
use godwinmix_protocol::method::{schema_of, MethodDef, Registry, Tier};
use godwinmix_protocol::scope::Scope;
use serde_json::{json, Value};
use std::path::PathBuf;

#[path = "fx_tools.rs"]
mod tools;
#[path = "fx_edit.rs"]
mod edit;
use edit::{assign, preview, remove, set};
#[path = "fx_take.rs"]
mod take;
pub use take::{assigned, catalogue, spec_for, transition_names};

pub fn register(reg: &mut Registry<Call>) {
    tools::register(reg);
}

/// Run `f` on a blocking worker and turn its error into a refusal that
/// carries the sentence and the next step.
async fn blocking<T: Send + 'static>(f: impl FnOnce() -> anyhow::Result<T> + Send + 'static) -> Result<T, RpcError> {
    tokio::task::spawn_blocking(f)
        .await
        .map_err(|e| RpcError::internal(format!("the fx worker stopped: {e}")))?
        .map_err(|e| RpcError::new(ErrorCode::InvalidParams, format!("{e:#}")).with("list", "fx.list"))
}

/// The library: the gallery's folder.
fn media(_call: &Call) -> PathBuf {
    godwinmix_core::gallery::dir()
}

async fn list(call: Call, params: Value) -> Result<Value, RpcError> {
    let req: FxListRequest = call.params(&params)?;
    let dir = media(&call);
    let (fx, errors, gpu, assigned) = blocking(move || {
        let (all, errors) = library::list(&dir);
        let gpu = fx::shader::probe::available();
        let assigned = fx::assign::load(&dir);
        Ok((all.iter().map(|(m, d)| library::entry(m, d)).collect::<Vec<_>>(), errors, gpu, assigned))
    })
    .await?;
    let fx = fx
        .into_iter()
        .filter(|e| match req.role.as_deref() {
            Some("transition") => e.manifest.transition,
            Some("effect") => e.manifest.effect,
            _ => true,
        })
        .collect();
    body(FxList { fx, errors, gpu, assigned })
}

/// Import inline when it is quick, and as a task when it is not: a pack of
/// twenty clips takes longer than any call may hold a client.
pub async fn import_now(app: &AppState, req: FxImportRequest) -> Result<Value, RpcError> {
    let (root, media) = (godwinmix_core::gallery::dir(), app.library.dir().to_path_buf());
    let (tx, mut rx) = tokio::sync::oneshot::channel();
    tokio::task::spawn_blocking(move || {
        let _ = tx.send(fx::import::import(&root, &media, &req));
    });
    match tokio::time::timeout(std::time::Duration::from_secs(4), &mut rx).await {
        Ok(Ok(Ok(done))) => body(done),
        Ok(Ok(Err(e))) => Err(RpcError::new(ErrorCode::InvalidParams, format!("{e:#}")).with("formats", json!(FORMATS))),
        Ok(Err(_)) => Err(RpcError::internal("the import stopped before it finished")),
        Err(_) => Ok(super::tasks::spawn_task(&app.tasks, "fx.import", None, move |_ctx| async move {
            match rx.await {
                Ok(Ok(done)) => Ok(json!(done)),
                Ok(Err(e)) => Err(format!("{e:#}")),
                Err(_) => Err("the import stopped before it finished".to_string()),
            }
        })),
    }
}

/// What an import reads, for a refusal to list.
const FORMATS: &[&str] = &[
    "stinger: WebM VP8 or VP9 with alpha, ProRes 4444 MOV, QuickTime Animation MOV, PNG in MOV",
    "overlay: any clip on black (MP4, MOV, WebM), drawn with screen or add",
    "matte: a black to white PNG, JPEG or TIFF",
    "shader: a .glsl in the gl-transitions form",
    "a folder or a .zip of any of these",
];

async fn import(call: Call, params: Value) -> Result<Value, RpcError> {
    let req: FxImportRequest = call.params(&params)?;
    import_now(&call.app, req).await
}

async fn fire(call: Call, params: Value) -> Result<Value, RpcError> {
    let req: FxFireRequest = call.params(&params)?;
    let dir = media(&call);
    let name = req.name.clone();
    let plan = blocking(move || {
        let (mut m, folder) = library::find(&dir, &name)?;
        if !m.effect {
            anyhow::bail!("{} is a transition, not an effect: use it in a take with {{\"transition\": \"{}\"}}, or turn it on as an effect with fx.set {{\"effect\": true}} if it is a clip", m.name, m.name);
        }
        m.blend = req.blend.unwrap_or(m.blend);
        Plan::of(&m, &folder, None)
    })
    .await?;
    let fired = FxFired { name: plan.name.clone(), duration_ms: plan.duration_ms };
    let opacity = req.opacity.unwrap_or(1.0).clamp(0.0, 1.0);
    call.app
        .mixer
        .request(|ack| godwinmix_core::mixer::Command::FireFx { plan: Box::new(plan), opacity, ack: Some(ack) })
        .await
        .map_err(|e| call.mixer_error(e))?;
    body(fired)
}
