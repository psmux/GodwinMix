//! `/guest/<id>/whip` and `/guest/<id>/whep`: a guest publishing and
//! watching, on the control port, let in by the invite's key.
//!
//! | Route | Answer |
//! |---|---|
//! | `POST /guest/{id}/whip` | `201`, the SDP answer and the session's `Location` |
//! | `POST /guest/{id}/whep` | `201`, the return's answer and its `Location` |
//! | `DELETE /guest/{id}/{whip or whep}/{session}` | `200`, the session ends |
//! | `PATCH` on a session | `405`: every candidate is in the answer, no trickle |
//!
//! The key is the bearer token. `talkback` is the one id with no invite: it
//! is the operator's microphone, let in by a control token with the operate
//! scope. Outside every token check, like `/whip/`. Under a station the
//! station relays `/guest/` to the show `?show=` names, so a `Location` here
//! carries `?show=` back.

use super::{now_ms, return_of, Guests};
use crate::control::{presented_token, Ctx};
use axum::extract::{Path, State};
use axum::http::{header, HeaderMap, Method, StatusCode, Uri};
use axum::response::{IntoResponse, Response};
use axum::routing::{delete, post};
use axum::Router;
use godwinmix_protocol::guests::{MAX_GUESTS, TALKBACK};
use godwinmix_protocol::scope::{constant_time_eq, Scope};

pub fn router(ctx: Ctx) -> Router<Ctx> {
    Router::new()
        .route("/guest/{id}/whip", post(publish))
        .route("/guest/{id}/whep", post(watch))
        .route("/guest/{id}/{kind}/{session}", delete(end).patch(no_trickle))
        .with_state(ctx)
}

type Refused = (StatusCode, String);

fn refuse(status: u16, why: impl Into<String>) -> Refused {
    (StatusCode::from_u16(status).unwrap_or(StatusCode::BAD_REQUEST), why.into())
}

fn bearer(headers: &HeaderMap) -> String {
    crate::control::rest::bearer(headers).unwrap_or_default()
}

impl Guests {
    /// The invite's name when `key` is its key and it has room; why not
    /// otherwise, as the status to answer with and the sentence to show.
    fn admit(&self, id: &str, key: &str) -> Result<String, Refused> {
        let r = self.key_matches(id, key)?;
        if r.expires_ms <= now_ms() {
            return Err(refuse(403, "this invite has run out. Ask whoever invited you for a new link."));
        }
        if self.in_use(id) >= MAX_GUESTS {
            return Err(refuse(503, format!("this show already has its {MAX_GUESTS} guests on. Wait for one to leave, or ask the operator to let someone go.")));
        }
        Ok(r.name)
    }

    fn key_matches(&self, id: &str, key: &str) -> Result<super::store::Record, Refused> {
        let Some(r) = self.invites.lock().get(id).cloned() else {
            return Err(refuse(404, "there is no invite by that name on this mixer. It may have been revoked; ask whoever invited you for a new link."));
        };
        if key.is_empty() || !constant_time_eq(key.as_bytes(), r.key.as_bytes()) {
            return Err(refuse(403, "that is not this invite's key. Open the whole link you were sent, or ask for a new one."));
        }
        Ok(r)
    }
}

/// Who may come in as `id`: a guest with its invite's key, or for
/// `talkback` an operator with an operate token.
fn let_in(ctx: &Ctx, id: &str, headers: &HeaderMap) -> Result<String, Refused> {
    if id != TALKBACK {
        return ctx.app.guests.admit(id, &bearer(headers));
    }
    let uri: Uri = "/".parse().expect("a static uri");
    let token = ctx.app.tokens.authenticate(presented_token(&Method::POST, headers, &uri).as_deref());
    match token {
        Ok(t) if t.has(Scope::Operate) => Ok("Talkback".to_string()),
        Ok(_) => Err(refuse(403, "talkback needs a token with the operate scope.")),
        Err(reason) => Err(refuse(401, reason.message())),
    }
}

