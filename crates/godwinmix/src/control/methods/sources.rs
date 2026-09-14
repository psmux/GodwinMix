//! Sources: list, get, add, remove, the faders and the scrubber.

use super::{body, handler};
use godwinmix_protocol::error::RpcError;
use godwinmix_protocol::method::{schema_of, MethodDef, Registry, Tier};
use godwinmix_protocol::requests::*;
use godwinmix_protocol::scope::Scope;
use godwinmix_protocol::types::*;
use crate::control::call::Call;
use godwinmix_core::mixer::{AudioOutcome, Command, SeekOutcome};
use serde_json::Value;

pub fn register(reg: &mut Registry<Call>) {
    reg.register(
        MethodDef::new(
            "source.list",
            Scope::Read,
            "Every source, with its state, whether it has video and audio, and its fader.",
            handler(|call: Call, _| async move {
                let status = call.app.mixer.status().await.map_err(|e| call.mixer_error(e))?;
                body(status.sources)
            }),
        )
        .result(schema_of::<Vec<SourceStatus>>)
        .tool(
            "list_sources",
            Tier::Minimal,
            "Every source the mixer has, with the id you pass to `take`, the name, the \
             URL, whether it is connecting, live, stalled or failed, whether it currently \
             has video and audio, its fader and mute, and whether it can be scrubbed. Use \
             it to learn the ids before taking anything.",
        ),
    );

    reg.register(
        MethodDef::new(
            "source.get",
            Scope::Read,
            "One source. Refused with the ids that exist when there is no such source.",
            handler(|call: Call, params| async move {
                let req: IdRequest = call.params(&params)?;
                body(find(&call, &req.id).await?)
            }),
        )
        .params(schema_of::<IdRequest>)
        .result(schema_of::<SourceStatus>),
    );

    reg.register(
        MethodDef::new(
            "source.add",
            Scope::Operate,
            "Add a source while the mixer runs. Answers with the id it got and the whole \
             source record.",
            handler(add),
        )
        .params(schema_of::<AddSourceRequest>)
        .result(schema_of::<SourceStatus>)
        .tool(
            "add_source",
            Tier::Minimal,
            "Add a source while the mixer is running. The protocol is worked out from the \
             URL: rtmp, rtmps, an https .m3u8 or .mpd manifest, rtsp, srt, udp, a file path \
             or a media file URL. For a web page, pass its https URL with kind \"web\" and \
             the mixer renders it in a real browser, with its audio. Omit the id to have one \
             derived from the name or the host. The source is not on air until you `take` \
             it; the answer carries the id it got and its state.",
        ),
    );

    register_set(reg);

    reg.register(
        MethodDef::new(
            "source.remove",
            Scope::Operate,
            "Remove a source. If it is on programme the mixer cuts to the slate first.",
            handler(remove),
        )
        .params(schema_of::<IdRequest>)
        .destructive()
        .tool(
            "remove_source",
            Tier::Standard,
            "Remove a source by id and stop its pipeline. If it is on programme the mixer \
             cuts to the slate first, so this can take the picture off air: check \
             `agent_state` before using it on the live source. Pass dry_run true to see \
             what it would do without doing it. An unknown id is refused with the ids that \
             exist.",
        ),
    );

    reg.register(
        MethodDef::new(
            "source.audio.set",
            Scope::Operate,
            "Move a source's audio: the fader, the mute, and for a superimposed page the \
             balance between its own sound and the videos under it.",
            handler(set_audio),
        )
        .params(schema_of::<AudioSetParams>)
        .result(schema_of::<SourceAudioState>)
        .tool(
            "set_source_audio",
            Tier::Search,
            "Set a source's level or mute it. `gain` is a fader, 0.0 silent through 1.0 \
             unity to a ceiling of 10.0, and out of range is clamped rather than refused. \
             `muted` is held apart from the fader, so unmuting comes back to the level that \
             was set. `page` and `media` balance a superimposed page's own sound against \
             the videos the mixer decodes underneath it, and are refused on anything else. \
             Returns the levels read back off the pipeline, which is what actually took \
             effect.",
        ),
    );

    reg.register(
        MethodDef::new(
            "source.seek",
            Scope::Operate,
            "Move a seekable source to a position. Answers with where it actually landed.",
            handler(seek),
        )
        .params(schema_of::<SeekParams>)
        .result(schema_of::<SourcePositionState>)
        .tool(
            "seek_source",
            Tier::Search,
            "Move a file source to a position, in milliseconds from its start. Refused on \
             a camera or a live feed, which has no position to move to. A seek snaps to a \
             key frame, so the answer carries the position it actually landed on rather \
             than the one asked for, along with the duration when the demuxer knows it.",
        ),
    );
}

