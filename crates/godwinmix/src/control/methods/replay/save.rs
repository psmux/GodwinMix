//! `replay.save`: an event, or the whole buffer of one angle, written to an
//! MP4 in the media folder's `Replays` folder, which the Media panel lists.
//!
//! The clip is taken off the rings here, off the mixer thread, and written
//! on a blocking worker, so a slow disk holds up this caller and nobody else.

use super::super::{body, handler};
use super::refusal;
use crate::control::call::Call;
use godwinmix_core::replay::Refusal;
use godwinmix_protocol::error::{ErrorCode, RpcError};
use godwinmix_protocol::method::{schema_of, MethodDef, Registry, Tier};
use godwinmix_protocol::replay::{SaveRequest, SaveStatus};
use godwinmix_protocol::scope::Scope;
use serde_json::Value;
use std::path::{Path, PathBuf};

/// The folder in the media library saves go to.
pub const FOLDER: &str = "Replays";

pub fn register(reg: &mut Registry<Call>) {
    reg.register(
        MethodDef::new("replay.save", Scope::Operate, "Write an event, or the whole buffer of one angle, to an MP4 in the media folder, without encoding it again.", handler(save))
            .params(schema_of::<SaveRequest>)
            .result(schema_of::<SaveStatus>)
            .not_idempotent()
            .tool(
                "save_replay",
                Tier::Search,
                "Save a replay to a file, as OBS's replay buffer does: `event` saves that event, and without one \
                 the whole buffer of `angle` (default the programme) is saved. The MP4 lands in the media \
                 library under Replays/ and can be added as a clip.",
            ),
    );
}

/// The file's name: the one asked for, made safe, or the event and the time.
fn file_name(req: &SaveRequest, angle: &str) -> Result<String, RpcError> {
    let stamp = std::time::SystemTime::now().duration_since(std::time::UNIX_EPOCH).map(|d| d.as_secs()).unwrap_or(0);
    let raw = match &req.name {
        Some(n) if n.to_ascii_lowercase().ends_with(".mp4") => n.clone(),
        Some(n) => format!("{n}.mp4"),
        None => format!("{}-{angle}-{stamp}.mp4", req.event.as_deref().unwrap_or("buffer")),
    };
    godwinmix_core::media::safe_upload_name(&raw)
        .map_err(|e| RpcError::new(ErrorCode::InvalidParams, format!("{e:#}. Give `name` without a folder")).with("field", "name"))
}

/// A path in `dir` that is not taken: `goal.mp4`, then `goal-2.mp4`.
fn free_path(dir: &Path, name: &str) -> PathBuf {
    let stem = name.trim_end_matches(".mp4");
    (1..1000)
        .map(|n| if n == 1 { dir.join(name) } else { dir.join(format!("{stem}-{n}.mp4")) })
        .find(|p| !p.exists())
        .unwrap_or_else(|| dir.join(name))
}

async fn save(call: Call, params: Value) -> Result<Value, RpcError> {
    let req: SaveRequest = call.params(&params)?;
    let shared = call.app.mixer.replay_shared();
    let (angle, clip) = match &req.event {
        Some(event) => {
            let w = shared.window(Some(event), None, req.angle.as_deref()).map_err(refusal)?;
            (w.angle.clone(), shared.clip(&w, &w.angle).map_err(refusal)?)
        }
        None => {
            let angle = shared.pick_angle(req.angle.as_deref(), None).map_err(refusal)?;
            let clip = shared.ring(&angle).map(|r| r.lock().everything()).unwrap_or_default();
            if clip.is_empty() {
                return Err(refusal(Refusal::NotInState(format!("angle {angle} holds nothing yet; wait for its buffer to fill"))));
            }
            (angle, clip)
        }
    };
    let dir = call.app.library.dir().join(FOLDER);
    std::fs::create_dir_all(&dir)
        .map_err(|e| RpcError::not_in_state(format!("cannot make {}: {e}. Choose a media folder the mixer can write to", dir.display())))?;
    let path = free_path(&dir, &file_name(&req, &angle)?);
    let target = path.clone();
    let written = tokio::task::spawn_blocking(move || godwinmix_core::replay::save::write(&clip, &target))
        .await
        .map_err(|e| RpcError::internal(format!("the replay writer stopped: {e}")))?
        .map_err(|e| RpcError::not_in_state(format!("{e:#}")))?;
    let file = path.file_name().map(|f| f.to_string_lossy().to_string()).unwrap_or_default();
    body(SaveStatus {
        name: format!("{FOLDER}/{file}"),
        path: path.display().to_string(),
        angle,
        duration_ms: written.duration_ns / 1_000_000,
        bytes: written.bytes,
    })
}
