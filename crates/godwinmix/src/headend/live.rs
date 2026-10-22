//! What each stream is doing, read once a second off its show, in the
//! shape the API answers with, and the log lines a reader counts.
//!
//! Everything here is read from what the station already keeps (the host's
//! last `direct.input`, the HLS output's state); the only request it makes
//! is for each live output's master playlist, every five seconds, for the
//! tracks it serves.

use super::live_tracks;
use super::state::Headend;
use godwinmix_flussonic::api::Live;
use godwinmix_flussonic::clock::now_ms;
use godwinmix_flussonic::import::HLS_OUTPUT;
use godwinmix_flussonic::log::Event;
use godwinmix_protocol::destination::DestinationState;
use serde_json::Value;
use std::collections::BTreeMap;
use std::sync::Arc;
use std::time::Duration;

/// A held condition is logged again this often, as Flussonic logs each
/// time it closes and opens the input again.
const REPEAT_MS: i64 = 60_000;
const TRACKS_EVERY: u64 = 5;

/// What one stream carries from one reading to the next.
#[derive(Default)]
struct Memory {
    cc_errors: Option<u64>,
    scrambled_logged: Option<i64>,
    refused_logged: Option<i64>,
    scrambled_at: Vec<i64>,
}

fn scrambled(input: &Value) -> bool {
    let flagged = input["scrambled"].as_bool().unwrap_or(false) || input["scrambled"].as_u64().is_some_and(|n| n > 0);
    let said = input["error"].as_str().is_some_and(|e| e.contains("scrambled") || e.contains("encrypted"));
    flagged || said
}

/// An output error that means the GPU gave no encoder session.
fn refused(error: Option<&str>) -> bool {
    error.is_some_and(|e| {
        let e = e.to_ascii_lowercase();
        (e.contains("session") && (e.contains("encod") || e.contains("nvenc"))) || e.contains("governor-refused")
    })
}

/// Log `e` when it starts holding, and again every minute while it holds.
fn due(last: &mut Option<i64>, holds: bool, now: i64) -> bool {
    match (holds, *last) {
        (false, _) => {
            *last = None;
            false
        }
        (true, Some(t)) if now - t < REPEAT_MS => false,
        (true, _) => {
            *last = Some(now);
            true
        }
    }
}

fn read(h: &Headend, name: &str, id: &str, copy: bool, mem: &mut Memory) -> Live {
    let seen = h.st.direct.seen.lock().get(id).cloned().unwrap_or_default();
    let input = seen.input.clone().unwrap_or(Value::Null);
    let (out, _) = h.st.direct.hls.view(id, HLS_OUTPUT).unwrap_or_default();
    let now = now_ms();
    let cc = input["cc_errors"].as_u64().unwrap_or(0);
    let error_rate = mem.cc_errors.map(|before| cc.saturating_sub(before) as f64).unwrap_or(0.0);
    mem.cc_errors = Some(cc);
    let is_scrambled = scrambled(&input);
    if due(&mut mem.scrambled_logged, is_scrambled, now) {
        mem.scrambled_at.push(now);
        mem.scrambled_at.retain(|t| now - t < 600_000);
        h.log_line(name, Event::Scrambled);
    }
    if due(&mut mem.refused_logged, refused(out.error.as_deref()), now) {
        h.log_line(name, Event::EncoderRefused);
    }
    let tracks_in = live_tracks::input(&input);
    let output_live = out.state == DestinationState::Live;
    let tracks_out = match h.tracks.read().get(name) {
        Some(t) => live_tracks::output(t, out.kbps),
        None if copy => tracks_in.iter().map(|t| godwinmix_flussonic::api::MediaTrack { pid: None, ..t.clone() }).collect(),
        None => Vec::new(),
    };
    let last_frame_s = input["last_frame_ms"].as_u64().unwrap_or(0) / 1000;
    Live {
        input_live: seen.input_live(),
        output_live,
        bitrate_kbps: out.kbps,
        input_kbps: input["kbps"].as_u64().unwrap_or(0) as u32,
        lifetime_ms: if output_live { out.since_ms } else { 0 },
        tracks_out,
        tracks_in,
        scrambled_at: mem.scrambled_at.clone(),
        error_rate,
        no_data_s: if seen.input_live() { 0 } else { last_frame_s },
        restarts: out.reconnects,
        error: out.error.clone().or_else(|| input["error"].as_str().map(String::from)),
    }
}

async fn refresh_tracks(h: &Headend) {
    let ats: Vec<(String, godwinmix_flussonic::facade::access::At)> = {
        let live = h.live.read();
        h.running.lock().iter().filter(|(n, _)| live.get(*n).is_some_and(|l| l.output_live)).filter_map(|(n, _)| h.at(n).map(|a| (n.clone(), a))).collect()
    };
    let asks = ats.iter().map(|(n, at)| async move { (n.clone(), h.segments.tracks(at).await) });
    for (name, got) in futures_util::future::join_all(asks).await {
        if let Ok(t) = got {
            h.tracks.write().insert(name, t);
        }
    }
}

pub async fn keep(h: Arc<Headend>) {
    let mut memory: BTreeMap<String, Memory> = BTreeMap::new();
    let mut tick = tokio::time::interval(Duration::from_secs(1));
    for n in 0u64.. {
        tick.tick().await;
        let running: Vec<(String, String, bool)> = h.running.lock().iter().map(|(n, a)| (n.clone(), a.id.clone(), a.plan.copy)).collect();
        memory.retain(|n, _| running.iter().any(|(r, _, _)| r == n));
        let mut next = BTreeMap::new();
        for (name, id, copy) in &running {
            let live = read(&h, name, id, *copy, memory.entry(name.clone()).or_default());
            next.insert(name.clone(), live);
        }
        *h.live.write() = next;
        h.tracks.write().retain(|n, _| running.iter().any(|(r, _, _)| r == n));
        if n % TRACKS_EVERY == 0 {
            refresh_tracks(&h).await;
        }
    }
}
