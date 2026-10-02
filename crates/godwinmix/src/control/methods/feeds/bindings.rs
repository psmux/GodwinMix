//! `feed.binding.*`: from a value in a feed to a param, a field or a scene
//! parameter on air.

use super::super::{body, handler};
use super::ctx;
use crate::control::call::Call;
use godwinmix_protocol::feeds::*;
use godwinmix_protocol::method::{any_object, schema_of, MethodDef, Registry, Tier};
use godwinmix_protocol::scope::Scope;
use serde_json::{json, Value};

pub fn register(reg: &mut Registry<Call>) {
    reg.register(
        MethodDef::new(
            "feed.binding.add",
            Scope::Operate,
            "Bind a value in a feed to a target: a source's param by path (`params.text`, \
             `params.items`, `params.fields.headline`), a graphic's field through its update action, \
             or a scene parameter. It writes at once if the feed has been read, and after that only \
             when the value changes.",
            handler(|call: Call, params: Value| async move {
                let req: BindingAddRequest = call.params(&params)?;
                body(call.app.feeds.bind_add(&ctx(&call), req).await?)
            }),
        )
        .params(schema_of::<BindingAddRequest>)
        .result(schema_of::<BindingStatus>)
        .not_idempotent()
        .tool(
            "bind_feed",
            Tier::Search,
            "Put a value from a live data feed on screen and keep it current. {feed: \"news\", select: \
             \"items[].title\", limit: 10, to: {source: \"ticker\", path: \"params.items\"}} puts the \
             first ten RSS headlines in a ticker. `to` is one of {source, path} for a text, ticker or \
             any source param (path starts params.), {graphic, field, item?} for an OGraf graphic's \
             field, or {scene_param} for a {{name}} used across the scenes. `template` combines \
             fields, `join` makes a list one string. Use test_feed first to find the path. It is \
             written only when the value changes.",
        ),
    );

    reg.register(
        MethodDef::new(
            "feed.binding.set",
            Scope::Operate,
            "Change a binding's selection or target. Only what is named changes, and the value is \
             written again at once.",
            handler(|call: Call, params: Value| async move {
                let req: BindingSetRequest = call.params(&params)?;
                body(call.app.feeds.bind_set(&ctx(&call), req).await?)
            }),
        )
        .params(schema_of::<BindingSetRequest>)
        .result(schema_of::<BindingStatus>)
        .tool(
            "set_feed_binding",
            Tier::Search,
            "Change a feed binding's select, template, limit, join or target. Only the fields you give \
             change; an empty template or join, or a limit of 0, takes it away. The new value is \
             written at once.",
        ),
    );

    reg.register(
        MethodDef::new(
            "feed.binding.pause",
            Scope::Operate,
            "Stop a binding writing, or start it again with paused false. Started again, it writes \
             what the feed holds now.",
            handler(|call: Call, params: Value| async move {
                let req: PauseRequest = call.params(&params)?;
                body(call.app.feeds.bind_pause(&ctx(&call), &req.id, req.paused).await?)
            }),
        )
        .params(schema_of::<PauseRequest>)
        .result(schema_of::<BindingStatus>)
        .tool(
            "pause_feed_binding",
            Tier::Search,
            "Hold a feed binding ({id, paused: true}) so an operator can type over its target by \
             hand, or let it write again ({id, paused: false}), which writes the feed's current value.",
        ),
    );

    reg.register(
        MethodDef::new(
            "feed.binding.remove",
            Scope::Operate,
            "Forget a binding. What it wrote stays on air.",
            handler(|call: Call, params: Value| async move {
                let req: FeedIdRequest = call.params(&params)?;
                call.app.feeds.bind_remove(&req.id)?;
                Ok(json!({ "removed": req.id }))
            }),
        )
        .params(schema_of::<FeedIdRequest>)
        .result(any_object)
        .tool(
            "remove_feed_binding",
            Tier::Search,
            "Stop a feed binding writing for good. The feed keeps being read for its other bindings, \
             and what this one last wrote stays on screen.",
        ),
    );
}
