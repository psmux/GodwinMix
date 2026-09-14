//! `filter.add`, `filter.set`, `filter.remove` and `filter.list`.
//!
//! A filter is a piece of picture processing hung on one source or on the
//! programme. The mixer owns the pads; everything here does is turn a call
//! into a `MixerHandle` request and turn the refusal back into an error that
//! names the next step.

use super::{body, handler};
use godwinmix_protocol::error::{ErrorCode, RpcError};
use godwinmix_protocol::method::{schema_of, MethodDef, Registry, Tier};
use godwinmix_protocol::scope::Scope;
use crate::control::call::Call;
use godwinmix_core::mixer::FilterOutcome;
use schemars::JsonSchema;
use serde::{Deserialize, Serialize};
use serde_json::{Map, Value};

pub fn register(reg: &mut Registry<Call>) {
    reg.register(
        MethodDef::new(
            "filter.add",
            Scope::Operate,
            "Hang a filter on one source or on the programme, live.",
            handler(add),
        )
        .params(schema_of::<AddFilterRequest>)
        .result(schema_of::<FilterRecord>)
        .tool(
            "add_filter",
            Tier::Search,
            "Put a picture filter on one source or on the whole programme without \
             stopping anything. `type` is a filter type id from `list_plugins`, `id` is \
             the name you will use to change or remove it, `source` names the source it \
             belongs to (leave it out and set `programme: true` for the whole output), \
             and `params` is that filter's own settings.",
        ),
    );

    reg.register(
        MethodDef::new(
            "filter.set",
            Scope::Operate,
            "Change a filter's settings in place. A filter that cannot take the change \
             while running says so rather than being restarted behind your back.",
            handler(set),
        )
        .params(schema_of::<SetFilterRequest>)
        .result(schema_of::<FilterRecord>),
    );

    reg.register(
        MethodDef::new(
            "filter.remove",
            Scope::Operate,
            "Take a filter out of the pipeline.",
            handler(remove),
        )
        .params(schema_of::<FilterIdRequest>)
        .result(schema_of::<FilterRemoved>)
        .destructive(),
    );

    reg.register(
        MethodDef::new(
            "filter.list",
            Scope::Read,
            "Every filter in place, with what it is and where it sits.",
            handler(list),
        )
        .result(schema_of::<FilterListing>),
    );
}

/// `filter.add`.
#[derive(Debug, Clone, Serialize, Deserialize, JsonSchema)]
pub struct AddFilterRequest {
    /// The name this filter answers to afterwards. A slug, and yours to pick.
    pub id: String,
    /// A filter type id, as `plugin.list` and `core.api` `kinds` report them.
    #[serde(rename = "type")]
    pub type_id: String,
    /// The source to hang it on. Leave it out and set `programme` to filter
    /// everything that goes out.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub source: Option<String>,
    /// Filter the programme rather than one source.
    #[serde(default, skip_serializing_if = "std::ops::Not::not")]
    pub programme: bool,
    /// `input` puts it before the proxy boundary, where the thumbnail sees it
    /// too; `programme` puts it on this source's programme branch only.
    #[serde(default = "default_side")]
    pub side: String,
    /// The filter's own settings. What goes in here is the filter's business,
    /// not the core's.
    #[serde(default, skip_serializing_if = "Map::is_empty")]
    pub params: Map<String, Value>,
}

fn default_side() -> String {
    "input".into()
}

/// `filter.set`.
#[derive(Debug, Clone, Serialize, Deserialize, JsonSchema)]
pub struct SetFilterRequest {
    pub id: String,
    /// The settings to apply. Only the keys named are changed.
    #[serde(default)]
    pub params: Map<String, Value>,
}

/// `filter.remove`, and anything else that names one filter.
#[derive(Debug, Clone, Serialize, Deserialize, JsonSchema)]
pub struct FilterIdRequest {
    pub id: String,
}

/// One filter as the core reports it.
#[derive(Debug, Clone, Serialize, Deserialize, JsonSchema)]
pub struct FilterRecord {
    pub id: String,
    #[serde(rename = "type")]
    pub type_id: String,
    /// Absent on a programme filter.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub source: Option<String>,
    pub side: String,
}

#[derive(Debug, Clone, Serialize, Deserialize, JsonSchema)]
pub struct FilterListing {
    pub filters: Vec<FilterRecord>,
}

#[derive(Debug, Clone, Serialize, Deserialize, JsonSchema)]
pub struct FilterRemoved {
    pub removed: String,
}

