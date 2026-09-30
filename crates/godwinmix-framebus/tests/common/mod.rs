//! Helpers for tests that need a second process. The test binary starts
//! itself again with `FRAMEBUS_CHILD` set and runs only `child_entry`, which
//! plays the role named there and prints `RESULT key=value ...` lines.

#![allow(dead_code)]

use std::collections::HashMap;
use std::io::{BufRead, BufReader};
use std::path::PathBuf;
use std::process::{Child, ChildStdout, Command, Stdio};
use std::time::{Duration, Instant};

use godwinmix_framebus::{
    BusName, Format, Layout, Publisher, PublisherOptions, Registry, Subscriber,
};

pub mod roles;

pub fn registry() -> Registry {
    use std::sync::atomic::{AtomicU32, Ordering};
    static N: AtomicU32 = AtomicU32::new(0);
    let n = N.fetch_add(1, Ordering::Relaxed);
    let dir = PathBuf::from(format!("/tmp/fbt-{}-{n}", std::process::id()));
    let _ = std::fs::remove_dir_all(&dir);
    Registry::new(dir).unwrap()
}

pub fn name() -> BusName {
    BusName::camera("test-cam").unwrap()
}

pub fn small() -> Layout {
    Layout::new(Format::Nv12, 640, 360).unwrap()
}

pub fn publisher(reg: &Registry, layout: Layout, max_readers: usize, leases: usize) -> Publisher {
    let opts = PublisherOptions {
        max_readers,
        leases_per_reader: leases,
        checksum: true,
    };
    Publisher::create(reg, &name(), layout, opts).unwrap()
}

/// Fill a frame so that it differs everywhere from its neighbours.
pub fn paint(seq: u64, bytes: &mut [u8]) {
    bytes.fill(seq as u8);
    for (i, chunk) in bytes.chunks_mut(4096).enumerate() {
        chunk[..8].copy_from_slice(&(seq.wrapping_mul(31) + i as u64).to_le_bytes());
    }
}

pub fn subscribe(reg: &Registry) -> Subscriber {
    let deadline = Instant::now() + Duration::from_secs(5);
    loop {
        match Subscriber::connect(reg, &name()) {
            Ok(s) => return s,
            Err(_) if Instant::now() < deadline => std::thread::sleep(Duration::from_millis(20)),
            Err(e) => panic!("{e}"),
        }
    }
}

pub struct Kid {
    pub child: Child,
    lines: BufReader<ChildStdout>,
}

/// Start this test binary again as `role`, with the registry's directory.
pub fn spawn(role: &str, reg: &Registry, args: &str) -> Kid {
    let mut child = Command::new(std::env::current_exe().unwrap())
        .args(["--exact", "child_entry", "--nocapture", "--test-threads=1"])
        .env("FRAMEBUS_CHILD", role)
        .env("FRAMEBUS_ARGS", args)
        .env("GODWINMIX_BUS_DIR", reg.dir())
        .stdout(Stdio::piped())
        .spawn()
        .unwrap();
    let lines = BufReader::new(child.stdout.take().unwrap());
    Kid { child, lines }
}

impl Kid {
    /// The next `RESULT` line, as a map.
    pub fn result(&mut self) -> HashMap<String, String> {
        let mut line = String::new();
        loop {
            line.clear();
            if self.lines.read_line(&mut line).unwrap() == 0 {
                panic!("the child exited without a RESULT line");
            }
            // libtest prints "test child_entry ... " with no newline first.
            if let Some(rest) = line.trim().split_once("RESULT ").map(|(_, r)| r) {
                return rest
                    .split(' ')
                    .filter_map(|kv| kv.split_once('='))
                    .map(|(k, v)| (k.to_string(), v.to_string()))
                    .collect();
            }
        }
    }

    pub fn kill9(&mut self) {
        // SAFETY: a signal to our own child.
        unsafe { libc::kill(self.child.id() as i32, libc::SIGKILL) };
        let _ = self.child.wait();
    }
}

pub fn num(m: &HashMap<String, String>, k: &str) -> u64 {
    m.get(k)
        .unwrap_or_else(|| panic!("no {k} in {m:?}"))
        .parse()
        .unwrap()
}

/// Run by `child_entry` in every test binary.
pub fn child_main() {
    let Ok(role) = std::env::var("FRAMEBUS_CHILD") else {
        return;
    };
    let args = std::env::var("FRAMEBUS_ARGS").unwrap_or_default();
    let reg = Registry::from_env().unwrap();
    roles::run(&role, &args, &reg);
    std::process::exit(0);
}
