//! An encoder set up for Livebox, publishing unchanged to a channel moved
//! from it: `rtmp://<host>/Church/main?psk=<the password it already had>`.
//! A real ffmpeg on a real port; skipped where there is no ffmpeg.

use super::*;
use std::process::{Child, Command, Stdio};
use std::time::{Duration, Instant};

const PASSWORD: &str = "Sunday-2024";

fn table() -> Table {
    Table::from_params(&json!({"channels": [
        {"id": "church", "app": "Church", "enabled": true, "key_mode": "query",
         "keys": [{"id": "livebox", "secret": PASSWORD}]},
    ]}))
}

/// ffmpeg on the PATH, by its Windows name as well.
fn ffmpeg() -> Option<std::path::PathBuf> {
    let path = std::env::var_os("PATH")?;
    std::env::split_paths(&path)
        .flat_map(|d| [d.join("ffmpeg"), d.join("ffmpeg.exe")])
        .find(|c| c.is_file())
}

fn publish(url: &str) -> Option<Child> {
    Command::new(ffmpeg()?)
        .args(["-loglevel", "error", "-re", "-f", "lavfi", "-i", "testsrc2=size=320x240:rate=30"])
        .args(["-f", "lavfi", "-i", "sine=frequency=440", "-t", "4"])
        .args(["-c:v", "libx264", "-g", "15", "-preset", "ultrafast", "-c:a", "aac"])
        .args(["-f", "flv", url])
        .stdout(Stdio::null())
        .stderr(Stdio::null())
        .spawn()
        .ok()
}

fn wait_for(what: impl Fn() -> bool) -> bool {
    let until = Instant::now() + Duration::from_secs(8);
    while Instant::now() < until {
        if what() {
            return true;
        }
        std::thread::sleep(Duration::from_millis(50));
    }
    false
}

#[test]
fn an_encoder_set_up_for_livebox_is_let_in_unchanged() {
    let settings = Settings::from_params(&json!({"bind": "127.0.0.1", "rtmp_port": 0}));
    let device = Discover::start(&settings, table(), None).expect("the loopback has a free port");
    let port = device.port();
    let Some(mut livebox) = publish(&format!("rtmp://127.0.0.1:{port}/Church/main?psk={PASSWORD}")) else {
        eprintln!("skipping: no ffmpeg on PATH");
        return;
    };
    let mut lower = publish(&format!("rtmp://127.0.0.1:{port}/church/cam2?psk={PASSWORD}")).unwrap();
    let mut wrong = publish(&format!("rtmp://127.0.0.1:{port}/Church/main2?psk=Sunday-2025")).unwrap();
    let both = wait_for(|| device.hub().is_live("Church", "main") && device.hub().is_live("Church", "cam2"));
    let refused = wrong.wait().map(|s| !s.success()).unwrap_or(false);
    let _ = (livebox.kill(), lower.kill());
    let _ = (livebox.wait(), lower.wait());
    assert!(both, "Church/main and church/cam2 were both let in, under the channel's own spelling");
    assert!(refused, "a password the channel does not have is still turned away");
    assert!(!device.hub().is_live("Church", "main2"));
}