/// JSON in, TOML out, because a filter's settings are written in the config
/// file as well as over the wire and the plugin must see one shape.
///
/// A JSON null has no TOML spelling. Rather than inventing one, the key is
/// dropped, which is what "I am not setting this" means to a filter.
fn to_params(map: &Map<String, Value>) -> godwinmix_core::config::Params {
    let mut out = godwinmix_core::config::Params::new();
    for (k, v) in map {
        match toml::Value::try_from(v) {
            Ok(value) => {
                out.insert(k.clone(), value);
            }
            Err(_) => {
                tracing::debug!(key = %k, "a filter parameter had no TOML form and was dropped");
            }
        }
    }
    out
}

fn record(status: godwinmix_core::mixer::FilterStatus) -> FilterRecord {
    FilterRecord {
        id: status.id,
        type_id: status.type_id,
        source: status.source,
        side: status.side,
    }
}

/// The filter with this id, after the mixer has done the work, so the answer
/// says what is in the pipeline rather than what was asked for.
async fn read_back(call: &Call, id: &str) -> Result<FilterRecord, RpcError> {
    let filters = call.app.mixer.filters().await.map_err(|e| call.mixer_error(e))?;
    filters
        .into_iter()
        .find(|f| f.id == id)
        .map(record)
        .ok_or_else(|| RpcError::not_found("filter", id, &[]))
}

async fn add(call: Call, params: Value) -> Result<Value, RpcError> {
    let req: AddFilterRequest = call.params(&params)?;
    if req.source.is_none() && !req.programme {
        return Err(RpcError::invalid_params(
            "a filter goes somewhere: name a `source`, or set `programme: true` to filter \
             everything that goes out.",
        ));
    }
    let side = match req.side.as_str() {
        "input" => godwinmix_core::config::FilterAttachSide::Input,
        "programme" => godwinmix_core::config::FilterAttachSide::Programme,
        other => {
            return Err(RpcError::invalid_params(format!(
                "`side` is \"input\" or \"programme\", not \"{other}\". \"input\" also \
                 filters the thumbnail; \"programme\" filters only what goes out."
            )))
        }
    };
    let cfg = godwinmix_core::config::FilterConfig {
        id: req.id.clone(),
        type_id: req.type_id.clone(),
        attach: godwinmix_core::config::FilterAttach {
            source: req.source.clone(),
            side,
            programme: req.programme,
        },
        params: to_params(&req.params),
    };
    call.app.mixer.add_filter(cfg).await.map_err(|e| call.mixer_error(e))?;
    body(read_back(&call, &req.id).await?)
}

async fn set(call: Call, params: Value) -> Result<Value, RpcError> {
    let req: SetFilterRequest = call.params(&params)?;
    let outcome = call
        .app
        .mixer
        .set_filter(req.id.clone(), to_params(&req.params))
        .await
        .map_err(|e| call.mixer_error(e))?;
    match outcome {
        FilterOutcome::Applied => body(read_back(&call, &req.id).await?),
        FilterOutcome::RestartRequired(why) => Err(RpcError::new(
            ErrorCode::RestartRequired,
            format!(
                "{} cannot take that change while it is running: {why}. Remove it with \
                 filter.remove and add it again with the settings you want.",
                req.id
            ),
        )),
        FilterOutcome::NoSuchFilter(id) => {
            let known: Vec<String> = call
                .app
                .mixer
                .filters()
                .await
                .map(|f| f.into_iter().map(|f| f.id).collect())
                .unwrap_or_default();
            Err(RpcError::not_found("filter", &id, &known))
        }
        FilterOutcome::Failed(why) => Err(RpcError::not_in_state(why)),
    }
}

async fn remove(call: Call, params: Value) -> Result<Value, RpcError> {
    let req: FilterIdRequest = call.params(&params)?;
    // Checked here rather than left to the mixer, so an unknown id answers
    // with the ids that exist like every other -32004 does. The mixer's own
    // refusal says only that there is no such filter, which leaves a caller
    // guessing at the name.
    let ids = filter_ids(&call).await;
    if !ids.contains(&req.id) {
        return Err(RpcError::not_found("filter", &req.id, &ids));
    }
    if call.dry_run {
        return Ok(call.dry_run_answer(true, vec![format!("take the filter {} out", req.id)]));
    }
    call.app
        .mixer
        .remove_filter(req.id.clone())
        .await
        .map_err(|e| call.mixer_error(e))?;
    body(FilterRemoved { removed: req.id })
}

/// Every filter in place, wherever it is hung.
async fn filter_ids(call: &Call) -> Vec<String> {
    call.app
        .mixer
        .filters()
        .await
        .map(|list| list.into_iter().map(|f| f.id).collect())
        .unwrap_or_default()
}

async fn list(call: Call, _params: Value) -> Result<Value, RpcError> {
    let filters = call.app.mixer.filters().await.map_err(|e| call.mixer_error(e))?;
    body(FilterListing { filters: filters.into_iter().map(record).collect() })
}
