//! `media.folder.*`: the library's folders, which are directories on disk.

use super::media_org::{blocking, refused};
use super::{body, handler, media_refs};
use crate::control::call::Call;
use godwinmix_core::media_org::{self as org, Contents, Moved};
use godwinmix_protocol::error::RpcError;
use godwinmix_protocol::media::*;
use godwinmix_protocol::method::{schema_of, MethodDef, Registry, Tier};
use godwinmix_protocol::scope::Scope;
use godwinmix_protocol::types::Event;
use serde_json::{json, Value};

pub fn register(reg: &mut Registry<Call>) {
    reg.register(
        MethodDef::new("media.folder.list", Scope::Read, "Every folder in the media library with how many files are in it, and every tag in use.", handler(list))
            .result(schema_of::<MediaFolders>)
            .tool("list_media_folders", Tier::Search, "The folders of the media library, `Sermons/2026` style, each with the files right in it and the files anywhere under it, and every tag in use with how many files carry it."),
    );
    reg.register(
        MethodDef::new("media.folder.add", Scope::Operate, "Make a folder in the media library, inside another when the name has a / in it.", handler(add))
            .params(schema_of::<MediaFolderAddRequest>)
            .tool("add_media_folder", Tier::Search, "Make a folder in the media library: `Sermons`, or `Sermons/2026` to make one inside another. Asking for one that is there already answers it."),
    );
    reg.register(
        MethodDef::new("media.folder.move", Scope::Operate, "Rename a media folder, move it into another folder or to the top, or both. Sources using its files follow.", handler(move_folder))
            .params(schema_of::<MediaFolderMoveRequest>)
            .result(schema_of::<MediaMoved>)
            .not_idempotent()
            .tool("move_media_folder", Tier::Search, "Rename a folder of the media library (`to`), move it into another (`into`, \"\" for the top), or both. It moves on disk with everything in it, and sources using its files follow. A folder cannot go inside itself."),
    );
    reg.register(
        MethodDef::new("media.folder.remove", Scope::Operate, "Remove a media folder, moving what is in it up a level or to the top first. No file is deleted.", handler(remove))
            .params(schema_of::<MediaFolderRemoveRequest>)
            .result(schema_of::<MediaMoved>)
            .destructive()
            .tool("remove_media_folder", Tier::Search, "Remove a folder of the media library. Say what happens to what is in it: `contents` \"up\" moves it into the folder above, \"top\" to the top of the library. No clip is deleted, and sources using the files follow."),
    );
}

async fn list(call: Call, _: Value) -> Result<Value, RpcError> {
    let dir = call.app.library.dir().to_path_buf();
    let depth = call.app.library.cfg().max_depth;
    let answer = blocking(move || {
        let folders = org::list_folders(&dir, depth);
        let names: Vec<String> = super::project::media_names(&dir, depth);
        Ok(MediaFolders { folders: org::folder_counts(&folders, &names), tags: org::tag_counts(&org::load(&dir)) })
    })
    .await?;
    body(answer)
}

async fn add(call: Call, params: Value) -> Result<Value, RpcError> {
    let req: MediaFolderAddRequest = call.params(&params)?;
    let dir = call.app.library.dir().to_path_buf();
    call.app.library.ensure_dir().map_err(|e| RpcError::internal(format!("the media folder {} could not be made: {e}", dir.display())))?;
    let (path, made) = blocking(move || org::make_folder(&dir, &req.name)).await?;
    if made {
        call.app.mixer.emit(Event::MediaChanged { name: String::new(), conversion: None });
    }
    Ok(json!({ "path": path, "made": made }))
}

async fn move_folder(call: Call, params: Value) -> Result<Value, RpcError> {
    let req: MediaFolderMoveRequest = call.params(&params)?;
    let dir = call.app.library.dir().to_path_buf();
    let from = org::rel_path(&req.name, "name").map_err(refused)?;
    media_refs::check(&call, &[org::plain(&dir, &from)], req.force).await?;
    if call.dry_run {
        return Ok(call.dry_run_answer(true, vec![format!("move the folder {from} and everything in it")]));
    }
    let root = dir.clone();
    let m = blocking(move || {
        let m = org::move_folder(&root, &req.name, req.into.as_deref(), req.to.as_deref())?;
        org::moved(&root, &m.from, &m.to);
        Ok(m)
    })
    .await?;
    followed(&call, vec![m]).await
}

async fn remove(call: Call, params: Value) -> Result<Value, RpcError> {
    let req: MediaFolderRemoveRequest = call.params(&params)?;
    let contents = match req.contents.as_deref().map(|c| c.trim().to_lowercase()) {
        Some(c) if c == "up" || c == "parent" => Contents::Up,
        Some(c) if c == "top" || c == "all" => Contents::Top,
        _ => {
            return Err(RpcError::invalid_params(
                "say what happens to what is in the folder: contents \"up\" moves it into the folder above, \"top\" to the top of the library. No file is deleted either way.",
            )
            .with("field", "contents")
            .with("valid", vec!["up", "top"]))
        }
    };
    let dir = call.app.library.dir().to_path_buf();
    let rel = org::rel_path(&req.name, "name").map_err(refused)?;
    media_refs::check(&call, &[org::plain(&dir, &rel)], req.force).await?;
    if call.dry_run {
        return Ok(call.dry_run_answer(true, vec![format!("remove the folder {rel}, keeping everything in it")]));
    }
    let root = dir.clone();
    let moved = blocking(move || {
        let moved = org::remove_folder(&root, &rel, contents)?;
        for m in &moved {
            org::moved(&root, &m.from, &m.to);
        }
        Ok(moved)
    })
    .await?;
    followed(&call, moved).await
}

async fn followed(call: &Call, moved: Vec<Moved>) -> Result<Value, RpcError> {
    let updated = media_refs::follow(call, &moved).await;
    call.app.mixer.emit(Event::MediaChanged { name: String::new(), conversion: None });
    body(MediaMoved { moved: moved.into_iter().map(|m| MediaMove { from: m.from, to: m.to }).collect(), updated })
}
