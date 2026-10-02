//! `feed.*`: live data feeds and what they are bound to.
//!
//! The work is in `crate::feeds`; these are the rows of the method table and
//! the few lines that turn a call into a request and an answer.

use super::{body, handler};
use crate::control::call::Call;
use crate::feeds::Ctx;
use godwinmix_protocol::feeds::*;
use godwinmix_protocol::method::{any_object, schema_of, MethodDef, Registry, Tier};
use godwinmix_protocol::scope::Scope;
use serde_json::{json, Value};

mod bindings;

pub fn register(reg: &mut Registry<Call>) {
    register_reads(reg);
    register_edits(reg);
    bindings::register(reg);
}

pub(super) fn ctx(call: &Call) -> Ctx {
    Ctx { app: call.app.clone(), snapshots: call.snapshots.clone() }
}

fn register_reads(reg: &mut Registry<Call>) {
    reg.register(
        MethodDef::new(
            "feed.list",
            Scope::Read,
            "Every live data feed with its state (ok, failing with the reason, paused), when it was \
             last read and last changed, and every binding with the value it last wrote.",
            handler(|call: Call, _| async move { body(call.app.feeds.list()) }),
        )
        .result(schema_of::<FeedList>)
        .tool(
            "list_feeds",
            Tier::Search,
            "The live data feeds (RSS, JSON, CSV, websocket, event stream) this show reads, each with \
             state ok, failing (with last_error) or paused, and the bindings that write a value from a \
             feed into a source's params, a graphic's field or a scene parameter, each with the value \
             it last wrote. Header values come back as \"__secret__\".",
        ),
    );

    reg.register(
        MethodDef::new(
            "feed.test",
            Scope::Operate,
            "Fetch a feed once and show what came back: its top keys, a cut down copy, and every \
             path with an example. With `select` (and `template`, `limit`, `join`) it also shows what \
             that picks and what a binding would write. A path that picks nothing is refused with \
             where it stopped and the keys there. Nothing is stored and nothing is written.",
            handler(|call: Call, params: Value| async move {
                let req: FeedTestRequest = call.params(&params)?;
                body(crate::feeds::test(&call.app.feeds, req).await?)
            }),
        )
        .params(schema_of::<FeedTestRequest>)
        .result(schema_of::<FeedTestResult>)
        .tool(
            "test_feed",
            Tier::Search,
            "Fetch a live data feed once and see what it holds, before binding it. Give `address` \
             (https, http, wss or ws) for a new feed or `id` for one that exists. The answer's `paths` \
             lists every path with an example value: pick one and call again with `select` set to it \
             (`items[].title` is every RSS headline, `rows[0].Name` a sheet's cell, `data.home.score` \
             a JSON value), plus `template` like \"{home} {home_score} : {away_score} {away}\" to \
             combine fields and `limit` to keep the first N. `value` is exactly what a binding would \
             write. A wrong path is refused with the keys that are there.",
        ),
    );
}

fn register_edits(reg: &mut Registry<Call>) {
    reg.register(
        MethodDef::new(
            "feed.add",
            Scope::Operate,
            "Add a live data feed: an http(s) address polled every `interval_s` (5 s at least, 30 by \
             default, honouring ETag and Last-Modified), a ws(s) address whose messages are read as \
             they come, or an http(s) event stream with format sse. Header values are sealed.",
            handler(|call: Call, params: Value| async move {
                let req: FeedAddRequest = call.params(&params)?;
                body(call.app.feeds.add(&ctx(&call), req.into())?)
            }),
        )
        .params(schema_of::<FeedAddRequest>)
        .result(schema_of::<FeedStatus>)
        .not_idempotent()
        .tool(
            "add_feed",
            Tier::Search,
            "Start reading a live data feed: {id: \"news\", address: \"https://example.com/rss\"}. RSS \
             and Atom, JSON and CSV (a published Google Sheet) are polled every interval_s seconds (at \
             least 5, 30 by default) and an unchanged feed costs one 304; a wss:// address is a \
             websocket; format \"sse\" reads an event stream. Put an API key in headers, never in \
             the address: header values are sealed and never shown again. Call test_feed first to \
             see what it holds, then bind_feed to put a value on screen.",
        ),
    );

    reg.register(
        MethodDef::new(
            "feed.set",
            Scope::Operate,
            "Change a feed's address, format, interval, timeout or headers. Only what is named \
             changes; a header value of \"__secret__\" keeps the stored one.",
            handler(|call: Call, params: Value| async move {
                let req: FeedSetRequest = call.params(&params)?;
                body(call.app.feeds.set(&ctx(&call), req)?)
            }),
        )
        .params(schema_of::<FeedSetRequest>)
        .result(schema_of::<FeedStatus>)
        .tool(
            "set_feed",
            Tier::Search,
            "Change a live data feed: its address, format, interval_s, timeout_s or headers. Only the \
             fields you give change. Headers are replaced as a whole; send \"__secret__\" as a value \
             to keep the one already stored under that name.",
        ),
    );

    reg.register(
        MethodDef::new(
            "feed.pause",
            Scope::Operate,
            "Stop reading a feed, or start it again with paused false. While paused nothing is \
             fetched and nothing is written; what was written stays on air.",
            handler(|call: Call, params: Value| async move {
                let req: PauseRequest = call.params(&params)?;
                body(call.app.feeds.pause(&ctx(&call), &req.id, req.paused)?)
            }),
        )
        .params(schema_of::<PauseRequest>)
        .result(schema_of::<FeedStatus>)
        .tool(
            "pause_feed",
            Tier::Search,
            "Stop a live data feed being fetched ({id, paused: true}) or start it again ({id, paused: \
             false}). What it last wrote stays on screen while it is paused.",
        ),
    );

    reg.register(
        MethodDef::new(
            "feed.refresh",
            Scope::Operate,
            "Fetch a polled feed now rather than at the end of its interval. A pushed feed that is \
             waiting to reconnect tries at once.",
            handler(|call: Call, params: Value| async move {
                let req: FeedIdRequest = call.params(&params)?;
                body(call.app.feeds.refresh(&req.id)?)
            }),
        )
        .params(schema_of::<FeedIdRequest>)
        .result(schema_of::<FeedStatus>)
        .not_idempotent()
        .tool(
            "refresh_feed",
            Tier::Search,
            "Fetch a live data feed now instead of waiting for its interval, for when you know the \
             source has just changed. Its bindings write only what changed.",
        ),
    );

    reg.register(
        MethodDef::new(
            "feed.remove",
            Scope::Operate,
            "Stop and forget a feed, its sealed headers and every binding that reads it. What they \
             wrote stays on air until something else changes it.",
            handler(|call: Call, params: Value| async move {
                let req: FeedIdRequest = call.params(&params)?;
                let bindings = call.app.feeds.remove(&req.id)?;
                Ok(json!({ "removed": req.id, "bindings": bindings }))
            }),
        )
        .params(schema_of::<FeedIdRequest>)
        .result(any_object)
        .tool(
            "remove_feed",
            Tier::Search,
            "Remove a live data feed and every binding that reads it. The words they last wrote stay \
             on screen; change or remove those sources separately if they should go too.",
        ),
    );
}
