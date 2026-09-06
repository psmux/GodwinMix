//! `liveboxmix ctl`: drive a running mixer from the command line.
//!
//! The daemon is controlled entirely over HTTP, so this is a thin client rather
//! than a second control path. Everything it does can equally be done with curl
//! against the same endpoints; it exists so that scripting the mixer does not
//! require hand-assembling JSON.

use anyhow::{bail, Context, Result};
use clap::Subcommand;
use serde_json::json;

#[derive(Subcommand, Debug)]
pub enum Ctl {
    /// Show what is on air, which sources are live and how the outputs are doing.
    Status {
        /// Print the raw JSON instead of a summary.
        #[arg(long)]
        json: bool,
    },
    /// Put a source on program. Omit the id to cut to black.
    Take {
        source: Option<String>,
        /// Land the cut on this program running time, in milliseconds.
        #[arg(long)]
        at: Option<u64>,
    },
    /// List, add and remove sources.
    #[command(subcommand)]
    Source(SourceCmd),
    /// List, add and remove RTMP destinations.
    #[command(subcommand)]
    Output(OutputCmd),
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
    },
    /// Force a reconnect.
    Reconnect {
        id: String,
    },
}

pub async fn run(base: &str, cmd: Ctl) -> Result<()> {
    let base = base.trim_end_matches('/');
    match cmd {
        Ctl::Status { json } => {
            let body: serde_json::Value = get(base, "/api/status").await?;
            if json {
                println!("{}", serde_json::to_string_pretty(&body)?);
            } else {
                print_status(&body);
            }
        }
        Ctl::Take { source, at } => {
            post(base, "/api/take", json!({ "source": source, "at_running_time_ms": at })).await?;
            println!("on program: {}", source.as_deref().unwrap_or("black"));
        }
        Ctl::Source(SourceCmd::List) => {
            let body: serde_json::Value = get(base, "/api/status").await?;
            for s in body["sources"].as_array().into_iter().flatten() {
                println!(
                    "{:<10} {:<10} {}{}",
                    s["id"].as_str().unwrap_or(""),
                    s["state"].as_str().unwrap_or(""),
                    s["uri"].as_str().unwrap_or(""),
                    superimposed_mark(s)
                );
            }
        }
        Ctl::Source(SourceCmd::Add { id, uri, name, web, superimpose }) => {
            let id = (id != "-").then_some(id);
            let kind = web.then_some("web");
            let body =
                json!({ "id": id, "uri": uri, "name": name, "kind": kind, "superimpose": superimpose });
            post(base, "/api/sources", body).await?;
            println!("added source {}", id.as_deref().unwrap_or("(id derived from the URL)"));
        }
        Ctl::Source(SourceCmd::Remove { id }) => {
            delete(base, &format!("/api/sources/{id}")).await?;
            println!("removed source {id}");
        }
        Ctl::Output(OutputCmd::List) => {
            let body: serde_json::Value = get(base, "/api/outputs").await?;
            for o in body.as_array().into_iter().flatten() {
                println!(
                    "{:<12} {:<13} {} reconnects, {:.1}s buffered",
                    o["id"].as_str().unwrap_or(""),
                    o["state"].as_str().unwrap_or(""),
                    o["reconnects"].as_u64().unwrap_or(0),
                    o["queue_secs"].as_f64().unwrap_or(0.0)
                );
            }
        }
        Ctl::Output(OutputCmd::Add { id, uri, policy }) => {
            post(base, "/api/outputs", json!({ "id": id, "uri": uri, "policy": policy })).await?;
            println!("added output {id}");
        }
        Ctl::Output(OutputCmd::Remove { id }) => {
            delete(base, &format!("/api/outputs/{id}")).await?;
            println!("removed output {id}");
        }
        Ctl::Output(OutputCmd::Reconnect { id }) => {
            post(base, &format!("/api/outputs/{id}/reconnect"), json!({})).await?;
            println!("reconnecting output {id}");
        }
        Ctl::Ad { uri, at, return_to } => {
            post(
                base,
                "/api/adbreak",
                json!({ "uri": uri, "at_running_time_ms": at, "return_to": return_to }),
            )
            .await?;
            println!("ad break: {uri}");
        }
        Ctl::EndAd => {
            post(base, "/api/adbreak/end", json!({})).await?;
            println!("ad break ended");
        }
        Ctl::Media => {
            let body: serde_json::Value = get(base, "/api/media").await?;
            if let Some(err) = body["error"].as_str() {
                bail!("{}: {err}", body["dir"].as_str().unwrap_or("library"));
            }
            for i in body["items"].as_array().into_iter().flatten() {
                let secs = i["duration_ms"].as_u64().map(|m| m as f64 / 1000.0);
                println!(
                    "{:<28} {:>7} {}",
                    i["name"].as_str().unwrap_or(""),
                    secs.map(|s| format!("{s:.1}s")).unwrap_or_else(|| "?".into()),
                    if i["has_audio"].as_bool().unwrap_or(false) { "" } else { "(no audio)" }
                );
            }
        }
    }
    Ok(())
}

