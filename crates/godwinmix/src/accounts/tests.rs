//! The account store, the session as a token, the scope it carries, ending
//! sessions, the rate limit and the cookie's forgery checks.

use std::sync::Arc;

use axum::http::{HeaderMap, HeaderValue};
use godwinmix_protocol::accounts::{AccountRegistry, Role, UserAddRequest, UserUpdateRequest};
use godwinmix_protocol::scope::{AuthFailure, ConfirmPolicy, Scope, Token, Tokens};

use super::*;

fn scratch(name: &str) -> PathBuf {
    let dir = std::env::temp_dir().join(format!("gmx-accounts-{name}-{}", std::process::id()));
    let _ = std::fs::remove_dir_all(&dir);
    std::fs::create_dir_all(&dir).unwrap();
    dir.join("godwinmix.users.toml")
}

fn add(a: &Accounts, name: &str, role: Role) {
    let req = UserAddRequest { name: name.into(), role, password: format!("{name}-password"), confirm: ConfirmPolicy::None };
    a.add(req, &|_| false).unwrap();
}

fn tokens_with(a: Arc<Accounts>) -> Tokens {
    Tokens::new(vec![Token::legacy("desk-secret")], false).with_accounts(a)
}

#[test]
fn the_file_keeps_an_argon2id_hash_and_no_password() {
    let path = scratch("hash");
    let a = Accounts::open(Some(path.clone()));
    add(&a, "sam", Role::Admin);
    let text = std::fs::read_to_string(&path).unwrap();
    assert!(text.contains("$argon2id$"), "an argon2id PHC string: {text}");
    assert!(!text.contains("sam-password"), "never the password itself");
    let again = Accounts::open(Some(path));
    assert_eq!(again.list()[0].name, "sam", "read back by a second process");
}

#[test]
fn a_session_authenticates_as_a_token_with_the_role_as_its_scope() {
    let a = Arc::new(Accounts::open(None));
    add(&a, "boss", Role::Admin);
    add(&a, "viewer", Role::View);
    let tokens = tokens_with(a.clone());
    let made = a.sign_in("Viewer", "viewer-password", "192.0.2.5", "test").unwrap();
    let who = tokens.authenticate(Some(&made.secret)).expect("the session is a credential");
    assert_eq!(who.id, "viewer", "the history names the person");
    assert!(who.has(Scope::Read));
    assert!(!who.has(Scope::Operate), "a viewer cannot take a shot");
    let take = crate::control::methods::registry().get("program.take").unwrap().scope;
    assert!(!who.has(take), "program.take needs {take:?}");
    assert!(tokens.authenticate(Some("desk-secret")).is_ok(), "the configured token still works");
}

#[test]
fn a_wrong_password_and_an_unknown_name_get_the_same_answer() {
    let a = Accounts::open(None);
    add(&a, "sam", Role::Admin);
    let wrong = a.sign_in("sam", "not-it-at-all", "192.0.2.6", "").err().unwrap();
    let nobody = a.sign_in("nobody", "not-it-at-all", "192.0.2.6", "").err().unwrap();
    assert_eq!(wrong, Refused::NoMatch);
    assert_eq!(wrong.message(), nobody.message(), "no enumeration in the words");
}

#[test]
fn too_many_wrong_passwords_for_one_name_are_refused_even_with_the_right_one() {
    let a = Accounts::open(None);
    add(&a, "sam", Role::Admin);
    for i in 0..limit::PER_USER {
        let from = format!("192.0.2.{}", 10 + i);
        assert_eq!(a.sign_in("sam", "wrong-password", &from, "").err(), Some(Refused::NoMatch));
    }
    let held = a.sign_in("sam", "sam-password", "192.0.2.99", "").err().unwrap();
    assert!(matches!(held, Refused::Wait(_)), "spread over addresses, still held: {held:?}");
    assert!(held.message().contains("Wait"), "{}", held.message());
}

#[test]
fn too_many_wrong_tries_from_one_address_are_refused() {
    let a = Accounts::open(None);
    add(&a, "sam", Role::Admin);
    for i in 0..limit::PER_ADDRESS {
        let _ = a.sign_in(&format!("guess{i}"), "wrong-password", "192.0.2.50", "");
    }
    assert!(matches!(a.sign_in("sam", "sam-password", "192.0.2.50", ""), Err(Refused::Wait(_))));
    assert!(a.sign_in("sam", "sam-password", "192.0.2.51", "").is_ok(), "another address is not held");
}

