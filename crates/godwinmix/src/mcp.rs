//! `godwinmix mcp`: the mixer as a set of tools for an AI agent.
//!
//! A Model Context Protocol server over stdio. An MCP client (Claude Code,
//! Claude Desktop, Codex, Cursor, anything that speaks the protocol) starts
//! this binary as a child process, writes JSON-RPC 2.0 requests one per line
//! on stdin and reads replies one per line on stdout. Nothing else may ever go
//! to stdout, because the client parses every byte of it as JSON; logging goes
//! to stderr.
//!
//! There is no list of tools in this file. Every tool is a method in
//! `godwinmix_protocol` that carries an MCP binding, its input schema is that method's
//! params schema, and its annotations are read off the same flags the server
//! enforces. Adding a method adds a tool; nothing here has to be edited.
//!
//! Two profiles, because a tool list is charged for on every single call.
//! `standard` is at most twelve hot tools and `minimal` is five, with the rest
//! behind `search_tools`. The hot list is a pure function of the profile, so
//! adding a source or a plugin never invalidates a client's prompt cache.

use godwinmix_protocol::mcp_tools;
use godwinmix_protocol::method::{rest_transform, Registry};
use godwinmix_protocol::scope::Profile;
use crate::control::call::Call;
use anyhow::{Context, Result};
use base64::Engine;
use reqwest::header::CONTENT_TYPE;
use reqwest::{Method, StatusCode};
use serde_json::{json, Map, Value};
use tokio::io::{AsyncBufReadExt, AsyncWriteExt, BufReader};
use tracing::{debug, warn};

const SERVER_NAME: &str = "godwinmix";
/// Offered when the client asks for a revision we have not heard of. The
/// client then either accepts it or disconnects; either is better than
/// pretending to speak something we do not.
const DEFAULT_PROTOCOL: &str = "2025-06-18";
/// Revisions whose initialize, tools/list and tools/call shapes match what
/// this file produces. A client asking for one of these gets it echoed back.
const KNOWN_PROTOCOLS: &[&str] = &["2024-11-05", "2025-03-26", "2025-06-18"];

// JSON-RPC 2.0 error codes.
const PARSE_ERROR: i64 = -32700;
const INVALID_REQUEST: i64 = -32600;
const METHOD_NOT_FOUND: i64 = -32601;
const INVALID_PARAMS: i64 = -32602;

pub struct Server {
    base: String,
    token: Option<String>,
    client: reqwest::Client,
    registry: Registry<Call>,
    profile: Profile,
}

/// Serve until stdin closes. EOF is how a client says goodbye, so it exits 0.
pub async fn run(url: &str, token: Option<String>, profile: Profile) -> Result<()> {
    let server = Server::new(url, token, profile);
    debug!(base = %server.base, profile = profile.as_str(), "mcp server ready");
    let mut lines = BufReader::new(tokio::io::stdin()).lines();
    let mut stdout = tokio::io::stdout();
    while let Some(line) = lines.next_line().await.context("reading stdin")? {
        if line.trim().is_empty() {
            continue;
        }
        if let Some(reply) = server.handle(&line).await {
            let mut text = serde_json::to_string(&reply).context("encoding reply")?;
            text.push('\n');
            stdout.write_all(text.as_bytes()).await.context("writing stdout")?;
            stdout.flush().await.context("flushing stdout")?;
        }
    }
    debug!("stdin closed, exiting");
    Ok(())
}

impl Server {
    pub fn new(url: &str, token: Option<String>, profile: Profile) -> Self {
        Self {
            base: url.trim_end_matches('/').to_string(),
            token: token.filter(|t| !t.trim().is_empty()),
            client: reqwest::Client::new(),
            registry: crate::control::methods::registry(),
            profile,
        }
    }

    /// The hot list this client is shown.
    pub fn tools(&self) -> Vec<Value> {
        mcp_tools::tools(&self.registry, self.profile)
    }

