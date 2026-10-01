//! The compositing switch against the real binary: a direct show turned
//! into a mixed one, its output moved to the show process and measured live
//! at an RTMP server (ffmpeg listening), then turned back. Skipped where
//! ffmpeg is not installed. No direct host runs here, so the way back has
//! nothing to come live at and reports no gap.

use super::support::*;
use serde_json::{json, Value};
use std::process::{Command, Stdio};
use std::time::Duration;

struct Listener(std::process::Child);

impl Drop for Listener {
    fn drop(&mut self) {
        let _ = self.0.kill();
        let _ = self.0.wait();
    }
}

fn rtmp_server(port: u16) -> Option<Listener> {
    let url = format!("rtmp://127.0.0.1:{port}/live/switch");
    let child = Command::new("ffmpeg")
        .args(["-hide_banner", "-loglevel", "error", "-listen", "1", "-timeout", "60", "-i", &url, "-f", "null", "-"])
        .stdout(Stdio::null())
        .stderr(Stdio::null())
        .spawn()
        .ok()?;
    std::thread::sleep(Duration::from_millis(500));
    Some(Listener(child))
}

fn result(answer: &Value) -> &Value {
    assert!(answer.get("error").is_none(), "{answer}");
    &answer["result"]
}

#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
async fn compositing_turns_on_with_the_output_moved_and_off_again() {
    let rtmp = std::net::TcpListener::bind("127.0.0.1:0").unwrap().local_addr().unwrap().port();
    let Some(_server) = rtmp_server(rtmp) else {
        return eprintln!("ffmpeg is not installed; the switch test needs it as an RTMP server");
    };
    let (dir, port) = folder("switch");
    let st = start(dir.clone(), port, &[]).await;
    let mut ws = rpc(&st, "").await;
    let output = format!("rtmp://127.0.0.1:{rtmp}/live/switch");
    let added = call(&mut ws, 1, "show.add", json!({
        "name": "Feed", "compositing": false, "input": {"uri": "udp://@239.1.1.1:5000"},
        "outputs": [{"id": "out", "uri": output}]
    })).await;
    result(&added);

    let refused = call(&mut ws, 2, "show.set", json!({"id": "main", "compositing": false, "input": {"uri": "udp://@239.9.9.9:5000"}})).await;
    assert!(refused["error"]["message"].as_str().unwrap_or_default().contains("main"), "{refused}");

    let on = call(&mut ws, 3, "show.set", json!({"id": "feed", "compositing": true})).await;
    let on = result(&on);
    assert_eq!(on["compositing"], true, "{on}");
    assert_eq!(on["switch"]["outputs"], json!(["out"]), "{on}");
    let gap = on["switch"]["gap_ms"].as_u64();
    eprintln!("compositing on: outputs off for {gap:?} ms (show start, output connect)");
    assert!(gap.is_some(), "the output went live at the RTMP server: {on}");
    assert!(dir.join("shows/feed/godwinmix.toml").exists(), "a mixed show has a folder");
    let outs = get(&st, "/api/v1/outputs?show=feed").await;
    assert!(outs.to_string().contains("\"out\""), "the output is in the show now: {outs}");

    let off = call(&mut ws, 4, "show.set", json!({"id": "feed", "compositing": false})).await;
    let off = result(&off);
    assert_eq!(off["compositing"], false, "{off}");
    assert_eq!(off["state"], "running");
    assert_eq!(off["switch"]["outputs"], json!(["out"]), "{off}");
    assert_eq!(off["outputs"].as_array().map(Vec::len), Some(1), "the output came back to the station: {off}");
    let list = get(&st, "/api/v1/shows").await;
    let feed = list["shows"].as_array().unwrap().iter().find(|s| s["id"] == "feed").cloned().unwrap();
    assert_eq!(feed["compositing"], false, "{feed}");
}
