//! `source.page.input` and `source.page.navigate`: using a web page source
//! from the desk, the way one uses a page in a browser.
//!
//! The events are checked here and turned into the renderer's control lines;
//! the mixer hands them on without waiting for the page. See
//! `godwinmix_protocol::page` for the shapes and `godwinmix_core::mixer::page`
//! for the path down.

use super::super::{body, handler};
use super::find;
use crate::control::call::Call;
use godwinmix_core::mixer::PageCall;
use godwinmix_protocol::error::RpcError;
use godwinmix_protocol::method::{schema_of, MethodDef, Registry, Tier};
use godwinmix_protocol::page::*;
use godwinmix_protocol::scope::Scope;
use serde_json::Value;

/// The only kind that takes events.
const PAGE_KIND: &str = "browser/source";

pub fn register(reg: &mut Registry<Call>) {
    reg.register(
        MethodDef::new(
            "source.page.input",
            Scope::Operate,
            "Click, scroll and type into a web page source, as a person would in a browser. \
             Points are shares of the page from 0.0 to 1.0. Nothing waits on the page: events \
             the renderer cannot keep up with are dropped and counted.",
            handler(input),
        )
        .params(schema_of::<PageInputRequest>)
        .result(schema_of::<PageInputResult>)
        .not_idempotent()
        .tool(
            "use_web_page",
            Tier::Search,
            "Click, scroll or type into a web page source the mixer is rendering, for a page \
             that needs a person: a play button, a cookie banner, a search box. Each event \
             names a point as x and y from 0.0 to 1.0 across and down the page. Types: move, \
             down and up (button left, middle or right; count 2 for a double click), wheel \
             (dx, dy in pixels, positive dy scrolls down), key (code is the browser keyCode, \
             13 Enter, 32 Space, 37 to 40 the arrows; key the character), text (typed into \
             the focused box). Look at the source's picture before and after.",
        ),
    );

    reg.register(
        MethodDef::new(
            "source.page.navigate",
            Scope::Operate,
            "Take a web page source to another address, or back, forward or reload. A new \
             address is kept as the source's own, so a saved project opens it there.",
            handler(navigate),
        )
        .params(schema_of::<PageNavigateRequest>)
        .result(schema_of::<PageNavigateResult>)
        .not_idempotent()
        .tool(
            "navigate_web_page",
            Tier::Search,
            "Send a web page source to another page without removing it: action go with a \
             url (kept as the source's address), or back, forward, reload. The picture stays \
             on air while the new page loads.",
        ),
    );
}

/// The source, refused unless it is a page the renderer draws.
async fn page_source(call: &Call, id: &str) -> Result<godwinmix_protocol::types::SourceStatus, RpcError> {
    let source = find(call, id).await?;
    let kind = source.extra("type").and_then(Value::as_str).unwrap_or_default().to_string();
    if kind != PAGE_KIND || source.extra("page").is_none() {
        let what = if kind.is_empty() { "not a web page" } else { kind.as_str() };
        return Err(RpcError::invalid_params(format!(
            "'{id}' is {what}, and only a web page drawn by the browser renderer takes clicks, \
             keys and addresses. Add the page with source.add and kind web, or pick another source"
        ))
        .with("id", id)
        .with("type", kind));
    }
    Ok(source)
}

async fn input(call: Call, params: Value) -> Result<Value, RpcError> {
    let req: PageInputRequest = call.params(&params)?;
    if req.events.len() > MAX_EVENTS {
        return Err(RpcError::invalid_params(format!(
            "one call carries at most {MAX_EVENTS} events and this one has {}; send the rest in another",
            req.events.len()
        )));
    }
    for (i, e) in req.events.iter().enumerate() {
        e.check().map_err(|m| RpcError::invalid_params(format!("event {i}: {m}")).with("event", i))?;
    }
    page_source(&call, &req.id).await?;
    let lines = req.events.iter().map(PageEvent::line).collect();
    let answer = call.app.mixer.page(req.id.clone(), PageCall::Lines(lines)).await.map_err(|e| call.mixer_error(e))?;
    let count = |k: &str| answer.get(k).and_then(Value::as_u64).unwrap_or(0) as usize;
    body(PageInputResult { id: req.id, sent: count("sent"), dropped: count("dropped") })
}

async fn navigate(call: Call, params: Value) -> Result<Value, RpcError> {
    let req: PageNavigateRequest = call.params(&params)?;
    let source = page_source(&call, &req.id).await?;
    let now = source.extra("page").and_then(|p| p.get("url")).and_then(Value::as_str).unwrap_or_default().to_string();
    let to = match req.action {
        PageAction::Go => {
            let typed = req.url.as_deref().unwrap_or_default();
            let url = typed_url(typed).map_err(|m| RpcError::invalid_params(m).with("url", typed))?;
            if call.dry_run {
                return Ok(call.dry_run_answer(true, vec![format!("take {} to {url}", req.id)]));
            }
            call.app.mixer.page(req.id.clone(), PageCall::Go(url.clone())).await.map_err(|e| call.mixer_error(e))?;
            url
        }
        action => {
            if call.dry_run {
                return Ok(call.dry_run_answer(true, vec![format!("{action:?} on {}", req.id).to_lowercase()]));
            }
            let line = navigate_line(action, None);
            call.app.mixer.page(req.id.clone(), PageCall::Lines(vec![line])).await.map_err(|e| call.mixer_error(e))?;
            now
        }
    };
    body(PageNavigateResult { id: req.id, action: req.action, url: to })
}
