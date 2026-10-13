//! `gmx ctl media`: the media library from a terminal. With no subcommand
//! it lists the clips, narrowed by `--folder`, `--tag` and `--search`.

use super::ctl_rpc::call;
use super::Api;
use anyhow::Result;
use serde_json::{json, Value};

#[derive(Debug, Clone, clap::Args)]
pub struct MediaArgs {
    #[command(subcommand)]
    pub cmd: Option<MediaCmd>,
    /// Only the clips in this folder: `Sermons/2026`. "" is the top.
    #[arg(long)]
    pub folder: Option<String>,
    /// With --folder: its subfolders' clips too.
    #[arg(long)]
    pub subfolders: bool,
    /// Only clips with this tag. Repeat for clips with every one.
    #[arg(long = "tag")]
    pub tags: Vec<String>,
    /// Words to find in a clip's path or tags.
    #[arg(long)]
    pub search: Option<String>,
}

#[derive(Debug, Clone, clap::Subcommand)]
pub enum MediaCmd {
    /// Every folder, with its clips, and every tag in use.
    Folders,
    /// Make a folder: `Sermons`, or `Sermons/2026` inside another.
    Mkdir { path: String },
    /// Rename a folder or move it into another (`--into ""` for the top).
    MoveFolder {
        path: String,
        #[arg(long)]
        into: Option<String>,
        /// Its new name.
        #[arg(long)]
        to: Option<String>,
        #[arg(long)]
        force: bool,
    },
    /// Remove a folder. Say where what is inside goes: `up` or `top`.
    Rmdir {
        path: String,
        #[arg(long)]
        contents: String,
    },
    /// Move clips into a folder, or rename one with --name.
    Move {
        files: Vec<String>,
        /// The folder to move them into: `Sermons/2026`, "" for the top.
        #[arg(long)]
        to: Option<String>,
        /// A new file name, for one clip.
        #[arg(long)]
        name: Option<String>,
        /// Move a clip even while it is on programme.
        #[arg(long)]
        force: bool,
    },
    /// Tag clips: --add and --remove keep the others, --set replaces them.
    Tag {
        files: Vec<String>,
        #[arg(long)]
        add: Vec<String>,
        #[arg(long)]
        remove: Vec<String>,
        #[arg(long)]
        set: Option<Vec<String>>,
    },
}

pub async fn run(api: &Api, args: MediaArgs) -> Result<()> {
    let Some(cmd) = args.cmd else { return list(api, args).await };
    let answer = match cmd {
        MediaCmd::Folders => {
            let f = call(api, "media.folder.list", json!({})).await?;
            for d in f["folders"].as_array().into_iter().flatten() {
                println!("{:<40} {:>4} {:>4}", d["path"].as_str().unwrap_or(""), d["files"], d["total"]);
            }
            for t in f["tags"].as_array().into_iter().flatten() {
                println!("#{:<39} {:>4}", t["name"].as_str().unwrap_or(""), t["count"]);
            }
            return Ok(());
        }
        MediaCmd::Mkdir { path } => call(api, "media.folder.add", json!({ "name": path })).await?,
        MediaCmd::MoveFolder { path, into, to, force } => call(api, "media.folder.move", json!({ "name": path, "into": into, "to": to, "force": force })).await?,
        MediaCmd::Rmdir { path, contents } => call(api, "media.folder.remove", json!({ "name": path, "contents": contents })).await?,
        MediaCmd::Move { files, to, name, force } => call(api, "media.move", files_params(files, json!({ "folder": to, "new_name": name, "force": force }))).await?,
        MediaCmd::Tag { files, add, remove, set } => call(api, "media.tag", files_params(files, json!({ "add_tags": add, "remove_tags": remove, "tags": set }))).await?,
    };
    println!("{}", serde_json::to_string_pretty(&answer)?);
    Ok(())
}

/// The first file is the one the route names; the rest ride in `names`.
fn files_params(files: Vec<String>, mut params: Value) -> Value {
    let mut files = files.into_iter();
    if let (Some(map), Some(first)) = (params.as_object_mut(), files.next()) {
        map.retain(|_, v| !v.is_null());
        map.insert("id".into(), json!(first));
        map.insert("names".into(), json!(files.collect::<Vec<_>>()));
    }
    params
}

async fn list(api: &Api, args: MediaArgs) -> Result<()> {
    let mut q = json!({ "subfolders": args.subfolders });
    if let Some(f) = args.folder {
        q["folder"] = json!(f);
    }
    if !args.tags.is_empty() {
        q["tag"] = json!(args.tags.join(","));
    }
    if let Some(s) = args.search {
        q["query"] = json!(s);
    }
    let listing = call(api, "media.list", q).await?;
    if let Some(err) = listing["error"].as_str() {
        anyhow::bail!("{}: {err}", listing["dir"].as_str().unwrap_or(""));
    }
    for i in listing["items"].as_array().into_iter().flatten() {
        let secs = i["duration_ms"].as_u64().map(|m| format!("{:.1}s", m as f64 / 1000.0)).unwrap_or_else(|| "?".into());
        let tags: Vec<&str> = i["tags"].as_array().into_iter().flatten().filter_map(Value::as_str).collect();
        let mute = if i["has_audio"].as_bool() == Some(false) { " (no audio)" } else { "" };
        println!("{:<40} {secs:>7}{mute} {}", i["name"].as_str().unwrap_or(""), tags.iter().map(|t| format!("#{t}")).collect::<Vec<_>>().join(" "));
    }
    Ok(())
}
