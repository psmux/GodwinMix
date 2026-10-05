//! `template.fields`: what each field of a running graphic shows, and where
//! a client sets it.

use super::super::{body, handler};
use crate::control::call::Call;
use godwinmix_core::graphics::fill::value_of;
use godwinmix_core::graphics::html;
use godwinmix_core::plugin::kinds::html::params;
use godwinmix_core::plugin::kinds::template;
use godwinmix_protocol::error::RpcError;
use godwinmix_protocol::graphics::*;
use godwinmix_protocol::method::{schema_of, MethodDef, Registry, Tier};
use godwinmix_protocol::scope::Scope;
use serde_json::Value;

pub(super) fn register(reg: &mut Registry<Call>) {
    reg.register(
        MethodDef::new("template.fields", Scope::Read, "A running graphic's fields: each one's label, type, default and what it shows now.", handler(fields))
            .params(schema_of::<TemplateFieldsRequest>)
            .result(schema_of::<TemplateFields>)
            .tool(
                "template_fields",
                Tier::Search,
                "What each field of a graphic on this mixer shows now, with its label, type and default, \
                 and whether the source set it. Change one on air with `set_source` and \
                 params {\"fields\": {\"<name>\": \"<value>\"}}: only the fields you name change, the \
                 graphic is drawn again in place, and the programme does not miss a frame.",
            ),
    );
}

async fn fields(call: Call, params: Value) -> Result<Value, RpcError> {
    let req: TemplateFieldsRequest = call.params(&params)?;
    let configs = call.app.mixer.configs().await.map_err(|e| call.mixer_error(e))?;
    let Some(cfg) = configs.sources.iter().find(|s| s.id == req.id) else {
        return Err(RpcError::not_found("source", &req.id, &call.source_ids().await?));
    };
    let is_graphic = |uri: &str| template::name_in(uri).is_some() || html::name_in(uri).is_some();
    if !is_graphic(&cfg.uri) {
        let graphics: Vec<String> = configs.sources.iter().filter(|s| is_graphic(&s.uri)).map(|s| s.id.clone()).collect();
        return Err(RpcError::invalid_params(format!(
            "{} is not a graphic template, so it has no fields. The graphics on this mixer are: {}",
            req.id,
            if graphics.is_empty() { "none".to_string() } else { graphics.join(", ") }
        ))
        .with("id", req.id.clone())
        .with("graphics", graphics));
    }
    let unreadable = |e: anyhow::Error| RpcError::not_in_state(format!("{} cannot read its template: {e:#}", req.id)).with("id", req.id.clone());
    let (info, values) = match html::name_in(&cfg.uri) {
        Some(_) => {
            let read = params::validate(&cfg.effective_params()).map_err(unreadable)?;
            (read.template.info.clone(), read.values)
        }
        None => {
            let read = template::validate(&cfg.effective_params()).map_err(unreadable)?;
            (read.template.info.clone(), read.values)
        }
    };
    let brand = super::brand();
    let fields = info
        .fields
        .iter()
        .map(|f| {
            let (value, set) = value_of(f, &values, &brand);
            FieldValue { field: f.clone(), value, set }
        })
        .collect();
    body(TemplateFields { id: req.id, template: info.name.clone(), fields, path: "params.fields.<name>".into() })
}
