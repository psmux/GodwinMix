//! A headend set up on the station itself, with no Flussonic file: channels
//! added, refused, changed in place or made again, switched off in bulk,
//! moved to a saved profile, removed, and kept across a restart. A channel
//! set up by hand and the same stream imported from a Flussonic config make
//! the same direct show. And `headend.scan` lists the programs of a real
//! two program multiplex.

use super::direct_live::{free_udp, staged_ingest};
use super::support::*;
use gstreamer::prelude::*;
use serde_json::{json, Value};
use std::time::Duration;

fn ok(answer: &Value) -> &Value {
    assert!(answer.get("error").is_none(), "{answer}");
    &answer["result"]
}

fn show<'a>(list: &'a Value, id: &str) -> Option<&'a Value> {
    list["shows"].as_array()?.iter().find(|s| s["id"] == id)
}

/// No reserve, alone on the machine, as headend_import.rs: what is checked
/// here is the headend, not the governor's room on a small runner.
async fn roomy(name: &str) -> (std::path::PathBuf, u16, Running) {
    let (dir, port) = folder(name);
    let config = dir.join("godwinmix.toml");
    let text = std::fs::read_to_string(&config).unwrap();
    std::fs::write(&config, format!("{text}\n[governor]\nreserve_cores = 0\n")).unwrap();
    let st = start_alone(dir.clone(), port, &[]).await;
    (dir, port, st)
}

fn copy(name: &str, uri: &str) -> Value {
    json!({"name": name, "input": {"uri": uri}})
}

#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
async fn channels_are_set_up_changed_and_kept_with_no_flussonic_file() {
    let (dir, port, st) = roomy("headend-native").await;
    let mut ws = rpc(&st, "").await;

    let setup = call(&mut ws, 1, "headend.channel.list", json!({})).await;
    let r = ok(&setup);
    assert_eq!(r["channels"], json!([]), "{r}");
    let ids: Vec<&str> = r["profiles"].as_array().unwrap().iter().filter_map(|p| p["id"].as_str()).collect();
    assert!(ids.contains(&"copy") && ids.contains(&"h264-720p-1500"), "{r}");
    assert!(r["encoders"].as_array().unwrap().iter().any(|e| e["on"] == "cpu"), "x264 at least: {r}");

    let dry = call(&mut ws, 2, "headend.channel.add", json!({"dry_run": true, "channels": [copy("NEWS", "rtmp://127.0.0.1:1/live/news")]})).await;
    assert_eq!(ok(&dry)["done"], json!(["NEWS"]), "{dry}");
    assert!(show(ok(&call(&mut ws, 3, "show.list", json!({})).await), "news").is_none(), "a dry run makes nothing");

    let bad = json!({"name": "has space", "input": {"uri": "udp://@239.0.0.1:5000"}});
    let nouri = json!({"name": "SPORT", "input": {"uri": "239.0.0.2:5000"}});
    let add = call(&mut ws, 4, "headend.channel.add", json!({"channels": [copy("NEWS", "rtmp://127.0.0.1:1/live/news"), bad, nouri, copy("NEWS", "rtmp://127.0.0.1:1/live/b")]})).await;
    let r = ok(&add);
    assert_eq!(r["done"], json!(["NEWS"]), "{r}");
    let why: Vec<&str> = r["refused"].as_array().unwrap().iter().map(|x| x["why"].as_str().unwrap()).collect();
    assert!(why[0].contains("CH117") && why[1].contains("udp://@239.0.0.2:5000") && why[2].contains("already a channel"), "{why:?}");
    let list = call(&mut ws, 5, "show.list", json!({})).await;
    let news = show(ok(&list), "news").expect("NEWS runs as a show").clone();
    assert_eq!((news["compositing"].as_bool(), news["outputs"][0]["id"].as_str()), (Some(false), Some("flussonic")), "{news}");

    // A title moves in place; a new profile makes the show again.
    let mut ch = ok(&call(&mut ws, 6, "headend.channel.list", json!({})).await)["channels"][0]["channel"].clone();
    ch["title"] = json!("News");
    let up = call(&mut ws, 7, "headend.channel.update", json!({"name": "NEWS", "channel": ch})).await;
    assert!(ok(&up).get("restarted").is_none(), "a title is no restart: {up}");
    ch["profile"] = json!("aac-sound");
    let up = call(&mut ws, 8, "headend.channel.update", json!({"name": "NEWS", "channel": ch})).await;
    assert_eq!(ok(&up)["restarted"], json!(["NEWS"]), "{up}");
    let news = show(ok(&call(&mut ws, 9, "show.list", json!({})).await), "news").unwrap().clone();
    assert_eq!((news["name"].as_str(), news["outputs"][0]["rendition"]["audio"]["codec"].as_str()), (Some("News"), Some("aac")), "{news}");

    // A saved profile, taken by two channels at once, and refused removal.
    // Sound only, so it fits on a small runner: what is checked is the
    // profile, not the governor's room for a picture encode.
    let profile = json!({"id": "sd-test", "name": "SD test", "audio": {"codec": "aac", "bitrate_kbps": 96}});
    ok(&call(&mut ws, 10, "headend.profile.save", json!({"profile": profile})).await);
    ok(&call(&mut ws, 11, "headend.channel.add", json!({"channels": [copy("MOVIES", "rtmp://127.0.0.1:1/live/movies")]})).await);
    let bulk = call(&mut ws, 12, "headend.channel.bulk", json!({"names": ["NEWS", "MOVIES"], "profile": "sd-test", "enabled": false})).await;
    assert_eq!(ok(&bulk)["done"], json!(["NEWS", "MOVIES"]), "{bulk}");
    let list = call(&mut ws, 13, "show.list", json!({})).await;
    for id in ["news", "movies"] {
        let s = show(ok(&list), id).unwrap();
        assert_eq!((s["state"].as_str(), s["outputs"][0]["rendition"]["audio"]["bitrate_kbps"].as_u64()), (Some("stopped"), Some(96)), "{s}");
    }
    let refused = call(&mut ws, 14, "headend.profile.remove", json!({"id": "sd-test"})).await;
    assert!(refused["error"]["message"].as_str().unwrap().contains("move_to"), "{refused}");
    ok(&call(&mut ws, 15, "headend.profile.remove", json!({"id": "sd-test", "move_to": "copy"})).await);

    let export = call(&mut ws, 16, "headend.export", json!({})).await;
    let text = ok(&export)["text"].as_str().unwrap().to_string();
    assert!(text.contains("stream NEWS {") && text.contains("title News;") && text.contains("disabled true;"), "{text}");

    ok(&call(&mut ws, 17, "headend.channel.remove", json!({"names": ["MOVIES"]})).await);
    assert!(show(ok(&call(&mut ws, 18, "show.list", json!({})).await), "movies").is_none(), "its show went with it");
    drop(ws);
    drop(st);

    // Kept across a restart, with its show, and not made twice.
    let st = start_alone(dir.clone(), port, &[]).await;
    let mut ws = rpc(&st, "").await;
    let setup = call(&mut ws, 20, "headend.channel.list", json!({})).await;
    let names: Vec<&str> = ok(&setup)["channels"].as_array().unwrap().iter().map(|c| c["channel"]["name"].as_str().unwrap()).collect();
    assert_eq!(names, ["NEWS"], "{setup}");
    assert_eq!(ok(&setup)["channels"][0]["show"], "news", "{setup}");
    let list = call(&mut ws, 21, "show.list", json!({})).await;
    assert_eq!(ok(&list)["shows"].as_array().unwrap().iter().filter(|s| s["id"].as_str().unwrap().starts_with("news")).count(), 1, "{list}");
}