#[test]
fn disabling_removing_or_demoting_ends_the_sessions_at_once() {
    let a = Arc::new(Accounts::open(None));
    add(&a, "boss", Role::Admin);
    add(&a, "op", Role::Operate);
    let tokens = tokens_with(a.clone());
    let s = a.sign_in("op", "op-password", "192.0.2.7", "").unwrap();
    let who = tokens.authenticate(Some(&s.secret)).unwrap();
    assert!(!tokens.revoked(&who));

    a.update(UserUpdateRequest { name: "op".into(), role: Some(Role::View), disabled: None, confirm: None }).unwrap();
    assert!(tokens.revoked(&who), "an open connection does not keep operate after a demotion");
    let now = tokens.authenticate(Some(&s.secret)).unwrap();
    assert!(!now.has(Scope::Operate) && !tokens.revoked(&now), "a new connection gets the new role");

    let changed = a.update(UserUpdateRequest { name: "op".into(), role: None, disabled: Some(true), confirm: None }).unwrap();
    assert_eq!(changed.sessions_ended, 1);
    assert!(tokens.revoked(&now));
    assert_eq!(tokens.authenticate(Some(&s.secret)), Err(AuthFailure::Wrong));
    assert_eq!(a.sign_in("op", "op-password", "192.0.2.7", "").err(), Some(Refused::NoMatch), "disabled cannot sign in");

    a.update(UserUpdateRequest { name: "op".into(), role: None, disabled: Some(false), confirm: None }).unwrap();
    let again = a.sign_in("op", "op-password", "192.0.2.7", "").unwrap();
    let who = tokens.authenticate(Some(&again.secret)).unwrap();
    assert_eq!(a.remove("op").unwrap().sessions_ended, 1);
    assert!(tokens.revoked(&who), "removed: gone at once");
}

#[test]
fn sign_out_everywhere_ends_every_session_of_one_person() {
    let a = Arc::new(Accounts::open(None));
    add(&a, "boss", Role::Admin);
    let tokens = tokens_with(a.clone());
    let one = a.sign_in("boss", "boss-password", "192.0.2.8", "phone").unwrap();
    let two = a.sign_in("boss", "boss-password", "192.0.2.9", "laptop").unwrap();
    assert_eq!(a.sessions(Some("boss"), None).len(), 2);
    assert_eq!(a.sign_out(Some("boss"), None).unwrap(), 2);
    assert!(tokens.authenticate(Some(&one.secret)).is_err());
    assert!(tokens.authenticate(Some(&two.secret)).is_err());
}

#[test]
fn changing_your_own_password_keeps_this_session_and_ends_the_others() {
    let a = Arc::new(Accounts::open(None));
    add(&a, "boss", Role::Admin);
    let tokens = tokens_with(a.clone());
    let here = a.sign_in("boss", "boss-password", "192.0.2.8", "").unwrap();
    let there = a.sign_in("boss", "boss-password", "192.0.2.9", "").unwrap();
    let me = tokens.authenticate(Some(&here.secret)).unwrap();
    assert!(a.change_own(&me, "wrong-current", "a-new-password").is_err());
    assert_eq!(a.change_own(&me, "boss-password", "a-new-password").unwrap(), 1);
    assert!(tokens.authenticate(Some(&here.secret)).is_ok());
    assert!(tokens.authenticate(Some(&there.secret)).is_err());
    assert!(a.sign_in("boss", "a-new-password", "192.0.2.8", "").is_ok());
    let token = Token::legacy("desk-secret");
    assert!(a.change_own(&token, "x", "a-new-password").is_err(), "a token has no password");
}

#[test]
fn an_open_port_closes_when_the_first_account_is_made() {
    let a = Arc::new(Accounts::open(None));
    let tokens = Tokens::new(vec![], false).with_accounts(a.clone());
    assert!(tokens.is_open(), "no accounts: nothing changes");
    assert_eq!(tokens.authenticate(None).unwrap().id, "open");
    add(&a, "boss", Role::Admin);
    assert!(!tokens.is_open());
    assert_eq!(tokens.authenticate(None), Err(AuthFailure::Missing));
}

