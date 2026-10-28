//! `headend.scan`: what a multiplex carries. Two looks, each through the
//! same input a channel would use (`station::direct::probe`): the whole
//! feed until its program table has settled, then one probe per program,
//! all on the one shared socket, until each has said what its picture and
//! sound are. Every probe is closed before the answer goes back.

use super::{scan_read, Native};
use crate::station::direct::probe::PREFIX;
use crate::station::state::Station;
use godwinmix_protocol::error::RpcError;
use godwinmix_protocol::headend::{ScanRequest, ScanResult, ScannedProgram};
use serde_json::{json, Value};
use std::sync::atomic::{AtomicU64, Ordering};
use std::sync::Arc;
use std::time::{Duration, Instant};

/// The most programs looked at one by one.
const MOST: usize = 48;
const TICK: Duration = Duration::from_millis(300);
/// How long the program table must stay the same to count as settled: the
/// SDT, which names the programs, comes round every two seconds or so.
const SETTLED: Duration = Duration::from_millis(2500);

static NEXT: AtomicU64 = AtomicU64::new(1);

fn spec(req: &ScanRequest, program: Option<u16>) -> Value {
    let mut v = json!({"uri": req.uri.trim()});
    if let Some(card) = req.interface.as_deref().map(str::trim).filter(|c| !c.is_empty()) {
        v["params"] = json!({"interface": card});
    }
    if let Some(p) = program {
        v["program"] = json!(p);
    }
    v
}

/// Probes that close however the scan ends.
struct Opened<'a> {
    st: &'a Station,
    ids: Vec<String>,
}

impl Opened<'_> {
    fn open(&mut self, spec: Value) -> String {
        let id = format!("{PREFIX}{}", NEXT.fetch_add(1, Ordering::Relaxed));
        self.st.direct.probe(&id, spec);
        self.ids.push(id.clone());
        id
    }

    /// Close every probe. Answers the ask, for `settle`.
    fn close(&mut self) -> Option<u64> {
        (!self.ids.is_empty()).then(|| self.st.direct.unprobe(&std::mem::take(&mut self.ids)))
    }
}

impl Drop for Opened<'_> {
    fn drop(&mut self) {
        self.close();
    }
}

/// Wait until the host has the table that closed the whole feed's probe,
/// so a unicast port is free before the programs' probes open on it.
async fn settle(st: &Arc<Station>, asked: Option<u64>) {
    let Some(asked) = asked else { return };
    let st = st.clone();
    let _ = tokio::task::spawn_blocking(move || st.direct.wait_handed(asked, Duration::from_secs(3))).await;
}

pub async fn scan(st: &Arc<Station>, n: &Native, req: ScanRequest) -> Result<ScanResult, RpcError> {
    if !req.uri.contains("://") {
        return Err(RpcError::invalid_params(format!("'{}' is not a feed's address. Give it as udp://@239.0.0.1:5000 for multicast, or udp://0.0.0.0:5000 for a feed sent to this machine.", req.uri)));
    }
    let wait = Duration::from_millis(u64::from(req.wait_ms.unwrap_or(8000).clamp(2000, 20_000)));
    let started = Instant::now();
    let mut opened = Opened { st, ids: Vec::new() };
    let whole = opened.open(spec(&req, None));
    let (mut programs, mut error) = (Vec::<ScannedProgram>::new(), None);
    let mut since = Instant::now();
    while started.elapsed() < wait * 2 / 3 {
        tokio::time::sleep(TICK).await;
        let got = scan_read::stats(st, std::slice::from_ref(&whole)).await.map_err(RpcError::not_in_state)?;
        let input = got.get(&whole).cloned().unwrap_or(Value::Null);
        error = input["error"].as_str().map(String::from);
        let now = scan_read::listed(&input);
        if now != programs {
            programs = now;
            since = Instant::now();
        } else if !programs.is_empty() && since.elapsed() >= SETTLED {
            break;
        }
    }
    let asked = opened.close();
    settle(st, asked).await;
    programs.truncate(MOST);
    let ids: Vec<String> = programs.iter().map(|p| opened.open(spec(&req, Some(p.number)))).collect();
    while !ids.is_empty() && started.elapsed() < wait {
        tokio::time::sleep(TICK).await;
        let got = scan_read::stats(st, &ids).await.map_err(RpcError::not_in_state)?;
        for (p, id) in programs.iter_mut().zip(&ids) {
            p.tracks.clear();
            scan_read::fill(p, got.get(id), st.direct.probes.input(id).as_ref());
        }
        if programs.iter().zip(&ids).all(|(p, id)| scan_read::heard(p, got.get(id))) {
            break;
        }
    }
    opened.close();
    mark_taken(n, &req, &mut programs);
    let error = if programs.is_empty() { Some(error.unwrap_or_else(|| nothing(&req))) } else { None };
    Ok(ScanResult { uri: req.uri, programs, listened_ms: started.elapsed().as_millis() as u64, error })
}

fn nothing(req: &ScanRequest) -> String {
    format!("nothing arrived on {} in the time listened. Check the address and port, that the sender sends to it, and the card it is joined on (interface).", req.uri)
}

/// Name the channel that already takes each program from this feed.
fn mark_taken(n: &Native, req: &ScanRequest, programs: &mut [ScannedProgram]) {
    let s = n.store.lock();
    for p in programs.iter_mut() {
        p.channel = s.channels.iter().find(|c| c.input.uri.trim() == req.uri.trim() && c.input.program == Some(p.number)).map(|c| c.name.clone());
    }
}
