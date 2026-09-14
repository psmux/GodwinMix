//! `godwinmix ctl`: drive a running mixer from the command line.
//!
//! A thin client over `/api/v1` rather than a second control path. Every
//! command builds one of the request types from `godwinmix_protocol` and reads the
//! answer back into an api type, so nothing here assembles a body by hand and
//! the CLI cannot drift from the protocol the UI and an agent use.
//!
//! The paths are not written down either: `api::method::rest_transform` turns
//! a method name into its route, which is the same function the server builds
//! its router from.

use godwinmix_protocol::method::rest_transform;
use godwinmix_protocol::types::{MixerStatus, OutputStatus, SourceStatus};
use godwinmix_protocol::{
    AddOutputRequest, AddSourceRequest, AdBreakRequest, CoreInfo, GoLiveRequest, GoLiveResult,
    ProgramState, TakeRecord, TakeRequest,
};
use godwinmix_core::media::MediaListing;
use anyhow::{bail, Context, Result};
use clap::Subcommand;
use reqwest::header::{HeaderMap, HeaderValue, AUTHORIZATION};
use serde::de::DeserializeOwned;
use serde::Serialize;
use serde_json::{json, Value};

#[derive(Subcommand, Debug)]
pub enum Ctl {
    /// Show what is on air, which sources are live and how the outputs are doing.
    Status {
        /// Print the raw JSON instead of a summary.
        #[arg(long)]
        json: bool,
    },
    /// Put a scene or a source on program. With no name, the armed scene goes
    /// on air; with nothing armed either, the programme cuts to black.
    Take {
        /// A source id or a scene name. Sources are looked at first.
        name: Option<String>,
        /// Read the name as a scene even when a source shares it.
        #[arg(long)]
        scene: bool,
        /// Land the cut on this program running time, in milliseconds.
        #[arg(long)]
        at: Option<u64>,
    },
    /// Take back to the shot that was on air before this one.
    Revert,
    /// What has been on air, newest first, and who put it there.
    History {
        #[arg(long, default_value_t = 20)]
        limit: u32,
    },
    /// Version, api level, features and limits of the mixer you are talking to.
    Info,
    /// List, add and remove sources.
    #[command(subcommand)]
    Source(SourceCmd),
    /// List, add and remove RTMP destinations.
    #[command(subcommand)]
    Output(OutputCmd),
    /// Build and drive scenes on a running mixer.
    #[command(subcommand)]
    Scene(SceneCmd),
    /// Interrupt the programme with a clip, then rejoin live.
    Ad {
        /// Path or URI of the clip.
        uri: String,
        /// Program running time to open the break on, in milliseconds.
        #[arg(long)]
        at: Option<u64>,
        /// Source to rejoin. Defaults to whatever is on program.
        #[arg(long)]
        return_to: Option<String>,
    },
    /// Cut the current ad short.
    EndAd,
    /// List the clips available in the media library.
    Media,
    /// Put a web page on air in one go: add it as a source, add the RTMP
    /// destination if given, and take it to programme once it renders.
    Golive {
        /// The page to put on air.
        url: String,
        /// RTMP destination. Added as an output unless one already sends there.
        #[arg(long)]
        rtmp: Option<String>,
        /// "auto" or "off". See `source add --superimpose`.
        #[arg(long, default_value = "auto")]
        superimpose: String,
        /// Source id. Derived from the host when omitted.
        #[arg(long)]
        id: Option<String>,
    },
}

#[derive(Subcommand, Debug)]
pub enum SourceCmd {
    List,
    /// Add a source. The protocol is worked out from the URL.
    ///
    /// rtmp://…, https://….m3u8, rtsp://…, srt://… and file paths are all
    /// recognised. Prefix a page with web+ (web+https://host/page) to render
    /// the site itself, with its audio, as a source.
    Add {
        /// Stable id. Pass "-" to have one derived from the name or the host.
        id: String,
        uri: String,
        #[arg(long)]
        name: Option<String>,
        /// Render the URL as a website (with its audio) rather than opening
        /// it as a stream. Same as writing web+ in front of it.
        #[arg(long)]
        web: bool,
        /// Websites only: "auto" decodes the page's own video outside the
        /// browser and draws the page over the top, which saves about a CPU
        /// core. Falls back to "off" without saying so when the page has no
        /// address to hand over, which is the case for YouTube and for DRM.
        #[arg(long, default_value = "off")]
        superimpose: String,
    },
    Remove {
        id: String,
        /// Say what it would do and change nothing.
        #[arg(long)]
        dry_run: bool,
    },
}