    /// One line in, at most one line out. Notifications (no `id`) never get a
    /// reply, whatever their method, because JSON-RPC forbids answering them.
    pub async fn handle(&self, line: &str) -> Option<Value> {
        let req: Value = match serde_json::from_str(line) {
            Ok(v) => v,
            Err(e) => {
                return Some(error_reply(Value::Null, PARSE_ERROR, format!("invalid JSON: {e}")))
            }
        };
        let id = req.get("id").cloned();
        let method = req.get("method").and_then(Value::as_str);
        let params = req.get("params").cloned().unwrap_or(Value::Null);
        let Some(method) = method else {
            return Some(error_reply(id.unwrap_or(Value::Null), INVALID_REQUEST, "missing method"));
        };
        let Some(id) = id else {
            debug!(method, "notification");
            return None;
        };
        debug!(method, "request");
        let result = match method {
            "initialize" => Ok(initialize_result(&params, self.profile)),
            "ping" => Ok(json!({})),
            "tools/list" => Ok(json!({ "tools": self.tools() })),
            "tools/call" => match params.get("name").and_then(Value::as_str) {
                Some(name) => {
                    let args = params.get("arguments").cloned().unwrap_or_else(|| json!({}));
                    Ok(self.call(name, &args).await)
                }
                None => Err((INVALID_PARAMS, "tools/call needs params.name".to_string())),
            },
            _ => Err((METHOD_NOT_FOUND, format!("unknown method {method}"))),
        };
        Some(match result {
            Ok(result) => json!({ "jsonrpc": "2.0", "id": id, "result": result }),
            Err((code, message)) => error_reply(id, code, message),
        })
    }

    /// Run one tool. Anything that goes wrong, from a misspelled tool name to
    /// the mixer refusing, is a result with `isError` rather than a protocol
    /// error: the agent is expected to read it and try something else.
    async fn call(&self, name: &str, args: &Value) -> Value {
        if name == mcp_tools::SEARCH_TOOL {
            return self.search(args).await;
        }
        // A plugin's tool, if the mixer has one by that name. Routed through
        // `tool.call`, which reaches the plugin process and brings the answer
        // back in MCP's own shape. Nothing about this is in the hot list, so
        // installing a plugin never changes what an agent is shown.
        if name.starts_with("gmx_") && mcp_tools::method_for(&self.registry, name).is_none() {
            return self.plugin_tool(name, args).await;
        }
        let plan = match self.plan(name, args) {
            Ok(p) => p,
            Err(msg) => return error_result(msg),
        };
        match self.execute(plan).await {
            Ok(v) => v,
            Err(msg) => {
                warn!(tool = name, %msg, "tool call failed");
                error_result(msg)
            }
        }
    }

    /// `search_tools`: everything that is not in the hot list, found by what
    /// the agent is trying to do rather than by name.
    async fn search(&self, args: &Value) -> Value {
        let query = args.get("query").and_then(Value::as_str).unwrap_or_default();
        if query.trim().is_empty() {
            return error_result(
                "search_tools needs a `query`: what you are trying to do, in plain words, \
                 such as \"stop sending to youtube\" or \"play a clip\"."
                    .to_string(),
            );
        }
        let limit = args.get("limit").and_then(Value::as_u64).unwrap_or(5) as usize;
        let mut found = mcp_tools::search(&self.registry, query, limit);
        // Whatever the plugins installed on this mixer contribute, ranked the
        // same way and shown in the same list. This is the only route to a
        // plugin's tools, which is what keeps the hot list a fixed shape and
        // the prompt cache valid across an install.
        found.extend(self.plugin_tools(query, limit).await);
        if found.is_empty() {
            let names: Vec<&str> = self
                .registry
                .iter()
                .filter_map(|m| m.mcp.as_ref().map(|b| b.tool))
                .collect();
            return text_result(format!(
                "Nothing matches {query:?}. Every tool this mixer has: {}.",
                names.join(", ")
            ));
        }
        let text = serde_json::to_string_pretty(&json!({ "tools": found }))
            .unwrap_or_else(|_| "{}".into());
        text_result(format!(
            "Call any of these by name with tools/call. They are not in your tool list, and \
             they do not need to be.\n{text}"
        ))
    }

    /// The plugin tools this mixer has, matching a query.
    ///
    /// Asked of the mixer rather than held here, because the MCP server is a
    /// separate process and a plugin can be installed while it runs. A mixer
    /// that cannot be reached contributes nothing rather than failing the
    /// search: the core tools are still worth finding.
    async fn plugin_tools(&self, query: &str, limit: usize) -> Vec<Value> {
        let Ok(listing) = self.get_json("/api/v1/plugins").await else { return Vec::new() };
        let words: Vec<String> =
            query.to_lowercase().split_whitespace().map(str::to_string).collect();
        let mut found = Vec::new();
        for plugin in listing["plugins"].as_array().cloned().unwrap_or_default() {
            if !plugin["enabled"].as_bool().unwrap_or(false) {
                continue;
            }
            let name = plugin["name"].as_str().unwrap_or("").to_string();
            let description = plugin["description"].as_str().unwrap_or("").to_lowercase();
            for tool in plugin["tools"].as_array().cloned().unwrap_or_default() {
                let Some(tool) = tool.as_str() else { continue };
                let haystack = format!("{tool} {name} {description}");
                if words.iter().any(|w| haystack.contains(w.as_str())) {
                    found.push(json!({
                        "name": tool,
                        "description": format!(
                            "A tool from the `{name}` plugin. Call it by name with \
                             tools/call; `describe_plugin {{name: \"{name}\"}}` says what \
                             arguments it takes."
                        ),
                        "plugin": name,
                    }));
                }
                if found.len() >= limit {
                    return found;
                }
            }
        }
        found
    }