#[test]
fn the_last_admin_cannot_be_disabled_demoted_or_removed_while_others_remain() {
    let a = Accounts::open(None);
    add(&a, "boss", Role::Admin);
    add(&a, "op", Role::Operate);
    assert!(a.update(UserUpdateRequest { name: "boss".into(), role: None, disabled: Some(true), confirm: None }).is_err());
    assert!(a.update(UserUpdateRequest { name: "boss".into(), role: Some(Role::View), disabled: None, confirm: None }).is_err());
    assert!(a.remove("boss").is_err());
    a.remove("op").unwrap();
    assert!(a.remove("boss").is_ok(), "alone, the last account may go");
}

#[test]
fn names_and_passwords_are_checked() {
    let a = Accounts::open(None);
    let bad = |name: &str, pw: &str| a.add(UserAddRequest { name: name.into(), role: Role::Admin, password: pw.into(), confirm: ConfirmPolicy::None }, &|id| id == "default").is_err();
    assert!(bad("sam", "short"));
    assert!(bad("has space", "long-enough"));
    assert!(bad("default", "long-enough"), "a token's id is taken");
    assert!(bad("sam", " padded-password "));
    assert!(!bad("Sam", "long-enough"));
    assert_eq!(a.list()[0].name, "sam", "kept lower case");
    assert!(bad("sam", "long-enough"), "no two accounts share a name");
}

#[test]
fn a_change_made_by_another_process_reaches_this_one() {
    let path = scratch("shared");
    let core = Arc::new(Accounts::open(Some(path.clone())));
    let cli = Accounts::open(Some(path));
    add(&cli, "boss", Role::Admin);
    let tokens = tokens_with(core.clone());
    let s = core.sign_in("boss", "boss-password", "192.0.2.4", "").expect("the core reads the file the command line wrote");
    let who = tokens.authenticate(Some(&s.secret)).unwrap();
    cli.update(UserUpdateRequest { name: "boss".into(), role: None, disabled: Some(true), confirm: None }).unwrap_err();
    add(&cli, "two", Role::Admin);
    cli.update(UserUpdateRequest { name: "boss".into(), role: None, disabled: Some(true), confirm: None }).unwrap();
    std::thread::sleep(RECHECK + std::time::Duration::from_millis(100));
    assert!(tokens.revoked(&who), "disabled from the command line, out of the running mixer");
}

#[test]
fn the_first_admin_comes_from_the_environment_once() {
    let path = scratch("seed");
    let a = Accounts::open(Some(path.clone()));
    let file = path.with_file_name("admin-password");
    std::fs::write(&file, "seeded-password\n").unwrap();
    assert_eq!(seed::seed(&a, "owner", &file), Ok(true));
    assert!(!file.exists(), "the password file is deleted once read");
    assert!(a.sign_in("owner", "seeded-password", "192.0.2.3", "").is_ok());
    std::fs::write(&file, "another-password").unwrap();
    assert_eq!(seed::seed(&a, "owner", &file), Ok(false), "never again once an account exists");
    assert!(a.sign_in("owner", "seeded-password", "192.0.2.3", "").is_ok(), "the password was not put back");
}

fn headers(pairs: &[(&str, &str)]) -> HeaderMap {
    let mut h = HeaderMap::new();
    for (k, v) in pairs {
        h.append(axum::http::HeaderName::from_bytes(k.as_bytes()).unwrap(), HeaderValue::from_str(v).unwrap());
    }
    h
}

#[test]
fn the_cookie_is_used_only_by_this_mixers_own_page() {
    let mine = [("host", "192.0.2.1:8080"), ("cookie", "other=1; gmx_session_8080=abc")];
    assert_eq!(cookie::session(&headers(&mine)).as_deref(), Some("abc"));
    let same = [mine[0], mine[1], ("origin", "https://192.0.2.1:8080"), ("sec-fetch-site", "same-origin")];
    assert_eq!(cookie::session(&headers(&same)).as_deref(), Some("abc"));
    let elsewhere = [mine[0], mine[1], ("origin", "https://attacker.example.com")];
    assert_eq!(cookie::session(&headers(&elsewhere)), None, "another origin may not use it");
    let cross = [mine[0], mine[1], ("sec-fetch-site", "cross-site")];
    assert_eq!(cookie::session(&headers(&cross)), None);
    let sibling = [mine[0], mine[1], ("sec-fetch-site", "same-site")];
    assert_eq!(cookie::session(&headers(&sibling)), None, "same site is not same origin");
    let other_port = [("host", "192.0.2.1:8081"), mine[1]];
    assert_eq!(cookie::session(&headers(&other_port)), None, "another mixer on this host has its own cookie");
    let both = [mine[0], mine[1], ("authorization", "Bearer desk")];
    assert_eq!(crate::control::rest::bearer(&headers(&both)).as_deref(), Some("desk"), "a bearer token wins");
    let set = cookie::set(&headers(&[("host", "192.0.2.1:8080"), ("origin", "https://192.0.2.1:8080")]), "abc");
    let set = set.to_str().unwrap();
    assert!(set.contains("HttpOnly") && set.contains("SameSite=Strict") && set.contains("Secure"), "{set}");
    let plain = cookie::set(&headers(&[("host", "192.0.2.1:8080")]), "abc");
    assert!(!plain.to_str().unwrap().contains("Secure"), "not Secure on plain HTTP");
}