/// `gmx ctl scene ...`: the scene server from a command line.
///
/// Everything here is one `scene.*` call, and every one of them takes a name
/// as readily as an id, so an operator types what they see on the tile.
#[derive(Subcommand, Debug)]
pub enum SceneCmd {
    /// Every scene, with its items, its sources and whether it is armed.
    List,
    /// One scene: every item and the box it lands in.
    Get {
        scene: String,
        /// Print the raw JSON instead of a summary.
        #[arg(long)]
        json: bool,
    },
    /// Make a scene from a set of sources, laid out by how many there are.
    New {
        /// What to call it.
        name: String,
        /// The sources to put in it.
        sources: Vec<String>,
        /// A layout name instead of the one the count would pick.
        #[arg(long)]
        layout: Option<String>,
    },
    /// Put something on a scene.
    Add {
        scene: String,
        /// A source id.
        source: String,
        /// What to call the item. Defaults to the source's id.
        #[arg(long)]
        name: Option<String>,
    },
    /// Move or resize an item: --at X,Y and --size WxH.
    Set {
        scene: String,
        item: String,
        /// Position in canvas pixels, as X,Y.
        #[arg(long)]
        at: Option<String>,
        /// Size in canvas pixels, as WxH.
        #[arg(long)]
        size: Option<String>,
        /// 0 to 1.
        #[arg(long)]
        opacity: Option<f64>,
        /// How long the move takes, in milliseconds. 0 is a cut.
        #[arg(long)]
        duration: Option<u64>,
    },
    /// Apply a layout, making a scene or reshaping one.
    Layout {
        /// A layout name. Use --list to see them.
        #[arg(required_unless_present = "list")]
        layout: Option<String>,
        /// a=cam1,b=cam2,inset=0.4
        #[arg(long, default_value = "")]
        values: String,
        /// Reshape this scene instead of making a new one, which keeps the
        /// item ids so the change is a move and not a cut.
        #[arg(long)]
        scene: Option<String>,
        /// How long the change takes, in milliseconds.
        #[arg(long)]
        duration: Option<u64>,
        /// List the layouts this core has and what each one takes.
        #[arg(long)]
        list: bool,
    },
    /// Arm a scene, so `take` with no name puts it on air.
    Arm {
        /// Leave it out to disarm.
        scene: Option<String>,
    },
    /// Line items up on an edge: left, right, top, bottom, center-x, center-y.
    Align {
        scene: String,
        edge: String,
        items: Vec<String>,
    },
    /// Lay items out in a grid.
    Grid {
        scene: String,
        #[arg(long, default_value_t = 2)]
        cols: usize,
        items: Vec<String>,
    },
    /// Undo the last change.
    Undo,
    /// Put back what undo took away.
    Redo,
    /// Report overlaps, items off the canvas and safe area breaches.
    Check {
        /// Leave it out to check every scene.
        scene: Option<String>,
    },
    /// Delete a scene.
    Remove {
        scene: String,
        /// Say what it would do and change nothing.
        #[arg(long)]
        dry_run: bool,
    },
    /// Copy one scene's geometry onto another's items, matched by name.
    CopyLayout {
        from: String,
        to: String,
        /// "name" or "order".
        #[arg(long, default_value = "name")]
        r#match: String,
    },
}

#[derive(Subcommand, Debug)]
pub enum OutputCmd {
    List,
    Add {
        id: String,
        uri: String,
        /// "own" reconnects fast; "cdn" backs off harder.
        #[arg(long, default_value = "own")]
        policy: String,
    },
    Remove {
        id: String,
        /// Say what it would do and change nothing.
        #[arg(long)]
        dry_run: bool,
    },
    /// Force a reconnect.
    Reconnect {
        id: String,
    },
}