#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
async fn a_channel_set_up_here_and_one_imported_are_the_same_show() {
    let (_dir, _port, st) = roomy("headend-same").await;
    let mut ws = rpc(&st, "").await;
    let config = "stream CH201 {\n  input mpts-udp://eth1@239.0.0.9:2001?program=7;\n  transcoder gop=50 hw=nvenc vb=1500k vcodec=h264 size=1280x720:fit:#000000 ab=128k;\n}\n";
    let made = call(&mut ws, 1, "headend.import", json!({"text": config, "dry_run": false})).await;
    if ok(&made)["shows"]["added"].as_array().is_some_and(Vec::is_empty) {
        eprintln!("CH201 was refused for room on this machine, so the two shows were not compared: {made}");
        return;
    }
    let custom = json!({"video": {"codec": "h264", "width": 1280, "height": 720, "fit": "pad", "pad_color": "#000000", "bitrate_kbps": 1500, "keyframe_frames": 50, "deinterlace": "off", "preset": "p1"},
                        "audio": {"codec": "aac", "bitrate_kbps": 128, "tracks": "all"}});
    let by_hand = json!({"name": "CH202", "input": {"uri": "udp://@239.0.0.9:2001", "program": 7, "interface": "eth1"}, "profile": "custom", "custom": custom});
    ok(&call(&mut ws, 2, "headend.channel.add", json!({"channels": [by_hand]})).await);
    // An HLS output's params are read back once the packager has it, which
    // for the show made second can take a moment.
    let mut list = Value::Null;
    for id in 0..40 {
        list = call(&mut ws, 100 + id, "show.list", json!({})).await;
        let handed = |s: &str| show(ok(&list), s).is_some_and(|x| x["outputs"][0]["params"].is_object());
        if handed("ch201") && handed("ch202") {
            break;
        }
        tokio::time::sleep(Duration::from_millis(250)).await;
    }
    let (a, b) = (show(ok(&list), "ch201").expect("the import's show"), show(ok(&list), "ch202").expect("the channel's show"));
    assert_eq!(a["input"], b["input"], "{a}\n{b}");
    assert_eq!(a["compositing"], b["compositing"]);
    let out = |s: &Value| s["outputs"].as_array().unwrap().iter().map(|o| (o["id"].clone(), o["uri"].clone(), o["rendition"].clone(), o["params"].clone())).collect::<Vec<_>>();
    assert_eq!(out(a), out(b), "{a}\n{b}");
    // Both are channels the station keeps, and both edit the same way.
    let setup = call(&mut ws, 4, "headend.channel.list", json!({})).await;
    let entries = ok(&setup)["channels"].as_array().unwrap().clone();
    assert_eq!(entries.len(), 2, "{setup}");
    assert!(entries[0]["channel"]["origin"].as_str().unwrap().starts_with("import of"), "{setup}");
    assert_eq!(entries[0]["channel"]["custom"], entries[1]["channel"]["custom"], "{setup}");
}

