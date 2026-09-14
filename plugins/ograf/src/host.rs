//! The page server: HTTP/1.1 over a `TcpListener`, on the loopback only.
//!
//! Five routes and no framework. A sidecar that serves an index, a wrapper
//! page, a manifest, a static file and an event stream should not carry a
//! router, a middleware stack and a TLS backend into a Raspberry Pi's memory.
//!
//! | Route | What it is |
//! |---|---|
//! | `GET /` | what this host can serve and what it is driving, for a person |
//! | `GET /health` | `{ok, graphics, instances}`, for the check script |
//! | `GET /graphic/<plugin>/<id>?instance=<i>` | the wrapper page a browser source loads |
//! | `GET /graphic/<plugin>/<id>/manifest.json` | the OGraf manifest |
//! | `GET /graphic/<plugin>/<id>/<file>` | the graphic's own files, its module among them |
//! | `GET /state/<instance>` | what that placement is showing now |
//! | `GET /events/<instance>` | the actions, as `text/event-stream` |
//!
//! It binds 127.0.0.1 and nothing else. A graphics host reachable from the
//! network is a way to put words on somebody's programme, and nothing here
//! needs to be reachable: the only client is a browser the mixer started on
//! this machine.

use std::net::{Ipv4Addr, SocketAddr};
use std::path::{Path, PathBuf};

use serde_json::{json, Value};
use tokio::io::{AsyncBufReadExt, AsyncReadExt, AsyncWriteExt, BufReader};
use tokio::net::{TcpListener, TcpStream};

use crate::catalogue;
use crate::state::Host;

/// The port the host asks for first. Taken, it takes a free one and says so.
pub const DEFAULT_PORT: u16 = 7841;

/// How long a request line is allowed to be. A URL longer than this is not a
/// graphic; it is somebody probing.
const MAX_REQUEST: u64 = 8 * 1024;

/// The server, once it is listening.
pub struct Serving {
    pub base: String,
    pub port: u16,
}

/// Bind and serve, for as long as the process lives.
///
/// Returns as soon as it is listening, with the address it got, so
/// `initialize` can answer with a base URL the core can put in a source.
pub async fn serve(root: PathBuf, host: Host, want: u16) -> std::io::Result<Serving> {
    let listener = match TcpListener::bind(SocketAddr::from((Ipv4Addr::LOCALHOST, want))).await {
        Ok(l) => l,
        // A port already taken is the ordinary case on a machine running two
        // mixers, and is not worth refusing to start over.
        Err(_) if want != 0 => {
            TcpListener::bind(SocketAddr::from((Ipv4Addr::LOCALHOST, 0))).await?
        }
        Err(e) => return Err(e),
    };
    let port = listener.local_addr()?.port();
    let base = format!("http://127.0.0.1:{port}");
    let serving = Serving {
        base: base.clone(),
        port,
    };
    tokio::spawn(async move {
        loop {
            let Ok((stream, _)) = listener.accept().await else {
                continue;
            };
            let (root, host, base) = (root.clone(), host.clone(), base.clone());
            // One task per connection, so a page holding an event stream open
            // for an hour does not stop the next page loading.
            tokio::spawn(async move {
                let _ = handle(stream, &root, &host, &base).await;
            });
        }
    });
    Ok(serving)
}

async fn handle(stream: TcpStream, root: &Path, host: &Host, base: &str) -> std::io::Result<()> {
    let (read, mut write) = stream.into_split();
    let mut lines = BufReader::new(read.take(MAX_REQUEST)).lines();
    let Some(request) = lines.next_line().await? else {
        return Ok(());
    };
    // The headers are read and dropped: nothing here varies by any of them,
    // and leaving them in the socket makes a keep alive client hang.
    while let Some(line) = lines.next_line().await? {
        if line.is_empty() {
            break;
        }
    }
    let mut parts = request.split_whitespace();
    let method = parts.next().unwrap_or_default();
    let target = parts.next().unwrap_or("/");
    if method != "GET" {
        return write
            .write_all(&text(405, "text/plain", "this host answers GET"))
            .await;
    }
    let (path, query) = target.split_once('?').unwrap_or((target, ""));
    match route(path, query, root, host, base) {
        Route::Bytes(bytes) => write.write_all(&bytes).await,
        Route::Stream(instance) => stream_events(write, host, &instance).await,
    }
}

enum Route {
    Bytes(Vec<u8>),
    Stream(String),
}

