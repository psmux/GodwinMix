//! The clip library, ad breaks, and looking at the pictures.

use super::{body, handler};
use crate::control::call::Call;
use base64::Engine;
use godwinmix_core::mixer::Command;
use godwinmix_protocol::error::{ErrorCode, RpcError};
use godwinmix_protocol::method::{any_object, schema_of, MethodDef, Registry, Tier};
use godwinmix_protocol::requests::*;
use godwinmix_protocol::scope::Scope;
use godwinmix_protocol::types::*;
use serde_json::{json, Value};

pub fn register(reg: &mut Registry<Call>) {
    reg.register(
        MethodDef::new(
            "media.list",
            Scope::Read,
            "The clips in the library, with durations and whether each has audio.",
            handler(|call: Call, _| async move {
                let library = call.app.library.clone();
                let converter = call.app.converter.clone();
                let listing =
                    tokio::task::spawn_blocking(move || library.list_with(Some(&converter)))
                        .await
                        .map_err(|e| RpcError::internal(format!("media scan failed: {e}")))?;
                body(listing)
            }),
        )
        .result(schema_of::<godwinmix_core::media::MediaListing>)
        .tool(
            "list_media",
            Tier::Search,
            "The clips in the mixer's media library: each one's name, absolute path, \
             duration in milliseconds and whether it has an audio track. Use it to find a \
             uri for `ad_break`. If the library directory is not configured or not \
             readable the answer carries an `error` field saying why.",
        ),
    );

    reg.register(
        MethodDef::new(
            "media.upload",
            Scope::Operate,
            "Stream a file into the library. HTTP only: the body is the file.",
            handler(|_call: Call, _| async move {
                Err(RpcError::new(
                    ErrorCode::NotInState,
                    "media.upload carries a file body and has no JSON-RPC form. \
                     POST the bytes to /api/v1/media/upload?name=<file name> instead.",
                ))
            }),
        )
        .not_idempotent()
        .rest_at("POST", "/api/v1/media/upload"),
    );

    reg.register(
        MethodDef::new(
            "media.convert",
            Scope::Operate,
            "Transcode a library file to a web safe copy, in the background.",
            handler(|call: Call, params| async move {
                let req: NameRequest = call.params(&params)?;
                let input = call.app.library.resolve(&req.name).map_err(|e| {
                    RpcError::not_found("media file", &req.name, &[]).with("detail", e.to_string())
                })?;
                let state = call
                    .app
                    .converter
                    .start(req.name, input)
                    .map_err(|e| call.mixer_error(e))?;
                body(state)
            }),
        )
        .params(schema_of::<NameRequest>)
        .result(schema_of::<godwinmix_core::convert::ConversionState>)
        .tool(
            "convert_media",
            Tier::Search,
            "Start a background transcode of a library file to a web safe copy (H.264 and \
             AAC in MP4), so a browser and the mixer can both read it. Answers at once with \
             the conversion state; watch it with `list_media`, which folds the converted \
             copy onto the original.",
        ),
    );

    reg.register(
        MethodDef::new(
            "media.remove",
            Scope::Operate,
            "Delete a library file and its converted copy. Refused while it is a live \
             source.",
            handler(remove_media),
        )
        .params(schema_of::<NameRequest>)
        .destructive()
        .tool(
            "remove_media",
            Tier::Search,
            "Delete a clip from the library, along with any converted copy of it. Refused \
             while the file is a source on this mixer, because deleting it would be the one \
             way this call could take the show off air; remove the source first. Pass \
             dry_run true to see what it would delete.",
        ),
    );

    register_adbreak(reg);
    register_snapshot(reg);
}

/// Interrupting the programme with a clip, and rejoining live.
fn register_adbreak(reg: &mut Registry<Call>) {
    reg.register(
        MethodDef::new(
            "adbreak.start",
            Scope::Operate,
            "Interrupt the programme with a clip, then rejoin live when it ends.",
            handler(|call: Call, params| async move {
                let req: AdBreakRequest = call.params(&params)?;
                call.app
                    .mixer
                    .request(|ack| Command::AdBreak {
                        uri: req.uri,
                        at_running_time_ms: req.at_running_time_ms,
                        return_to: req.return_to,
                        ack: Some(ack),
                    })
                    .await
                    .map_err(|e| call.mixer_error(e))?;
                let status = call
                    .app
                    .mixer
                    .status()
                    .await
                    .map_err(|e| call.mixer_error(e))?;
                body(json!({ "ad": status.ad, "program": status.program }))
            }),
        )
        .params(schema_of::<AdBreakRequest>)
        .result(any_object)
        .tool(
            "ad_break",
            Tier::Search,
            "Interrupt the programme with a clip, then rejoin live automatically when the \
             clip ends. The clip is a file path on the machine running the mixer or a URL; \
             `list_media` shows the library. There is no time shift: whatever the live \
             source did during the break is not shown afterwards. `return_to` picks the \
             source to rejoin, defaulting to whatever was on air. `at_running_time_ms` \
             schedules the break on a frame.",
        ),
    );

    reg.register(
        MethodDef::new(
            "adbreak.end",
            Scope::Operate,
            "Cut a running ad short, or disarm one that is scheduled.",
            handler(|call: Call, _| async move {
                call.app
                    .mixer
                    .request(|ack| Command::EndAdBreak(Some(ack)))
                    .await
                    .map_err(|e| call.mixer_error(e))?;
                let status = call
                    .app
                    .mixer
                    .status()
                    .await
                    .map_err(|e| call.mixer_error(e))?;
                body(json!({ "ad": status.ad, "program": status.program }))
            }),
        )
        .tool(
            "end_ad_break",
            Tier::Search,
            "Cut a running ad short and return to live now, or disarm one that is \
             scheduled and has not started. Refused, with the reason, when no ad break is \
             armed or on air.",
        ),
    );
}

