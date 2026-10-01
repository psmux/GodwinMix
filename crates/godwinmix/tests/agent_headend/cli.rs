//! `gmx shows` against a real station: the same list as a file, a dry run,
//! the real add, the stats table, and compositing switched on for one show.

use super::agent::GMX;
use super::support;
use std::process::{Command, Output};
use std::time::{Duration, Instant};

fn gmx(url: &str, args: &[&str]) -> Output {
    let mut cmd = Command::new(GMX);
    cmd.arg("shows").args(["--url", &format!("http://{url}")]).args(args);
    // Turning compositing on waits up to 30 s for the outputs to be live
    // again, and nothing is sending to these inputs, so the switch takes
    // that long here.
    cmd.env_remove("GODWINMIX_TOKEN").env("GODWINMIX_HTTP_TIMEOUT_SECS", "90");
    cmd.output().expect("gmx runs")
}

fn text(o: &Output) -> String {
    format!("{}{}", String::from_utf8_lossy(&o.stdout), String::from_utf8_lossy(&o.stderr))
}

#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
async fn gmx_shows_adds_a_list_watches_it_and_turns_compositing_on() {
    let (dir, port) = support::folder("gmx-shows");
    let st = support::start(dir.clone(), port, &[]).await;
    let csv = dir.join("feeds.csv");
    let mut list = String::from("name,input,program,outputs\n");
    for i in 0..5 {
        list.push_str(&format!("Feed {i},udp://@127.0.0.1:{},,udp://127.0.0.1:{}\n", 27_000 + i, 28_000 + i));
    }
    std::fs::write(&csv, list).unwrap();
    let csv = csv.to_string_lossy().to_string();

    let dry = gmx(&st.url, &["add", "--from", &csv, "--dry-run"]);
    assert!(dry.status.success(), "{}", text(&dry));
    assert!(text(&dry).contains("would add 5 of 5 shows"), "{}", text(&dry));
    let listed = gmx(&st.url, &["list"]);
    assert!(!text(&listed).contains("feed-0"), "a dry run adds nothing: {}", text(&listed));

    let added = gmx(&st.url, &["add", "--from", &csv]);
    assert!(text(&added).contains("added 5 of 5 shows"), "{}", text(&added));
    let listed = gmx(&st.url, &["list"]);
    assert!(text(&listed).contains("feed-0"), "{}", text(&listed));

    let started = Instant::now();
    loop {
        let stats = gmx(&st.url, &["stats"]);
        let out = text(&stats);
        if stats.status.success() && out.lines().filter(|l| l.starts_with("feed-")).count() == 5 {
            assert!(out.starts_with("6 shows:"), "main and the five: {out}");
            break;
        }
        assert!(started.elapsed() < Duration::from_secs(30), "stats never listed the five: {out}");
        tokio::time::sleep(Duration::from_millis(500)).await;
    }

    let set = gmx(&st.url, &["set", "feed-0", "--compositing", "on"]);
    assert!(set.status.success(), "{}", text(&set));
    assert!(text(&set).contains("\"compositing\": true"), "{}", text(&set));
}
