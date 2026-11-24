//! `user.*` and `account.*`: optional usernames and passwords. The work is in
//! `crate::accounts`; a station answers the same methods itself.
//!
//! The user methods are admin only and never answer with a password or its
//! hash. `account.get` and `account.change_password` are for whoever is
//! signed in, about themselves.

use super::handler;
use crate::control::call::Call;
use godwinmix_protocol::accounts::{
    AccountInfo, AccountPasswordRequest, SessionList, SignedOut, UserAddRequest, UserChanged, UserList, UserNameRequest,
    UserPasswordRequest, UserSessionsRequest, UserSignOutRequest, UserUpdateRequest,
};
use godwinmix_protocol::method::{no_params, schema_of, MethodDef, Registry, Tier};
use godwinmix_protocol::scope::Scope;
use serde_json::Value;

async fn run(call: Call, params: Value) -> Result<Value, godwinmix_protocol::error::RpcError> {
    crate::accounts::calls::call(&call.app.tokens, &call.token, call.method, params).await
}

pub fn register(reg: &mut Registry<Call>) {
    reg.register(
        MethodDef::new("user.list", Scope::Admin, "Every account: its name, role, whether it is disabled, and how many sessions it has signed in. Never a password.", handler(run))
            .params(no_params)
            .result(schema_of::<UserList>)
            .mutating(false)
            .tool("user_list", Tier::Search, "List the people who can sign in to this mixer with a username and password: name, role (view, operate or admin), disabled, and how many browsers each has signed in. Admin only. Never shows a password."),
    );
    reg.register(
        MethodDef::new("user.add", Scope::Admin, "Make an account with a username, a role (view, operate or admin) and a password of at least 8 characters. The first account must be an admin, and once one exists the page asks everybody for a username and password instead of a token.", handler(run))
            .params(schema_of::<UserAddRequest>)
            .result(schema_of::<UserChanged>)
            .not_idempotent()
            .tool("user_add", Tier::Search, "Add a person who signs in with a username and password. role is view (watches), operate (runs the show) or admin (everything, including users). The first account must be admin, and adding it makes the page ask everybody to sign in. Only do this when a person asked for it, and tell them the password went through you."),
    );
    reg.register(
        MethodDef::new("user.update", Scope::Admin, "Change an account's role, confirm policy, or whether it is disabled. Disabling ends every session it has at once. The last admin cannot be disabled or given a lower role.", handler(run))
            .params(schema_of::<UserUpdateRequest>)
            .result(schema_of::<UserChanged>)
            .tool("user_update", Tier::Search, "Change a person's role (view, operate, admin) or disable or enable their account. Disabling signs them out everywhere at once. The last admin cannot be disabled or demoted."),
    );
    reg.register(
        MethodDef::new("user.reset_password", Scope::Admin, "Give an account a new password, for somebody who forgot theirs. Every session it has ends, so whoever knew the old password is signed out.", handler(run))
            .params(schema_of::<UserPasswordRequest>)
            .result(schema_of::<UserChanged>)
            .tool("user_reset_password", Tier::Search, "Set a new password for a person who forgot theirs. Signs that person out everywhere. Only do this when they or an admin asked, and hand the new password to them, not to anybody else."),
    );
    reg.register(
        MethodDef::new("user.remove", Scope::Admin, "Remove an account and end every session it has at once. The last admin cannot be removed while other accounts remain.", handler(run))
            .params(schema_of::<UserNameRequest>)
            .result(schema_of::<UserChanged>)
            .destructive()
            .tool("user_remove", Tier::Search, "Remove a person's account; they are signed out everywhere at once and cannot sign in again. The last admin cannot be removed while others remain. Confirm with the person who asked first."),
    );
    reg.register(
        MethodDef::new("user.sessions", Scope::Admin, "The signed in browsers, everybody's or one account's: when each signed in, from which address and browser, when it expires and when it was last seen.", handler(run))
            .params(schema_of::<UserSessionsRequest>)
            .result(schema_of::<SessionList>)
            .mutating(false)
            .tool("user_sessions", Tier::Search, "List where people are signed in: one session per browser or phone, with its address, device, when it signed in and when it was last seen. Pass name for one person's. Use the id with user_sign_out to end one."),
    );
    reg.register(
        MethodDef::new("user.sign_out", Scope::Admin, "End sessions: one by its id, every session of one account by its name, or with neither every session of everybody. Each browser it ends is asked to sign in again on its next call.", handler(run))
            .params(schema_of::<UserSignOutRequest>)
            .result(schema_of::<SignedOut>)
            .tool("user_sign_out", Tier::Search, "Sign people out: session for one browser, name for everywhere one person is signed in, neither for everybody. They can sign in again with their password."),
    );
    reg.register(
        MethodDef::new("account.get", Scope::Read, "Who this call is signed in as: the account and its role for a sign in, or that it is a token. Also whether this mixer has accounts at all.", handler(run))
            .params(no_params)
            .result(schema_of::<AccountInfo>)
            .mutating(false),
    );
    reg.register(
        MethodDef::new("account.change_password", Scope::Read, "Change the signed in person's own password, given the current one. Every other session of theirs ends; this one stays signed in. A token has no password and is refused.", handler(run))
            .params(schema_of::<AccountPasswordRequest>)
            .result(schema_of::<SignedOut>),
    );
}