/// `source.audio.set` takes an id as well as the levels: the id comes off the
/// path on REST and out of the params on `/rpc`, and both land in one object.
#[derive(Debug, serde::Serialize, serde::Deserialize, schemars::JsonSchema)]
pub struct AudioSetParams {
    /// Source id.
    pub id: String,
    #[serde(flatten)]
    pub audio: AudioRequest,
}

#[derive(Debug, serde::Serialize, serde::Deserialize, schemars::JsonSchema)]
pub struct SeekParams {
    /// Source id.
    pub id: String,
    #[serde(flatten)]
    pub seek: SeekRequest,
}

async fn find(call: &Call, id: &str) -> Result<SourceStatus, RpcError> {
    let status = call.app.mixer.status().await.map_err(|e| call.mixer_error(e))?;
    status
        .sources
        .iter()
        .find(|s| s.id == id)
        .cloned()
        .ok_or_else(|| {
            let ids = status.sources.iter().map(|s| s.id.clone()).collect::<Vec<_>>();
            RpcError::not_found("source", id, &ids)
        })
}

async fn add(call: Call, params: Value) -> Result<Value, RpcError> {
    let req: AddSourceRequest = call.params(&params)?;
    let id = crate::control::add_source_now(&call.app, req)
        .await
        .map_err(|e| call.mixer_error(e))?;
    // The whole resulting object, so no follow up read is needed (AIP-134).
    let record = find(&call, &id).await?;
    // The hook call site. Nothing waits on it; see `control/hooks/`.
    call.app.hooks.fire(godwinmix_core::hooks::name::SOURCE_ADDED, || {
        serde_json::json!({ "source": record.id, "uri": record.uri, "state": record.state })
    });
    body(record)
}

async fn remove(call: Call, params: Value) -> Result<Value, RpcError> {
    let req: IdRequest = call.params(&params)?;
    let source = find(&call, &req.id).await?;
    let status = call.app.mixer.status().await.map_err(|e| call.mixer_error(e))?;
    if call.dry_run {
        let mut diff = vec![format!("remove source {} ({})", source.id, source.uri)];
        if status.program.as_deref() == Some(source.id.as_str()) {
            diff.push("cut the programme to the slate, because it is on air".into());
        }
        return Ok(call.dry_run_answer(true, diff));
    }
    call.app
        .mixer
        .request(|ack| Command::RemoveSource(req.id.clone(), Some(ack)))
        .await
        .map_err(|e| call.mixer_error(e))?;
    let after = call.app.mixer.status().await.map_err(|e| call.mixer_error(e))?;
    // The hook call site.
    call.app.hooks.fire(godwinmix_core::hooks::name::SOURCE_REMOVED, || {
        serde_json::json!({ "source": req.id, "uri": source.uri })
    });
    Ok(serde_json::json!({
        "removed": req.id,
        "program": after.program,
        "sources": after.sources.iter().map(|s| s.id.clone()).collect::<Vec<_>>(),
    }))
}

async fn set_audio(call: Call, params: Value) -> Result<Value, RpcError> {
    let req: AudioSetParams = call.params(&params)?;
    let gain = req.audio.gain.map(crate::control::checked_gain).transpose().map_err(refuse)?;
    let page = req.audio.page.map(crate::control::checked_gain).transpose().map_err(refuse)?;
    let media = req
        .audio
        .media
        .into_iter()
        .map(|g| g.map(crate::control::checked_gain).transpose())
        .collect::<anyhow::Result<Vec<_>>>()
        .map_err(refuse)?;
    let outcome = call
        .app
        .mixer
        .set_audio(req.id.clone(), gain, req.audio.muted, page, media)
        .await
        .map_err(|e| call.mixer_error(e))?;
    match outcome {
        AudioOutcome::Set(state) => body(state),
        AudioOutcome::NoSuchSource => {
            Err(RpcError::not_found("source", &req.id, &call.source_ids().await))
        }
        AudioOutcome::NotSuperimposed => Err(RpcError::not_in_state(format!(
            "source {} is not superimposed, so its sounds arrive already mixed and there is \
             nothing to balance. Its gain and mute still work: send those without page or media.",
            req.id
        ))),
    }
}

