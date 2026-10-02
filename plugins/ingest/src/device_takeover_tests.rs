//! An RTMP publisher that stops sending without hanging up (a laptop put to
//! sleep, a network that went away) gives its name to the one that comes
//! back, with real ffmpeg publishers on the real listener. See `hub::takeover`.

use super::*;
use std::process::{Child, Command, Stdio};
use std::time::{Duration, Instant};

fn table() -> Table {
    Table::from_params(&json!({"channels": [
        {"id": "church", "app": "church", "enabled": true, "key_mode": "query",
         "keys": [{"id": "obs", "secret": "s3cret"}]},
    ]}))
}

fn which(program: &str) -> Option<std::path::PathBuf> {
    let path = std::env::var_os("PATH")?;
    std::env::split_paths(&path).map(|d| d.join(program)).find(|c| c.is_file())
}

/// Thirty seconds of a small test picture, published to `url` in real time.
fn publish(url: &str) -> Option<Child> {
    Command::new(which("ffmpeg")?)
        .args(["-loglevel", "error", "-re", "-f", "lavfi", "-i", "testsrc2=size=320x240:rate=30", "-t", "30"])
        .args(["-c:v", "libx264", "-g", "15", "-preset", "ultrafast", "-f", "flv", url])
        .stdout(Stdio::null())
        .stderr(Stdio::null())
        .spawn()
        .ok()
}

fn wait_for(limit: Duration, mut what: impl FnMut() -> bool) -> bool {
    let until = Instant::now() + limit;
    while Instant::now() < until {
        if what() {
            return true;
        }
        std::thread::sleep(Duration::from_millis(50));
    }
    false
}

fn signal(child: &Child, sig: i32) {
    // SAFETY: a signal to a process this test started.
    unsafe { libc::kill(child.id() as i32, sig) };
}

fn from(device: &Discover) -> String {
    device.hub().stream("church", "main").map(|s| s["from"].as_str().unwrap_or("").to_string()).unwrap_or_default()
}

#[test]
fn an_rtmp_publisher_that_stopped_sending_gives_its_name_to_the_one_that_came_back() {
    let settings = Settings::from_params(&json!({"bind": "127.0.0.1", "rtmp_port": 0}));
    let device = Discover::start(&settings, table(), None).expect("the loopback has a free port");
    let url = format!("rtmp://127.0.0.1:{}/church/main?psk=s3cret", device.port());
    let Some(mut asleep) = publish(&url) else {
        eprintln!("skipping: no ffmpeg on PATH");
        return;
    };
    assert!(wait_for(Duration::from_secs(8), || device.hub().is_live("church", "main")), "the first went live");
    let first = from(&device);

    // Still sending: a second publisher is refused and the first stays.
    let mut second = publish(&url).expect("ffmpeg ran once already");
    std::thread::sleep(Duration::from_secs(1));
    assert_eq!(from(&device), first, "a live publisher is not taken over");
    let refused = wait_for(Duration::from_secs(5), || matches!(second.try_wait(), Ok(Some(_))));
    assert!(refused, "the second publisher was turned away");

    // Asleep: the connection stays open and nothing more arrives.
    signal(&asleep, libc::SIGSTOP);
    let paused = Instant::now();
    // As long as a page takes to reload: past `HESITATE`, short of `STALE`,
    // so the newcomer waits for the name rather than being turned away.
    std::thread::sleep(crate::hub::HESITATE + Duration::from_millis(300));
    let mut back = publish(&url).expect("ffmpeg ran once already");
    let took = wait_for(Duration::from_secs(10), || {
        let now = from(&device);
        !now.is_empty() && now != first
    });
    let after = paused.elapsed();
    println!("the publisher that came back was on air {after:?} after the first stopped sending");
    let still_live = device.hub().is_live("church", "main");

    for child in [&mut asleep, &mut back, &mut second] {
        signal(child, libc::SIGCONT);
        let _ = child.kill();
        let _ = child.wait();
    }
    assert!(took, "the publisher that came back never got the name");
    assert!(after < crate::hub::STALE + Duration::from_secs(4), "it took {after:?}");
    assert!(still_live);
}
