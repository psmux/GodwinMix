//! The plugin protocol's `Service`, and the command line mode that drives
//! switchers without a core: one tool call per line on stdin.

use std::io::BufRead;
use std::sync::{Arc, Mutex};

use godwinmix_atem::Timing;
use godwinmix_sdk::prelude::*;
use serde_json::{json, Value};
use tokio::runtime::Runtime;

use crate::desk::Desk;
use crate::settings::Settings;
use crate::tools::{self, Shared};
use crate::{sync, Log};

pub struct AtemService {
    pub env: PluginEnv,
    pub desk: Shared,
    runtime: Runtime,
    syncing: bool,
}

/// Two threads carry every switcher's session and the follow and lead loop.
pub fn runtime() -> Runtime {
    tokio::runtime::Builder::new_multi_thread().worker_threads(2).thread_name("gmx-atem").enable_all().build().expect("a tokio runtime")
}

impl AtemService {
    pub fn new(env: PluginEnv) -> AtemService {
        let runtime = runtime();
        let desk = Arc::new(Mutex::new(Desk::new(runtime.handle().clone(), Timing::default())));
        AtemService { env, desk, runtime, syncing: false }
    }

    /// Start follow and lead against the core at `url`. Once.
    pub fn start_sync(&mut self, url: String, token: Option<String>, log: Arc<dyn Log>) {
        if !std::mem::replace(&mut self.syncing, true) {
            self.runtime.spawn(sync::run::run(self.desk.clone(), url, token, log));
        }
    }

    fn configure_from(&mut self, params: &Value) -> Vec<String> {
        self.desk.lock().unwrap_or_else(|e| e.into_inner()).configure(Settings::from_value(params))
    }
}

struct CoreLog(Reporter);

impl Log for CoreLog {
    fn info(&self, message: &str) {
        self.0.info(message);
    }
    fn warn(&self, message: &str) {
        self.0.warn(message);
    }
}

impl Service for AtemService {
    fn initialize(&mut self, ready: &Ready, reporter: Reporter) -> Result<InitializeResult, RpcError> {
        for p in self.configure_from(&ready.params) {
            reporter.warn(p);
        }
        if self.env.rpc.is_empty() {
            reporter.info("no GMX_RPC in the environment, so there is no core to follow or lead. The switchers answer their tools; follow and lead do nothing.");
        } else {
            let token = Some(self.env.token.clone()).filter(|t| !t.is_empty());
            self.start_sync(self.env.rpc.clone(), token, Arc::new(CoreLog(reporter.clone())));
        }
        let ids = self.desk.lock().unwrap_or_else(|e| e.into_inner()).switchers.ids();
        reporter.info(format!("{} switcher(s): {}", ids.len(), ids.join(", ")));
        Ok(InitializeResult { latency_ms: Some(0) })
    }

    fn configure(&mut self, params: Value) -> Result<Configure, RpcError> {
        let problems = self.configure_from(&params);
        if !problems.is_empty() {
            return Err(RpcError::new(codes::INVALID_PARAMS, problems.join("; ")));
        }
        Ok(Configure::applied())
    }

    fn health(&mut self) -> Health {
        let desk = self.desk.lock().unwrap_or_else(|e| e.into_inner());
        let down: Vec<String> = desk
            .switchers
            .iter()
            .map(|a| a.snapshot())
            .filter(|s| !s.link.is_up())
            .map(|s| format!("{}: {}", s.id, s.link.describe()))
            .collect();
        if down.is_empty() { Health::ok() } else { Health::degraded(down.join("; ")) }
    }

    fn tool_call(&mut self, name: &str, arguments: Value) -> Result<ToolResult, RpcError> {
        tools::call(&self.desk, name, &arguments)
    }
}

/// `tool {json}` per line in, the answer as one JSON line out. Blank lines
/// and lines starting with `#` are skipped.
pub fn repl(desk: &Shared, input: impl BufRead, mut out: impl std::io::Write) {
    for line in input.lines().map_while(Result::ok) {
        let line = line.trim();
        if line.is_empty() || line.starts_with('#') {
            continue;
        }
        let (name, args) = line.split_once(char::is_whitespace).unwrap_or((line, "{}"));
        let answer = match serde_json::from_str::<Value>(args.trim()) {
            Ok(a) => match tools::call(desk, name, &a) {
                Ok(r) => json!({"tool": name, "text": r.content[0]["text"], "result": r.structured_content}),
                Err(e) => json!({"tool": name, "error": e.message, "data": e.data}),
            },
            Err(e) => json!({"tool": name, "error": format!("the arguments are not JSON: {e}. Example: cut {{\"switcher\": \"atem-main\"}}")}),
        };
        let _ = writeln!(out, "{answer}");
    }
}