async fn seek(call: Call, params: Value) -> Result<Value, RpcError> {
    let req: SeekParams = call.params(&params)?;
    let position_ms = crate::control::checked_position(req.seek.position_ms).map_err(refuse)?;
    let outcome = call
        .app
        .mixer
        .seek(req.id.clone(), position_ms)
        .await
        .map_err(|e| call.mixer_error(e))?;
    match outcome {
        SeekOutcome::Moved(at) => body(at),
        SeekOutcome::NoSuchSource => {
            Err(RpcError::not_found("source", &req.id, &call.source_ids().await))
        }
        SeekOutcome::NotSeekable => Err(RpcError::not_in_state(format!(
            "source {} cannot be scrubbed: a live feed has no position to move to, it is \
             wherever it is now. Call source.list and seek one whose seekable is true.",
            req.id
        ))),
        SeekOutcome::Failed(message) => Err(RpcError::not_in_state(message)),
    }
}

fn refuse(e: anyhow::Error) -> RpcError {
    RpcError::invalid_params(e.to_string())
}

// ---------------------------------------------------------------------------
// source.set: changing a source while it runs, including where it runs
// ---------------------------------------------------------------------------

/// `source.set`: a full state assignment for one source.
///
/// Every field is optional and only what is named moves, which is how every
/// other setter in this protocol works. The one that matters here is `place`:
/// it moves a running source between the core, a sidecar and a node.
#[derive(Debug, serde::Serialize, serde::Deserialize, schemars::JsonSchema)]
pub struct SetSourceRequest {
    /// Source id.
    pub id: String,
    /// What to call it in the UI.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub name: Option<String>,
    /// Where it runs: `core`, `in-process`, `sidecar` or `node:<name>`.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub place: Option<godwinmix_core::node::Place>,
    /// How a remote source's media travels: `rtp`, `srt` or `whip`.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub transport: Option<godwinmix_core::node::BridgeTransport>,
    /// The latency budget in milliseconds, answered on the LATENCY query.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub latency_ms: Option<u32>,
    /// Params for the source's own kind. Merged over what it has.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub params: Option<std::collections::BTreeMap<String, Value>>,
}

pub(crate) fn register_set(reg: &mut Registry<Call>) {
    reg.register(
        MethodDef::new(
            "source.set",
            Scope::Operate,
            "Change a running source: its name, its params, or where it runs. Moving a \
             source between the core, a sidecar and a node is `place`; the programme keeps \
             its frame rate across the move and the compositor covers the swap.",
            handler(set),
        )
        .params(schema_of::<SetSourceRequest>)
        .result(schema_of::<SourceStatus>)
        .tool(
            "set_source",
            Tier::Search,
            "Change a running source in place: rename it, change its params, or move it \
             between the core, a sidecar process and a node on another machine with \
             `place`. Only the fields you name change. Moving a source rebuilds it where \
             you asked for it; the programme's frame rate is not affected, and a source \
             that is on air holds its picture while the new instance comes up. A plugin \
             that did not declare the placement is refused with the placements it did \
             declare.",
        ),
    );
}