    /// Call one plugin's tool through the mixer's `tool.call`.
    ///
    /// The MCP name is resolved by asking the mixer which plugin declares it,
    /// rather than by taking `gmx_<plugin>_<tool>` apart here. A plugin name
    /// may carry a hyphen and an MCP tool name may not, so the two halves
    /// cannot be told apart from the string alone: `gmx_colour_bars_draw` is
    /// `colour-bars/draw` or `colour/bars_draw` and only the mixer knows which.
    async fn plugin_tool(&self, name: &str, args: &Value) -> Value {
        let Some(qualified) = self.resolve_plugin_tool(name).await else {
            return error_result(format!(
                "there is no tool {name:?} on this mixer. Use search_tools to find one by \
                 what it does, or list_plugins to see what is installed."
            ));
        };
        let body = json!({ "name": qualified, "arguments": args });
        let url = format!("{}/api/v1/tool/call", self.base);
        let mut request = self.client.post(&url).json(&body);
        if let Some(token) = &self.token {
            request = request.bearer_auth(token);
        }
        match request.send().await {
            Err(e) => error_result(format!("could not reach the mixer at {}: {e}", self.base)),
            Ok(response) => match response.json::<Value>().await {
                Ok(v) if v.get("error").is_some() => error_result(
                    v["error"]["message"].as_str().unwrap_or("the mixer refused").to_string(),
                ),
                Ok(v) => v,
                Err(e) => error_result(format!("the mixer's answer was not JSON: {e}")),
            },
        }
    }

    /// Which plugin declares this MCP tool name, as `<plugin>/<tool>`.
    async fn resolve_plugin_tool(&self, name: &str) -> Option<String> {
        let listing = self.get_json("/api/v1/plugins").await.ok()?;
        for plugin in listing["plugins"].as_array()? {
            if !plugin["enabled"].as_bool().unwrap_or(false) {
                continue;
            }
            let declared = plugin["tools"].as_array()?;
            if !declared.iter().any(|t| t.as_str() == Some(name)) {
                continue;
            }
            let plugin_name = plugin["name"].as_str()?;
            // The tool's own name is what is left after the prefix the loader
            // put on: `gmx_` plus the plugin's name with hyphens as
            // underscores, plus one more underscore.
            let prefix = format!("gmx_{}_", plugin_name.replace('-', "_"));
            let tool = name.strip_prefix(&prefix)?;
            return Some(format!("{plugin_name}/{tool}"));
        }
        None
    }

    /// One GET against the mixer, as JSON.
    async fn get_json(&self, path: &str) -> Result<Value, String> {
        let mut request = self.client.get(format!("{}{path}", self.base));
        if let Some(token) = &self.token {
            request = request.bearer_auth(token);
        }
        let response = request.send().await.map_err(|e| e.to_string())?;
        response.json::<Value>().await.map_err(|e| e.to_string())
    }

