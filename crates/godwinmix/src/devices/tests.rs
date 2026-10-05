use super::*;
use godwinmix_protocol::devices::{DeviceRegistry, TokenCreateRequest};
use godwinmix_protocol::scope::{Scope, Tokens};

fn dir(name: &str) -> PathBuf {
    let dir = std::env::temp_dir().join(format!("gmx-devices-{name}-{}", std::process::id()));
    let _ = std::fs::remove_dir_all(&dir);
    std::fs::create_dir_all(&dir).unwrap();
    dir
}

fn guarded(devices: Arc<Devices>) -> Tokens {
    Tokens::new(vec![Token::legacy("desk-secret")], false).with_devices(devices)
}

fn ask(label: &str, scope: Scope) -> TokenCreateRequest {
    TokenCreateRequest { label: Some(label.into()), id: None, scope }
}

#[test]
fn a_made_token_signs_in_with_its_scope_and_revoking_it_shuts_it_out() {
    let path = dir("round").join("godwinmix.devices.toml");
    let devices = Arc::new(Devices::open(Some(path.clone())));
    let tokens = guarded(devices.clone());
    let made = devices.create(ask("Sam's phone", Scope::Operate), &|_| false).unwrap();
    assert_eq!(made.device.id, "sam-s-phone");
    assert!(made.token.len() >= 32);

    let who = tokens.authenticate(Some(&made.token)).unwrap();
    assert_eq!(who.id, "sam-s-phone");
    assert!(who.has(Scope::Operate) && !who.has(Scope::Admin));
    assert!(!tokens.revoked(&who));

    let text = std::fs::read_to_string(&path).unwrap();
    assert!(!text.contains(&made.token), "the secret itself must never be written down");

    devices.revoke("sam-s-phone").unwrap();
    assert!(tokens.revoked(&who), "a connection that signed in before the revoke loses its reach");
    assert!(tokens.authenticate(Some(&made.token)).is_err());
    assert!(!tokens.revoked(&Token::legacy("desk-secret")), "a configured token is never a revoked device");
}

#[test]
fn another_process_sees_a_new_token_at_once_and_a_revoke_within_a_second() {
    let path = dir("shared").join("godwinmix.devices.toml");
    let station = Arc::new(Devices::open(Some(path.clone())));
    let show = guarded(Arc::new(Devices::open(Some(path.clone()))));
    let made = station.create(ask("Tablet", Scope::Read), &|_| false).unwrap();
    let who = show.authenticate(Some(&made.token)).expect("the show reads the file on an unknown secret");
    station.revoke(&made.device.id).unwrap();
    std::thread::sleep(RECHECK + std::time::Duration::from_millis(50));
    assert!(show.revoked(&who));
    assert!(show.authenticate(Some(&made.token)).is_err());
}

#[test]
fn tokens_survive_a_restart_and_ids_stay_unique() {
    let path = dir("restart").join("godwinmix.devices.toml");
    let first = Devices::open(Some(path.clone()));
    let a = first.create(ask("Phone", Scope::Operate), &|_| false).unwrap();
    let b = first.create(ask("Phone", Scope::Operate), &|_| false).unwrap();
    assert_eq!((a.device.id.as_str(), b.device.id.as_str()), ("phone", "phone-2"));
    let taken = first.create(ask("Default", Scope::Read), &|id| id == "default").unwrap();
    assert_eq!(taken.device.id, "default-2", "a configured token's id is never reused");

    let again = guarded(Arc::new(Devices::open(Some(path))));
    assert_eq!(again.authenticate(Some(&b.token)).unwrap().id, "phone-2");
    assert_eq!(again.devices().unwrap().list().len(), 3);
}

#[test]
fn a_bad_request_says_what_to_send_instead() {
    let devices = Devices::open(None);
    let plugin = devices.create(ask("x", Scope::Plugin), &|_| false).unwrap_err();
    assert!(plugin.message.contains("read, operate or admin"), "{plugin}");
    let req = TokenCreateRequest { id: Some("Not A Slug".into()), ..ask("x", Scope::Read) };
    let bad = devices.create(req, &|_| false).unwrap_err();
    assert!(bad.message.contains("'not-a-slug'"), "{bad}");
    let missing = devices.revoke("nobody").unwrap_err();
    assert_eq!(missing.code, godwinmix_protocol::ErrorCode::NotFound.number());
    assert!(missing.message.contains("There are none"), "{missing}");
}

#[tokio::test]
async fn an_open_core_refuses_to_make_device_tokens() {
    let open = Tokens::default().with_devices(Arc::new(Devices::open(None)));
    let refusal = call(&open, "token.create", serde_json::json!({})).await.unwrap_err();
    assert!(refusal.message.contains("GODWINMIX_TOKEN"), "{refusal}");
    let guarded = guarded(Arc::new(Devices::open(None)));
    let made = call(&guarded, "token.create", serde_json::json!({ "label": "Phone" })).await.unwrap();
    assert_eq!(made["scope"], "operate", "operate is the default scope");
    let listed = call(&guarded, "token.list", serde_json::json!({})).await.unwrap();
    assert!(listed["tokens"][0].get("token").is_none(), "a list never carries a secret");
}