pub async fn run(base: &str, token: Option<&str>, cmd: Ctl) -> Result<()> {
    let api = Api::new(base, token)?;
    let api = &api;
    match cmd {
        Ctl::Status { json } => status(api, json).await?,
        Ctl::Take { name, scene, at } => {
            let req = if scene {
                TakeRequest { source: None, scene: name, transition: None, at_running_time_ms: at }
            } else {
                TakeRequest { source: name, scene: None, transition: None, at_running_time_ms: at }
            };
            let state: ProgramState = api.call("program.take", None, &req).await?;
            println!("on program: {}", on_air(&state));
        }
        Ctl::Revert => {
            let state: ProgramState = api.call("program.revert", None, &()).await?;
            println!("on program: {}", on_air(&state));
        }
        Ctl::History { limit } => {
            let takes: Vec<TakeRecord> =
                api.get("program.history", None, &[("limit", limit.to_string())]).await?;
            for t in takes {
                println!(
                    "{:>10} ms  {:<12} by {}",
                    t.at_running_time_ms,
                    t.source.as_deref().unwrap_or("black"),
                    t.by
                );
            }
        }
        Ctl::Info => info(api).await?,
        Ctl::Source(cmd) => source(api, cmd).await?,
        Ctl::Output(cmd) => output(api, cmd).await?,
        Ctl::Scene(cmd) => scene(api, cmd).await?,
        Ctl::Ad { uri, at, return_to } => {
            let req = AdBreakRequest { uri: uri.clone(), at_running_time_ms: at, return_to };
            let _: Value = api.call("adbreak.start", None, &req).await?;
            println!("ad break: {uri}");
        }
        Ctl::EndAd => {
            let _: Value = api.call("adbreak.end", None, &()).await?;
            println!("ad break ended");
        }
        Ctl::Golive { url, rtmp, superimpose, id } => {
            let req = GoLiveRequest { url, rtmp, superimpose: Some(superimpose), id };
            let reply: GoLiveResult = api.call("program.golive", None, &req).await?;
            println!(
                "source {} is {:?}; it goes on programme as soon as it is live{}",
                reply.source,
                reply.state,
                match &reply.output {
                    Some(o) => format!(", sending to output {o}"),
                    None => String::new(),
                }
            );
        }
        Ctl::Media => {
            let listing: MediaListing = api.get("media.list", None, &[]).await?;
            if let Some(err) = listing.error {
                bail!("{}: {err}", listing.dir);
            }
            for i in listing.items {
                let secs = i.duration_ms.map(|m| m as f64 / 1000.0);
                println!(
                    "{:<28} {:>7} {}",
                    i.name,
                    secs.map(|s| format!("{s:.1}s")).unwrap_or_else(|| "?".into()),
                    if i.has_audio { "" } else { "(no audio)" }
                );
            }
        }
    }
    Ok(())
}

async fn status(api: &Api, json: bool) -> Result<()> {
    if json {
        let raw: Value = api.get("core.status", None, &[]).await?;
        println!("{}", serde_json::to_string_pretty(&raw)?);
        return Ok(());
    }
    let s: MixerStatus = api.get("core.status", None, &[]).await?;
    println!("program : {}", s.program.as_deref().unwrap_or("black"));
    if let Some(ad) = &s.ad {
        println!("ad      : {} ({})", if ad.on_air { "on air" } else { "armed" }, ad.uri);
    }
    println!(
        "backend : {} ({})",
        s.backend.video_encoder,
        if s.backend.hardware_accelerated { "hardware" } else { "software" }
    );
    for src in &s.sources {
        println!("source  : {}", source_line(src));
    }
    for o in &s.outputs {
        println!(
            "output  : {:<10} {:<13} {} reconnects",
            o.id,
            format!("{:?}", o.state).to_lowercase(),
            o.reconnects
        );
    }
    Ok(())
}

async fn info(api: &Api) -> Result<()> {
    let info: CoreInfo = api.get("core.info", None, &[]).await?;
    println!("core     : {} {}", info.core, info.version);
    println!("api      : level {} (compatible from {})", info.api_level, info.api_compatible);
    println!("canvas   : {}x{} at {} fps", info.canvas.width, info.canvas.height, info.canvas.fps);
    println!("features : {}", info.features.join(", "));
    if let Some(t) = info.token {
        println!("token    : {} ({}), confirm {}", t.id, t.scopes.join("+"), t.confirm);
    }
    if info.rehearsal {
        println!("rehearsal: this core refuses output.add");
    }
    Ok(())
}