fn route(path: &str, query: &str, root: &Path, host: &Host, base: &str) -> Route {
    let trimmed = path.trim_matches('/');
    if trimmed.is_empty() {
        return Route::Bytes(text(
            200,
            "text/html; charset=utf-8",
            &index(root, host, base),
        ));
    }
    let segments: Vec<&str> = trimmed.split('/').collect();
    match segments.as_slice() {
        ["health"] => Route::Bytes(json_body(
            200,
            &json!({
                "ok": true,
                "graphics": catalogue::all(root).iter().map(|g| g.type_id()).collect::<Vec<_>>(),
                "instances": host.all().len(),
                "base": base,
            }),
        )),
        ["state", instance] => match host.get(instance) {
            Some(state) => Route::Bytes(json_body(200, &state.as_json(instance))),
            None => Route::Bytes(not_found(&format!(
                "nothing is loaded into {instance}. Put a graphic on a scene and the \
                 core loads it here."
            ))),
        },
        ["events", instance] => Route::Stream((*instance).to_string()),
        ["graphic", plugin, provide] => {
            let type_id = format!("{plugin}/{provide}");
            let Some(graphic) = catalogue::find(root, &type_id) else {
                return Route::Bytes(no_such_graphic(root, &type_id));
            };
            let instance = param(query, "instance").unwrap_or_else(|| type_id.replace('/', "-"));
            Route::Bytes(text(
                200,
                "text/html; charset=utf-8",
                &crate::page::wrapper(&graphic, &instance),
            ))
        }
        ["graphic", plugin, provide, file] => {
            let type_id = format!("{plugin}/{provide}");
            let Some(graphic) = catalogue::find(root, &type_id) else {
                return Route::Bytes(no_such_graphic(root, &type_id));
            };
            if *file == "manifest.json" {
                return Route::Bytes(json_body(200, &graphic.manifest));
            }
            match catalogue::asset(&graphic, file).and_then(|at| std::fs::read(at).ok()) {
                Some(bytes) => Route::Bytes(raw(200, mime(file), &bytes)),
                None => Route::Bytes(not_found(&format!(
                    "the graphic {type_id} has no file {file}."
                ))),
            }
        }
        _ => Route::Bytes(not_found(
            "this host serves /, /health, /graphic/<plugin>/<id>, /state/<instance> \
             and /events/<instance>.",
        )),
    }
}

fn no_such_graphic(root: &Path, type_id: &str) -> Vec<u8> {
    let known: Vec<String> = catalogue::all(root).iter().map(|g| g.type_id()).collect();
    not_found(&format!(
        "there is no graphic {type_id}. This host has: {}.",
        if known.is_empty() {
            "none".into()
        } else {
            known.join(", ")
        }
    ))
}

/// Hold the socket open and write every action for this instance to it.
///
/// The current state goes first, so a page that has just loaded, or one whose
/// browser was restarted under it, comes up showing what it was showing.
async fn stream_events(
    mut write: tokio::net::tcp::OwnedWriteHalf,
    host: &Host,
    instance: &str,
) -> std::io::Result<()> {
    let mut follow = host.follow();
    write
        .write_all(
            b"HTTP/1.1 200 OK\r\ncontent-type: text/event-stream\r\n\
              cache-control: no-store\r\nconnection: close\r\n\r\n",
        )
        .await?;
    if let Some(state) = host.get(instance) {
        write
            .write_all(event("load", &state.as_json(instance)).as_bytes())
            .await?;
    }
    loop {
        match follow.recv().await {
            Ok(action) if action.instance == instance => {
                write
                    .write_all(event(&action.verb, &action.body).as_bytes())
                    .await?;
            }
            Ok(_) => {}
            // Behind by more than the queue: tell the page to fetch its whole
            // state rather than showing it half the story.
            Err(tokio::sync::broadcast::error::RecvError::Lagged(_)) => {
                write
                    .write_all(event("resync", &json!({})).as_bytes())
                    .await?;
            }
            Err(_) => return Ok(()),
        }
    }
}

fn event(name: &str, body: &Value) -> String {
    format!("event: {name}\ndata: {body}\n\n")
}

/// A query parameter, without a URL parsing crate for one key.
fn param(query: &str, key: &str) -> Option<String> {
    query
        .split('&')
        .filter_map(|pair| pair.split_once('='))
        .find(|(k, _)| *k == key)
        .map(|(_, v)| percent_decode(v))
}

