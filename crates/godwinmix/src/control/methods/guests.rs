//! `guest.*`: invites, and talkback into a guest's return.
//!
//! The work is in `crate::guests`; these are the rows of the method table.
//! Making and taking back an invite is admin, because an invite is a
//! credential: it lets someone put a camera into the show. Talkback is an
//! operator's.

use super::{body, handler};
use crate::control::call::Call;
use godwinmix_protocol::guests::*;
use godwinmix_protocol::method::{schema_of, MethodDef, Registry, Tier};
use godwinmix_protocol::scope::Scope;
use serde_json::Value;

pub fn register(reg: &mut Registry<Call>) {
    reg.register(
        MethodDef::new(
            "guest.invite.create",
            Scope::Admin,
            "Invite a guest: a link with a key of its own that publishes only this guest's camera \
             and microphone, as the source guest-<id>, and plays only this guest's return, the \
             programme picture with everyone's sound but theirs. Answers the link and a QR code. \
             It lasts a day unless expires_in_secs says otherwise.",
            handler(|call: Call, params: Value| async move {
                let req: InviteCreateRequest = call.params(&params)?;
                body(call.app.guests.create(req).await?)
            }),
        )
        .params(schema_of::<InviteCreateRequest>)
        .result(schema_of::<InviteCreated>)
        .not_idempotent()
        .tool(
            "create_guest_invite",
            Tier::Search,
            "Invite a remote guest into the show: {name: \"Ana Lopez\"}. Answers link, the address to send \
             them, and qr_svg. Opening the link on a phone or laptop puts their camera in as the source \
             guest-<id> and plays them the programme without their own voice. A note says when no other \
             device could open the link as it stands. Put the source in a scene to put them on air.",
        ),
    );
    reg.register(
        MethodDef::new(
            "guest.invite.list",
            Scope::Read,
            "Every invite, without its key: the guest's name, when it runs out, whether they are \
             invited, joining, live or gone, their source and return bus, and talkback.",
            handler(|call: Call, _| async move { body(call.app.guests.list()) }),
        )
        .result(schema_of::<GuestList>)
        .tool(
            "list_guests",
            Tier::Search,
            "The invited guests and where each is: invited (link not opened), joining, live or gone. \
             source is the mixer source to put in a scene; talkback says whether the operator is \
             talking to them now.",
        ),
    );
    reg.register(
        MethodDef::new(
            "guest.invite.link",
            Scope::Admin,
            "An invite's link and QR code again, to copy or show.",
            handler(|call: Call, params: Value| async move {
                let req: InviteIdRequest = call.params(&params)?;
                body(call.app.guests.link(&req.id)?)
            }),
        )
        .params(schema_of::<InviteIdRequest>)
        .result(schema_of::<InviteLink>),
    );
    reg.register(
        MethodDef::new(
            "guest.invite.revoke",
            Scope::Admin,
            "Take an invite back. The guest's stream and return end at once, their source leaves \
             every scene, and the link lets nobody in again.",
            handler(|call: Call, params: Value| async move {
                let req: InviteIdRequest = call.params(&params)?;
                body(call.app.guests.revoke(&req.id).await?)
            }),
        )
        .params(schema_of::<InviteIdRequest>)
        .result(schema_of::<InviteRevoked>)
        .destructive(),
    );
    reg.register(
        MethodDef::new(
            "guest.talkback.set",
            Scope::Operate,
            "Talk to one guest, or every guest that has joined, off air: on adds the talkback \
             source to their return, off takes it out. The talkback source is kept muted on the \
             programme. source picks another input to talk with.",
            handler(|call: Call, params: Value| async move {
                let req: TalkbackRequest = call.params(&params)?;
                body(call.app.guests.set_talkback(req).await?)
            }),
        )
        .params(schema_of::<TalkbackRequest>)
        .result(schema_of::<GuestList>),
    );
}