fn register_snapshot(reg: &mut Registry<Call>) {
    reg.register(
        MethodDef::new(
            "snapshot.get",
            Scope::Read,
            "One JPEG: the whole contact sheet, the programme, or one source cut out of \
             the mosaic.",
            handler(snapshot),
        )
        .params(schema_of::<SnapshotRequest>)
        .result(any_object)
        .rest_at("GET", "/api/v1/snapshot/{id}")
        .tool(
            "snapshot",
            Tier::Standard,
            "Look at the pictures. Returns a JPEG you can view directly. `id` is \"sheet\" \
             for a contact sheet of every source and the programme side by side (the best \
             first look), \"program\" for what is going out right now, or a source id for \
             that one source. `width` scales it down, and smaller is much cheaper to look \
             at: 320 is enough to tell whether anyone is in the shot. Needs multiview \
             switched on; the answer says so plainly when it is not.",
        ),
    );
}

/// `snapshot.get` on `/rpc` and through MCP. The REST route serves the same
/// bytes raw, because an `<img>` tag cannot read base64 out of JSON.
#[derive(Debug, serde::Serialize, serde::Deserialize, schemars::JsonSchema)]
pub struct SnapshotRequest {
    /// "sheet", "program", or a source id. A `.jpg` on the end is accepted.
    pub id: String,
    /// Scale down to this many pixels across, keeping the aspect. Never
    /// enlarges. Omit for the `[snapshot] default_width` of 320, which is
    /// enough to see who is in shot; `width: 0` for the cell's own size.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub width: Option<u32>,
    /// Ignore the per client rate limit for this one request.
    #[serde(default, skip_serializing_if = "std::ops::Not::not")]
    pub force: bool,
    /// Permit a width above the `[snapshot] max_width` ceiling.
    #[serde(default, skip_serializing_if = "std::ops::Not::not")]
    pub allow_large: bool,
}

async fn snapshot(call: Call, params: Value) -> Result<Value, RpcError> {
    let req: SnapshotRequest = call.params(&params)?;
    let ask = godwinmix_core::snapshot::Ask {
        width: req.width,
        force: req.force,
        allow_large: req.allow_large,
    };
    // The rate limit bucket is the calling token, so one agent polling hard
    // cannot slow the operator's own UI down.
    let jpeg =
        crate::control::snapshot_bytes(&call.snapshots, &call.token.id, &req.id, &ask).await?;
    Ok(json!({
        "mime": "image/jpeg",
        "bytes": jpeg.len(),
        "base64": base64::engine::general_purpose::STANDARD.encode(&jpeg),
    }))
}

async fn remove_media(call: Call, params: Value) -> Result<Value, RpcError> {
    let req: NameRequest = call.params(&params)?;
    let path = call.app.library.resolve(&req.name).map_err(|e| {
        RpcError::not_found("media file", &req.name, &[]).with("detail", e.to_string())
    })?;
    let target = godwinmix_core::input::to_uri(&path.display().to_string());
    let configs = call
        .app
        .mixer
        .configs()
        .await
        .map_err(|e| call.mixer_error(e))?;
    if let Some(s) = configs
        .sources
        .iter()
        .find(|s| godwinmix_core::input::to_uri(&s.uri) == target)
    {
        return Err(RpcError::not_in_state(format!(
            "{} is the source \"{}\" on this mixer. Remove the source first, then delete \
             the file.",
            req.name, s.id
        ))
        .with("source", s.id.clone()));
    }
    let converted = godwinmix_core::convert::converted_sibling(&path);
    let would: Vec<String> = [path.clone(), converted]
        .into_iter()
        .filter(|p| p.exists())
        .map(|p| p.display().to_string())
        .collect();
    if call.dry_run {
        return Ok(call.dry_run_answer(
            !would.is_empty(),
            would.iter().map(|p| format!("delete {p}")).collect(),
        ));
    }
    let mut removed = Vec::new();
    for p in &would {
        if std::fs::remove_file(p).is_ok() {
            removed.push(p.clone());
        }
    }
    call.app.mixer.emit(Event::MediaChanged {
        name: req.name.clone(),
        conversion: None,
    });
    Ok(json!({ "removed": removed, "name": req.name }))
}