#[test]
fn a_session_unknown_here_is_found_after_a_reread() {
    let path = scratch("reread");
    let a = Accounts::open(Some(path.clone()));
    add(&a, "boss", Role::Admin);
    let b = Accounts::open(Some(path));
    let s = a.sign_in("boss", "boss-password", "192.0.2.2", "").unwrap();
    assert!(b.find(&s.secret).is_some(), "a station's show finds a session the station made");
    assert!(b.any());
}

/// The three routes against a real listener: a sign in sets the cookie, the
/// cookie is the credential, a page on another origin is refused, sign out
/// ends it.
#[tokio::test(flavor = "multi_thread")]
async fn the_sign_in_routes_over_http() {
    let a = Arc::new(Accounts::open(None));
    add(&a, "boss", Role::Admin);
    let tokens = Arc::new(tokens_with(a.clone()));
    let listener = tokio::net::TcpListener::bind("127.0.0.1:0").await.unwrap();
    let addr = listener.local_addr().unwrap();
    let app = http::router(tokens.clone()).into_make_service_with_connect_info::<std::net::SocketAddr>();
    let server = tokio::spawn(async move { axum::serve(listener, app).await });
    let base = format!("http://{addr}");
    let web = reqwest::Client::new();
    let login = |user: &str, pw: &str, origin: &str| {
        web.post(format!("{base}/api/v1/auth/login")).header("origin", origin).json(&serde_json::json!({ "username": user, "password": pw })).send()
    };

    let status: serde_json::Value = web.get(format!("{base}/api/v1/auth/status")).send().await.unwrap().json().await.unwrap();
    assert_eq!(status["accounts"], true);
    assert_eq!(status["via"], "none");

    assert_eq!(login("boss", "wrong-password", &base).await.unwrap().status(), 401);
    assert_eq!(login("boss", "boss-password", "http://attacker.example.com").await.unwrap().status(), 403, "another origin cannot sign in");
    let ok = login("boss", "boss-password", &base).await.unwrap();
    assert_eq!(ok.status(), 200);
    let set = ok.headers()["set-cookie"].to_str().unwrap().to_string();
    assert!(set.starts_with(&format!("gmx_session_{}=", addr.port())) && set.contains("HttpOnly"), "{set}");
    let cookie = set.split(';').next().unwrap().to_string();

    let me: serde_json::Value = web.get(format!("{base}/api/v1/auth/status")).header("cookie", &cookie).send().await.unwrap().json().await.unwrap();
    assert_eq!((me["user"].as_str(), me["role"].as_str(), me["via"].as_str()), (Some("boss"), Some("admin"), Some("session")));

    let out = web.post(format!("{base}/api/v1/auth/logout")).header("cookie", &cookie).header("origin", &base).send().await.unwrap();
    assert!(out.headers()["set-cookie"].to_str().unwrap().contains("Max-Age=0"));
    let after: serde_json::Value = web.get(format!("{base}/api/v1/auth/status")).header("cookie", &cookie).send().await.unwrap().json().await.unwrap();
    assert_eq!(after["via"], "none", "signed out");

    for _ in 0..limit::PER_USER {
        let _ = login("boss", "wrong-password", &base).await;
    }
    let held = login("boss", "boss-password", &base).await.unwrap();
    assert_eq!(held.status(), 429);
    assert!(held.headers().contains_key("retry-after"));
    server.abort();
}
