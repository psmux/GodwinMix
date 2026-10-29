//! Guest invites against a real mixer on a real port: a guest's browser
//! (a `webrtcbin` in this process) publishes with the invite's key and
//! plays its return, talkback is held into that return, and a revoke ends
//! it all. No mocks: ICE, DTLS, SRTP, the bus and the WHEP output all run.

mod core;
mod rtc;

use crate::core::Core;
use serde_json::{json, Value};
use std::time::{Duration, Instant};

async fn post(url: &str, key: &str, sdp: String) -> (u16, String, String) {
    let res = reqwest::Client::new()
        .post(url)
        .header("Content-Type", "application/sdp")
        .bearer_auth(key)
        .body(sdp)
        .send()
        .await
        .expect("the mixer answers");
    let status = res.status().as_u16();
    let location = res.headers().get("location").and_then(|v| v.to_str().ok()).unwrap_or("").to_string();
    (status, location, res.text().await.unwrap_or_default())
}

async fn state_of(core: &Core, id: &str, want: &str, secs: u64) -> String {
    let until = Instant::now() + Duration::from_secs(secs);
    loop {
        let list = core.ok("guest.invite.list", json!({})).await;
        let state = list["guests"].as_array().unwrap().iter().find(|g| g["id"] == id).map(|g| g["state"].as_str().unwrap().to_string());
        let state = state.unwrap_or_else(|| "none".into());
        if state == want || Instant::now() > until {
            return state;
        }
        tokio::time::sleep(Duration::from_millis(100)).await;
    }
}

/// Publish as `id` with `key` and wait for it to be live.
async fn join(core: &Core, base: &str, id: &str, key: &str) -> rtc::Peer {
    let guest = rtc::sender();
    let offer = guest.offer();
    let (status, location, answer) = post(&format!("{base}/guest/{id}/whip"), key, offer).await;
    assert_eq!(status, 201, "the offer was refused: {answer}");
    assert!(location.starts_with(&format!("/guest/{id}/whip/")), "{location}");
    guest.answer(&answer);
    if id != "talkback" {
        assert_eq!(state_of(core, id, "live", 15).await, "live");
    }
    guest
}

fn has(list: &Value, key: &str, id: &str) -> bool {
    list[key].as_array().unwrap().iter().any(|s| s["id"] == id)
}

#[tokio::test(flavor = "multi_thread", worker_threads = 4)]
async fn a_guest_joins_with_their_own_key_hears_a_return_and_is_gone_within_a_second_of_a_revoke() {
    if !rtc::ready() {
        return;
    }
    let core = Core::start("join").await;
    let base = core.serve_http().await;
    let made = core.ok("guest.invite.create", json!({ "name": "Ana Lopez" })).await;
    let id = made["invite"]["id"].as_str().unwrap().to_string();
    assert_eq!(id, "ana-lopez");
    let link = made["link"].as_str().unwrap();
    assert!(link.contains("/join/#guest=ana-lopez&key=") && link.contains("&title=Ana%20Lopez"), "{link}");
    assert!(made["qr_svg"].as_str().unwrap().starts_with("<svg"));
    let key = link.split("key=").nth(1).unwrap().split('&').next().unwrap().to_string();
    let listed = core.ok("guest.invite.list", json!({})).await;
    assert!(!listed.to_string().contains(&key), "the list never carries a key: {listed}");
    assert_eq!(listed["guests"][0]["state"], "invited");

    let (wrong, _, why) = post(&format!("{base}/guest/{id}/whip"), "not-the-key", "v=0".into()).await;
    assert_eq!(wrong, 403, "{why}");
    assert!(why.contains("whole link"), "{why}");

    let guest = join(&core, &base, &id, &key).await;
    let configs = core.ok("source.list", json!({})).await.to_string();
    assert!(configs.contains("guest-ana-lopez") && configs.contains("Ana Lopez"), "the guest's source: {configs}");
    let buses = core.ok("audio.bus.list", json!({})).await;
    let bus = buses["buses"].as_array().unwrap().iter().find(|b| b["id"] == "guest-ana-lopez").expect("the return bus").clone();
    assert_eq!(bus["minus"], "guest-ana-lopez", "{bus}");
    assert_eq!(bus["outputs"], json!(["guest-ana-lopez-return"]), "{bus}");

    let viewer = rtc::viewer();
    let offer = viewer.offer();
    let until = Instant::now() + Duration::from_secs(15);
    let (status, _, answer) = loop {
        let got = post(&format!("{base}/guest/{id}/whep"), &key, offer.clone()).await;
        if got.0 != 503 || Instant::now() > until {
            break got;
        }
        tokio::time::sleep(Duration::from_millis(300)).await;
    };
    assert_eq!(status, 201, "the return was refused: {answer}");
    viewer.answer(&answer);
    let (frames, sound) = viewer.wait_for(30, 25, 20).await;
    assert!(frames >= 30 && sound >= 25, "the return decoded {frames} frames and {sound} Opus packets");

    let revoked = Instant::now();
    core.ok("guest.invite.revoke", json!({ "id": id })).await;
    let took = revoked.elapsed();
    assert!(took < Duration::from_secs(1), "the revoke took {took:?}");
    let before = viewer.frames.load(std::sync::atomic::Ordering::Relaxed);
    tokio::time::sleep(Duration::from_millis(1500)).await;
    let after = viewer.frames.load(std::sync::atomic::Ordering::Relaxed);
    assert!(after <= before + 5, "the return kept playing after the revoke: {before} then {after}");
    assert!(!core.ok("source.list", json!({})).await.to_string().contains("guest-ana-lopez"), "the source went");
    assert!(!has(&core.ok("audio.bus.list", json!({})).await, "buses", "guest-ana-lopez"), "the bus went");
    let (again, _, _) = post(&format!("{base}/guest/{id}/whip"), &key, guest.offer()).await;
    assert_eq!(again, 404, "a revoked key lets nobody in");
}

