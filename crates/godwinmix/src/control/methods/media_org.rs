//! `media.tag` and `media.move`: tagging files, and moving or renaming them
//! on disk with everything that uses them following.

use super::{body, handler, media_refs};
use crate::control::call::Call;
use godwinmix_core::media_org::{self as org, Moved, Refusal};
use godwinmix_protocol::error::{ErrorCode, RpcError};
use godwinmix_protocol::media::*;
use godwinmix_protocol::method::{schema_of, MethodDef, Registry, Tier};
use godwinmix_protocol::scope::Scope;
use godwinmix_protocol::types::Event;
use serde_json::Value;

pub fn register(reg: &mut Registry<Call>) {
    reg.register(
        MethodDef::new("media.tag", Scope::Operate, "Change the tags of one library file or several: replace them, add some, or take some off.", handler(tag))
            .params(schema_of::<MediaTagRequest>)
            .result(schema_of::<Vec<MediaTagged>>)
            .tool(
                "tag_media",
                Tier::Search,
                "Tag clips and pictures in the media library so they can be found: `add_tags` keeps the tags a file has, \
                 `tags` replaces them, `remove_tags` takes some off. Name files by their path from list_media, one in `name` \
                 or several in `names`. Tags are free words, matched without regard to case.",
            ),
    );
    reg.register(
        MethodDef::new("media.move", Scope::Operate, "Move library files into another folder, or rename one, on disk. Sources using them follow.", handler(move_files))
            .params(schema_of::<MediaMoveRequest>)
            .result(schema_of::<MediaMoved>)
            .not_idempotent()
            .tool(
                "move_media",
                Tier::Search,
                "Move clips and pictures into a folder of the media library (`folder`, a path such as Sermons/2026, \"\" for the top), \
                 or rename one (`new_name`). The files move on disk with their tags, and every source using one is pointed at the \
                 new place. Refused while a file is on programme unless `force` is true. Pass dry_run true to see the plan.",
            ),
    );
}

/// A refusal from the library as the protocol says it: not found when the
/// thing named is not there, bad params otherwise, the field and the
/// alternatives in `data` either way.
pub fn refused(r: Refusal) -> RpcError {
    let code = if r.missing { ErrorCode::NotFound } else { ErrorCode::InvalidParams };
    let mut e = RpcError::new(code, r.message).with("field", r.field);
    if !r.valid.is_empty() {
        e = e.with("valid", r.valid);
    }
    e
}

/// The files a request names, one or several, at least one.
pub fn named(one: Option<String>, many: Vec<String>) -> Result<Vec<String>, RpcError> {
    let mut out: Vec<String> = one.into_iter().chain(many).map(|n| n.trim().to_string()).filter(|n| !n.is_empty()).collect();
    out.dedup();
    if out.is_empty() {
        return Err(RpcError::invalid_params("name the file in `name`, or several in `names`, by their paths from media.list.").with("field", "name"));
    }
    Ok(out)
}

async fn tag(call: Call, params: Value) -> Result<Value, RpcError> {
    let req: MediaTagRequest = call.params(&params)?;
    let names = named(req.name, req.names)?;
    let dir = call.app.library.dir().to_path_buf();
    for n in &names {
        call.app.library.resolve(n).map_err(|e| {
            RpcError::not_found("media file", n, &[]).with("detail", e.to_string()).with("field", "name")
        })?;
    }
    let replace = req.tags.as_ref().map(|v| tag_words(Some(v)));
    let (add, remove) = (tag_words(req.add_tags.as_ref()), tag_words(req.remove_tags.as_ref()));
    let list = names.clone();
    let after = blocking(move || org::tag_files(&dir, &list, replace.as_deref(), &add, &remove)).await?;
    for n in &names {
        call.app.mixer.emit(Event::MediaChanged { name: n.clone(), conversion: None });
    }
    body(names.into_iter().zip(after).map(|(name, tags)| MediaTagged { name, tags }).collect::<Vec<_>>())
}

async fn move_files(call: Call, params: Value) -> Result<Value, RpcError> {
    let req: MediaMoveRequest = call.params(&params)?;
    let names = named(req.name, req.names)?;
    if req.new_name.is_some() && names.len() > 1 {
        return Err(RpcError::invalid_params("new_name renames one file, and this names several. Move them first, then rename each on its own.").with("field", "new_name"));
    }
    let root = call.app.library.dir().to_path_buf();
    let (folder, new_name) = (req.folder.clone(), req.new_name.clone());
    let plans: Vec<Moved> = blocking(move || {
        names.iter().map(|n| org::plan_file(&root, n, folder.as_deref(), new_name.as_deref())).collect::<Result<Vec<_>, _>>()
    })
    .await?;
    let plans: Vec<Moved> = plans.into_iter().filter(|m| m.from != m.to).collect();
    let froms: Vec<_> = plans.iter().map(|m| m.from_path.clone()).collect();
    media_refs::check(&call, &froms, req.force).await?;
    if call.dry_run {
        return Ok(call.dry_run_answer(!plans.is_empty(), plans.iter().map(|m| format!("move {} to {}", m.from, m.to)).collect()));
    }
    done(&call, plans).await
}

/// Do the moves, carry the tags, point everything at the new places, and
/// tell every open page.
pub async fn done(call: &Call, plans: Vec<Moved>) -> Result<Value, RpcError> {
    let dir = call.app.library.dir().to_path_buf();
    let todo = plans.clone();
    blocking(move || {
        for m in &todo {
            org::move_file(m)?;
            org::moved(&dir, &m.from, &m.to);
        }
        Ok(())
    })
    .await?;
    let updated = media_refs::follow(call, &plans).await;
    for m in &plans {
        call.app.mixer.emit(Event::MediaChanged { name: m.to.clone(), conversion: None });
    }
    body(MediaMoved { moved: plans.into_iter().map(|m| MediaMove { from: m.from, to: m.to }).collect(), updated })
}

/// File work off the async workers, a refusal as the protocol says it.
pub async fn blocking<T: Send + 'static>(f: impl FnOnce() -> Result<T, Refusal> + Send + 'static) -> Result<T, RpcError> {
    tokio::task::spawn_blocking(f).await.map_err(|e| RpcError::internal(format!("the media library work stopped: {e}")))?.map_err(refused)
}
