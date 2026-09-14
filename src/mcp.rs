//! `godwinmix mcp`: the mixer as a set of tools for an AI agent.
//!
//! A Model Context Protocol server over stdio. An MCP client (Claude Code,
//! Claude Desktop, anything that speaks the protocol) starts this binary as a
//! child process, writes JSON-RPC 2.0 requests one per line on stdin and reads
//! replies one per line on stdout. Nothing else may ever go to stdout, because
//! the client parses every byte of it as JSON; logging goes to stderr.
//!
//! Like `ctl`, this is a thin client for the HTTP API rather than a second
//! control path. Every tool is a request to a running mixer, so the agent and
//! the operator's browser see the same state and the same refusals. The tool
//! descriptions below are the agent's only manual for the mixer, which is why
//! they say when to use each tool and what comes back, not just what it does.

use anyhow::{Context, Result};
use base64::Engine;
use reqwest::header::CONTENT_TYPE;
use reqwest::{Method, StatusCode};
use serde_json::{json, Value};
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
}

/// Serve until stdin closes. EOF is how a client says goodbye, so it exits 0.
pub async fn run(url: &str, token: Option<String>) -> Result<()> {
    let server = Server::new(url, token);
    debug!(base = %server.base, "mcp server ready");
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
    pub fn new(url: &str, token: Option<String>) -> Self {
        Self {
            base: url.trim_end_matches('/').to_string(),
            token: token.filter(|t| !t.trim().is_empty()),
            client: reqwest::Client::new(),
        }
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
            "initialize" => Ok(initialize_result(&params)),
            "ping" => Ok(json!({})),
            "tools/list" => Ok(json!({ "tools": tool_list() })),
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
        let plan = match plan(name, args) {
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

    async fn execute(&self, plan: Plan) -> Result<Value, String> {
        let resp = self.send(&plan.method, &plan.path, plan.body.as_ref()).await?;
        let status = resp.status();
        if status == StatusCode::NOT_FOUND {
            match plan.missing {
                Missing::Explain(why) => return Err(why.to_string()),
                Missing::FallBackToStatus => {
                    let resp = self.send(&Method::GET, "/api/status", None).await?;
                    let status = resp.status();
                    let text = read_text(resp).await?;
                    if !status.is_success() {
                        return Err(format!("HTTP {status}: {}", text.trim()));
                    }
                    return Ok(text_result(format!(
                        "This mixer build has no /api/agent/state (404), so this is the full \
                         /api/status instead. It has no motion scores.\n{}",
                        render_body(&text)
                    )));
                }
                Missing::Refusal => {}
            }
        }
        if !status.is_success() {
            let text = read_text(resp).await.unwrap_or_default();
            return Err(format!("HTTP {status}: {}", text.trim()));
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
            return Ok(json!({
                "content": [{ "type": "image", "data": data, "mimeType": mime }]
            }));
        }
        let text = read_text(resp).await?;
        Ok(text_result(render_body(&text)))
    }

    async fn send(
        &self,
        method: &Method,
        path: &str,
        body: Option<&Value>,
    ) -> Result<reqwest::Response, String> {
        let url = format!("{}{path}", self.base);
        let mut req = self.client.request(method.clone(), &url);
        if let Some(token) = &self.token {
            req = req.bearer_auth(token);
        }
        if let Some(body) = body {
            req = req.json(body);
        }
        req.send()
            .await
            .map_err(|e| format!("could not reach the mixer at {url}: {e}"))
    }
}

async fn read_text(resp: reqwest::Response) -> Result<String, String> {
    resp.text().await.map_err(|e| format!("reading response: {e}"))
}

/// The API answers most commands with an empty 200. An empty string is a poor
/// thing to hand a language model, so it becomes a small JSON object; a JSON
/// body is reformatted so it reads well; anything else passes through.
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

fn initialize_result(params: &Value) -> Value {
    let asked = params.get("protocolVersion").and_then(Value::as_str);
    let version = match asked {
        Some(v) if KNOWN_PROTOCOLS.contains(&v) => v,
        _ => DEFAULT_PROTOCOL,
    };
    json!({
        "protocolVersion": version,
        "capabilities": { "tools": {} },
        "serverInfo": { "name": SERVER_NAME, "version": env!("CARGO_PKG_VERSION") },
        "instructions": "GodwinMix is a live video mixer: several sources come in, one is on \
            program at a time, and the program goes out to RTMP destinations without \
            interruption. Start with `status` or `agent_state` to learn the source ids, then \
            `take` to switch what is on air. Use `snapshot` to look at the pictures before \
            deciding. Every tool talks to the running mixer over its HTTP API, so refusals \
            come back verbatim as error results with the mixer's own reason."
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

/// What a 404 from the API means for a given tool.
enum Missing {
    /// The endpoint exists and 404 is its answer, for instance an unknown id.
    /// Reported like any other failure.
    Refusal,
    /// The endpoint is newer than the mixer build we are talking to. Say so
    /// in words the agent can act on.
    Explain(&'static str),
    /// Fetch /api/status instead and say that is what happened.
    FallBackToStatus,
}

/// A tool call worked out into an HTTP request, before anything is sent.
/// Separating this from sending is what lets the tests cover every tool
/// without a mixer to talk to.
struct Plan {
    method: Method,
    path: String,
    body: Option<Value>,
    missing: Missing,
    /// The response is a picture, to be returned as MCP image content.
    image: bool,
}

impl Plan {
    fn get(path: impl Into<String>) -> Self {
        Self { method: Method::GET, path: path.into(), body: None, missing: Missing::Refusal, image: false }
    }
    fn post(path: impl Into<String>, body: Value) -> Self {
        Self { method: Method::POST, path: path.into(), body: Some(body), missing: Missing::Refusal, image: false }
    }
    fn delete(path: impl Into<String>) -> Self {
        Self { method: Method::DELETE, path: path.into(), body: None, missing: Missing::Refusal, image: false }
    }
}

fn plan(name: &str, args: &Value) -> Result<Plan, String> {
    Ok(match name {
        "status" => Plan::get("/api/status"),
        "agent_state" => Plan { missing: Missing::FallBackToStatus, ..Plan::get("/api/agent/state") },
        "take" => {
            // Absent, null or an empty string all mean the slate.
            let source = args.get("source").and_then(Value::as_str).filter(|s| !s.is_empty());
            Plan::post(
                "/api/take",
                json!({ "source": source, "at_running_time_ms": opt_u64(args, "at_running_time_ms")? }),
            )
        }
        "add_source" => Plan::post(
            "/api/sources",
            json!({
                "id": opt_str(args, "id"),
                "name": opt_str(args, "name"),
                "uri": req_str(name, args, "uri")?,
                "kind": opt_str(args, "kind"),
                "superimpose": opt_str(args, "superimpose"),
            }),
        ),
        "remove_source" => Plan::delete(format!("/api/sources/{}", req_id(name, args, "id")?)),
        "list_outputs" => Plan::get("/api/outputs"),
        "add_output" => Plan::post(
            "/api/outputs",
            json!({
                "id": req_str(name, args, "id")?,
                "uri": req_str(name, args, "uri")?,
                "policy": opt_str(args, "policy").unwrap_or_else(|| "own".to_string()),
            }),
        ),
        "remove_output" => Plan::delete(format!("/api/outputs/{}", req_id(name, args, "id")?)),
        "reconnect_output" => {
            Plan::post(format!("/api/outputs/{}/reconnect", req_id(name, args, "id")?), json!({}))
        }
        "ad_break" => Plan::post(
            "/api/adbreak",
            json!({
                "uri": req_str(name, args, "uri")?,
                "at_running_time_ms": opt_u64(args, "at_running_time_ms")?,
                "return_to": opt_str(args, "return_to"),
            }),
        ),
        "end_ad_break" => Plan::post("/api/adbreak/end", json!({})),
        "list_media" => Plan::get("/api/media"),
        "snapshot" => {
            let what = req_id(name, args, "what")?;
            let mut path = format!("/api/snapshot/{what}.jpg");
            if let Some(w) = opt_u64(args, "width")? {
                path.push_str(&format!("?width={w}"));
            }
            Plan {
                missing: Missing::Explain(
                    "This mixer build has no snapshot endpoint (GET /api/snapshot/... returned \
                     404), so there is no picture to show. Use `status` or `agent_state` to \
                     reason about the sources instead.",
                ),
                image: true,
                ..Plan::get(path)
            }
        }
        "go_live" => Plan {
            missing: Missing::Explain(
                "This mixer build has no /api/golive (404). Do it in steps instead: `add_source` \
                 with kind \"web\" for the page, `add_output` for the RTMP destination, then \
                 `take` the new source.",
            ),
            ..Plan::post(
                "/api/golive",
                json!({
                    "url": req_str(name, args, "url")?,
                    "rtmp": req_str(name, args, "rtmp")?,
                    "superimpose": opt_str(args, "superimpose"),
                }),
            )
        },
        _ => {
            let known: Vec<String> =
                tool_list().iter().filter_map(|t| t["name"].as_str().map(String::from)).collect();
            return Err(format!("unknown tool {name:?}. Tools: {}", known.join(", ")));
        }
    })
}

fn opt_str(args: &Value, key: &str) -> Option<String> {
    args.get(key).and_then(Value::as_str).map(str::trim).filter(|s| !s.is_empty()).map(String::from)
}

fn req_str(tool: &str, args: &Value, key: &str) -> Result<String, String> {
    opt_str(args, key).ok_or_else(|| format!("{tool} needs a non-empty string `{key}`"))
}

/// An id that goes into a URL path. Slashes and query characters would turn
/// it into a different request, so they are refused rather than encoded.
fn req_id(tool: &str, args: &Value, key: &str) -> Result<String, String> {
    let id = req_str(tool, args, key)?;
    if id.contains(['/', '?', '#', '%']) {
        return Err(format!("{tool}: `{key}` {id:?} is not a valid id"));
    }
    Ok(id)
}

fn opt_u64(args: &Value, key: &str) -> Result<Option<u64>, String> {
    match args.get(key) {
        None | Some(Value::Null) => Ok(None),
        Some(v) => v.as_u64().map(Some).ok_or_else(|| format!("`{key}` must be a non-negative integer")),
    }
}

fn tool(name: &str, description: &str, properties: Value, required: &[&str]) -> Value {
    json!({
        "name": name,
        "description": description,
        "inputSchema": {
            "type": "object",
            "properties": properties,
            "required": required,
            "additionalProperties": false,
        }
    })
}

/// The agent reads these and nothing else, so each one says what the tool
/// does, when to reach for it and what it returns.
pub fn tool_list() -> Vec<Value> {
    let ms = json!({
        "type": "integer",
        "description": "Program running time, in milliseconds, to land this on. The mixer arms \
            it on the pipeline clock so it hits the intended frame. Omit for right now. Read the \
            current running time from `status` first."
    });
    vec![
        tool(
            "status",
            "Full snapshot of the mixer: what is on program, every source with its id, name, \
             URL, connection state and whether it currently has video and audio, every output \
             with its state and reconnect count, the encoder backend, and any ad break that is \
             armed or on air. Use it first to learn the source ids you can `take`, and after a \
             change to confirm it happened. Returns the JSON from GET /api/status.",
            json!({}),
            &[],
        ),
        tool(
            "agent_state",
            "Compact state written for agents: the program source, each source's state and a \
             motion score saying how much its picture is changing, so you can tell a live \
             camera from a frozen or black one without looking at it. Prefer this over `status` \
             when deciding what to put on air. If the mixer build lacks this endpoint the tool \
             says so and returns the full `status` instead, without motion scores.",
            json!({}),
            &[],
        ),
        tool(
            "take",
            "Put a source on program. The cut is instant and the outgoing stream is not \
             disturbed. Pass the source id from `status`; omit it or pass null to cut to black. \
             Use `at_running_time_ms` to schedule the cut on a frame instead of now. Returns \
             {\"ok\": true}, or an error with the mixer's reason such as an unknown id.",
            json!({
                "source": {
                    "type": ["string", "null"],
                    "description": "Id of the source to put on air. Null or omitted cuts to black."
                },
                "at_running_time_ms": ms,
            }),
            &[],
        ),
        tool(
            "add_source",
            "Add a source while the mixer is running. The protocol is worked out from the \
             URL: rtmp://, rtmps://, an https:// .m3u8 or .mpd manifest, rtsp://, srt://, \
             udp://, a file path or a media file URL all work. To show a web page (a YouTube \
             watch page, a scoreboard, a dashboard) pass its https:// URL with kind \"web\": \
             the mixer renders the page in a real browser, with its audio, and that picture is \
             the source. For web sources, superimpose \"auto\" makes the mixer find the page's \
             own video and decode it itself where it can, drawing the page over the top; this \
             saves about a CPU core and falls back to plain rendering without saying so when \
             the page gives nothing to hand over (YouTube and DRM pages do this). Omit id to \
             have one derived from the name or the host. The source starts connecting at once; \
             check `status` for its state before you `take` it. Returns {\"ok\": true}.",
            json!({
                "id": {
                    "type": "string",
                    "description": "Stable id used by `take` and `remove_source`. Lowercase letters, digits and dashes. Derived from the name or host when omitted."
                },
                "name": { "type": "string", "description": "Name shown to the operator. Defaults to the host of the URL." },
                "uri": { "type": "string", "description": "Stream URL, file path or, with kind \"web\", the address of a page." },
                "kind": {
                    "type": "string",
                    "enum": ["auto", "web"],
                    "description": "\"web\" renders the URL as a page. \"auto\" (the default) opens it as a stream or file according to the URL."
                },
                "superimpose": {
                    "type": "string",
                    "enum": ["off", "auto"],
                    "description": "Web sources only. \"auto\" lets the mixer decode the page's own video outside the browser when the page allows it. Default \"off\"."
                },
            }),
            &["uri"],
        ),
        tool(
            "remove_source",
            "Remove a source by id. If it is on program the mixer cuts to black first. \
             Returns {\"ok\": true}, or an error naming the problem.",
            json!({ "id": { "type": "string", "description": "Source id as listed by `status`." } }),
            &["id"],
        ),
        tool(
            "list_outputs",
            "List the RTMP destinations the program is being sent to, with each one's id, \
             URL, connection state, reconnect count and how many seconds are buffered. Use it \
             to check that the broadcast is actually reaching its destinations. Returns the \
             JSON array from GET /api/outputs.",
            json!({}),
            &[],
        ),
        tool(
            "add_output",
            "Start sending the program to another RTMP destination, for instance a YouTube or \
             Twitch ingest URL with the stream key on the end. The output encoder is shared, so \
             adding one costs nothing on air. `policy` sets the reconnect behaviour: \"own\" \
             (default) retries quickly, for servers you run; \"cdn\" backs off harder, for \
             public platforms that penalise hammering. Returns {\"ok\": true}.",
            json!({
                "id": { "type": "string", "description": "Stable id for this destination, for `remove_output` and `reconnect_output`." },
                "uri": { "type": "string", "description": "rtmp:// or rtmps:// URL including the stream key." },
                "policy": { "type": "string", "enum": ["own", "cdn"], "description": "Reconnect policy. Default \"own\"." },
            }),
            &["id", "uri"],
        ),
        tool(
            "remove_output",
            "Stop sending to a destination and forget it. Other outputs are unaffected. \
             Returns {\"ok\": true}.",
            json!({ "id": { "type": "string", "description": "Output id as listed by `list_outputs`." } }),
            &["id"],
        ),
        tool(
            "reconnect_output",
            "Drop and re-establish one destination's RTMP connection now, without waiting \
             for its reconnect policy. Use it when `list_outputs` shows an output stuck or \
             the platform reports no data arriving. Returns {\"ok\": true}.",
            json!({ "id": { "type": "string", "description": "Output id as listed by `list_outputs`." } }),
            &["id"],
        ),
        tool(
            "ad_break",
            "Interrupt the program with a clip, then rejoin live automatically when the clip \
             ends. The clip is a file path on the machine running the mixer or a URL; `list_media` \
             shows the clips in the mixer's library with their durations. There is no time \
             shift: whatever the live source did during the break is not shown afterwards. \
             `return_to` picks the source to rejoin, defaulting to whatever was on program. \
             `at_running_time_ms` schedules the break on a frame. Returns {\"ok\": true}.",
            json!({
                "uri": { "type": "string", "description": "Path or URL of the clip to play." },
                "at_running_time_ms": ms,
                "return_to": { "type": "string", "description": "Source id to rejoin after the clip. Defaults to the current program source." },
            }),
            &["uri"],
        ),
        tool(
            "end_ad_break",
            "Cut a running ad short and return to live now, or disarm one that is scheduled \
             and has not started. Returns {\"ok\": true}, or an error if no ad break is active.",
            json!({}),
            &[],
        ),
        tool(
            "list_media",
            "List the clips in the mixer's media library: each one's path, name, duration in \
             milliseconds and whether it has an audio track. Use it to find a `uri` for \
             `ad_break`. Returns the JSON from GET /api/media; if the library directory is not \
             configured or readable the JSON carries an `error` field saying why.",
            json!({}),
            &[],
        ),
        tool(
            "snapshot",
            "Look at the pictures. Returns a JPEG as image content you can view directly. \
             `what` is \"sheet\" for a contact sheet of every source and the program side by \
             side (the best first look), \"program\" for what is going out right now, or a \
             source id for that one source. Use it to check a source is showing the right \
             thing before you `take` it, or to confirm what viewers see. `width` scales the \
             image down; smaller is faster and cheaper to look at. If this mixer build has no \
             snapshot endpoint the tool returns a text error saying so.",
            json!({
                "what": { "type": "string", "description": "\"sheet\", \"program\", or a source id from `status`." },
                "width": { "type": "integer", "description": "Width in pixels to scale the image to. Omit for the native size." },
            }),
            &["what"],
        ),
        tool(
            "go_live",
            "One call to put a web page on air: add the page as a web source, add the RTMP \
             destination, and take the page to program. Use it when someone says \"stream this \
             page to that RTMP URL\" and nothing is set up yet. `superimpose` is the same option \
             as in `add_source`. Returns the mixer's JSON describing what it created. If this \
             mixer build lacks the endpoint the tool returns a text error telling you to do \
             the three steps with `add_source`, `add_output` and `take` instead.",
            json!({
                "url": { "type": "string", "description": "The https:// address of the page to render." },
                "rtmp": { "type": "string", "description": "rtmp:// or rtmps:// destination including the stream key." },
                "superimpose": { "type": "string", "enum": ["off", "auto"], "description": "\"auto\" lets the mixer decode the page's own video itself where it can." },
            }),
            &["url", "rtmp"],
        ),
    ]
}

#[cfg(test)]
mod tests {
    use super::*;

    fn server() -> Server {
        Server::new("http://127.0.0.1:1", None)
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
    }

    #[tokio::test]
    async fn initialize_falls_back_to_the_default_version_for_an_unknown_one() {
        let r = ask(r#"{"jsonrpc":"2.0","id":"a","method":"initialize","params":{"protocolVersion":"1999-01-01"}}"#).await;
        assert_eq!(r["id"], "a");
        assert_eq!(r["result"]["protocolVersion"], DEFAULT_PROTOCOL);
        // No version at all is treated the same way.
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

    #[tokio::test]
    async fn tools_list_names_every_tool_with_a_schema() {
        let r = ask(r#"{"jsonrpc":"2.0","id":3,"method":"tools/list"}"#).await;
        let tools = r["result"]["tools"].as_array().expect("tools array");
        let names: Vec<&str> = tools.iter().map(|t| t["name"].as_str().unwrap()).collect();
        for expected in [
            "status", "agent_state", "take", "add_source", "remove_source", "list_outputs",
            "add_output", "remove_output", "reconnect_output", "ad_break", "end_ad_break",
            "list_media", "snapshot", "go_live",
        ] {
            assert!(names.contains(&expected), "missing tool {expected}");
        }
        for t in tools {
            assert!(!t["description"].as_str().unwrap().is_empty(), "{} has no description", t["name"]);
            assert_eq!(t["inputSchema"]["type"], "object", "{} schema is not an object", t["name"]);
            assert!(t["inputSchema"]["properties"].is_object());
            assert!(t["inputSchema"]["required"].is_array());
            // Every plan must be reachable by name, so the two tables agree.
            let name = t["name"].as_str().unwrap();
            let args = json!({
                "id": "x", "uri": "rtmp://h/l/k", "what": "sheet", "url": "https://e.com", "rtmp": "rtmp://h/l/k"
            });
            assert!(plan(name, &args).is_ok(), "no plan for tool {name}");
        }
        // Names are unique.
        let mut sorted = names.clone();
        sorted.sort();
        sorted.dedup();
        assert_eq!(sorted.len(), names.len());
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
    }

    #[tokio::test]
    async fn a_missing_required_argument_is_an_error_result() {
        let r = ask(r#"{"jsonrpc":"2.0","id":9,"method":"tools/call","params":{"name":"add_source","arguments":{"name":"x"}}}"#).await;
        assert_eq!(r["result"]["isError"], true);
        assert!(r["result"]["content"][0]["text"].as_str().unwrap().contains("uri"));
    }

    #[tokio::test]
    async fn an_unreachable_mixer_is_an_error_result() {
        // Port 1 has nothing listening; the failure must come back as a tool
        // result the agent can read, not take the server down.
        let r = ask(r#"{"jsonrpc":"2.0","id":10,"method":"tools/call","params":{"name":"status"}}"#).await;
        assert_eq!(r["result"]["isError"], true);
        assert!(r["result"]["content"][0]["text"].as_str().unwrap().contains("could not reach"));
    }

    #[test]
    fn plans_match_the_http_api() {
        let p = plan("take", &json!({})).unwrap();
        assert_eq!(p.method, Method::POST);
        assert_eq!(p.path, "/api/take");
        assert_eq!(p.body, Some(json!({ "source": null, "at_running_time_ms": null })));

        let p = plan("take", &json!({ "source": "cam2", "at_running_time_ms": 1500 })).unwrap();
        assert_eq!(p.body, Some(json!({ "source": "cam2", "at_running_time_ms": 1500 })));
        assert!(plan("take", &json!({ "at_running_time_ms": -1 })).is_err());

        let p = plan("remove_source", &json!({ "id": "cam1" })).unwrap();
        assert_eq!((p.method, p.path.as_str()), (Method::DELETE, "/api/sources/cam1"));
        assert!(plan("remove_source", &json!({ "id": "../status" })).is_err());

        let p = plan("reconnect_output", &json!({ "id": "yt" })).unwrap();
        assert_eq!((p.method, p.path.as_str()), (Method::POST, "/api/outputs/yt/reconnect"));

        let p = plan("add_output", &json!({ "id": "yt", "uri": "rtmp://a/b" })).unwrap();
        assert_eq!(p.body.unwrap()["policy"], "own");

        let p = plan("snapshot", &json!({ "what": "program", "width": 640 })).unwrap();
        assert_eq!(p.path, "/api/snapshot/program.jpg?width=640");
        assert!(p.image);
        assert!(matches!(p.missing, Missing::Explain(_)));

        let p = plan("agent_state", &json!({})).unwrap();
        assert!(matches!(p.missing, Missing::FallBackToStatus));

        let p = plan("add_source", &json!({ "uri": "https://example.com", "kind": "web", "superimpose": "auto" })).unwrap();
        let body = p.body.unwrap();
        assert_eq!(body["kind"], "web");
        assert_eq!(body["superimpose"], "auto");
        assert!(body["id"].is_null());
    }

    #[test]
    fn empty_bodies_become_json_and_json_is_pretty_printed() {
        assert_eq!(render_body(""), "{\"ok\": true}");
        assert_eq!(render_body("{\"a\":1}"), "{\n  \"a\": 1\n}");
        assert_eq!(render_body("not json"), "not json");
    }
}
