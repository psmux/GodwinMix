//! Graphic templates: list them, read one, save one, and read a running
//! graphic's fields (`source.fields`). Drawing one is `source.add` with a
//! `template:` address, and changing a field on air is `source.set` with
//! `params.fields`, so nothing here touches the programme except
//! `template.save` with `replace`, which asks every source drawing the file
//! to draw it again.

use super::{body, handler};
use crate::control::call::Call;
use godwinmix_core::graphics::{self, pack, Template};
use godwinmix_protocol::error::RpcError;
use godwinmix_protocol::graphics::*;
use godwinmix_protocol::method::{schema_of, MethodDef, Registry, Tier};
use godwinmix_protocol::scope::Scope;
use serde_json::Value;

#[path = "templates_fields.rs"]
mod fields;

pub fn register(reg: &mut Registry<Call>) {
    reg.register(
        MethodDef::new("template.list", Scope::Read, "The graphic templates: the built in pack and the SVG templates in the media library, each with its fields.", handler(list))
            .result(schema_of::<TemplateList>)
            .tool(
                "list_templates",
                Tier::Search,
                "Every graphic template this mixer can draw: the built in pack (news lower third, breaking \
                 news bar, headline strap, score bug, logo bug, title card, quote card, location tag) and any \
                 SVG template in the media library. Each comes with its `uri` for `add_source` and its \
                 fields, with label, type and default. Start here when asked for a lower third, a strap, a \
                 bug or a card.",
            ),
    );
    reg.register(
        MethodDef::new("template.get", Scope::Read, "One template, with its SVG as written.", handler(get))
            .params(schema_of::<TemplateGetRequest>)
            .result(schema_of::<TemplateDoc>)
            .tool(
                "get_template",
                Tier::Search,
                "One graphic template with its whole SVG. Read a pack template this way to copy its \
                 layout, change it, and save the result under a new name with `save_template`.",
            ),
    );
    reg.register(
        MethodDef::new("template.save", Scope::Operate, "Check an SVG template and write it into the media library.", handler(save))
            .params(schema_of::<TemplateSaveRequest>)
            .result(schema_of::<TemplateSaved>)
            .not_idempotent()
            .tool(
                "save_template",
                Tier::Search,
                "Write an SVG graphic template into the mixer's media library, after checking it renders: it \
                 needs a viewBox, field names in {{double braces}}, and no address on the network. The answer \
                 has the fields it found and the `uri` to add it by. With `replace` true it writes over a \
                 file of the same name, and every source drawing it is drawn again on air with no rebuild, \
                 which is how to fix a design you are looking at.",
            ),
    );
    fields::register(reg);
}

async fn list(call: Call, _: Value) -> Result<Value, RpcError> {
    let dir = call.app.library.dir().to_path_buf();
    let (library, errors) = tokio::task::spawn_blocking(move || pack::library(&dir))
        .await
        .map_err(|e| RpcError::internal(format!("reading the media library: {e}")))?;
    let templates = pack::pack().into_iter().chain(library).map(|t| t.info).collect();
    body(TemplateList { templates, errors })
}

/// The template `name` names, or the error that lists the ones there are.
pub(crate) fn load(name: &str) -> Result<Template, RpcError> {
    pack::load(name).map_err(|e| {
        let names: Vec<String> = pack::PACK.iter().map(|(n, _)| n.to_string()).collect();
        RpcError::not_found("template", name, &names).with("detail", e.to_string()).with("list", "template.list")
    })
}

async fn get(call: Call, params: Value) -> Result<Value, RpcError> {
    let req: TemplateGetRequest = call.params(&params)?;
    let t = load(&req.name)?;
    body(TemplateDoc { info: t.info, svg: t.svg })
}

async fn save(call: Call, params: Value) -> Result<Value, RpcError> {
    let req: TemplateSaveRequest = call.params(&params)?;
    if !call.app.library.cfg().allow_upload {
        return Err(RpcError::not_in_state(
            "this mixer does not take files into its media library (media.allow_upload is off). Turn it on in Settings, or copy the SVG into the media folder by hand",
        )
        .with_action(godwinmix_protocol::ErrorAction::open_setting("Allow uploads", "media.allow_upload")));
    }
    let dir = call.app.library.dir().to_path_buf();
    let file = pack::library_name(&req.name);
    let (name, svg, replace) = (req.name.clone(), req.svg.clone(), req.replace);
    let saved = tokio::task::spawn_blocking(move || pack::save(&dir, &name, &svg, replace))
        .await
        .map_err(|e| RpcError::internal(format!("saving the template: {e}")))?
        .map_err(|e| RpcError::invalid_params(format!("{e:#}. Nothing was written.")).with("name", file.clone()))?;
    let redrawn = if req.replace { redraw(&call, &saved.info.name).await } else { Vec::new() };
    let path = call.app.library.dir().join(&saved.info.name).display().to_string();
    body(TemplateSaved { template: saved.info, path, redrawn })
}

/// Ask every running source that draws library template `name` to take its
/// own params again, which reads the file again and draws it in place.
async fn redraw(call: &Call, name: &str) -> Vec<String> {
    let Ok(configs) = call.app.mixer.configs().await else { return Vec::new() };
    let mut redrawn = Vec::new();
    for cfg in configs.sources {
        let names = godwinmix_core::plugin::kinds::template::name_in(&cfg.uri).map(pack::library_name);
        if names.as_deref() != Some(name) {
            continue;
        }
        if call.app.mixer.configure_source(cfg.id.clone(), cfg.params.clone()).await.is_ok() {
            redrawn.push(cfg.id);
        }
    }
    redrawn
}

/// The brand and the library, for a caller outside the core.
pub(crate) fn brand() -> graphics::BrandConfig {
    graphics::brand::brand()
}
