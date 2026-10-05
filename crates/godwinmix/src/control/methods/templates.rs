//! Graphic templates: list them, read one, check one, save one, and read a
//! running graphic's fields (`template.fields`). Drawing one is `source.add`
//! with a `template:` (SVG) or `html:` (HTML) address, and changing a field
//! on air is `source.set` with `params.fields`, so nothing here touches the
//! programme except `template.save` with `replace`, which asks every source
//! drawing the file to draw it again.

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
#[path = "templates_html.rs"]
mod html;

pub fn register(reg: &mut Registry<Call>) {
    reg.register(
        MethodDef::new("template.list", Scope::Read, "The graphic templates: the built in packs and the SVG and HTML templates in the media library, each with its fields.", handler(list))
            .result(schema_of::<TemplateList>)
            .tool(
                "list_templates",
                Tier::Standard,
                "Every graphic template this mixer can draw, each with its `uri` for `add_source` and its \
                 fields (label, type, default). Two formats. `svg`: stills the mixer draws for almost \
                 nothing (news lower third, breaking news bar, headline strap, score bug, logo bug, title \
                 card, quote card, location tag, virtual set desks). `html`: designs that move, drawn by the \
                 browser renderer (animated lower thirds, tickers, a live score bug, a countdown, 3D title \
                 card and logo, looping backgrounds, starting soon, virtual set backgrounds). Plus any \
                 template in the media library. Start here when asked for a lower third, a strap, a bug, a \
                 ticker, a card, a background or a set.",
            ),
    );
    reg.register(
        MethodDef::new("template.get", Scope::Read, "One template, with its SVG or HTML as written.", handler(get))
            .params(schema_of::<TemplateGetRequest>)
            .result(schema_of::<TemplateDoc>)
            .tool(
                "get_template",
                Tier::Search,
                "One graphic template with its whole SVG or HTML. Read a pack template this way to copy \
                 it, change the words, colours and layout, and save the result under a new name with \
                 `save_template`.",
            ),
    );
    reg.register(
        MethodDef::new("template.save", Scope::Operate, "Check an SVG or HTML template and write it into the media library.", handler(save))
            .params(schema_of::<TemplateSaveRequest>)
            .result(schema_of::<TemplateSaved>)
            .not_idempotent()
            .tool(
                "save_template",
                Tier::Standard,
                "Write a graphic template into the mixer's media library after checking it: `svg` for a \
                 still (a viewBox, fields in {{double braces}}), or `html` for one that moves (a \
                 gmx-template block declaring its fields, a transparent background, nothing from the \
                 network). A template with a mistake is refused with what to fix; `check_template` says \
                 the same without writing. With `replace` true it writes over a file of the same name, \
                 and every source drawing it is drawn again on air.",
            ),
    );
    html::register(reg);
    fields::register(reg);
}

async fn list(call: Call, _: Value) -> Result<Value, RpcError> {
    let dir = call.app.library.dir().to_path_buf();
    let (library, errors) = tokio::task::spawn_blocking(move || pack::library(&dir))
        .await
        .map_err(|e| RpcError::internal(format!("reading the media library: {e}")))?;
    let (pages, page_errors) = html::listed(&call).await?;
    let templates = pack::pack().into_iter().chain(library).map(|t| t.info).chain(pages).collect();
    body(TemplateList { templates, errors: errors.into_iter().chain(page_errors).collect() })
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
    if let Some(doc) = html::get(&req.name) {
        return body(doc?);
    }
    let t = load(&req.name)?;
    body(TemplateDoc { info: t.info, svg: t.svg, html: None })
}

async fn save(call: Call, params: Value) -> Result<Value, RpcError> {
    let req: TemplateSaveRequest = call.params(&params)?;
    if !call.app.library.cfg().allow_upload {
        return Err(RpcError::not_in_state(
            "this mixer does not take files into its media library (media.allow_upload is off). Turn it on in Settings, or copy the file into the media folder by hand",
        )
        .with_action(godwinmix_protocol::ErrorAction::open_setting("Allow uploads", "media.allow_upload")));
    }
    if let Some(page) = req.html.clone() {
        return html::save(&call, &req, page).await;
    }
    if req.svg.trim().is_empty() {
        return Err(RpcError::invalid_params("give the template as svg (a whole SVG document) or html (a whole HTML document). Nothing was written."));
    }
    let dir = call.app.library.dir().to_path_buf();
    let file = pack::library_name(&req.name);
    let (name, svg, replace) = (req.name.clone(), req.svg.clone(), req.replace);
    let saved = tokio::task::spawn_blocking(move || pack::save(&dir, &name, &svg, replace))
        .await
        .map_err(|e| RpcError::internal(format!("saving the template: {e}")))?
        .map_err(|e| RpcError::invalid_params(format!("{e:#}. Nothing was written.")).with("name", file.clone()))?;
    let redrawn = if req.replace { redraw(&call, &saved.info.name, godwinmix_core::plugin::kinds::template::name_in, pack::library_name).await } else { Vec::new() };
    let path = call.app.library.dir().join(&saved.info.name).display().to_string();
    body(TemplateSaved { template: saved.info, path, redrawn })
}

/// Ask every running source that draws library template `name` to take its
/// own params again, which reads the file again and draws it in place.
pub(crate) async fn redraw(call: &Call, name: &str, name_in: fn(&str) -> Option<&str>, file: fn(&str) -> String) -> Vec<String> {
    let Ok(configs) = call.app.mixer.configs().await else { return Vec::new() };
    let mut redrawn = Vec::new();
    for cfg in configs.sources {
        if name_in(&cfg.uri).map(file).as_deref() != Some(name) {
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
