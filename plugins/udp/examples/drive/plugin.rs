//! The plugin process as the core sees it: JSON lines in on stdin, answers
//! and its `initialize` on stderr, read on a thread and kept by id.

use serde_json::{json, Value};
use std::collections::HashMap;
use std::io::{BufRead, BufReader, Write};
use std::process::{Child, ChildStdin, Command, Stdio};
use std::sync::{Arc, Condvar, Mutex};
use std::time::{Duration, Instant};

#[derive(Default)]
pub struct Answers {
    hello: Option<Value>,
    by_id: HashMap<u64, Value>,
}

pub type Shared = Arc<(Mutex<Answers>, Condvar)>;

/// Keep one line from the plugin: its `initialize`, or an answer by id.
pub fn take(line: &str, shared: &Shared) {
    let Ok(msg) = serde_json::from_str::<Value>(line) else { return };
    let (lock, cv) = &**shared;
    let mut a = lock.lock().unwrap();
    if msg["method"] == "initialize" {
        a.hello = Some(msg);
    } else if let (Some(id), None) = (msg["id"].as_u64(), msg.get("method")) {
        a.by_id.insert(id, msg);
    }
    cv.notify_all();
}

/// Wait up to ten seconds for `pick` to find something.
pub fn wait<T>(shared: &Shared, mut pick: impl FnMut(&mut Answers) -> Option<T>) -> Option<T> {
    let (lock, cv) = &**shared;
    let end = Instant::now() + Duration::from_secs(10);
    let mut a = lock.lock().unwrap();
    loop {
        if let Some(v) = pick(&mut a) {
            return Some(v);
        }
        let left = end.checked_duration_since(Instant::now())?;
        a = cv.wait_timeout(a, left.min(Duration::from_millis(100))).unwrap().0;
    }
}

pub struct Plugin {
    child: Child,
    stdin: ChildStdin,
    answers: Shared,
    next: u64,
}

impl Plugin {
    pub fn spawn(binary: &str, out: Option<&str>) -> Result<Plugin, String> {
        let root = std::path::Path::new(binary).parent().and_then(|p| p.parent()).unwrap_or(std::path::Path::new("."));
        let stdout = match out {
            Some(path) => Stdio::from(std::fs::File::create(path).map_err(|e| format!("could not write {path}: {e}"))?),
            None => Stdio::null(),
        };
        let mut child = Command::new(binary)
            .envs([("GMX_PLUGIN", "udp"), ("GMX_PROVIDE", "source"), ("GMX_INSTANCE", "drive")])
            .env("GMX_PLUGIN_ROOT", root)
            .stdin(Stdio::piped())
            .stdout(stdout)
            .stderr(Stdio::piped())
            .spawn()
            .map_err(|e| format!("could not start {binary}: {e}. Stage it with ./build first."))?;
        let answers: Shared = Arc::default();
        let (stderr, shared) = (child.stderr.take().expect("piped"), answers.clone());
        std::thread::spawn(move || BufReader::new(stderr).lines().map_while(Result::ok).for_each(|l| take(&l, &shared)));
        let stdin = child.stdin.take().expect("piped");
        Ok(Plugin { child, stdin, answers, next: 10 })
    }

    pub fn pid(&self) -> u32 {
        self.child.id()
    }

    fn send(&mut self, v: &Value) {
        let _ = writeln!(self.stdin, "{v}");
        let _ = self.stdin.flush();
    }

    /// Answer its `initialize` and start the source.
    pub fn handshake(&mut self, params: Value) -> Result<(), String> {
        wait(&self.answers, |a| a.hello.take()).ok_or("the plugin did not say initialize within 10 s")?;
        let canvas = json!({ "width": 1920, "height": 1080, "fps": 30 });
        self.send(&json!({ "jsonrpc": "2.0", "id": 0, "result": {
            "core": "drive", "version": "0", "api_level": 1, "api_compatible": 1, "canvas": canvas,
            "transport": "container", "media": "", "instance": "drive", "provide": "source", "params": params }}));
        let started = self.call("start", json!({ "canvas": canvas, "transport": "container", "media": "" }));
        match started.get("error") {
            Some(e) => Err(format!("start was refused: {e}")),
            None => Ok(()),
        }
    }

    /// One request and its answer, or `{}` after ten seconds.
    pub fn call(&mut self, method: &str, params: Value) -> Value {
        self.next += 1;
        let id = self.next;
        self.send(&json!({ "jsonrpc": "2.0", "id": id, "method": method, "params": params }));
        wait(&self.answers, |a| a.by_id.remove(&id)).unwrap_or(json!({}))
    }

    pub fn wait(mut self) {
        drop(self.stdin);
        let end = Instant::now() + Duration::from_secs(10);
        while Instant::now() < end && matches!(self.child.try_wait(), Ok(None)) {
            std::thread::sleep(Duration::from_millis(50));
        }
        let _ = self.child.kill();
    }
}

/// A JSON object with its keys in the order given, one space of indent, as
/// the summary has always been printed.
pub fn ordered_json(fields: &[(&str, Value)]) -> String {
    let mut out = String::from("{\n");
    for (i, (k, v)) in fields.iter().enumerate() {
        // serde_json indents by two; every line after the first is halved,
        // then moved in one for the object this value sits in.
        let pretty = serde_json::to_string_pretty(v).expect("a Value serialises");
        let text = pretty
            .lines()
            .enumerate()
            .map(|(n, l)| {
                let body = l.trim_start_matches(' ');
                if n == 0 { body.to_string() } else { format!("{} {body}", " ".repeat((l.len() - body.len()) / 2)) }
            })
            .collect::<Vec<_>>()
            .join("\n");
        out.push_str(&format!(" \"{k}\": {text}{}\n", if i + 1 < fields.len() { "," } else { "" }));
    }
    out.push('}');
    out
}

#[cfg(test)]
#[path = "plugin_tests.rs"]
mod tests;