async fn source(api: &Api, cmd: SourceCmd) -> Result<()> {
    match cmd {
        SourceCmd::List => {
            let sources: Vec<SourceStatus> = api.get("source.list", None, &[]).await?;
            for s in &sources {
                println!("{}", source_line(s));
            }
        }
        SourceCmd::Add { id, uri, name, web, superimpose } => {
            let req = AddSourceRequest {
                id: (id != "-").then_some(id),
                name,
                uri,
                kind: web.then(|| "web".to_string()),
                superimpose: Some(superimpose),
                params: Default::default(),
            };
            // The whole record comes back, so the id it actually got is in the
            // answer and nobody has to diff the status to find out.
            let added: SourceStatus = api.call("source.add", None, &req).await?;
            println!("added source {} ({:?})", added.id, added.state);
        }
        SourceCmd::Remove { id, dry_run } => {
            let body: Value = api.call_with("source.remove", Some(&id), &(), dry_run).await?;
            if dry_run {
                for line in body["diff"].as_array().into_iter().flatten() {
                    println!("would {}", line.as_str().unwrap_or_default());
                }
            } else {
                println!("removed source {id}");
            }
        }
    }
    Ok(())
}

async fn output(api: &Api, cmd: OutputCmd) -> Result<()> {
    match cmd {
        OutputCmd::List => {
            let outputs: Vec<OutputStatus> = api.get("output.list", None, &[]).await?;
            for o in &outputs {
                println!(
                    "{:<12} {:<13} {} reconnects, {:.1}s buffered",
                    o.id,
                    format!("{:?}", o.state).to_lowercase(),
                    o.reconnects,
                    o.queue_secs
                );
            }
        }
        OutputCmd::Add { id, uri, policy } => {
            let req = AddOutputRequest {
                id: id.clone(),
                uri,
                policy: Some(policy),
                params: Default::default(),
            };
            let added: OutputStatus = api.call("output.add", None, &req).await?;
            println!("added output {} to {}", added.id, added.uri_host);
        }
        OutputCmd::Remove { id, dry_run } => {
            let body: Value = api.call_with("output.remove", Some(&id), &(), dry_run).await?;
            if dry_run {
                for line in body["diff"].as_array().into_iter().flatten() {
                    println!("would {}", line.as_str().unwrap_or_default());
                }
            } else {
                println!("removed output {id}");
            }
        }
        OutputCmd::Reconnect { id } => {
            let _: OutputStatus = api.call("output.reconnect", Some(&id), &()).await?;
            println!("reconnecting output {id}");
        }
    }
    Ok(())
}

/// What is on air, in the words an operator uses for it.
fn on_air(state: &ProgramState) -> String {
    match (&state.scene, &state.program) {
        (Some(scene), Some(source)) => format!("{scene} (the source {source})"),
        (Some(scene), None) => scene.clone(),
        (None, Some(source)) => source.clone(),
        (None, None) => "black".into(),
    }
}

/// One source, the way an operator reads it.
///
/// The superimposed mark is the only feedback that the handover really
/// happened: `--superimpose auto` falls back quietly, so a page that could not
/// give up its media looks exactly like one that never asked, and the
/// difference is about a core of CPU.
fn source_line(s: &SourceStatus) -> String {
    format!(
        "{:<10} {:<10} {}{}",
        s.id,
        format!("{:?}", s.state).to_lowercase(),
        s.uri,
        if s.superimposed() { "  (superimposed)" } else { "" }
    )
}

/// Where the mixer is and how to be let in. The token, when there is one,
/// rides as a default header so no call site can forget it.
struct Api {
    base: String,
    client: reqwest::Client,
}

impl Api {
    fn new(base: &str, token: Option<&str>) -> Result<Self> {
        let mut headers = HeaderMap::new();
        if let Some(t) = token.map(str::trim).filter(|t| !t.is_empty()) {
            let mut v = HeaderValue::from_str(&format!("Bearer {t}"))
                .context("the token has characters a header cannot carry")?;
            v.set_sensitive(true);
            headers.insert(AUTHORIZATION, v);
        }
        let client = reqwest::Client::builder()
            .default_headers(headers)
            .build()
            .context("building the HTTP client")?;
        Ok(Self { base: base.trim_end_matches('/').to_string(), client })
    }

