//! `gmx shows`: many shows from a terminal.
//!
//! A thin client of `show.list`, `show.add_many`, `show.stats` and
//! `show.set`, over the same `/api/v1` the page and the MCP server use. It
//! decides nothing: the station prices, admits and refuses, and its refusal
//! already says what to do next.

mod feeds;
mod report;
mod set;
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
    fn yes(self) -> bool { matches!(self, OnOff::On) }
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
        Shows::Set { id, compositing, name, input } => set::run(&api, &id, compositing.map(OnOff::yes), name, input).await,
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
    print!("{}", report::outcome(&answer, &feeds, dry_run));
    Ok(())
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
