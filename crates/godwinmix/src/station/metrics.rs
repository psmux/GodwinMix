//! `GET /metrics/shows`: every show's health and numbers in Prometheus text.
//!
//! The same numbers `show.stats` answers with, so a wall of fifty channels
//! can be watched and alarmed on in Grafana without a scraper of its own. It
//! is read from what the station already holds, at the price of one
//! `show.stats`, and only when something scrapes it. `/metrics` itself still
//! goes on to a show's own mixer, as it always has.

use super::state::Station;
use axum::extract::State;
use axum::http::{header, HeaderMap, Method, StatusCode, Uri};
use axum::response::{IntoResponse, Response};
use godwinmix_protocol::health::HealthState;
use godwinmix_protocol::scope::Scope;
use godwinmix_protocol::shows::{ShowStats, ShowStatsList, ShowStatsRequest};
use std::fmt::Write;
use std::sync::Arc;

pub async fn shows(State(st): State<Arc<Station>>, headers: HeaderMap, uri: Uri) -> Response {
    let presented = crate::control::presented_token(&Method::GET, &headers, &uri);
    match st.tokens.authenticate(presented.as_deref()) {
        Ok(t) if t.has(Scope::Read) => {}
        Ok(_) => return (StatusCode::FORBIDDEN, "this token does not carry the read scope, which /metrics/shows needs\n").into_response(),
        Err(f) => return (StatusCode::UNAUTHORIZED, format!("{}. Send it as `Authorization: Bearer <token>`.\n", f.message())).into_response(),
    }
    let list = match super::shows_stats::stats(&st, ShowStatsRequest::default()).await {
        Ok(v) => serde_json::from_value::<ShowStatsList>(v).unwrap_or(ShowStatsList { shows: Vec::new() }),
        Err(e) => return (StatusCode::SERVICE_UNAVAILABLE, format!("{}\n", e.message)).into_response(),
    };
    ([(header::CONTENT_TYPE, "text/plain; version=0.0.4")], render(&list.shows)).into_response()
}

/// A label value with Prometheus's three escapes.
fn label(v: &str) -> String {
    v.replace('\\', "\\\\").replace('"', "\\\"").replace('\n', "\\n")
}

/// One family: its help, its type, and a line for each value it has.
fn family(out: &mut String, name: &str, kind: &str, help: &str, rows: &[(String, f64)]) {
    let _ = writeln!(out, "# HELP {name} {help}\n# TYPE {name} {kind}");
    for (labels, value) in rows {
        let _ = writeln!(out, "{name}{{{labels}}} {value}");
    }
}

pub fn render(shows: &[ShowStats]) -> String {
    let show = |s: &ShowStats| format!("show=\"{}\"", label(&s.id));
    let health = |s: &ShowStats| match s.health.state {
        HealthState::Ok => 0.0,
        HealthState::Warning => 1.0,
        _ => 2.0,
    };
    let per_show = |f: &dyn Fn(&ShowStats) -> Option<f64>| shows.iter().filter_map(|s| f(s).map(|v| (show(s), v))).collect::<Vec<_>>();
    let mut out = String::new();
    family(&mut out, "gmx_show_health", "gauge", "0 ok, 1 warning, 2 alarm.", &per_show(&|s| Some(health(s))));
    family(&mut out, "gmx_show_alarms", "gauge", "Alarms that hold now.", &per_show(&|s| Some(s.health.alarms.len() as f64)));
    family(&mut out, "gmx_show_cpu_millicores", "gauge", "What the show costs the machine, thousandths of a core.", &per_show(&|s| s.cpu_millicores.map(f64::from)));
    family(&mut out, "gmx_show_input_kbps", "gauge", "Input bitrate.", &per_show(&|s| s.input.as_ref().map(|i| f64::from(i.kbps))));
    family(&mut out, "gmx_show_input_fps", "gauge", "Input frames a second.", &per_show(&|s| s.input.as_ref().map(|i| i.fps)));
    family(&mut out, "gmx_show_input_cc_errors_total", "counter", "MPEG-TS continuity errors on the input.", &per_show(&|s| s.input.as_ref().map(|i| i.cc_errors as f64)));
    family(&mut out, "gmx_show_input_packets_lost_total", "counter", "Packets lost on the input.", &per_show(&|s| s.input.as_ref().map(|i| i.packets_lost as f64)));
    family(&mut out, "gmx_show_input_last_frame_ms", "gauge", "Milliseconds since the input's last frame.", &per_show(&|s| s.input.as_ref().and_then(|i| i.last_frame_ms).map(|v| v as f64)));
    let per_output = |f: &dyn Fn(&godwinmix_protocol::shows::OutputStats) -> f64| {
        shows
            .iter()
            .flat_map(|s| s.outputs.iter().map(move |o| (format!("{},output=\"{}\"", show(s), label(&o.id)), f(o))))
            .collect::<Vec<_>>()
    };
    family(&mut out, "gmx_show_output_live", "gauge", "1 while the output is live.", &per_output(&|o| if o.state == "live" { 1.0 } else { 0.0 }));
    family(&mut out, "gmx_show_output_kbps", "gauge", "Output bitrate.", &per_output(&|o| f64::from(o.kbps)));
    family(&mut out, "gmx_show_output_reconnects_total", "counter", "Times the output has reconnected.", &per_output(&|o| f64::from(o.reconnects)));
    out
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn every_show_and_output_has_a_line_and_the_labels_are_escaped() {
        let shows: Vec<ShowStats> = serde_json::from_value(serde_json::json!([
            { "id": "news", "health": { "state": "alarm", "alarms": [] }, "work": "transcode", "cpu_millicores": 120,
              "input": { "kbps": 4000, "fps": 29.97, "width": 1280, "height": 720, "audio_channels": 2, "cc_errors": 3, "packets_lost": 0, "last_frame_ms": 40 },
              "outputs": [{ "id": "hls", "state": "live", "kbps": 3000, "reconnects": 1, "rendition_text": "", "cpu_millicores": 0, "steps": [] }] },
            { "id": "odd\"name", "health": { "state": "ok", "alarms": [] }, "outputs": [] }
        ]))
        .unwrap();
        let text = render(&shows);
        assert!(text.contains("gmx_show_health{show=\"news\"} 2"), "{text}");
        assert!(text.contains("gmx_show_input_fps{show=\"news\"} 29.97"));
        assert!(text.contains("gmx_show_input_cc_errors_total{show=\"news\"} 3"));
        assert!(text.contains("gmx_show_output_live{show=\"news\",output=\"hls\"} 1"));
        assert!(text.contains("gmx_show_health{show=\"odd\\\"name\"} 0"), "a quote is escaped");
        assert!(!text.contains("gmx_show_input_kbps{show=\"odd"), "no input, no line");
    }
}