#[tokio::test(flavor = "multi_thread", worker_threads = 4)]
async fn talkback_is_held_into_one_guests_return_and_kept_off_the_programme() {
    if !rtc::ready() {
        return;
    }
    let core = Core::start("talk").await;
    let base = core.serve_http().await;
    let made = core.ok("guest.invite.create", json!({ "name": "Ben", "id": "ben" })).await;
    let key = made["link"].as_str().unwrap().split("key=").nth(1).unwrap().split('&').next().unwrap().to_string();
    let early = core.refused("guest.talkback.set", json!({ "on": true, "guest": "ben" })).await;
    assert_eq!(early.code, -32001, "nobody has joined: {early:?}");
    let _guest = join(&core, &base, "ben", &key).await;
    let missing = core.refused("guest.talkback.set", json!({ "on": true })).await;
    assert_eq!(missing.data["id"], "guest-talkback", "no microphone yet: {missing:?}");
    assert!(missing.message.contains("Press Talk"), "{}", missing.message);

    let _mic = join(&core, &base, "talkback", "open").await;
    let until = Instant::now() + Duration::from_secs(15);
    while core.ok("guest.invite.list", json!({})).await["talkback"]["present"] != true && Instant::now() < until {
        tokio::time::sleep(Duration::from_millis(100)).await;
    }
    let held = core.ok("guest.talkback.set", json!({ "on": true, "guest": "ben" })).await;
    assert_eq!(held["talkback"]["held"], json!(["ben"]), "{held}");
    assert_eq!(held["guests"][0]["talkback"], true);
    let sources = core.ok("source.list", json!({})).await;
    let mic = sources.as_array().unwrap().iter().find(|s| s["id"] == "guest-talkback").expect("the microphone").clone();
    assert_eq!(mic["muted"], true, "talkback is muted on the programme: {mic}");
    core.ok("source.audio.set", json!({ "id": "guest-talkback", "muted": false })).await;
    let sources = core.ok("source.list", json!({})).await;
    let mic = sources.as_array().unwrap().iter().find(|s| s["id"] == "guest-talkback").unwrap().clone();
    assert_eq!(mic["muted"], true, "unmuting the talkback source did not stick: {mic}");
    let off = core.ok("guest.talkback.set", json!({ "on": false })).await;
    assert_eq!(off["talkback"]["held"], json!([]));
    let unknown = core.refused("guest.talkback.set", json!({ "on": true, "guest": "nobody" })).await;
    assert_eq!(unknown.code, -32004);
}

#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
async fn invites_are_kept_across_a_restart_and_refusals_say_what_to_send() {
    let core = Core::start("keep").await;
    core.ok("guest.invite.create", json!({ "name": "Cara", "expires_in_secs": 3600 })).await;
    let file = core.path.with_file_name("godwinmix.guests.json");
    assert!(file.exists(), "the invites file is beside the config");
    let shape = godwinmix_core::guest::Shape { fps: 30, rate: 48_000, channels: 2 };
    let (detached, _rx) = godwinmix_core::mixer::MixerHandle::detached();
    let again = godwinmix::guests::Guests::open(&core.path, detached, shape);
    assert_eq!(again.list().guests[0].id, "cara", "the invite came back from the file");
    assert_eq!(again.link("cara").unwrap().link, core.ok("guest.invite.link", json!({ "id": "cara" })).await["link"], "with the same link");

    let unnamed = core.refused("guest.invite.create", json!({ "name": "  " })).await;
    assert_eq!(unnamed.data["field"], "name");
    let short = core.refused("guest.invite.create", json!({ "name": "Dee", "expires_in_secs": 5 })).await;
    assert_eq!(short.data["field"], "expires_in_secs");
    let reserved = core.refused("guest.invite.create", json!({ "name": "Op", "id": "talkback" })).await;
    assert_eq!(reserved.data["field"], "id");
    let base = core.refused("guest.invite.create", json!({ "name": "Eve", "base": "studio.example.com" })).await;
    assert_eq!(base.data["field"], "base");
    let gone = core.refused("guest.invite.revoke", json!({ "id": "dan" })).await;
    assert_eq!(gone.code, -32004);
    assert_eq!(gone.data["valid"], json!(["cara"]));
}
