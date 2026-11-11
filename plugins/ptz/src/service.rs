//! The plugin protocol's `Service`, and the command line mode that drives
//! cameras without a core: one tool call per line on stdin.

use std::io::BufRead;

use godwinmix_sdk::prelude::*;
use serde_json::{json, Value};

use crate::core::Core;
use crate::rig::Rig;
use crate::settings::Settings;
use crate::tools;

pub struct PtzService {
    pub env: PluginEnv,
    pub rig: Option<Rig>,
}

impl PtzService {
    fn rig(&mut self) -> &mut Rig {
        self.rig.get_or_insert_with(|| Rig::new(Settings::default(), None))
    }
}

impl Service for PtzService {
    fn initialize(&mut self, ready: &Ready, reporter: Reporter) -> Result<InitializeResult, RpcError> {
        let core = Core::from_url(&self.env.rpc, Some(self.env.token.clone()));
        if core.is_none() {
            reporter.info("no GMX_RPC in the environment, so there is no mixer to take pictures from. Cameras move; presets have no pictures and tracking is off.");
        }
        let rig = Rig::new(Settings::from_value(&ready.params), core);
        for p in &rig.problems {
            reporter.warn(p);
        }
        reporter.info(format!("{} camera(s): {}", rig.cameras.len(), rig.ids().join(", ")));
        self.rig = Some(rig);
        Ok(InitializeResult { latency_ms: Some(0) })
    }

    fn configure(&mut self, params: Value) -> Result<Configure, RpcError> {
        self.rig().configure(Settings::from_value(&params));
        Ok(Configure::applied())
    }

    fn health(&mut self) -> Health {
        let failing: Vec<String> = self
            .rig()
            .cameras
            .values()
            .filter_map(|c| c.handle.status.lock().unwrap().last_error.clone().map(|e| format!("{}: {e}", c.spec.id)))
            .collect();
        if failing.is_empty() {
            Health::ok()
        } else {
            Health::degraded(failing.join("; "))
        }
    }

    fn tool_call(&mut self, name: &str, arguments: Value) -> Result<ToolResult, RpcError> {
        tools::call(self.rig(), name, &arguments)
    }
}

/// `tool {json}` per line in, the answer as one JSON line out. Blank lines
/// and lines starting with `#` are skipped.
pub fn repl(rig: &mut Rig, input: impl BufRead, mut out: impl std::io::Write) {
    for line in input.lines().map_while(Result::ok) {
        let line = line.trim();
        if line.is_empty() || line.starts_with('#') {
            continue;
        }
        let (name, args) = line.split_once(char::is_whitespace).unwrap_or((line, "{}"));
        let answer = match serde_json::from_str::<Value>(args.trim()) {
            Ok(a) => match tools::call(rig, name, &a) {
                Ok(r) => json!({"tool": name, "text": r.content[0]["text"], "result": r.structured_content}),
                Err(e) => json!({"tool": name, "error": e.message, "data": e.data}),
            },
            Err(e) => json!({"tool": name, "error": format!("the arguments are not JSON: {e}. Example: move {{\"camera\": \"cam-1\", \"pan\": 0.5}}")}),
        };
        let _ = writeln!(out, "{answer}");
    }
}