    /// The route one method sits at, with the id filled in.
    ///
    /// The same transform the server builds its router from, so a path is
    /// never written twice.
    fn route(&self, method: &str, id: Option<&str>) -> Result<(reqwest::Method, String)> {
        let rest = rest_transform(method)
            .with_context(|| format!("{method} has no REST route"))?;
        let path = match id {
            Some(id) => rest.path.replace("{id}", &urlencode(id)),
            None => rest.path.clone(),
        };
        let verb = reqwest::Method::from_bytes(rest.http.as_bytes())
            .with_context(|| format!("{} is not an HTTP method", rest.http))?;
        Ok((verb, format!("{}{path}", self.base)))
    }

    /// A read, with query parameters.
    async fn get<T: DeserializeOwned>(
        &self,
        method: &str,
        id: Option<&str>,
        query: &[(&str, String)],
    ) -> Result<T> {
        let (verb, url) = self.route(method, id)?;
        let r = self
            .client
            .request(verb, &url)
            .query(query)
            .send()
            .await
            .with_context(|| format!("calling {method} at {url}"))?;
        read(method, r).await
    }

    async fn call<Req: Serialize, T: DeserializeOwned>(
        &self,
        method: &str,
        id: Option<&str>,
        req: &Req,
    ) -> Result<T> {
        self.call_with(method, id, req, false).await
    }

    async fn call_with<Req: Serialize, T: DeserializeOwned>(
        &self,
        method: &str,
        id: Option<&str>,
        req: &Req,
        dry_run: bool,
    ) -> Result<T> {
        let (verb, url) = self.route(method, id)?;
        let mut body = serde_json::to_value(req).unwrap_or(Value::Null);
        if !body.is_object() {
            body = Value::Object(Default::default());
        }
        if dry_run {
            if let Some(map) = body.as_object_mut() {
                map.insert("dry_run".into(), Value::Bool(true));
            }
        }
        let r = self
            .client
            .request(verb, &url)
            .json(&body)
            .send()
            .await
            .with_context(|| format!("calling {method} at {url}"))?;
        read(method, r).await
    }
}

/// Percent encode an id for a path segment. Ids are slugs, so this is a guard
/// rather than a general encoder.
fn urlencode(id: &str) -> String {
    id.bytes()
        .map(|b| match b {
            b'A'..=b'Z' | b'a'..=b'z' | b'0'..=b'9' | b'-' | b'_' | b'.' | b'~' => {
                (b as char).to_string()
            }
            other => format!("%{other:02X}"),
        })
        .collect()
}

/// Surface the mixer's own reason for refusing.
///
/// `/api/v1` has one error shape, so the message is read out of it rather than
/// guessed at from a status code. The message names the current state and the
/// next step, which is exactly what a person at a terminal needs.
async fn read<T: DeserializeOwned>(method: &str, r: reqwest::Response) -> Result<T> {
    let status = r.status();
    let text = r.text().await.unwrap_or_default();
    if !status.is_success() {
        bail!("{method}: {}", refusal_message(&text));
    }
    if text.trim().is_empty() {
        return serde_json::from_str("null").context("decoding an empty response");
    }
    serde_json::from_str(&text).with_context(|| format!("decoding the answer to {method}"))
}

/// The sentence out of a refusal.
///
/// `/api/v1` has one error shape, so the message is read out of it rather than
/// guessed at from a status code. A legacy route answers plain text, and that
/// falls through unchanged.
fn refusal_message(text: &str) -> String {
    serde_json::from_str::<Value>(text)
        .ok()
        .and_then(|v| v["error"]["message"].as_str().map(String::from))
        .unwrap_or_else(|| text.trim().to_string())
}

// ---------------------------------------------------------------------------
// `gmx ctl scene ...`
//
// A thin client like the rest of this file: each arm builds one request and
// prints what came back. Nothing here decides anything about a scene; the
// scene server does, and an error it sends already names the next step.
// ---------------------------------------------------------------------------