/// Two programs in one TS over UDP to `port`: 1 is 320x180 H.264 and AAC,
/// 2 is 640x360 H.264 and AAC.
fn two_programs(port: u16) -> gstreamer::Pipeline {
    gstreamer::init().unwrap();
    let picture = |w: u32, h: u32, pad: u32| format!("videotestsrc is-live=true ! video/x-raw,format=I420,width={w},height={h},framerate=25/1 ! x264enc tune=zerolatency key-int-max=25 bitrate=400 ! h264parse ! mux.sink_{pad}");
    let sound = |pad: u32| format!("audiotestsrc is-live=true ! audioconvert ! audioresample ! audio/x-raw,rate=48000 ! avenc_aac ! aacparse ! mux.sink_{pad}");
    let text = format!(
        "{} {} {} {} mpegtsmux name=mux alignment=7 prog-map=program_map,sink_300=1,sink_301=1,sink_302=2,sink_303=2 ! udpsink host=127.0.0.1 port={port} sync=false",
        picture(320, 180, 300), sound(301), picture(640, 360, 302), sound(303)
    );
    let p = gstreamer::parse::launch(&text).expect("x264enc, avenc_aac and mpegtsmux are installed").downcast::<gstreamer::Pipeline>().unwrap();
    p.set_state(gstreamer::State::Playing).unwrap();
    p
}

#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
async fn a_scan_lists_every_program_with_its_picture_and_sound() {
    let (dir, port) = folder("headend-scan");
    let source = staged_ingest(&dir);
    let st = start(dir.clone(), port, &[]).await;
    let mut ws = rpc(&st, "").await;
    ok(&call(&mut ws, 1, "plugin.add", json!({"source": source.to_string_lossy()})).await);
    let input = free_udp();
    let feed = two_programs(input);
    let uri = format!("udp://0.0.0.0:{input}");
    let mut found = Value::Null;
    // The plugin starts in the background; the first scans may find it down.
    for attempt in 0..6 {
        let got = call_within(&mut ws, 10 + attempt, "headend.scan", json!({"uri": uri, "wait_ms": 10000}), Duration::from_secs(40)).await;
        if got.get("error").is_none() && got["result"]["programs"].as_array().is_some_and(|p| p.len() == 2) {
            found = got["result"].clone();
            break;
        }
        tokio::time::sleep(Duration::from_secs(3)).await;
    }
    feed.set_state(gstreamer::State::Null).unwrap();
    let programs = found["programs"].as_array().unwrap_or_else(|| panic!("no scan found both programs: {found}"));
    let numbers: Vec<u64> = programs.iter().map(|p| p["number"].as_u64().unwrap()).collect();
    assert_eq!(numbers, [1, 2], "{found}");
    let video = |p: &Value| p["tracks"].as_array().unwrap().iter().find(|t| t["kind"] == "video").cloned().unwrap_or_default();
    assert_eq!((video(&programs[0])["width"].as_u64(), video(&programs[1])["width"].as_u64()), (Some(320), Some(640)), "{found}");
    assert!(programs.iter().all(|p| p["tracks"].as_array().unwrap().iter().any(|t| t["kind"] == "audio" && t["codec"] == "aac")), "{found}");
    // Nothing is left behind: a probe is no show.
    let list = call(&mut ws, 30, "show.list", json!({})).await;
    assert!(ok(&list)["shows"].as_array().unwrap().iter().all(|s| !s["id"].as_str().unwrap().starts_with("scan")), "{list}");
}