/// Enough percent decoding for an instance id, which is a slug.
fn percent_decode(s: &str) -> String {
    let bytes = s.as_bytes();
    let mut out = String::with_capacity(s.len());
    let mut i = 0;
    while i < bytes.len() {
        match bytes[i] {
            b'%' if i + 2 < bytes.len() => match u8::from_str_radix(&s[i + 1..i + 3], 16) {
                Ok(b) => {
                    out.push(b as char);
                    i += 3;
                }
                Err(_) => {
                    out.push('%');
                    i += 1;
                }
            },
            b'+' => {
                out.push(' ');
                i += 1;
            }
            b => {
                out.push(b as char);
                i += 1;
            }
        }
    }
    out
}

/// What a person sees when they open the host in a browser to see if it is up.
fn index(root: &Path, host: &Host, base: &str) -> String {
    let graphics: String = catalogue::all(root)
        .iter()
        .map(|g| {
            format!(
                "<li><a href=\"/graphic/{id}\">{id}</a> &mdash; {name}</li>",
                id = g.type_id(),
                name = escape(
                    g.manifest
                        .get("name")
                        .and_then(|v| v.as_str())
                        .unwrap_or("")
                )
            )
        })
        .collect();
    let live: String = host
        .all()
        .iter()
        .map(|(id, i)| {
            format!(
                "<li><code>{id}</code> {g}, step {s}{p}</li>",
                g = escape(&i.graphic),
                s = i.step,
                p = if i.playing { ", playing" } else { "" }
            )
        })
        .collect();
    format!(
        "<!doctype html><meta charset=utf-8><title>GodwinMix graphics host</title>\
         <style>body{{font:14px system-ui;margin:2rem;max-width:44rem}}code{{font-size:.9em}}</style>\
         <h1>Graphics host</h1><p>Serving on <code>{base}</code>.</p>\
         <h2>Graphics it can serve</h2><ul>{graphics}</ul>\
         <h2>On the canvas now</h2><ul>{live}</ul>\
         <p>A graphic is placed with <code>scene.item.add</code> and filled with \
         <code>scene.apply_graphic</code>. See docs/how-to/make-a-graphic.md.</p>",
        graphics = if graphics.is_empty() {
            "<li>none. Write one with <code>gmx plugin new --kind graphic</code>.</li>".into()
        } else {
            graphics
        },
        live = if live.is_empty() { "<li>nothing yet.</li>".into() } else { live }
    )
}

/// HTML escaping for the two characters that matter in a text node.
pub fn escape(s: &str) -> String {
    s.replace('&', "&amp;")
        .replace('<', "&lt;")
        .replace('>', "&gt;")
}

fn mime(file: &str) -> &'static str {
    match file.rsplit('.').next().unwrap_or_default() {
        "mjs" | "js" => "text/javascript; charset=utf-8",
        "css" => "text/css; charset=utf-8",
        "json" => "application/json",
        "svg" => "image/svg+xml",
        "png" => "image/png",
        "jpg" | "jpeg" => "image/jpeg",
        "webp" => "image/webp",
        "woff2" => "font/woff2",
        "html" => "text/html; charset=utf-8",
        _ => "application/octet-stream",
    }
}

fn text(status: u16, kind: &str, body: &str) -> Vec<u8> {
    raw(status, kind, body.as_bytes())
}

fn json_body(status: u16, body: &Value) -> Vec<u8> {
    raw(status, "application/json", body.to_string().as_bytes())
}

fn not_found(why: &str) -> Vec<u8> {
    json_body(404, &json!({ "error": why }))
}

fn raw(status: u16, kind: &str, body: &[u8]) -> Vec<u8> {
    let reason = match status {
        200 => "OK",
        404 => "Not Found",
        405 => "Method Not Allowed",
        _ => "Error",
    };
    let mut out = format!(
        "HTTP/1.1 {status} {reason}\r\ncontent-type: {kind}\r\ncontent-length: {}\r\n\
         cache-control: no-store\r\nconnection: close\r\n\r\n",
        body.len()
    )
    .into_bytes();
    out.extend_from_slice(body);
    out
}

#[cfg(test)]
mod tests {
    use super::*;

    fn root() -> PathBuf {
        PathBuf::from(env!("CARGO_MANIFEST_DIR"))
    }

    fn body_of(bytes: &[u8]) -> String {
        let text = String::from_utf8_lossy(bytes);
        text.split_once("\r\n\r\n")
            .map(|(_, b)| b.to_string())
            .unwrap_or_default()
    }

