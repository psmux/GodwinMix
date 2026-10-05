//! Two shows, one killed: the station starts it again and says so, the
//! other carries on without a restart, and the governor's book, which held
//! the killed show's rendition, gives it back.

use super::support::*;
use serde_json::{json, Value};
use std::time::{Duration, Instant};

async fn governor_used(st: &Running) -> (u64, u64) {
    let g = get(st, "/api/v1/governor/status").await;
    let sessions = g["devices"].as_array().map(|d| d.iter().filter_map(|x| x["sessions_used"].as_u64()).sum()).unwrap_or(0);
    (g["cpu"]["used_millicores"].as_u64().unwrap_or(0), sessions)
}

async fn until<F, Fut>(what: &str, limit: Duration, mut test: F)
where
    F: FnMut() -> Fut,
    Fut: std::future::Future<Output = bool>,
{
    let start = Instant::now();
    while !test().await {
        assert!(start.elapsed() < limit, "timed out after {limit:?} waiting for {what}");
        tokio::time::sleep(Duration::from_millis(100)).await;
    }
}

fn pid_of(dir: &std::path::Path, show: &str) -> Option<i32> {
    let config = dir.join("shows").join(show).join("godwinmix.toml");
    let out = std::process::Command::new("pgrep").args(["-f", &config.to_string_lossy()]).output().ok()?;
    String::from_utf8_lossy(&out.stdout).lines().next()?.trim().parse().ok()
}

#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
async fn a_show_killed_is_started_again_the_other_runs_on_and_its_share_goes_back() {
    let (dir, port) = folder("kill");
    // No reserve: this is about the book, a ticket given back and asked for
    // again, not about how much a machine keeps free. The reserve grows with
    // how far the load jumps, and a show killed and started again is such a
    // jump: on a four core runner the restarted show's 0.6 core rendition
    // was refused with "0.0 cores is free" and never asked for again.
    let config = dir.join("godwinmix.toml");
    let text = std::fs::read_to_string(&config).unwrap();
    std::fs::write(&config, format!("{text}\n[governor]\nreserve_cores = 0\n")).unwrap();
    let st = start_alone(dir.clone(), port, &[]).await;
    // The station measures its encoders in a process of its own when it
    // starts, at full speed, and the governor counts that process as another
    // program on the machine. On a four core runner that left no room at the
    // moment the killed show came back. Wait for the measurement first, for
    // two minutes at most: a station whose measurement fails carries on.
    let calibrating = Instant::now();
    while calibrating.elapsed() < Duration::from_secs(120) {
        let g = get(&st, "/api/v1/governor/status").await;
        if !g["calibrated_at"].is_null() && g["calibrating"] == false {
            break;
        }
        tokio::time::sleep(Duration::from_millis(250)).await;
    }
    let mut ws = rpc(&st, "").await;
    call(&mut ws, 1, "core.subscribe", json!({"events": ["show.*"]})).await;
    let added = call(&mut ws, 2, "show.add", json!({"name": "Second"})).await;
    assert_eq!(added["result"]["id"], "second", "{added}");
    let running = |p: &Value| p["show"]["id"] == "second" && p["show"]["state"] == "running";
    assert!(event(&mut ws, "show.changed", Duration::from_secs(90), running).await.is_some(), "the new show came up");

    // `show` in core.subscribe moves a connection that named no show.
    let mut moved = rpc(&st, "").await;
    call(&mut moved, 1, "core.subscribe", json!({"show": "second", "events": ["scene.*"]})).await;
    let made = call(&mut moved, 2, "scene.add", json!({"name": "Only in second"})).await;
    assert!(made.get("result").is_some(), "{made}");
    let in_second = get(&st, "/api/v1/scenes?show=second").await.to_string();
    let in_main = get(&st, "/api/v1/scenes").await.to_string();
    assert!(in_second.contains("Only in second") && !in_main.contains("Only in second"), "{in_second} / {in_main}");

    let recordings = dir.join("recordings");
    let output = json!({"id": "archive", "uri": "record://programme", "type": "record/output",
        "params": {"format": "mkv", "directory": recordings}, "rendition": {"preset": "youtube-720p30"}});
    let mut second = rpc(&st, "?show=second").await;
    let made = call(&mut second, 1, "output.add", output).await;
    assert!(made.get("result").is_some(), "{made}");
    until("the station's governor to hold the second show's rendition", Duration::from_secs(20), || async {
        governor_used(&st).await != (0, 0)
    })
    .await;

    let before = get(&st, "/api/v1/core/status?show=main").await["uptime_secs"].as_u64().unwrap();
    let pid = pid_of(&dir, "second").expect("the second show's process");
    let held = governor_used(&st).await;
    assert!(held.0 > 0 || held.1 > 0, "held before the kill: {held:?}");
    unsafe {
        libc::kill(pid, libc::SIGKILL);
    }
    // The show comes back within a second and asks again for the output it
    // kept, so the moment the share is free is short: look often.
    let start = Instant::now();
    let mut freed = false;
    while start.elapsed() < Duration::from_secs(3) && !freed {
        freed = governor_used(&st).await.0 < held.0.max(1);
        tokio::time::sleep(Duration::from_millis(5)).await;
    }
    assert!(freed, "the killed show's share went back to the book (held {held:?})");
    let died = |p: &Value| p["show"]["id"] == "second" && p["show"]["state"] == "starting" && p["show"]["restarts"] == 1;
    assert!(event(&mut ws, "show.changed", Duration::from_secs(10), died).await.is_some(), "the event says it died");
    let back = |p: &Value| p["show"]["id"] == "second" && p["show"]["state"] == "running";
    assert!(event(&mut ws, "show.changed", Duration::from_secs(90), back).await.is_some(), "and that it is back");
    let list = get(&st, "/api/v1/shows").await;
    let second = list["shows"].as_array().unwrap().iter().find(|s| s["id"] == "second").cloned().unwrap();
    assert_eq!(second["restarts"], 1, "{list}");
    assert_ne!(pid_of(&dir, "second"), Some(pid), "a new process");

    let start = Instant::now();
    let mut now = governor_used(&st).await;
    while now.0 != held.0 && start.elapsed() < Duration::from_secs(20) {
        tokio::time::sleep(Duration::from_millis(100)).await;
        now = governor_used(&st).await;
    }
    if now.0 != held.0 {
        let shows = get(&st, "/api/v1/shows").await;
        let governor = get(&st, "/api/v1/governor/status").await;
        // What else is taking the machine: the governor's room is what is
        // left after every other program's recent peak.
        let sort = if cfg!(target_os = "macos") { "-r" } else { "--sort=-pcpu" };
        let busy = std::process::Command::new("ps").args(["-Ao", "pid,pcpu,etime,args", sort]).output().map(|o| String::from_utf8_lossy(&o.stdout).lines().take(15).map(|l| l.chars().take(200).collect::<String>()).collect::<Vec<_>>().join("\n")).unwrap_or_default();
        panic!("the restarted show holds its rendition again, once: {now:?} against {held:?} before\nshows: {shows}\ngovernor: {governor}\nbusiest:\n{busy}");
    }

    let main = list["shows"].as_array().unwrap().iter().find(|s| s["id"] == "main").cloned().unwrap();
    assert_eq!((main["state"].clone(), main["restarts"].clone()), (json!("running"), json!(0)), "{list}");
    let after = get(&st, "/api/v1/core/status?show=main").await["uptime_secs"].as_u64().unwrap();
    assert!(after >= before, "main was not restarted: uptime {before} then {after}");
}
