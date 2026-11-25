//! Guest invites on the wire: a link with a key of its own that lets one
//! person publish their camera into the show and see and hear a return made
//! for them, and talkback from the operator into that return.
//!
//! An invite's key publishes only that guest's stream, to
//! `/guest/<id>/whip`, and plays only that guest's return, from
//! `/guest/<id>/whep`. It is never a channel key and never a control token,
//! so a guest who forwards the link gives away one seat in the show and
//! nothing else, until the invite is revoked or runs out.

use schemars::JsonSchema;
use serde::{Deserialize, Serialize};

/// The most guests one show takes at once.
pub const MAX_GUESTS: usize = 8;
/// How long an invite lasts unless the request says otherwise: one day.
pub const DEFAULT_EXPIRES_SECS: u64 = 24 * 60 * 60;
/// The longest an invite can last: thirty days.
pub const MAX_EXPIRES_SECS: u64 = 30 * 24 * 60 * 60;
/// The id the operator's own talkback microphone publishes as. No invite
/// can have it.
pub const TALKBACK: &str = "talkback";

/// Where a guest is.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize, JsonSchema)]
#[serde(rename_all = "lowercase")]
pub enum GuestState {
    /// The invite exists and nobody has opened it yet.
    Invited,
    /// The guest's browser has offered and the connection is being made.
    Joining,
    /// The guest's picture and sound are arriving.
    Live,
    /// The guest was live and has left, or lost the network. The invite still
    /// works, and the same link brings them back.
    Gone,
}

/// One invite, without its key.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize, JsonSchema)]
pub struct Invite {
    /// A slug, made from the name unless one was given.
    pub id: String,
    /// What the guest is called on screen and in the desk.
    pub name: String,
    /// When it was made, RFC 3339.
    pub created: String,
    /// When it stops letting the guest in, RFC 3339. The guest is let go then.
    pub expires: String,
    pub state: GuestState,
    /// The mixer source the guest's camera is, `guest-<id>`. It exists from
    /// the guest's first offer until the invite goes.
    pub source: String,
    /// The mix minus bus the guest hears, `guest-<id>`: the programme
    /// without the guest, with talkback added while it is held.
    pub bus: String,
    /// Whether talkback is held into this guest's return now.
    pub talkback: bool,
}

/// `guest.invite.create`.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize, JsonSchema)]
pub struct InviteCreateRequest {
    /// The guest's name, as it should appear on screen. 1 to 64 characters.
    pub name: String,
    /// A slug for the invite. Made from the name when left out.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub id: Option<String>,
    /// How long the invite lasts, in seconds. One day when left out; at most
    /// thirty days.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub expires_in_secs: Option<u64>,
    /// The address the link starts with, such as `https://studio.example.com`,
    /// for a mixer reached through a name or a tunnel. Left out, the first
    /// https address of this machine that is not loopback.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub base: Option<String>,
}

/// The link to send a guest, and the same as a QR code.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize, JsonSchema)]
pub struct InviteLink {
    pub id: String,
    /// The `/join/` page with the guest's id and key in the fragment, which a
    /// browser never sends to a server.
    pub link: String,
    /// The link as a QR code, an SVG document a page can show as it is.
    pub qr_svg: String,
    /// Set when no guest on another device could open the link as it stands,
    /// with what to change.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub note: Option<String>,
}

/// What `guest.invite.create` answers.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize, JsonSchema)]
pub struct InviteCreated {
    pub invite: Invite,
    #[serde(flatten)]
    pub link: InviteLink,
}

/// `guest.invite.revoke` and `guest.invite.link`.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize, JsonSchema)]
pub struct InviteIdRequest {
    pub id: String,
}

/// What `guest.invite.revoke` answers.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize, JsonSchema)]
pub struct InviteRevoked {
    pub revoked: Invite,
}

/// What `guest.invite.list` answers, and the payload of `event/guest.changed`.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize, JsonSchema)]
pub struct GuestList {
    pub guests: Vec<Invite>,
    pub talkback: TalkbackState,
    /// The most guests this show takes at once.
    pub max_guests: usize,
}

/// Talkback as it stands.
#[derive(Debug, Clone, PartialEq, Default, Serialize, Deserialize, JsonSchema)]
pub struct TalkbackState {
    /// The source that talks: `guest-talkback` (the operator's browser
    /// microphone, published to `/guest/talkback/whip`) unless another input
    /// was chosen. It is kept muted on the programme.
    pub source: String,
    /// Whether that source is in the mixer now.
    pub present: bool,
    /// The guests it is held into now.
    pub held: Vec<String>,
}

/// `guest.talkback.set`.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize, JsonSchema)]
pub struct TalkbackRequest {
    /// Hold talkback (true) or let it go (false).
    pub on: bool,
    /// The guest to talk to. Left out, every guest that is live or joining.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub guest: Option<String>,
    /// Another input to talk with, such as a microphone on the mixer. It is
    /// muted on the programme from then on, for as long as it is the
    /// talkback source. Left out, the source stays what it was.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub source: Option<String>,
}

/// The guest events.
pub fn events() -> Vec<crate::protocol::EventDef> {
    vec![crate::protocol::EventDef {
        name: "guest.changed",
        since: "1",
        summary: "An invite was made, revoked or ran out, a guest joined, went live or left, \
                  or talkback was held or let go. Carries the whole list, as guest.invite.list \
                  answers it, without any key.",
        ext: None,
        legacy: None,
        payload: crate::method::schema_of::<GuestList>,
    }]
}