async fn set(call: Call, params: Value) -> Result<Value, RpcError> {
    let req: SetSourceRequest = call.params(&params)?;
    let configs = call.app.mixer.configs().await.map_err(|e| call.mixer_error(e))?;
    let Some(current) = configs.sources.iter().find(|s| s.id == req.id).cloned() else {
        return Err(RpcError::not_found("source", &req.id, &call.source_ids().await));
    };

    let mut wanted = current.clone();
    if let Some(name) = req.name.clone() {
        wanted.name = Some(name);
    }
    if let Some(place) = req.place.clone() {
        wanted.place = Some(place);
    }
    if let Some(transport) = req.transport {
        wanted.transport = Some(transport);
    }
    if let Some(ms) = req.latency_ms {
        wanted.latency_ms = Some(ms);
    }
    if let Some(extra) = &req.params {
        for (key, value) in extra {
            match toml::Value::try_from(value) {
                Ok(v) => {
                    wanted.params.insert(key.clone(), v);
                }
                Err(e) => {
                    return Err(RpcError::invalid_params(format!(
                        "`params.{key}` is not something a config can hold: {e}"
                    )))
                }
            }
        }
    }
    let moving = wanted.placement() != current.placement();
    if moving {
        check_move(&call, &current, &wanted)?;
    }
    if call.dry_run {
        let mut diff = Vec::new();
        if moving {
            diff.push(format!(
                "move {} from {} to {}",
                req.id,
                current.placement(),
                wanted.placement()
            ));
        }
        if req.name.is_some() {
            diff.push(format!("rename {} to {}", req.id, wanted.display_name()));
        }
        if req.params.is_some() {
            diff.push(format!("change the params of {}", req.id));
        }
        return Ok(call.dry_run_answer(!diff.is_empty(), diff));
    }
    if !moving && req.params.is_none() && req.name.is_none() {
        // Nothing to do, and saying so is better than rebuilding a live source
        // for no reason (Kubernetes server side dry run's `would_change`).
        return body(find(&call, &req.id).await?);
    }

    // The move itself. Both commands go on the same queue, in order, with
    // nothing between them: the mixer serialises every request through one
    // path, so no take can land in the middle. The compositor is `force-live`
    // and the slate sits under every pad, so the programme keeps producing
    // frames at the canvas rate throughout and the frame interval does not
    // change. The source's own picture is held by the compositor pad until the
    // new instance's first frame replaces it.
    if let Some(runtime) = godwinmix_core::node::runtime::get() {
        // The reconciler's desired state moves first, so a tick landing during
        // the swap does not try to put the old instance back.
        match wanted.placement().node() {
            Some(node) => runtime.reconciler.want(godwinmix_core::node::reconcile::Desired {
                instance: wanted.id.clone(),
                type_id: wanted.type_id.clone().unwrap_or_default(),
                place: wanted.placement(),
                params: serde_json::to_value(wanted.effective_params()).unwrap_or(Value::Null),
                transport: wanted.bridge_transport(),
                latency_ms: wanted.latency_ms,
            }),
            None => {
                runtime.reconciler.forget(&wanted.id);
            }
        }
    }
    call.app
        .mixer
        .request(|ack| Command::RemoveSource(req.id.clone(), Some(ack)))
        .await
        .map_err(|e| call.mixer_error(e))?;
    call.app
        .mixer
        .request(|ack| Command::AddSource(Box::new(wanted.clone()), Some(ack)))
        .await
        .map_err(|e| call.mixer_error(e))?;
    let record = find(&call, &req.id).await?;
    call.app.hooks.fire(godwinmix_core::hooks::name::SOURCE_ADDED, || {
        serde_json::json!({
            "source": record.id,
            "uri": record.uri,
            "state": record.state,
            "place": wanted.placement().as_config(),
        })
    });
    body(record)
}

/// Refuse a move the plugin did not declare, before anything is torn down.
///
/// Error -32005, with `data.placements` listing what it did declare, so a
/// caller can offer the operator the ones that would work.
fn check_move(
    call: &Call,
    current: &godwinmix_core::config::SourceConfig,
    wanted: &godwinmix_core::config::SourceConfig,
) -> Result<(), RpcError> {
    let _ = call;
    let Some(type_id) = wanted.type_id.as_deref() else {
        // A source written as a bare URI has no plugin to ask, and the built
        // in kinds are all `core`. Moving one is a configuration mistake
        // rather than a placement refusal.
        return Err(RpcError::new(
            godwinmix_protocol::error::ErrorCode::Placement,
            format!(
                "`{}` is written as a bare URI, so it has no plugin to place. Write `type` \
                 saying which plugin it is, then set `place`.",
                current.id
            ),
        ));
    };
    let place = wanted.placement();
    let declared = godwinmix_core::plugin::remote::plugin_manifest(type_id, place.node())
        .map(|m| m.plugin.placements)
        .or_else(|| {
            godwinmix_core::plugin::loader::get(type_id.split('/').next().unwrap_or(type_id))
                .map(|p| p.manifest.plugin.placements)
        })
        // A built in kind declares nothing and runs in the core, which is the
        // one placement it has.
        .unwrap_or_else(|| vec!["core".to_string()]);
    godwinmix_core::node::check_placement(type_id, &place, &declared).map_err(|e| {
        RpcError::new(godwinmix_protocol::error::ErrorCode::Placement, format!("{e:#}"))
            .with("placements", serde_json::json!(declared))
            .with("retryable", serde_json::json!(false))
    })
}
