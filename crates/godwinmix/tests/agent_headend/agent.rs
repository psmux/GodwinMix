//! An agent, as far as the MCP server can tell: a process on the other end
//! of `gmx mcp`'s stdin and stdout, sending one JSON-RPC request per line.
//!
//! It counts what an agent pays for: every tool call, and every byte that
//! crosses the pipe in each direction, because a model is charged for the
//! bytes it reads back.

use serde_json::{json, Value};
use std::time::{Duration, Instant};
use tokio::io::{AsyncBufReadExt, AsyncWriteExt, BufReader, Lines};
use tokio::process::{Child, ChildStdin, ChildStdout, Command};

pub const GMX: &str = env!("CARGO_BIN_EXE_gmx");

pub struct Agent {
    _child: Child,
    stdin: ChildStdin,
    lines: Lines<BufReader<ChildStdout>>,
    next_id: u64,
    pub tool_calls: u32,
    pub sent_bytes: usize,
    pub read_bytes: usize,
    pub started: Instant,
}

impl Agent {
    /// `gmx mcp --url <station>` on the headend profile, the one a headend's
    /// agent is set up with, which keeps the show tools in its list.
    pub fn connect(url: &str) -> Agent {
        let mut child = Command::new(GMX)
            .args(["mcp", "--profile", "headend", "--url", &format!("http://{url}")])
            .env_remove("GODWINMIX_TOKEN")
            .env_remove("GODWINMIX_MCP_PROFILE")
            .stdin(std::process::Stdio::piped())
            .stdout(std::process::Stdio::piped())
            .stderr(std::process::Stdio::null())
            .kill_on_drop(true)
            .spawn()
            .expect("gmx mcp starts");
        let stdin = child.stdin.take().unwrap();
        let lines = BufReader::new(child.stdout.take().unwrap()).lines();
        Agent {
            _child: child,
            stdin,
            lines,
            next_id: 1,
            tool_calls: 0,
            sent_bytes: 0,
            read_bytes: 0,
            started: Instant::now(),
        }
    }

    /// One request and its answer, within thirty seconds. Server initiated
    /// notifications on the way are read and counted, since an agent's
    /// client reads them too.
    pub async fn request(&mut self, method: &str, params: Value) -> Value {
        let id = self.next_id;
        self.next_id += 1;
        let mut line = json!({ "jsonrpc": "2.0", "id": id, "method": method, "params": params }).to_string();
        line.push('\n');
        self.sent_bytes += line.len();
        self.stdin.write_all(line.as_bytes()).await.unwrap();
        self.stdin.flush().await.unwrap();
        let wait = tokio::time::timeout(Duration::from_secs(30), async {
            while let Ok(Some(text)) = self.lines.next_line().await {
                self.read_bytes += text.len() + 1;
                let v: Value = serde_json::from_str(&text).unwrap_or_default();
                if v["id"] == json!(id) {
                    return v;
                }
            }
            Value::Null
        });
        wait.await.unwrap_or_else(|_| panic!("{method} did not answer within 30 s"))
    }

    /// A tools/call, answered with the JSON the tool's text carries, or a
    /// panic naming the refusal.
    pub async fn tool(&mut self, name: &str, arguments: Value) -> Value {
        self.tool_calls += 1;
        let r = self.request("tools/call", json!({ "name": name, "arguments": arguments })).await;
        let text = r["result"]["content"][0]["text"].as_str().unwrap_or_default().to_string();
        assert!(r["result"]["isError"] != true, "{name} was refused: {text}");
        serde_json::from_str(&text).unwrap_or(Value::String(text))
    }
}
