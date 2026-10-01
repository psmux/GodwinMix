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

/// `show.set` with a switch: it answers inside five seconds with a task,
/// and `task.get` at the station carries the usual answer once it is done.
async fn switched(ws: &mut Ws, id: u64, params: Value) -> Value {
    let asked = std::time::Instant::now();
    let answer = call(ws, id, "show.set", params).await;
    assert!(asked.elapsed() < Duration::from_secs(5), "show.set took {:?}", asked.elapsed());
    let task = result(&answer)["task_id"].as_str().unwrap_or_else(|| panic!("a switch answers with a task: {answer}")).to_string();
    assert!(answer["result"]["show"]["id"].is_string(), "the show as it is now rides along: {answer}");
    finished(ws, id * 1000, &task).await
}

/// Poll a station task until it has an answer, for at most a minute.
async fn finished(ws: &mut Ws, first: u64, task: &str) -> Value {
    let started = std::time::Instant::now();
    for n in 0.. {
        let view = call(ws, first + n, "task.get", json!({"task_id": task})).await;
        let view = result(&view).clone();
        match view["state"].as_str() {
            Some("completed") => return view["result"].clone(),
            Some("running") => {}
            _ => panic!("the switch did not complete: {view}"),
        }
        assert!(started.elapsed() < Duration::from_secs(60), "the switch is still running after a minute: {view}");
        tokio::time::sleep(Duration::from_millis(250)).await;
    }
    unreachable!()
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

    let on = switched(&mut ws, 3, json!({"id": "feed", "compositing": true})).await;
    assert_eq!(on["compositing"], true, "{on}");
    assert_eq!(on["switch"]["outputs"], json!(["out"]), "{on}");
    let gap = on["switch"]["gap_ms"].as_u64();
    eprintln!("compositing on: outputs off for {gap:?} ms (show start, output connect)");
    assert!(gap.is_some(), "the output went live at the RTMP server: {on}");
    assert!(dir.join("shows/feed/godwinmix.toml").exists(), "a mixed show has a folder");
    let outs = get(&st, "/api/v1/outputs?show=feed").await;
    assert!(outs.to_string().contains("\"out\""), "the output is in the show now: {outs}");

    let off = switched(&mut ws, 4, json!({"id": "feed", "compositing": false})).await;
    assert_eq!(off["compositing"], false, "{off}");
    assert_eq!(off["state"], "running");
    assert_eq!(off["switch"]["outputs"], json!(["out"]), "{off}");
    assert_eq!(off["outputs"].as_array().map(Vec::len), Some(1), "the output came back to the station: {off}");
    let list = get(&st, "/api/v1/shows").await;
    let feed = list["shows"].as_array().unwrap().iter().find(|s| s["id"] == "feed").cloned().unwrap();
    assert_eq!(feed["compositing"], false, "{feed}");
}

/// The slow case: a direct show whose output has nowhere to go, so the
/// switch waits its whole half minute for it to come live. The call still
/// answers at once, a second switch meanwhile is refused with the task to
/// wait for, and the task finishes with a note instead of a gap.
#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
async fn a_slow_switch_answers_at_once_and_finishes_as_a_task() {
    let (dir, port) = folder("switch-slow");
    let st = start(dir, port, &[]).await;
    let mut ws = rpc(&st, "").await;
    let nowhere = std::net::TcpListener::bind("127.0.0.1:0").unwrap().local_addr().unwrap().port();
    let added = call(&mut ws, 1, "show.add", json!({
        "name": "Quiet", "compositing": false, "input": {"uri": "udp://@239.1.1.2:5000"},
        "outputs": [{"id": "out", "uri": format!("rtmp://127.0.0.1:{nowhere}/live/none")}]
    })).await;
    result(&added);

    let asked = std::time::Instant::now();
    let answer = call(&mut ws, 2, "show.set", json!({"id": "quiet", "compositing": true})).await;
    assert!(asked.elapsed() < Duration::from_secs(5), "show.set took {:?}", asked.elapsed());
    let task = result(&answer)["task_id"].as_str().expect("a task").to_string();
    assert!(task.starts_with("show-set-"), "{answer}");
    assert_eq!(answer["result"]["switching"], "on", "{answer}");

    let again = call(&mut ws, 3, "show.set", json!({"id": "quiet", "compositing": true})).await;
    assert!(again.get("error").is_none() || again["error"]["data"]["task_id"] == task.as_str(), "{again}");
    let rest = reqwest::get(format!("http://{}/api/v1/tasks/{task}", st.url)).await.unwrap();
    assert_eq!(rest.status(), 200, "a plain REST client reads the station's task by its path");
    let cancel = call(&mut ws, 4, "task.cancel", json!({"task_id": task})).await;
    assert!(cancel["error"]["message"].as_str().unwrap_or_default().contains("cannot stop halfway"), "{cancel}");

    let done = finished(&mut ws, 100, &task).await;
    assert_eq!(done["compositing"], true, "{done}");
    assert!(done["switch"]["gap_ms"].is_null(), "nothing came live: {done}");
    assert!(!done["switch"]["note"].as_str().unwrap_or_default().is_empty(), "the note says why: {done}");
    eprintln!("slow switch finished after {:?}", asked.elapsed());

    // `gmx shows set` follows the task and prints the usual answer. No
    // direct host runs here, so the way back does not wait for the output.
    let gmx = env!("CARGO_BIN_EXE_gmx");
    let url = format!("http://{}", st.url);
    let out = tokio::task::spawn_blocking(move || {
        Command::new(gmx).args(["shows", "--url", &url, "set", "quiet", "--compositing", "off"]).env_remove("GODWINMIX_TOKEN").output().unwrap()
    });
    let out = tokio::time::timeout(Duration::from_secs(90), out).await.expect("gmx shows set within 90 s").unwrap();
    let printed = String::from_utf8_lossy(&out.stdout).to_string();
    assert!(out.status.success(), "{printed}{}", String::from_utf8_lossy(&out.stderr));
    assert!(String::from_utf8_lossy(&out.stderr).contains("switching compositing off"), "{}", String::from_utf8_lossy(&out.stderr));
    let shown: Value = serde_json::from_str(&printed).unwrap_or_else(|e| panic!("{e}: {printed}"));
    assert_eq!(shown["compositing"], false, "{shown}");
    assert_eq!(shown["switch"]["outputs"], json!(["out"]), "{shown}");
}
