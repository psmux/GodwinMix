//! Who is let in at `/guest/`: a guest with their invite's key while it
//! lasts and there is room, or for `talkback` an operator's token.

use super::{now_ms, Guests};
use crate::control::{presented_token, Ctx};
use axum::http::{HeaderMap, Method, StatusCode, Uri};
use godwinmix_protocol::guests::{MAX_GUESTS, TALKBACK};
use godwinmix_protocol::scope::{constant_time_eq, Scope};

/// A refusal: the status to answer with and the sentence to show.
pub type Refused = (StatusCode, String);

pub fn refuse(status: u16, why: impl Into<String>) -> Refused {
    (StatusCode::from_u16(status).unwrap_or(StatusCode::BAD_REQUEST), why.into())
}

pub fn bearer(headers: &HeaderMap) -> String {
    crate::control::rest::bearer(headers).unwrap_or_default()
}

impl Guests {
    /// The invite's name when `key` is its key and it has room; why not
    /// otherwise, as the status to answer with and the sentence to show.
    pub(super) fn admit(&self, id: &str, key: &str) -> Result<String, Refused> {
        let r = self.key_matches(id, key)?;
        if r.expires_ms <= now_ms() {
            return Err(refuse(403, "this invite has run out. Ask whoever invited you for a new link."));
        }
        if self.in_use(id) >= MAX_GUESTS {
            return Err(refuse(503, format!("this show already has its {MAX_GUESTS} guests on. Wait for one to leave, or ask the operator to let someone go.")));
        }
        Ok(r.name)
    }

    pub(super) fn key_matches(&self, id: &str, key: &str) -> Result<super::store::Record, Refused> {
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
pub fn let_in(ctx: &Ctx, id: &str, headers: &HeaderMap) -> Result<String, Refused> {
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

