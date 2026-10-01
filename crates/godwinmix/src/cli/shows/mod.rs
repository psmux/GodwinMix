//! `gmx shows`: many shows from a terminal.
//!
//! A thin client of `show.list`, `show.add_many`, `show.stats` and
//! `show.set`, over the same `/api/v1` the page and the MCP server use. It
//! decides nothing: the station prices, admits and refuses, and its refusal
//! already says what to do next.

mod feeds;
mod table;

use anyhow::{Context, Result};
use clap::{Subcommand, ValueEnum};
use serde_json::{json, Value};
use std::path::PathBuf;
use std::time::Duration;

#[derive(Debug, Clone, Copy, ValueEnum)]
pub enum OnOff {
    On,
    Off,
}

impl OnOff {
    fn yes(self) -> bool {
        matches!(self, OnOff::On)
    }
}

#[derive(Debug, Subcommand)]
pub enum Shows {
    /// Every show: its state, what is on air, what it sends and costs.
    List,
    /// Add a show per feed in a list, in one call.
    ///
    /// The list is a CSV with a `name,input,program,outputs` header, or one
    /// address per line. See docs/reference/cli.md.
    Add {
        /// The list of feeds. `-` reads standard input.
        #[arg(long = "from", value_name = "FILE")]
        from: PathBuf,
        /// Say what would be added, what it costs and whether it fits, and
        /// add nothing.
        #[arg(long)]
        dry_run: bool,
        /// Give every show scenes and a programme encode. Off by default: each
        /// feed goes straight to its outputs, which is light.
        #[arg(long, value_enum, default_value = "off")]
        compositing: OnOff,
    },
    /// Health, input and outputs for every show, one line each, worst first.
    Stats {
        /// Draw it again every `--interval` seconds until Ctrl-C.
        #[arg(long)]
        watch: bool,
        #[arg(long, default_value_t = 2)]
        interval: u64,
        /// Only these shows, by id, comma separated.
        #[arg(long, value_delimiter = ',')]
        ids: Vec<String>,
    },
    /// Change one show: compositing on or off, its name, or its input.
    Set {
        id: String,
        #[arg(long, value_enum)]
        compositing: Option<OnOff>,
        #[arg(long)]
        name: Option<String>,
        /// A new input address.
        #[arg(long)]
        input: Option<String>,
    },
}

pub async fn run(base: &str, token: Option<&str>, cmd: Shows) -> Result<()> {
    let api = crate::ctl::Api::new(base, token)?;
    match cmd {
        Shows::List => list(&api).await,
        Shows::Add { from, dry_run, compositing } => add(&api, &from, dry_run, compositing.yes()).await,
        Shows::Stats { watch, interval, ids } => stats(&api, watch, interval, ids).await,
        Shows::Set { id, compositing, name, input } => {
            let mut req = json!({ "id": &id });
            if let Some(c) = compositing {
                req["compositing"] = json!(c.yes());
            }
            if let Some(n) = name {
                req["name"] = json!(n);
            }
            if let Some(uri) = input {
                req["input"] = json!({ "uri": uri });
            }
            let show: Value = api.call("show.set", Some(&id), &req).await?;
            println!("{}", serde_json::to_string_pretty(&show)?);
            Ok(())
        }
    }
}

async fn list(api: &crate::ctl::Api) -> Result<()> {
    let answer: Value = api.call("show.list", None, &json!({})).await?;
    println!("{:<22} {:<7} {:<9} {:<18} {:>9} {:>6} {:>7}", "SHOW", "KIND", "STATE", "ON AIR", "OUT KBPS", "CPU", "MEMORY");
    for s in answer["shows"].as_array().cloned().unwrap_or_default() {
        // A show that does not say is one from before compositing was a
        // property, which always composited.
        let kind = if s["compositing"] == json!(false) { "direct" } else { "mix" };
        println!(
            "{:<22} {:<7} {:<9} {:<18} {:>9} {:>5}% {:>4}MiB",
            s["id"].as_str().unwrap_or("-"),
            kind,
            s["state"].as_str().unwrap_or("-"),
            s["on_air"].as_str().unwrap_or("-"),
            s["programme_kbps"].as_u64().unwrap_or(0),
            s["cpu_millicores"].as_u64().unwrap_or(0) / 10,
            s["memory_mib"].as_u64().unwrap_or(0),
        );
    }
    Ok(())
}