async fn scene(api: &Api, cmd: SceneCmd) -> Result<()> {
    match cmd {
        SceneCmd::List => {
            let listing: Value = api.get("scene.list", None, &[]).await?;
            for s in listing["scenes"].as_array().into_iter().flatten() {
                println!(
                    "{:<24} {:>2} item(s)  {}{}",
                    s["name"].as_str().unwrap_or("?"),
                    s["items"].as_u64().unwrap_or(0),
                    sources_of(s),
                    if s["armed"].as_bool().unwrap_or(false) { "  (armed)" } else { "" }
                );
            }
        }
        SceneCmd::Get { scene, json } => {
            let view: Value = api.get("scene.get", None, &[("scene", scene)]).await?;
            if json {
                println!("{}", serde_json::to_string_pretty(&view)?);
                return Ok(());
            }
            print_scene(&view);
        }
        SceneCmd::New { name, sources, layout } => {
            let view: Value = api
                .call(
                    "scene.create_from",
                    None,
                    &json!({ "sources": sources, "layout": layout, "name": name }),
                )
                .await?;
            print_scene(&view);
        }
        SceneCmd::Add { scene, source, name } => {
            let view: Value = api
                .call(
                    "scene.item.add",
                    None,
                    &json!({ "scene": scene, "content": { "source": source }, "name": name }),
                )
                .await?;
            print_scene(&view);
        }
        SceneCmd::Set { scene, item, at, size, opacity, duration } => {
            let mut transform = serde_json::Map::new();
            if let Some(at) = at {
                let (x, y) = pair(&at, ',')?;
                transform.insert("position".into(), json!({ "x": x, "y": y }));
            }
            if let Some(size) = size {
                let (w, h) = pair(&size, 'x')?;
                transform.insert("frame".into(), json!({ "w": w, "h": h }));
            }
            let mut props = serde_json::Map::new();
            if !transform.is_empty() {
                props.insert("transform".into(), Value::Object(transform));
            }
            if let Some(o) = opacity {
                props.insert("opacity".into(), json!(o));
            }
            if props.is_empty() {
                bail!("nothing to set. Use --at X,Y, --size WxH or --opacity 0..1");
            }
            let view: Value = api
                .call(
                    "scene.item.set",
                    None,
                    &json!({ "scene": scene, "item": item, "props": props, "duration_ms": duration }),
                )
                .await?;
            print_scene(&view);
        }
        SceneCmd::Layout { layout, values, scene, duration, list } => {
            if list {
                let listing: Value = api.get("scene.layout.list", None, &[]).await?;
                for l in listing["layouts"].as_array().into_iter().flatten() {
                    println!(
                        "{:<16} sources: {}",
                        l["name"].as_str().unwrap_or("?"),
                        l["sources"]
                            .as_array()
                            .map(|a| a
                                .iter()
                                .filter_map(|v| v.as_str())
                                .collect::<Vec<_>>()
                                .join(", "))
                            .unwrap_or_default()
                    );
                }
                return Ok(());
            }
            let view: Value = api
                .call(
                    "scene.apply_layout",
                    None,
                    &json!({
                        "layout": layout,
                        "values": parse_values(&values)?,
                        "scene": scene,
                        "duration_ms": duration,
                    }),
                )
                .await?;
            print_scene(&view);
        }
        SceneCmd::Arm { scene } => {
            let answer: Value = api.call("scene.preview.set", None, &json!({ "scene": scene })).await?;
            match answer["preview"]["name"].as_str() {
                Some(name) => println!("armed: {name}"),
                None => println!("preview cleared"),
            }
        }
        SceneCmd::Align { scene, edge, items } => {
            let view: Value = api
                .call("scene.item.align", None, &json!({ "scene": scene, "items": items, "edge": edge }))
                .await?;
            print_scene(&view);
        }
        SceneCmd::Grid { scene, cols, items } => {
            let view: Value = api
                .call(
                    "scene.item.arrange_grid",
                    None,
                    &json!({ "scene": scene, "items": items, "cols": cols }),
                )
                .await?;
            print_scene(&view);
        }
        SceneCmd::Undo | SceneCmd::Redo => {
            let method = if matches!(cmd, SceneCmd::Undo) { "scene.undo" } else { "scene.redo" };
            let step: Value = api.call(method, None, &()).await?;
            println!(
                "{}: {} record(s) changed, {} step(s) left to undo",
                method.trim_start_matches("scene."),
                step["patch"]["updated"].as_array().map(|a| a.len()).unwrap_or(0)
                    + step["patch"]["added"].as_array().map(|a| a.len()).unwrap_or(0)
                    + step["patch"]["removed"].as_array().map(|a| a.len()).unwrap_or(0),
                step["undo"].as_u64().unwrap_or(0)
            );
        }
        SceneCmd::Check { scene } => {
            let query: Vec<(&str, String)> =
                scene.into_iter().map(|s| ("scene", s)).collect();
            let report: Value = api.get("scene.validate", None, &query).await?;
            let findings = report["findings"].as_array().cloned().unwrap_or_default();
            if findings.is_empty() {
                println!("nothing to fix");
            }
            for f in findings {
                println!(
                    "{:<8} {:<22} {}",
                    f["severity"].as_str().unwrap_or("?"),
                    f["code"].as_str().unwrap_or("?"),
                    f["message"].as_str().unwrap_or("")
                );
            }
        }
        SceneCmd::Remove { scene, dry_run } => {
            let body: Value = api
                .call_with("scene.remove", None, &json!({ "scene": scene }), dry_run)
                .await?;
            if dry_run {
                for line in body["diff"].as_array().into_iter().flatten() {
                    println!("would {}", line.as_str().unwrap_or_default());
                }
            } else {
                println!("removed scene {}", body["removed"].as_str().unwrap_or(&scene));
            }
        }
        SceneCmd::CopyLayout { from, to, r#match } => {
            let layout: Value = api.get("scene.layout.copy", None, &[("scene", from)]).await?;
            let view: Value = api
                .call(
                    "scene.layout.paste",
                    None,
                    &json!({ "scene": to, "layout": layout, "match": r#match }),
                )
                .await?;
            print_scene(&view);
        }
    }
    Ok(())
}

/// A scene, the way an operator reads it: the name, then one line per item
/// saying what it shows and where it is.
fn print_scene(view: &Value) {
    println!(
        "{} ({}x{})",
        view["name"].as_str().unwrap_or("?"),
        view["canvas"]["width"].as_u64().unwrap_or(0),
        view["canvas"]["height"].as_u64().unwrap_or(0)
    );
    for g in view["geometry"].as_array().into_iter().flatten() {
        println!(
            "  {:<20} {:<10} {:>5},{:<5} {:>5}x{:<5} alpha {:.2}",
            g["path"].as_str().unwrap_or("?"),
            g["source"].as_str().unwrap_or(""),
            g["x"].as_f64().unwrap_or(0.0).round(),
            g["y"].as_f64().unwrap_or(0.0).round(),
            g["width"].as_f64().unwrap_or(0.0).round(),
            g["height"].as_f64().unwrap_or(0.0).round(),
            g["opacity"].as_f64().unwrap_or(1.0)
        );
    }
    for f in view["findings"].as_array().into_iter().flatten() {
        println!("  ! {} {}", f["code"].as_str().unwrap_or("?"), f["message"].as_str().unwrap_or(""));
    }
}

fn sources_of(scene: &Value) -> String {
    scene["sources"]
        .as_array()
        .map(|a| a.iter().filter_map(|v| v.as_str()).collect::<Vec<_>>().join(", "))
        .unwrap_or_default()
}

/// `a=cam1,b=cam2,inset=0.4` into a JSON object, with numbers as numbers.
fn parse_values(text: &str) -> Result<serde_json::Map<String, Value>> {
    let mut out = serde_json::Map::new();
    for pair in text.split(',').map(str::trim).filter(|p| !p.is_empty()) {
        let (key, value) = pair.split_once('=').with_context(|| {
            format!("{pair:?} is not a value. Write them as name=value, separated by commas, for example a=cam1,b=cam2,inset=0.4")
        })?;
        let value = value.trim();
        let parsed = match value.parse::<f64>() {
            Ok(n) => json!(n),
            Err(_) => json!(value),
        };
        out.insert(key.trim().to_string(), parsed);
    }
    Ok(out)
}

/// `1920x1080` or `96,880` into two numbers.
fn pair(text: &str, sep: char) -> Result<(f64, f64)> {
    let (a, b) = text.split_once(sep).with_context(|| {
        format!("{text:?} is not two numbers. Write it as A{sep}B, for example 960{sep}540")
    })?;
    let read = |v: &str| {
        v.trim()
            .parse::<f64>()
            .with_context(|| format!("{v:?} is not a number"))
    };
    Ok((read(a)?, read(b)?))
}

#[cfg(test)]
mod tests {
    use super::*;

    fn api() -> Api {
        Api::new("http://mixer:8080/", None).unwrap()
    }

    /// The CLI does not carry a path table of its own. Every command resolves
    /// through the same transform the server's router is built from, so a
    /// renamed method cannot leave the CLI pointing at a dead URL.
    #[test]
    fn every_command_resolves_through_the_shared_transform() {
        let api = api();
        let at = |method: &str, id: Option<&str>| {
            let (verb, url) = api.route(method, id).unwrap();
            format!("{verb} {url}")
        };
        assert_eq!(at("core.status", None), "GET http://mixer:8080/api/v1/core/status");
        assert_eq!(at("program.take", None), "POST http://mixer:8080/api/v1/program/take");
        assert_eq!(at("program.revert", None), "POST http://mixer:8080/api/v1/program/revert");
        assert_eq!(at("source.list", None), "GET http://mixer:8080/api/v1/sources");
        assert_eq!(at("source.add", None), "POST http://mixer:8080/api/v1/sources");
        assert_eq!(
            at("source.remove", Some("cam1")),
            "DELETE http://mixer:8080/api/v1/sources/cam1"
        );
        assert_eq!(
            at("output.reconnect", Some("yt")),
            "POST http://mixer:8080/api/v1/outputs/yt/reconnect"
        );
        assert_eq!(at("media.list", None), "GET http://mixer:8080/api/v1/media");
        // A trailing slash on the base must not double up.
        assert!(!at("core.info", None).contains("//api"));
    }

    /// Bodies are built from the api request types, so the JSON the CLI sends
    /// is the JSON the schema describes.
    #[test]
    fn bodies_come_from_the_api_types() {
        let take = TakeRequest {
            source: Some("cam1".into()),
            scene: None,
            transition: None,
            at_running_time_ms: Some(1500),
        };
        let v = serde_json::to_value(&take).unwrap();
        assert_eq!(v["source"], "cam1");
        assert_eq!(v["at_running_time_ms"], 1500);
        // Omitted rather than null, so the server's defaults apply.
        assert!(v.get("scene").is_none());

        let add = AddSourceRequest {
            id: None,
            name: Some("Camera 2".into()),
            uri: "rtmp://h/l/k".into(),
            kind: Some("web".into()),
            superimpose: Some("auto".into()),
            params: Default::default(),
        };
        let v = serde_json::to_value(&add).unwrap();
        assert!(v.get("id").is_none(), "a derived id is an absent key, not a null");
        assert_eq!(v["kind"], "web");
        assert_eq!(v["superimpose"], "auto");
    }

    #[test]
    fn an_id_with_awkward_characters_cannot_change_the_route() {
        assert_eq!(urlencode("cam-1"), "cam-1");
        assert_eq!(urlencode("../status"), "..%2Fstatus");
        assert_eq!(urlencode("a b"), "a%20b");
        assert_eq!(urlencode("clip.mp4"), "clip.mp4");
    }

    /// A refusal is read out of the one error shape, so the operator sees the
    /// mixer's own sentence and not an HTTP status code.
    #[test]
    fn a_refusal_shows_the_mixers_own_message() {
        let body = serde_json::json!({
            "error": {
                "code": -32004,
                "message": "there is no source 'cam9'. The sources: cam1, cam2. Use one of those.",
                "data": { "valid": ["cam1", "cam2"], "retryable": false }
            },
            "trace_id": "0af7651916cd43dd8448eb211c80319c"
        });
        let message = refusal_message(&serde_json::to_string(&body).unwrap());
        assert!(message.contains("cam9"), "{message}");
        assert!(message.contains("cam1, cam2"), "{message}");

        // A legacy route answers plain text, and that has to survive too, or a
        // CLI pointed at an older mixer prints nothing useful.
        assert_eq!(refusal_message("  no such source cam9  "), "no such source cam9");
        // So does a body that is JSON but not an error envelope.
        assert_eq!(refusal_message("{\"ok\":true}"), "{\"ok\":true}");
    }
}
