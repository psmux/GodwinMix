//! `gmx ctl template`: the graphic templates, from a terminal.
//!
//! The four `template.*` methods with the shapes a person types: a name, a
//! file, a source id. Drawing one is `gmx ctl source add <id> template:<name>`
//! and changing a field is `gmx ctl source set <id> --param fields.<name>=...`.

use super::ctl_rpc::call;
use super::Api;
use anyhow::{Context, Result};
use serde_json::{json, Value};
use std::path::PathBuf;

#[derive(Debug, Clone, clap::Subcommand)]
pub enum TemplateCmd {
    /// Every graphic template: the built in pack and the SVG templates in
    /// the media library, with their fields.
    List,
    /// One template's SVG, to copy and change.
    Get {
        name: String,
        /// Write the SVG here instead of printing it.
        #[arg(long)]
        out: Option<PathBuf>,
    },
    /// Check an SVG template and write it into the media library.
    Save {
        /// The name to save it as, `.svg` added when it has none.
        name: String,
        /// The SVG file on this machine.
        file: PathBuf,
        /// Write over a template of the same name, and draw every source
        /// that uses it again.
        #[arg(long)]
        replace: bool,
    },
    /// What each field of a graphic on air shows now.
    Fields { source: String },
}

pub async fn run(api: &Api, cmd: TemplateCmd) -> Result<()> {
    match cmd {
        TemplateCmd::List => {
            let list = call(api, "template.list", json!({})).await?;
            for t in list["templates"].as_array().into_iter().flatten() {
                let fields: Vec<&str> = t["fields"].as_array().into_iter().flatten().filter_map(|f| f["name"].as_str()).collect();
                println!("{:<24} {:<8} {}", t["uri"].as_str().unwrap_or(""), t["origin"].as_str().unwrap_or(""), fields.join(", "));
            }
            for e in list["errors"].as_array().into_iter().flatten() {
                eprintln!("not a template: {}", e.as_str().unwrap_or(""));
            }
        }
        TemplateCmd::Get { name, out } => {
            let doc = call(api, "template.get", json!({ "name": name })).await?;
            let svg = doc["svg"].as_str().unwrap_or("");
            match out {
                Some(path) => {
                    std::fs::write(&path, svg).with_context(|| format!("writing {}", path.display()))?;
                    println!("wrote {}", path.display());
                }
                None => println!("{svg}"),
            }
        }
        TemplateCmd::Save { name, file, replace } => {
            let svg = std::fs::read_to_string(&file).with_context(|| format!("reading {}", file.display()))?;
            let saved = call(api, "template.save", json!({ "name": name, "svg": svg, "replace": replace })).await?;
            println!("saved {} as {}", saved["path"].as_str().unwrap_or(""), saved["template"]["uri"].as_str().unwrap_or(""));
            print_redrawn(&saved["redrawn"]);
        }
        TemplateCmd::Fields { source } => {
            let fields = call(api, "template.fields", json!({ "id": source })).await?;
            for f in fields["fields"].as_array().into_iter().flatten() {
                let mark = if f["set"].as_bool() == Some(true) { "set" } else { "default" };
                println!("{:<14} {:<8} {}", f["name"].as_str().unwrap_or(""), mark, f["value"].as_str().unwrap_or(""));
            }
        }
    }
    Ok(())
}

fn print_redrawn(redrawn: &Value) {
    let ids: Vec<&str> = redrawn.as_array().into_iter().flatten().filter_map(Value::as_str).collect();
    if !ids.is_empty() {
        println!("drawn again on air: {}", ids.join(", "));
    }
}