fn sdp_or_refuse(headers: &HeaderMap) -> Result<(), Refused> {
    let kind = headers.get(header::CONTENT_TYPE).and_then(|v| v.to_str().ok()).unwrap_or("");
    if kind.starts_with("application/sdp") {
        return Ok(());
    }
    Err(refuse(415, "an offer is the SDP itself, sent with Content-Type: application/sdp."))
}

fn created(id: &str, kind: &str, session: &str, sdp: String) -> Response {
    let show = crate::station::show::mode().map(|m| format!("?show={}", m.id)).unwrap_or_default();
    let location = format!("/guest/{id}/{kind}/{session}{show}");
    (StatusCode::CREATED, [(header::CONTENT_TYPE, "application/sdp".to_string()), (header::LOCATION, location)], sdp).into_response()
}

async fn publish(State(ctx): State<Ctx>, Path(id): Path<String>, headers: HeaderMap, body: String) -> Response {
    let name = match sdp_or_refuse(&headers).and_then(|_| let_in(&ctx, &id, &headers)) {
        Ok(name) => name,
        Err(r) => return r.into_response(),
    };
    let guests = ctx.app.guests.clone();
    if let Err(why) = guests.ensure(&id, &name).await {
        return refuse(503, format!("the mixer could not make room for this guest yet ({why}). The page tries again by itself.")).into_response();
    }
    guests.arm();
    let (who, shape) = (id.clone(), guests.shape);
    let ice = godwinmix_core::preview::whep::IceConfig::default();
    match tokio::task::spawn_blocking(move || godwinmix_core::guest::offer(&who, &body, shape, &ice)).await {
        Ok(Ok((session, answer))) => created(&id, "whip", &session, answer),
        Ok(Err(r)) => refuse(r.status, r.message).into_response(),
        Err(e) => refuse(500, format!("the offer was not handled: {e}")).into_response(),
    }
}

async fn watch(State(ctx): State<Ctx>, Path(id): Path<String>, headers: HeaderMap, body: String) -> Response {
    if id == TALKBACK {
        return refuse(404, "talkback has no return to watch.").into_response();
    }
    if let Err(r) = sdp_or_refuse(&headers).and_then(|_| ctx.app.guests.admit(&id, &bearer(&headers))) {
        return r.into_response();
    }
    let output = return_of(&id);
    match tokio::task::spawn_blocking(move || godwinmix_core::whep::offer(&output, &body)).await {
        Ok(Ok((_, session, answer))) => created(&id, "whep", &session, answer),
        Ok(Err(r)) if r.status == 404 => {
            refuse(503, "your return is not ready yet: it starts once your camera is in. The page tries again by itself.").into_response()
        }
        Ok(Err(r)) => refuse(r.status, r.message).into_response(),
        Err(e) => refuse(500, format!("the offer was not handled: {e}")).into_response(),
    }
}

/// A whip session's name is random, so knowing it is what lets a page end
/// its own. A return's session is numbered, so ending one takes the key.
async fn end(State(ctx): State<Ctx>, Path((id, kind, session)): Path<(String, String, String)>, headers: HeaderMap) -> Response {
    let ended = match kind.as_str() {
        "whip" => tokio::task::spawn_blocking(move || godwinmix_core::guest::end(&id, &session)).await.unwrap_or(false),
        "whep" => {
            if let Err(r) = ctx.app.guests.key_matches(&id, &bearer(&headers)) {
                return r.into_response();
            }
            tokio::task::spawn_blocking(move || godwinmix_core::whep::end(&return_of(&id), &session)).await.unwrap_or(false)
        }
        _ => false,
    };
    match ended {
        true => StatusCode::OK.into_response(),
        false => refuse(404, "no session by that name; it may have ended already.").into_response(),
    }
}

async fn no_trickle() -> Response {
    let why = "every candidate is in the answer and none are taken later. After a lost network, POST a new offer.";
    (StatusCode::METHOD_NOT_ALLOWED, [(header::ALLOW, "DELETE")], why).into_response()
}
