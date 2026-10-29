//! `program.key.set` and `program.key.list`: downstream keys.
//!
//! A key is a scene drawn above whatever is on air until it is switched off
//! (see `godwinmix_core::scene::server::keys`). Switching one takes what is on
//! air again with the keys as they now are, through a fade by default, and
//! since nothing under the key changes, only the key fades. That retake is not
//! a shot: it goes to the mixer without the hold and rate rules a take is
//! held to, so an agent keeping a lower third up is never refused for it.

use super::{body, handler, resolve_transition};
use crate::control::call::Call;
use godwinmix_core::mixer::slots::Placement;
use godwinmix_core::mixer::{Command, ProgramScene};
use godwinmix_core::scene::server::KeyOnAir;
use godwinmix_protocol::error::RpcError;
use godwinmix_protocol::method::{schema_of, MethodDef, Registry, Tier};
use godwinmix_protocol::requests::{TakeRequest, Transition};
use godwinmix_protocol::scope::Scope;
use schemars::JsonSchema;
use serde::{Deserialize, Serialize};
use serde_json::Value;

/// `program.key.set`.
#[derive(Debug, Clone, Serialize, Deserialize, JsonSchema)]
#[serde(deny_unknown_fields)]
pub struct KeySetRequest {
    /// The scene to put on air as a key, or take off, by name or by id.
    pub scene: String,
    /// True to put it on air, false to take it off.
    pub on: bool,
    /// How the key comes and goes: a name or {type, duration_ms, params}, as
    /// in `program.take`. A fade of 300 ms when absent; `"cut"` for none.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub transition: Option<Transition>,
}

/// The keys on air, bottom first.
#[derive(Debug, Clone, Serialize, Deserialize, JsonSchema)]
pub struct KeyListing {
    pub keys: Vec<KeyRecord>,
}

#[derive(Debug, Clone, Serialize, Deserialize, JsonSchema)]
pub struct KeyRecord {
    pub id: String,
    pub name: String,
}

fn listing(keys: Vec<KeyOnAir>) -> KeyListing {
    KeyListing { keys: keys.into_iter().map(|k| KeyRecord { id: k.id.to_string(), name: k.name }).collect() }
}

pub(super) fn register(reg: &mut Registry<Call>) {
    reg.register(
        MethodDef::new(
            "program.key.set",
            Scope::Operate,
            "Put a scene on air as a downstream key, above whatever is taken, or take it off.",
            handler(set),
        )
        .params(schema_of::<KeySetRequest>)
        .result(schema_of::<KeyListing>)
        .tool(
            "set_downstream_key",
            Tier::Search,
            "Keep a scene (a lower third, a logo, a ticker) on air above every take until it is \
             switched off, or switch it off. `scene` is a scene name, `on` true or false. It \
             fades in and out unless `transition` says otherwise. Returns the keys on air.",
        ),
    );
    reg.register(
        MethodDef::new("program.key.list", Scope::Read, "The downstream keys on air, bottom first.", handler(list))
            .result(schema_of::<KeyListing>),
    );
}

async fn list(call: Call, _params: Value) -> Result<Value, RpcError> {
    body(listing(call.app.scenes.keys()))
}

async fn set(call: Call, params: Value) -> Result<Value, RpcError> {
    let req: KeySetRequest = call.params(&params)?;
    let asked = TakeRequest { transition: Some(req.transition.clone().unwrap_or_else(fade)), ..Default::default() };
    let transition = resolve_transition(&call, &asked)?;
    let keys = call
        .app
        .scenes
        .set_key(&req.scene, req.on)
        .map_err(|e| crate::control::methods::scenes::scene_error(&call, e))?;
    if let Some(scene) = on_air(&call).await? {
        call.app
            .mixer
            .request(|ack| Command::TakeScene {
                scene: Box::new(scene),
                at_running_time_ms: None,
                duration_ms: None,
                transition: transition.clone(),
                ack: Some(ack),
            })
            .await
            .map_err(|e| call.mixer_error(e))?;
    }
    body(listing(keys))
}

fn fade() -> Transition {
    serde_json::from_value(serde_json::json!({ "type": "fade", "duration_ms": 300 })).expect("a fade is a transition")
}

/// What is on air, drawn again with the keys as they are now. None for the
/// slate: the keys go up with the next take.
async fn on_air(call: &Call) -> Result<Option<ProgramScene>, RpcError> {
    let status = call.app.mixer.status().await.map_err(|e| call.mixer_error(anyhow::anyhow!(e)))?;
    if let Some(name) = status.scene.as_deref().filter(|n| call.app.scenes.scene(n).is_ok()) {
        let (name, placements) =
            call.app.scenes.placements(name).map_err(|e| crate::control::methods::scenes::scene_error(call, e))?;
        return Ok(Some(ProgramScene { name, placements }));
    }
    Ok(status.program.map(|source| one_source(call, &source)))
}

/// One source full canvas, with the keys above it when any are up.
pub(super) fn one_source(call: &Call, source: &str) -> ProgramScene {
    call.app.scenes.source_with_keys(source).unwrap_or_else(|| {
        let canvas = call.app.scenes.canvas();
        let caps = godwinmix_core::caps::CanvasCaps::new(&godwinmix_core::config::Canvas {
            width: canvas.width as i32,
            height: canvas.height as i32,
            fps: canvas.fps.max(1) as i32,
            ..Default::default()
        });
        ProgramScene { name: source.to_string(), placements: vec![Placement::full_canvas(source.to_string(), &caps)] }
    })
}