    /// A tool call worked out into an HTTP request against `/api/v1`.
    ///
    /// The route comes from the same transform the server builds its router
    /// from, so a tool cannot point at a path that does not exist.
    fn plan(&self, tool: &str, args: &Value) -> Result<Plan, String> {
        let Some(method) = mcp_tools::method_for(&self.registry, tool) else {
            let names: Vec<&str> =
                self.registry.iter().filter_map(|m| m.mcp.as_ref().map(|b| b.tool)).collect();
            return Err(format!(
                "there is no tool {tool:?}. Tools: {}. Use search_tools to find one by what \
                 it does.",
                names.join(", ")
            ));
        };
        // The table wins: two methods carry bytes rather than JSON and sit at
        // a path of their own, and the table is where that is written down.
        let rest = self
            .registry
            .get(method)
            .and_then(|m| m.rest.clone())
            .or_else(|| rest_transform(method))
            .ok_or_else(|| format!("{tool} has no HTTP route"))?;
        let mut args = match args {
            Value::Object(map) => map.clone(),
            _ => Map::new(),
        };
        // `{id}` in the path is filled from the argument of that name, or from
        // `name` where the method calls it that.
        let path = if rest.path.contains("{id}") {
            let id = args
                .get("id")
                .or_else(|| args.get("name"))
                .and_then(Value::as_str)
                .map(str::trim)
                .filter(|s| !s.is_empty())
                .ok_or_else(|| format!("{tool} needs a non-empty string `id`"))?
                .to_string();
            if id.contains(['/', '?', '#', '%']) {
                return Err(format!("{tool}: `id` {id:?} is not a valid id"));
            }
            args.remove("id");
            rest.path.replace("{id}", &id)
        } else {
            rest.path.clone()
        };
        let verb = Method::from_bytes(rest.http.as_bytes())
            .map_err(|_| format!("{} is not an HTTP method", rest.http))?;
        let args = Value::Object(args);
        // Caught here as well as at the server, because a model that forgot an
        // argument should be told which one rather than being told the mixer
        // could not be reached, which is what a missing argument looks like
        // when the mixer happens to be down too.
        self.check_required(tool, method, &args, &path)?;
        let image = tool == "snapshot";
        Ok(Plan { verb, path, args, image })
    }

    /// Every required property of the tool's own input schema, present and
    /// not empty. The schema is the one the agent was shown, so this refuses
    /// exactly what the agent was told to send.
    fn check_required(
        &self,
        tool: &str,
        method: &str,
        args: &Value,
        path: &str,
    ) -> Result<(), String> {
        let Some(def) = mcp_tools::all_tools(&self.registry).into_iter().find(|t| t["name"] == tool)
        else {
            return Ok(());
        };
        let required = def["inputSchema"]["required"].as_array().cloned().unwrap_or_default();
        let missing: Vec<String> = required
            .iter()
            .filter_map(Value::as_str)
            // An id that has already gone into the path is not missing.
            .filter(|key| !(*key == "id" && !path.contains("{id}")))
            .filter(|key| {
                !args
                    .get(*key)
                    .map(|v| !v.is_null() && v.as_str().is_none_or(|s| !s.trim().is_empty()))
                    .unwrap_or(false)
            })
            .map(String::from)
            .collect();
        if missing.is_empty() {
            return Ok(());
        }
        Err(format!(
            "{tool} needs {}. Call it again with {} filled in; the schema is on the tool, \
             and {method} rejects it for the same reason.",
            missing.iter().map(|m| format!("`{m}`")).collect::<Vec<_>>().join(" and "),
            missing.join(" and ")
        ))
    }

    async fn execute(&self, plan: Plan) -> Result<Value, String> {
        let url = format!("{}{}", self.base, plan.path);
        let mut req = self.client.request(plan.verb.clone(), &url);
        if let Some(token) = &self.token {
            req = req.bearer_auth(token);
        }
        if plan.verb == Method::GET {
            if let Some(map) = plan.args.as_object() {
                let query: Vec<(String, String)> = map
                    .iter()
                    .map(|(k, v)| {
                        (k.clone(), v.as_str().map(String::from).unwrap_or_else(|| v.to_string()))
                    })
                    .collect();
                req = req.query(&query);
            }
        } else {
            req = req.json(&plan.args);
        }
        let resp = req
            .send()
            .await
            .map_err(|e| format!("could not reach the mixer at {url}: {e}"))?;
        let status = resp.status();
        if !status.is_success() {
            let text = resp.text().await.unwrap_or_default();
            return Err(refusal(status, &text));
        }
        if plan.image {
            let mime = resp
                .headers()
                .get(CONTENT_TYPE)
                .and_then(|v| v.to_str().ok())
                .unwrap_or("image/jpeg")
                .to_string();
            let bytes = resp.bytes().await.map_err(|e| format!("reading image: {e}"))?;
            let data = base64::engine::general_purpose::STANDARD.encode(&bytes);
            return Ok(json!({ "content": [{ "type": "image", "data": data, "mimeType": mime }] }));
        }
        let text = resp.text().await.map_err(|e| format!("reading response: {e}"))?;
        Ok(text_result(render_body(&text)))
    }
}

/// One tool call worked out into a request, before anything is sent.
/// Separating this from sending is what lets the tests cover every tool
/// without a mixer to talk to.
#[derive(Debug)]
struct Plan {
    verb: Method,
    path: String,
    args: Value,
    /// The answer is a picture, to be returned as MCP image content.
    image: bool,
}

