//! `gmx-scale add`: one show per row of a feeds list, with `show.add_many`
//! when the station has it and `show.add` once per row when it does not.
//! `--legacy` builds today's shape instead: a compositing show per feed with
//! a `udp/source`, a take and a `udp/output`.

use super::csv::{self, Row};
use super::rpc::Client;
use crate::args::Args;
use serde_json::{json, Value};
use std::time::{Duration, Instant};

const HELP: &str = "
gmx-scale add --csv FILE [--station 127.0.0.1:18480] [options]

  --csv FILE       name,input,program,output,format, as gmx-scale feeds writes it
  --station ADDR   the station's control address (127.0.0.1:8080)
  --token T        a bearer token, when the station has one (or GODWINMIX_TOKEN)
  --limit N        only the first N rows
  --legacy         a compositing show per row, with a udp/source and a udp/output
  --json FILE      write what happened there
";

pub fn main(a: Args) -> Result<(), String> {
    if a.help(HELP) {
        return Ok(());
    }
    let mut rows = csv::read(a.need("csv")?)?;
    rows.truncate(a.num("limit", rows.len())?);
    let token = a.str("token").map(String::from).or_else(|| std::env::var("GODWINMIX_TOKEN").ok());
    let c = Client::connect(a.str("station").unwrap_or("127.0.0.1:8080"), token)?;
    let start = Instant::now();
    let mut out = if a.flag("legacy") {
        legacy(&c, &rows)
    } else if c.has("show.add_many") {
        many(&c, &rows)?
    } else {
        one_by_one(&c, &rows)
    };
    out["requested"] = json!(rows.len());
    out["seconds"] = json!((start.elapsed().as_secs_f64() * 100.0).round() / 100.0);
    println!("{out}");
    if let Some(path) = a.str("json") {
        std::fs::write(path, format!("{out}\n")).map_err(|e| format!("could not write {path}: {e}"))?;
    }
    Ok(())
}

/// The contract's ShowAdd for one row: no compositing, the input, one output.
pub fn show_add(r: &Row) -> Value {
    let rendition = if r.format.is_empty() || r.format == "copy" { Value::Null } else { json!({"preset": r.format}) };
    json!({
        "name": r.name,
        "compositing": false,
        "input": {"uri": r.input, "program": r.program},
        "outputs": [{"id": "out", "uri": r.output, "rendition": rendition}],
    })
}

fn many(c: &Client, rows: &[Row]) -> Result<Value, String> {
    let shows: Vec<Value> = rows.iter().map(show_add).collect();
    let dry = c.call("show.add_many", json!({"shows": shows, "dry_run": true}), None)?;
    if !dry.ok() {
        return Err(format!("show.add_many with dry_run was refused: {}", dry.why()));
    }
    let t = Instant::now();
    let real = c.call("show.add_many", json!({"shows": shows, "dry_run": false}), None)?;
    if !real.ok() {
        return Err(format!("show.add_many was refused: {}", real.why()));
    }
    let added = real.body["added"].as_array().map_or(0, Vec::len);
    Ok(json!({
        "method": "show.add_many",
        "plan": dry.body["plan"],
        "added": added,
        "ids": real.body["added"],
        "refused": real.body["refused"],
        "apply_seconds": (t.elapsed().as_secs_f64() * 100.0).round() / 100.0,
    }))
}

fn one_by_one(c: &Client, rows: &[Row]) -> Value {
    let mut refused = Vec::new();
    let (mut added, mut ids) = (0, Vec::new());
    for (i, r) in rows.iter().enumerate() {
        match c.call("show.add", show_add(r), None) {
            Ok(a) if a.ok() && a.body["compositing"] != json!(false) => {
                let why = "show.add made a compositing show and ignored the input: this station predates wave 4. Run with --legacy for today's shape";
                refused.push(json!({"index": i, "name": r.name, "why": why}));
                break;
            }
            Ok(a) if a.ok() => {
                added += 1;
                ids.push(a.body["id"].clone());
            }
            Ok(a) => refused.push(json!({"index": i, "name": r.name, "why": a.why()})),
            Err(e) => refused.push(json!({"index": i, "name": r.name, "why": e})),
        }
    }
    json!({"method": "show.add", "added": added, "ids": ids, "refused": refused})
}

fn legacy(c: &Client, rows: &[Row]) -> Value {
    let mut refused = Vec::new();
    let mut added = 0;
    for (i, r) in rows.iter().enumerate() {
        match legacy_one(c, r) {
            Ok(()) => added += 1,
            Err(why) => refused.push(json!({"index": i, "name": r.name, "why": why})),
        }
    }
    json!({"method": "show.add, source.add, program.take, output.add", "added": added, "refused": refused})
}

fn legacy_one(c: &Client, r: &Row) -> Result<(), String> {
    let made = c.call("show.add", json!({"name": r.name}), None)?;
    if !made.ok() {
        return Err(made.why());
    }
    let id = made.body["id"].as_str().or(made.body["show"]["id"].as_str()).ok_or("show.add answered with no id")?.to_string();
    wait_running(c, &id)?;
    let steps = [
        ("source.add", json!({"id": "feed", "uri": r.input, "type": "udp/source", "params": {"program": r.program}})),
        ("program.take", json!({"source": "feed"})),
        ("output.add", json!({"id": "out", "uri": r.output, "type": "udp/output"})),
    ];
    for (method, params) in steps {
        let a = c.call(method, params, Some(&id))?;
        if !a.ok() {
            return Err(format!("{method} on {id}: {}", a.why()));
        }
    }
    Ok(())
}

fn wait_running(c: &Client, id: &str) -> Result<(), String> {
    let until = Instant::now() + Duration::from_secs(60);
    while Instant::now() < until {
        let list = c.call("show.list", json!({}), None)?;
        let state = list.body["shows"].as_array().into_iter().flatten().find(|s| s["id"] == id).map(|s| s["state"].clone());
        if state == Some(json!("running")) {
            return Ok(());
        }
        std::thread::sleep(Duration::from_millis(300));
    }
    Err(format!("show {id} was not running a minute after show.add"))
}