/// The only feedback an operator gets that the handover really happened.
/// `--superimpose auto` falls back quietly, so a page that could not give up
/// its media looks exactly like one that never asked, and the difference is
/// about a core of CPU.
fn superimposed_mark(s: &serde_json::Value) -> &'static str {
    if s["superimposed"].as_bool().unwrap_or(false) { "  (superimposed)" } else { "" }
}

fn print_status(b: &serde_json::Value) {
    println!("program : {}", b["program"].as_str().unwrap_or("black"));
    if let Some(ad) = b["ad"].as_object() {
        let state = if ad["on_air"].as_bool().unwrap_or(false) { "on air" } else { "armed" };
        println!("ad      : {state} ({})", ad["uri"].as_str().unwrap_or(""));
    }
    println!(
        "backend : {} ({})",
        b["backend"]["video_encoder"].as_str().unwrap_or(""),
        if b["backend"]["hardware_accelerated"].as_bool().unwrap_or(false) {
            "hardware"
        } else {
            "software"
        }
    );
    for s in b["sources"].as_array().into_iter().flatten() {
        println!(
            "source  : {:<10} {:<10} {}{}",
            s["id"].as_str().unwrap_or(""),
            s["state"].as_str().unwrap_or(""),
            s["uri"].as_str().unwrap_or(""),
            superimposed_mark(s)
        );
    }
    for o in b["outputs"].as_array().into_iter().flatten() {
        println!(
            "output  : {:<10} {:<13} {} reconnects",
            o["id"].as_str().unwrap_or(""),
            o["state"].as_str().unwrap_or(""),
            o["reconnects"].as_u64().unwrap_or(0)
        );
    }
}

async fn get<T: serde::de::DeserializeOwned>(base: &str, path: &str) -> Result<T> {
    let r = reqwest::get(format!("{base}{path}"))
        .await
        .with_context(|| format!("GET {base}{path}"))?;
    check(r).await?.json().await.context("decoding response")
}

async fn post(base: &str, path: &str, body: serde_json::Value) -> Result<()> {
    let r = reqwest::Client::new()
        .post(format!("{base}{path}"))
        .json(&body)
        .send()
        .await
        .with_context(|| format!("POST {base}{path}"))?;
    check(r).await?;
    Ok(())
}

async fn delete(base: &str, path: &str) -> Result<()> {
    let r = reqwest::Client::new()
        .delete(format!("{base}{path}"))
        .send()
        .await
        .with_context(|| format!("DELETE {base}{path}"))?;
    check(r).await?;
    Ok(())
}

/// Surface the mixer's own reason for refusing, rather than a status code.
async fn check(r: reqwest::Response) -> Result<reqwest::Response> {
    if r.status().is_success() {
        return Ok(r);
    }
    let status = r.status();
    let body = r.text().await.unwrap_or_default();
    bail!("{status}: {}", body.trim());
}