async fn add(api: &crate::ctl::Api, from: &PathBuf, dry_run: bool, compositing: bool) -> Result<()> {
    let text = if from.as_os_str() == "-" {
        std::io::read_to_string(std::io::stdin()).context("reading the list from standard input")?
    } else {
        std::fs::read_to_string(from).with_context(|| format!("reading {}", from.display()))?
    };
    let feeds = feeds::parse(&text)?;
    let shows: Vec<Value> = feeds.iter().map(|f| f.to_show(compositing)).collect();
    let req = json!({ "shows": shows, "dry_run": dry_run });
    let answer: Value = api.call("show.add_many", None, &req).await?;
    print!("{}", outcome(&answer, &feeds, dry_run));
    Ok(())
}

/// What `show.add_many` said, in sentences: what went in, what was refused
/// and why (by the line of the file it came from), and the cost.
fn outcome(answer: &Value, feeds: &[feeds::Feed], dry_run: bool) -> String {
    let added = answer["added"].as_array().map(Vec::len).unwrap_or(0);
    let refused = answer["refused"].as_array().cloned().unwrap_or_default();
    let mut out = String::new();
    let verb = if dry_run { "would add" } else { "added" };
    out.push_str(&format!("{verb} {added} of {} shows\n", feeds.len()));
    for r in &refused {
        let index = r["index"].as_u64().unwrap_or(0) as usize;
        let line = feeds.get(index).map(|f| f.line).unwrap_or(0);
        let name = r["name"].as_str().unwrap_or("?");
        out.push_str(&format!("  line {line} {name}: {}\n", r["why"].as_str().unwrap_or("refused")));
    }
    let plan = &answer["plan"];
    if !plan.is_null() {
        let fits = if plan["fits"].as_bool() == Some(false) { "does not fit" } else { "fits" };
        out.push_str(&format!("cost {}, {fits} on this machine\n", compact(&plan["cost"])));
    }
    if dry_run && refused.is_empty() {
        out.push_str("nothing was changed. Run it again without --dry-run to add them.\n");
    }
    out
}

/// The governor's cost in words: `1.2 cores, 340 MiB, 24000 kbps out`.
fn compact(v: &Value) -> String {
    let Some(map) = v.as_object() else {
        return v.as_str().map(String::from).unwrap_or_else(|| "unknown".into());
    };
    let n = |k: &str| map.get(k).and_then(Value::as_u64).unwrap_or(0);
    format!(
        "{:.1} cores, {} MiB, {} kbps out",
        n("cpu_millicores") as f64 / 1000.0,
        n("memory_mib"),
        n("egress_kbps")
    )
}

async fn stats(api: &crate::ctl::Api, watch: bool, interval: u64, ids: Vec<String>) -> Result<()> {
    let req = if ids.is_empty() { json!({}) } else { json!({ "ids": ids }) };
    loop {
        let answer: Result<Value> = api.call("show.stats", None, &req).await;
        if !watch {
            print!("{}", table::render(&answer?));
            return Ok(());
        }
        // Home and clear, so the table redraws in place. A station that is
        // restarting is a line on the screen, not the end of the watch.
        print!("\x1b[H\x1b[2J");
        match answer {
            Ok(answer) => print!("{}", table::render(&answer)),
            Err(e) => println!("{e:#}\nTrying again in {interval} s."),
        }
        tokio::time::sleep(Duration::from_secs(interval.max(1))).await;
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn a_refusal_is_reported_by_the_line_it_came_from() {
        let feeds = feeds::parse("name,input\nA,udp://@239.1.1.1:5000\nB,udp://@239.1.1.2:5000\n").unwrap();
        let answer = json!({
            "added": ["a"],
            "refused": [{ "index": 1, "name": "B", "why": "the port is taken by show a", "data": {} }],
            "plan": { "cost": { "cpu_millicores": 1250, "memory_mib": 300, "egress_kbps": 9000 }, "fits": true }
        });
        let text = outcome(&answer, &feeds, true);
        assert!(text.starts_with("would add 1 of 2 shows"), "{text}");
        assert!(text.contains("line 3 B: the port is taken"), "{text}");
        assert!(text.contains("cost 1.2 cores, 300 MiB, 9000 kbps out, fits"), "{text}");
    }
}