/// The mixer's own sentence, out of the one error shape.
///
/// Those messages name the current state and the next step, which is exactly
/// what an agent needs, so they are passed through rather than summarised.
fn refusal(status: StatusCode, text: &str) -> String {
    match serde_json::from_str::<Value>(text) {
        Ok(v) => {
            let message = v["error"]["message"].as_str().unwrap_or(text.trim());
            let data = &v["error"]["data"];
            if data.is_object() {
                format!("{message}\n{}", serde_json::to_string(data).unwrap_or_default())
            } else {
                message.to_string()
            }
        }
        Err(_) => format!("HTTP {status}: {}", text.trim()),
    }
}

/// An empty string is a poor thing to hand a language model, so it becomes a
/// small JSON object; a JSON body is reformatted so it reads well; anything
/// else passes through.
fn render_body(text: &str) -> String {
    let trimmed = text.trim();
    if trimmed.is_empty() {
        return "{\"ok\": true}".to_string();
    }
    match serde_json::from_str::<Value>(trimmed) {
        Ok(v) => serde_json::to_string_pretty(&v).unwrap_or_else(|_| trimmed.to_string()),
        Err(_) => trimmed.to_string(),
    }
}

fn initialize_result(params: &Value, profile: Profile) -> Value {
    let asked = params.get("protocolVersion").and_then(Value::as_str);
    let version = match asked {
        Some(v) if KNOWN_PROTOCOLS.contains(&v) => v,
        _ => DEFAULT_PROTOCOL,
    };
    json!({
        "protocolVersion": version,
        "capabilities": { "tools": {} },
        "serverInfo": { "name": SERVER_NAME, "version": env!("CARGO_PKG_VERSION") },
        "instructions": format!(
            "GodwinMix is a live video mixer: several sources come in, one is on programme at \
             a time, and the programme goes out to RTMP destinations without interruption. \
             Start with `agent_state` to learn the source ids and how much each picture is \
             moving, then `take` to switch what is on air. `snapshot` shows you the pictures \
             when a number is not enough. You are on the {} tool profile; anything not in \
             your list is reachable through `search_tools` and can be called by name. Every \
             tool talks to the running mixer over its HTTP API, so refusals come back \
             verbatim with the mixer's own reason and the next step to take. Mutating tools \
             accept an `idempotency_key`, so a retry after a timeout is free; destructive \
             ones accept `dry_run: true`, which answers what would change without changing it.",
            profile.as_str()
        )
    })
}

fn error_reply(id: Value, code: i64, message: impl Into<String>) -> Value {
    json!({
        "jsonrpc": "2.0",
        "id": id,
        "error": { "code": code, "message": message.into() }
    })
}

fn text_result(text: String) -> Value {
    json!({ "content": [{ "type": "text", "text": text }] })
}

fn error_result(text: String) -> Value {
    json!({ "content": [{ "type": "text", "text": text }], "isError": true })
}

#[cfg(test)]
mod tests {
    use super::*;

    fn server() -> Server {
        Server::new("http://127.0.0.1:1", None, Profile::Standard)
    }

    async fn ask(line: &str) -> Value {
        server().handle(line).await.expect("a request gets a reply")
    }

