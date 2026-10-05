//! A real station, or a single process core, started from the built binary
//! in a folder of its own, with no plugins so nothing opens a public port,
//! and taken down when the test is done. Every wait has a limit.

use futures_util::{SinkExt, StreamExt};
use serde_json::{json, Value};
use std::path::PathBuf;
use std::process::{Child, Command, Stdio};
use std::time::{Duration, Instant};
use tokio_tungstenite::tungstenite::Message;

pub const BIN: &str = env!("CARGO_BIN_EXE_godwinmix");

pub struct Running {
    child: Child,
    pub url: String,
    pub dir: PathBuf,
    _turn: Turn,
}

/// Stations share the machine, except a test that needs the governor to
/// find room: it waits until no other station runs, and none starts until
/// it is done. A four core runner running a handful of stations at once had
/// no CPU free, and the governor refused even a sound decode for four
/// minutes, which is the governor being right about a machine that is full.
static MACHINE: std::sync::RwLock<()> = std::sync::RwLock::new(());

thread_local! {
    /// Stations this thread holds a share for. A test that starts a second
    /// station while it has one must not queue behind a waiting writer.
    static SHARES: std::cell::Cell<usize> = const { std::cell::Cell::new(0) };
}

/// Held for what dropping it releases, never read.
#[allow(dead_code)]
enum Turn {
    Shared(Option<std::sync::RwLockReadGuard<'static, ()>>),
    Alone(std::sync::RwLockWriteGuard<'static, ()>),
}

impl Turn {
    fn shared() -> Turn {
        let first = SHARES.with(|n| {
            n.set(n.get() + 1);
            n.get() == 1
        });
        Turn::Shared(first.then(|| MACHINE.read().unwrap_or_else(|e| e.into_inner())))
    }

    fn alone() -> Turn {
        Turn::Alone(MACHINE.write().unwrap_or_else(|e| e.into_inner()))
    }
}

impl Drop for Turn {
    fn drop(&mut self) {
        if let Turn::Shared(_) = self {
            SHARES.with(|n| n.set(n.get().saturating_sub(1)));
        }
    }
}

impl Drop for Running {
    fn drop(&mut self) {
        #[cfg(unix)]
        unsafe {
            libc::kill(self.child.id() as i32, libc::SIGINT);
        }
        let start = Instant::now();
        while start.elapsed() < Duration::from_secs(20) {
            if let Ok(Some(_)) = self.child.try_wait() {
                return;
            }
            std::thread::sleep(Duration::from_millis(100));
        }
        let _ = self.child.kill();
        let _ = self.child.wait();
    }
}

impl Running {
    #[cfg(unix)]
    pub fn pid(&self) -> u32 {
        self.child.id()
    }
}

fn free_port() -> u16 {
    std::net::TcpListener::bind("127.0.0.1:0").unwrap().local_addr().unwrap().port()
}

/// A folder with the first run config, bound to a free loopback port and
/// reading plugins from an empty folder.
pub fn folder(name: &str) -> (PathBuf, u16) {
    let dir = std::env::temp_dir().join(format!("gmx-station-test-{name}-{}", std::process::id()));
    let _ = std::fs::remove_dir_all(&dir);
    std::fs::create_dir_all(dir.join("plugins")).unwrap();
    let port = free_port();
    let example = Command::new(BIN).arg("--example-config").output().unwrap().stdout;
    let text = String::from_utf8(example).unwrap();
    let text = text
        .replace("bind = \"0.0.0.0:8080\"", &format!("bind = \"127.0.0.1:{port}\""))
        .replace("# plugins_dir = \"~/.godwinmix/plugins\"", &format!("plugins_dir = {:?}", dir.join("plugins")));
    std::fs::write(dir.join("godwinmix.toml"), text).unwrap();
    (dir, port)
}

/// Start the binary on `dir`'s config with `extra` flags, and wait until it
/// answers `core.status`.
pub async fn start(dir: PathBuf, port: u16, extra: &[&str]) -> Running {
    launch(dir, port, extra, Turn::shared()).await
}

/// [`start`], with no other station running on the machine until it is
/// dropped.
pub async fn start_alone(dir: PathBuf, port: u16, extra: &[&str]) -> Running {
    launch(dir, port, extra, Turn::alone()).await
}

async fn launch(dir: PathBuf, port: u16, extra: &[&str], turn: Turn) -> Running {
    let mut cmd = Command::new(BIN);
    cmd.arg("--config").arg(dir.join("godwinmix.toml")).args(extra).args(["--log-format", "json"]);
    cmd.env("GODWINMIX_RUNTIME_DIR", dir.join("runtime")).env_remove("GODWINMIX_TOKEN");
    // Its own secret store, so an output key a test seals stays in its folder.
    cmd.env("GODWINMIX_HOME", dir.join("home"));
    cmd.stdout(Stdio::null()).stderr(Stdio::from(std::fs::File::create(dir.join("log.jsonl")).unwrap()));
    #[cfg(unix)]
    std::os::unix::process::CommandExt::process_group(&mut cmd, 0);
    let running = Running { child: cmd.spawn().unwrap(), url: format!("127.0.0.1:{port}"), dir, _turn: turn };
    let started = Instant::now();
    loop {
        if get(&running, "/api/v1/core/status").await.get("uptime_secs").is_some() {
            return running;
        }
        assert!(started.elapsed() < Duration::from_secs(90), "not up after 90 s; see {}", running.dir.join("log.jsonl").display());
        tokio::time::sleep(Duration::from_millis(200)).await;
    }
}

pub async fn get(r: &Running, path: &str) -> Value {
    let url = format!("http://{}{path}", r.url);
    match reqwest::Client::new().get(url).timeout(Duration::from_secs(20)).send().await {
        Ok(answer) => answer.json().await.unwrap_or(Value::Null),
        Err(_) => Value::Null,
    }
}

pub type Ws = tokio_tungstenite::WebSocketStream<tokio_tungstenite::MaybeTlsStream<tokio::net::TcpStream>>;

pub async fn rpc(r: &Running, query: &str) -> Ws {
    let (ws, _) = tokio_tungstenite::connect_async(format!("ws://{}/rpc{query}", r.url)).await.unwrap();
    ws
}

/// One call, skipping notifications, answered within twenty seconds.
pub async fn call(ws: &mut Ws, id: u64, method: &str, params: Value) -> Value {
    let frame = json!({"jsonrpc": "2.0", "id": id, "method": method, "params": params});
    ws.send(Message::Text(frame.to_string().into())).await.unwrap();
    let answer = tokio::time::timeout(Duration::from_secs(20), async {
        while let Some(Ok(m)) = ws.next().await {
            let Message::Text(t) = m else { continue };
            let v: Value = serde_json::from_str(t.as_str()).unwrap();
            if v.get("id") == Some(&json!(id)) {
                return v;
            }
        }
        Value::Null
    });
    answer.await.unwrap_or_else(|_| panic!("{method} did not answer within 20 s"))
}

/// The next `event/<name>` whose params pass `test`, within `wait`.
pub async fn event(ws: &mut Ws, name: &str, wait: Duration, test: impl Fn(&Value) -> bool) -> Option<Value> {
    let method = format!("event/{name}");
    tokio::time::timeout(wait, async {
        while let Some(Ok(m)) = ws.next().await {
            let Message::Text(t) = m else { continue };
            let v: Value = serde_json::from_str(t.as_str()).unwrap_or_default();
            if v["method"] == method.as_str() && test(&v["params"]) {
                return Some(v["params"].clone());
            }
        }
        None
    })
    .await
    .ok()
    .flatten()
}

/// The middle of a set of timings, in microseconds.
pub fn median(mut v: Vec<u128>) -> u128 {
    v.sort_unstable();
    v[v.len() / 2]
}