    fn get(path: &str, query: &str, host: &Host) -> Vec<u8> {
        match route(path, query, &root(), host, "http://127.0.0.1:7841") {
            Route::Bytes(b) => b,
            Route::Stream(i) => format!("stream:{i}").into_bytes(),
        }
    }

    #[test]
    fn health_says_what_it_can_serve() {
        let answer: Value =
            serde_json::from_str(&body_of(&get("/health", "", &Host::new()))).unwrap();
        assert_eq!(answer["ok"], true);
        assert_eq!(answer["graphics"][0], "ograf/lower-third");
    }

    #[test]
    fn the_wrapper_page_names_the_instance_and_loads_the_graphics_own_module() {
        let page = body_of(&get(
            "/graphic/ograf/lower-third",
            "instance=graphic-x-1",
            &Host::new(),
        ));
        assert!(
            page.contains("graphic-x-1"),
            "the page has to know which placement it is"
        );
        assert!(page.contains("graphic.mjs"), "{page}");
        // The instance goes through `encodeURIComponent`, so what is in the
        // page is the route and the id, not the two already joined.
        assert!(
            page.contains("/events/${encodeURIComponent(INSTANCE)}"),
            "{page}"
        );
    }

    #[test]
    fn the_manifest_is_served_so_a_client_can_read_the_schema_without_the_core() {
        let answer: Value = serde_json::from_str(&body_of(&get(
            "/graphic/ograf/lower-third/manifest.json",
            "",
            &Host::new(),
        )))
        .unwrap();
        assert_eq!(answer["stepCount"], 1);
        assert!(answer["schema"]["properties"]["name"].is_object());
    }

    #[test]
    fn the_graphics_own_module_is_served_as_javascript() {
        let bytes = get("/graphic/ograf/lower-third/graphic.mjs", "", &Host::new());
        let head = String::from_utf8_lossy(&bytes);
        assert!(head.contains("text/javascript"), "{head}");
        assert!(
            body_of(&bytes).contains("playAction"),
            "the component has to have the OGraf methods"
        );
    }

    #[test]
    fn a_file_outside_the_graphics_directory_is_not_served() {
        let answer = body_of(&get(
            "/graphic/ograf/lower-third/..%2f..%2fCargo.toml",
            "",
            &Host::new(),
        ));
        assert!(answer.contains("has no file"), "{answer}");
    }

    #[test]
    fn a_graphic_this_host_has_not_got_lists_the_ones_it_has() {
        let answer = body_of(&get("/graphic/nobody/nothing", "", &Host::new()));
        assert!(answer.contains("ograf/lower-third"), "{answer}");
    }

    #[test]
    fn the_state_route_answers_what_the_placement_is_showing() {
        let host = Host::new();
        host.load("graphic-x-1", "ograf/lower-third", &serde_json::Map::new());
        let answer: Value =
            serde_json::from_str(&body_of(&get("/state/graphic-x-1", "", &host))).unwrap();
        assert_eq!(answer["graphic"], "ograf/lower-third");
        let missing = body_of(&get("/state/nobody", "", &host));
        assert!(missing.contains("nothing is loaded"), "{missing}");
    }

    #[test]
    fn an_instance_id_survives_percent_encoding() {
        assert_eq!(
            param("instance=graphic%2Dx", "instance").as_deref(),
            Some("graphic-x")
        );
        assert_eq!(param("a=1&instance=x", "instance").as_deref(), Some("x"));
        assert_eq!(param("a=1", "instance"), None);
    }

    #[test]
    fn an_event_is_the_shape_an_event_source_reads() {
        assert_eq!(
            event("play", &json!({ "step": 1 })),
            "event: play\ndata: {\"step\":1}\n\n"
        );
    }

    #[tokio::test]
    async fn it_binds_the_loopback_and_answers_over_a_real_socket() {
        let host = Host::new();
        let serving = serve(root(), host.clone(), 0)
            .await
            .expect("binding a free port");
        assert!(
            serving.base.starts_with("http://127.0.0.1:"),
            "{}",
            serving.base
        );

        let mut stream = TcpStream::connect(("127.0.0.1", serving.port))
            .await
            .unwrap();
        stream
            .write_all(b"GET /health HTTP/1.1\r\nhost: x\r\n\r\n")
            .await
            .unwrap();
        let mut answer = Vec::new();
        stream.read_to_end(&mut answer).await.unwrap();
        let answer: Value = serde_json::from_str(&body_of(&answer)).unwrap();
        assert_eq!(answer["ok"], true);
    }
}
