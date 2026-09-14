//! Sources: list, get, add, remove, the faders and the scrubber.

use super::{body, handler};
use crate::api::error::RpcError;
use crate::api::method::{schema_of, MethodDef, Registry, Tier};
use crate::api::requests::*;
use crate::api::scope::Scope;
use crate::api::types::*;
use crate::control::call::Call;
use crate::mixer::{AudioOutcome, Command, SeekOutcome};
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
    body(find(&call, &id).await?)
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