    #[tokio::test]
    async fn initialize_echoes_a_known_version_and_names_the_server() {
        let r = ask(r#"{"jsonrpc":"2.0","id":1,"method":"initialize","params":{"protocolVersion":"2025-03-26","capabilities":{},"clientInfo":{"name":"t","version":"0"}}}"#).await;
        assert_eq!(r["jsonrpc"], "2.0");
        assert_eq!(r["id"], 1);
        assert_eq!(r["result"]["protocolVersion"], "2025-03-26");
        assert!(r["result"]["capabilities"]["tools"].is_object());
        assert_eq!(r["result"]["serverInfo"]["name"], "godwinmix");
        assert!(r.get("error").is_none());
        // The instructions tell the agent which surface it has and how to
        // reach the rest of it.
        let instructions = r["result"]["instructions"].as_str().unwrap();
        assert!(instructions.contains("standard"), "{instructions}");
        assert!(instructions.contains("search_tools"), "{instructions}");
    }

    #[tokio::test]
    async fn initialize_falls_back_to_the_default_version_for_an_unknown_one() {
        let r = ask(r#"{"jsonrpc":"2.0","id":"a","method":"initialize","params":{"protocolVersion":"1999-01-01"}}"#).await;
        assert_eq!(r["id"], "a");
        assert_eq!(r["result"]["protocolVersion"], DEFAULT_PROTOCOL);
        let r = ask(r#"{"jsonrpc":"2.0","id":2,"method":"initialize"}"#).await;
        assert_eq!(r["result"]["protocolVersion"], DEFAULT_PROTOCOL);
    }

    #[tokio::test]
    async fn notifications_get_no_reply() {
        let s = server();
        assert!(s.handle(r#"{"jsonrpc":"2.0","method":"notifications/initialized"}"#).await.is_none());
        // Even an unknown notification stays silent; answering it would be a
        // protocol violation.
        assert!(s.handle(r#"{"jsonrpc":"2.0","method":"notifications/whatever"}"#).await.is_none());
    }

    #[tokio::test]
    async fn ping_answers_with_an_empty_result() {
        let r = ask(r#"{"jsonrpc":"2.0","id":7,"method":"ping"}"#).await;
        assert_eq!(r["result"], json!({}));
    }

    /// Every hot tool has a schema, an honest set of annotations, and a plan
    /// that resolves. The list itself is generated, so this is checking the
    /// generator rather than a list somebody maintained.
    #[tokio::test]
    async fn tools_list_is_generated_and_every_tool_is_callable() {
        let s = server();
        let r = ask(r#"{"jsonrpc":"2.0","id":3,"method":"tools/list"}"#).await;
        let tools = r["result"]["tools"].as_array().expect("tools array");
        let names: Vec<&str> = tools.iter().map(|t| t["name"].as_str().unwrap()).collect();
        for expected in ["take", "list_sources", "agent_state", "add_source", "search_tools"] {
            assert!(names.contains(&expected), "missing tool {expected}: {names:?}");
        }
        for t in tools {
            let name = t["name"].as_str().unwrap();
            assert!(!t["description"].as_str().unwrap().is_empty(), "{name} has no description");
            assert_eq!(t["inputSchema"]["type"], "object", "{name} schema is not an object");
            assert!(t["annotations"]["readOnlyHint"].is_boolean(), "{name} has no annotations");
            if name == mcp_tools::SEARCH_TOOL {
                continue;
            }
            let args = json!({ "id": "cam1", "uri": "rtmp://h/l/k", "url": "https://e.com" });
            assert!(s.plan(name, &args).is_ok(), "no route for tool {name}");
        }
        // Names are unique.
        let mut sorted = names.clone();
        sorted.sort();
        sorted.dedup();
        assert_eq!(sorted.len(), names.len());
    }

    /// Installing a plugin does not change the hot list, in either profile.
    ///
    /// This is the one that keeps the prompt cache valid. Modifying any tool
    /// definition invalidates the whole cache, so a mixer whose tool list grew
    /// every time a plugin was installed would pay full price on every call
    /// afterwards. A plugin's tools live behind `search_tools` for exactly
    /// this reason, and this test is what stops somebody helpfully promoting
    /// one into the list.
    #[tokio::test]
    async fn installing_a_plugin_changes_nothing_in_the_hot_list() {
        let plugins = std::env::temp_dir().join(format!("gmx-mcp-hot-{}", std::process::id()));
        let _ = std::fs::remove_dir_all(&plugins);
        let dir = plugins.join("noisy").join("0.1.0");
        std::fs::create_dir_all(&dir).expect("a plugin directory");
        std::fs::write(dir.join("settings.json"), r#"{"type":"object"}"#).expect("settings");
        std::fs::write(dir.join("input.json"), r#"{"type":"object"}"#).expect("a tool schema");
        std::fs::write(dir.join("run.sh"), "#!/bin/sh
exit 0
").expect("an entry point");
        std::fs::write(
            dir.join("gmx-plugin.toml"),
            r#"
[plugin]
name = "noisy"
version = "0.1.0"
api = 1
description = "A plugin that contributes several tools, to prove they stay out of the hot list."
license = "MIT"
platforms = ["linux-x86_64", "linux-aarch64", "macos-aarch64", "macos-x86_64"]
placements = ["sidecar"]

[run]
shell = "run.sh"

[[provides]]
kind = "source"
id = "source"
media = { video = "raw", audio = "none" }
transports = ["container"]
settings = "settings.json"

[[tools]]
name = "list_senders"
description = "List every sender on the network. Example: list_senders {} returns a list."
input = "input.json"

[[tools]]
name = "rescan"
description = "Scan the network again. Example: rescan {} returns how many were found."
input = "input.json"
"#,
        )
        .expect("a manifest");

        for profile in [Profile::Standard, Profile::Minimal] {
            let s = Server::new("http://127.0.0.1:1", None, profile);
            let before: Vec<String> = s
                .tools()
                .iter()
                .map(|t| serde_json::to_string(t).expect("a tool serialises"))
                .collect();

            godwinmix_core::plugin::loader::set_dir(plugins.clone());
            let installed = godwinmix_core::plugin::loader::load_all(&Default::default());
            assert_eq!(installed.len(), 1, "the plugin loaded: {installed:#?}");
            assert_eq!(
                installed[0].tools,
                vec!["gmx_noisy_list_senders", "gmx_noisy_rescan"],
                "and it contributed two tools"
            );

            let after: Vec<String> = s
                .tools()
                .iter()
                .map(|t| serde_json::to_string(t).expect("a tool serialises"))
                .collect();
            assert_eq!(
                before, after,
                "the {} hot list changed when a plugin was installed, which throws away \
                 the prompt cache on every call afterwards",
                profile.as_str()
            );
            godwinmix_core::plugin::loader::remove("noisy");
        }
        let _ = std::fs::remove_dir_all(&plugins);
    }

    /// The minimal profile is five tools and no more, and the way out is in
    /// the list.
    #[tokio::test]
    async fn the_minimal_profile_is_five_tools_with_a_way_out() {
        let s = Server::new("http://127.0.0.1:1", None, Profile::Minimal);
        let tools = s.tools();
        assert_eq!(tools.len(), 5, "minimal is five tools: {tools:#?}");
        let names: Vec<&str> = tools.iter().map(|t| t["name"].as_str().unwrap()).collect();
        // Ordered by the method name behind each tool, so two runs of the same
        // build give byte identical output and a prompt cache survives a
        // reconnect. search_tools is last, because it is the way out.
        assert_eq!(
            names,
            vec!["agent_state", "take", "add_source", "list_sources", "search_tools"]
        );
        // A tool that is not in the list is still callable by name.
        assert!(s.plan("remove_output", &json!({ "id": "yt" })).is_ok());
    }

    /// Searching finds a tool by what it does, not only by its name, and says
    /// enough about it to call it.
    #[tokio::test]
    async fn search_tools_finds_what_is_not_in_the_hot_list() {
        let s = Server::new("http://127.0.0.1:1", None, Profile::Minimal);
        let r = s.call(mcp_tools::SEARCH_TOOL, &json!({ "query": "reconnect an output" })).await;
        assert!(r.get("isError").is_none(), "{r:#?}");
        let text = r["content"][0]["text"].as_str().unwrap();
        assert!(text.contains("reconnect_output"), "{text}");
        assert!(text.contains("inputSchema"), "a match has to carry its schema: {text}");

        // Plain words, no tool name in them at all.
        let r = s.call(mcp_tools::SEARCH_TOOL, &json!({ "query": "play a clip then rejoin live" })).await;
        let text = r["content"][0]["text"].as_str().unwrap();
        assert!(text.contains("ad_break"), "{text}");

        // Nothing at all still tells the agent what exists.
        let r = s.call(mcp_tools::SEARCH_TOOL, &json!({ "query": "zzzzqqq" })).await;
        let text = r["content"][0]["text"].as_str().unwrap();
        assert!(text.contains("Nothing matches"), "{text}");
        // And an empty query is a mistake worth naming.
        let r = s.call(mcp_tools::SEARCH_TOOL, &json!({})).await;
        assert_eq!(r["isError"], true);
    }

    /// A model that forgot an argument is told which one, before anything is
    /// sent, because "could not reach the mixer" is what a missing argument
    /// would otherwise look like when the mixer is down as well.
    #[tokio::test]
    async fn a_missing_required_argument_is_an_error_result() {
        let r = ask(r#"{"jsonrpc":"2.0","id":9,"method":"tools/call","params":{"name":"add_source","arguments":{"name":"x"}}}"#).await;
        assert_eq!(r["result"]["isError"], true);
        let text = r["result"]["content"][0]["text"].as_str().unwrap();
        assert!(text.contains("uri"), "{text}");
        assert!(!text.contains("could not reach"), "it never left the process: {text}");

        // An empty string is missing too, not a value.
        let s = server();
        assert!(s.plan("add_source", &json!({ "uri": "  " })).is_err());
        // And an id in the path is not missing from the body.
        assert!(s.plan("remove_source", &json!({ "id": "cam1" })).is_ok());
    }

    #[tokio::test]
    async fn unknown_methods_are_method_not_found() {
        let r = ask(r#"{"jsonrpc":"2.0","id":4,"method":"resources/list"}"#).await;
        assert_eq!(r["id"], 4);
        assert_eq!(r["error"]["code"], METHOD_NOT_FOUND);
        assert!(r.get("result").is_none());
    }

    #[tokio::test]
    async fn bad_json_and_missing_method_are_protocol_errors() {
        let r = ask("{this is not json").await;
        assert_eq!(r["error"]["code"], PARSE_ERROR);
        assert!(r["id"].is_null());
        let r = ask(r#"{"jsonrpc":"2.0","id":5}"#).await;
        assert_eq!(r["error"]["code"], INVALID_REQUEST);
        let r = ask(r#"{"jsonrpc":"2.0","id":6,"method":"tools/call","params":{}}"#).await;
        assert_eq!(r["error"]["code"], INVALID_PARAMS);
    }

    #[tokio::test]
    async fn calling_an_unknown_tool_is_an_error_result_not_a_protocol_error() {
        let r = ask(r#"{"jsonrpc":"2.0","id":8,"method":"tools/call","params":{"name":"explode","arguments":{}}}"#).await;
        assert!(r.get("error").is_none());
        assert_eq!(r["result"]["isError"], true);
        let text = r["result"]["content"][0]["text"].as_str().unwrap();
        assert!(text.contains("explode") && text.contains("take"), "{text}");
        assert!(text.contains("search_tools"), "the way out has to be named: {text}");
    }

    #[tokio::test]
    async fn an_unreachable_mixer_is_an_error_result() {
        // Port 1 has nothing listening; the failure must come back as a tool
        // result the agent can read, not take the server down.
        let r = ask(r#"{"jsonrpc":"2.0","id":10,"method":"tools/call","params":{"name":"status"}}"#).await;
        assert_eq!(r["result"]["isError"], true);
        assert!(r["result"]["content"][0]["text"].as_str().unwrap().contains("could not reach"));
    }

    /// Plans go to `/api/v1`, at the paths the transform rule produces.
    #[test]
    fn plans_match_the_versioned_api() {
        let s = server();
        let at = |tool: &str, args: Value| {
            let p = s.plan(tool, &args).unwrap();
            format!("{} {}", p.verb, p.path)
        };
        assert_eq!(at("take", json!({})), "POST /api/v1/program/take");
        assert_eq!(at("list_sources", json!({})), "GET /api/v1/sources");
        assert_eq!(at("add_source", json!({ "uri": "rtmp://h/l" })), "POST /api/v1/sources");
        assert_eq!(at("remove_source", json!({ "id": "cam1" })), "DELETE /api/v1/sources/cam1");
        assert_eq!(
            at("reconnect_output", json!({ "id": "yt" })),
            "POST /api/v1/outputs/yt/reconnect"
        );
        assert_eq!(at("agent_state", json!({})), "GET /api/v1/agent/state");
        assert_eq!(at("snapshot", json!({ "id": "program" })), "GET /api/v1/snapshot/program");

        // The id moves from the body to the path, so the body carries only the
        // rest of the arguments.
        let p = s.plan("snapshot", &json!({ "id": "program", "width": 640 })).unwrap();
        assert!(p.image);
        assert_eq!(p.args["width"], 640);
        assert!(p.args.get("id").is_none());

        // An id that would change the route is refused rather than encoded.
        assert!(s.plan("remove_source", &json!({ "id": "../status" })).is_err());
        assert!(s.plan("remove_source", &json!({})).is_err());
    }

    /// The mixer's refusal is what the agent reads: the sentence and the data
    /// beside it, not an HTTP status code.
    #[test]
    fn a_refusal_reaches_the_agent_with_its_next_step_intact() {
        let body = json!({
            "error": {
                "code": -32004,
                "message": "there is no source 'cam9'. The sources: cam1, cam2. Use one of those.",
                "data": { "valid": ["cam1", "cam2"], "retryable": false }
            }
        });
        let text = refusal(StatusCode::NOT_FOUND, &serde_json::to_string(&body).unwrap());
        assert!(text.contains("cam9") && text.contains("cam1, cam2"), "{text}");
        assert!(text.contains("\"retryable\":false"), "the data rides along: {text}");
        // A legacy route's plain text answer still reads.
        let text = refusal(StatusCode::BAD_REQUEST, "no such source cam9");
        assert!(text.contains("no such source cam9"), "{text}");
    }

    #[test]
    fn empty_bodies_become_json_and_json_is_pretty_printed() {
        assert_eq!(render_body(""), "{\"ok\": true}");
        assert_eq!(render_body("{\"a\":1}"), "{\n  \"a\": 1\n}");
        assert_eq!(render_body("not json"), "not json");
    }
}
