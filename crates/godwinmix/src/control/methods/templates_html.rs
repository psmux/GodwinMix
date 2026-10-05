//! The HTML half of the template methods, and `template.check`, which reads
//! either kind of template the way saving or drawing it would and answers
//! with every problem and its fix, writing nothing.

use super::super::{body, handler};
use crate::control::call::Call;
use godwinmix_core::graphics::html::{self, check, pack, problem};
use godwinmix_core::graphics::template::{Template, TemplateOrigin};
use godwinmix_protocol::error::RpcError;
use godwinmix_protocol::graphics::*;
use godwinmix_protocol::method::{schema_of, MethodDef, Registry, Tier};
use godwinmix_protocol::scope::Scope;
use serde_json::Value;

pub(super) fn register(reg: &mut Registry<Call>) {
    reg.register(
        MethodDef::new("template.check", Scope::Read, "Read an SVG or HTML template the way saving or drawing it would, and say what to fix. Writes nothing.", handler(check_one))
            .params(schema_of::<TemplateCheckRequest>)
            .result(schema_of::<TemplateChecked>)
            .tool(
                "check_template",
                Tier::Search,
                "Check a graphic template before saving or placing it: give `html` or `svg` (the whole \
                 document) or the `name` of one. Answers `ok` and every problem with its `fix`: a \
                 background that would cover the picture, a field shown and not declared, a font or \
                 script from the network, no way in. Fix each error and check again until `ok` is true.",
            ),
    );
}

/// The library's HTML templates and the pack's, for `template.list`.
pub(super) async fn listed(call: &Call) -> Result<(Vec<TemplateInfo>, Vec<String>), RpcError> {
    let dir = call.app.library.dir().to_path_buf();
    let (library, errors) = tokio::task::spawn_blocking(move || pack::library(&dir))
        .await
        .map_err(|e| RpcError::internal(format!("reading the media library: {e}")))?;
    Ok((pack::pack().into_iter().chain(library).map(|t| t.info).collect(), errors))
}

/// `template.get` for an HTML template: None when `name` is not one.
pub(super) fn get(name: &str) -> Option<Result<TemplateDoc, RpcError>> {
    let html_name = pack::PACK.iter().any(|(n, _)| *n == name.trim()) || name.trim().to_ascii_lowercase().ends_with(".html");
    if !html_name {
        return None;
    }
    Some(pack::load(name).map(|t| TemplateDoc { info: t.info, svg: String::new(), html: Some(t.html) }).map_err(|e| {
        let names: Vec<String> = pack::PACK.iter().map(|(n, _)| n.to_string()).collect();
        RpcError::not_found("template", name, &names).with("detail", format!("{e:#}")).with("list", "template.list")
    }))
}

/// `template.save` with `html`.
pub(super) async fn save(call: &Call, req: &TemplateSaveRequest, page: String) -> Result<Value, RpcError> {
    let dir = call.app.library.dir().to_path_buf();
    let file = pack::library_name(&req.name);
    let checked = serde_json::to_value(check::check(&page).0).unwrap_or_default();
    let (name, replace) = (req.name.clone(), req.replace);
    let saved = tokio::task::spawn_blocking(move || pack::save(&dir, &name, &page, replace))
        .await
        .map_err(|e| RpcError::internal(format!("saving the template: {e}")))?
        .map_err(|e| RpcError::invalid_params(format!("{e:#}. Nothing was written.")).with("name", file.clone()).with("problems", checked))?;
    let redrawn = if req.replace { super::redraw(call, &saved.info.name, html::name_in, pack::library_name).await } else { Vec::new() };
    let path = call.app.library.dir().join(&saved.info.name).display().to_string();
    body(TemplateSaved { template: saved.info, path, redrawn })
}

async fn check_one(call: Call, params: Value) -> Result<Value, RpcError> {
    let req: TemplateCheckRequest = call.params(&params)?;
    let checked = match (req.html, req.svg, req.name) {
        (Some(page), _, _) => html_checked(&page),
        (None, Some(svg), _) => svg_checked(&svg),
        (None, None, Some(name)) => named(&name),
        (None, None, None) => return Err(RpcError::invalid_params("give html or svg (a whole document) or the name of a template to check")),
    };
    body(checked)
}

fn html_checked(page: &str) -> TemplateChecked {
    let (problems, _) = check::check(page);
    let ok = problem::ok(&problems);
    let template = ok.then(|| html::HtmlTemplate::parse("checked.html", TemplateOrigin::Library, page.to_string()).ok().map(|t| t.info)).flatten();
    TemplateChecked { ok, problems, template }
}

fn svg_checked(svg: &str) -> TemplateChecked {
    let read = Template::parse("checked.svg", TemplateOrigin::Library, svg.to_string()).and_then(|t| {
        godwinmix_core::graphics::render(&t, &Default::default(), (480, 270)).map(|_| t)
    });
    match read {
        Ok(t) => TemplateChecked { ok: true, problems: Vec::new(), template: Some(t.info) },
        Err(e) => TemplateChecked {
            ok: false,
            problems: vec![problem::error(&format!("{e:#}"), "change what the message names and check again; get_template on a pack template shows a working one")],
            template: None,
        },
    }
}

fn named(name: &str) -> TemplateChecked {
    if let Some(found) = get(name) {
        return match found {
            Ok(doc) => html_checked(doc.html.as_deref().unwrap_or_default()),
            Err(e) => TemplateChecked { ok: false, problems: vec![problem::error(&e.message, "list_templates names every template")], template: None },
        };
    }
    match godwinmix_core::graphics::pack::load(name) {
        Ok(t) => svg_checked(&t.svg),
        Err(e) => TemplateChecked { ok: false, problems: vec![problem::error(&format!("{e:#}"), "list_templates names every template")], template: None },
    }
}
