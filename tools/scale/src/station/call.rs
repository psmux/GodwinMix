//! `gmx-scale call`: one method on the station, routed by its own table, for
//! the steps of a run that are a single call (the compositing toggle).

use super::rpc::Client;
use crate::args::Args;
use serde_json::{json, Value};
use std::time::Instant;

const HELP: &str = "
gmx-scale call --method NAME [--params JSON] [--show ID] [--station ADDR]

Prints one line: the method, whether it was answered, how long it took and
the answer. Exits non zero when the station refused it.

  --method NAME    show.set, show.stats, governor.status...
  --params JSON    the params object ({})
  --show ID        send it to one show, as ?show= does
  --station ADDR   the station's control address (127.0.0.1:8080)
  --token T        a bearer token, when the station has one (or GODWINMIX_TOKEN)
";

pub fn main(a: Args) -> Result<(), String> {
    if a.help(HELP) {
        return Ok(());
    }
    let method = a.need("method")?;
    let params: Value = serde_json::from_str(a.str("params").unwrap_or("{}")).map_err(|e| format!("--params is not JSON: {e}"))?;
    let token = a.str("token").map(String::from).or_else(|| std::env::var("GODWINMIX_TOKEN").ok());
    let c = Client::connect(a.str("station").unwrap_or("127.0.0.1:8080"), token)?;
    let t = Instant::now();
    let got = c.call(method, params, a.str("show"))?;
    let ms = (t.elapsed().as_secs_f64() * 10000.0).round() / 10.0;
    println!("{}", json!({"method": method, "ok": got.ok(), "ms": ms, "answer": got.body}));
    if got.ok() {
        Ok(())
    } else {
        Err(format!("{method} was refused: {}", got.why()))
    }
}
