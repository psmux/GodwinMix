//! An HLS output's MPEG-TS, for a caller inside the station that serves it
//! under its own paths and names: the Flussonic facade. No viewer key is
//! asked for; the caller decides who may read.
//!
//! ```ignore
//! let hls = &st.direct.hls;
//! let master = hls.ts_master(&st, "news", "news").await?;      // ts::Master
//! let text = master.render(|id| match id {
//!     None => "tracks-v1a1/mono.m3u8".into(),                      // main
//!     Some(id) => format!("tracks-a{}/mono.m3u8", n_of(id) + 1),   // "audio1" is track 1
//! });
//! let media = hls.ts_media(&st, "news", "news", "main").await?;  // ts::Media
//! let text = media.render(|e| ts::clock_name(e));                  // 2026/10/23/08/04/12-2000.ts
//! let (unix_s, duration_ms) = ts::parse_clock(name)?;
//! let bytes = hls.ts_segment(&st, "news", "news", "main", At::Clock { unix_s, duration_ms }).await?;
//! ```
//!
//! A rendition is `main` (the picture with sound track 0 muxed in) or
//! `audio<n>` (sound track n alone, n from 1); [`ts::Master`] lists every
//! sound with its track number, language and name. Each call is one
//! request to the HLS packager on loopback.

use super::Packagers;
use crate::station::packager::ts_serve::{MASTER, MEDIA, SEGMENT};
use crate::station::packager::wire::SHOW_HEADER;
use crate::station::state::Station;
use bytes::Bytes;
use godwinmix_core::hls::ts::{self, At};

/// Why a TS file could not be had: an HTTP status to answer with, and a
/// sentence for the body.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct TsRefusal {
    pub status: u16,
    pub message: String,
}

impl TsRefusal {
    fn new(status: u16, message: impl Into<String>) -> TsRefusal {
        TsRefusal { status, message: message.into() }
    }
}

impl Packagers {
    /// The output's TS multivariant playlist as data.
    pub async fn ts_master(&self, st: &Station, show: &str, output: &str) -> Result<ts::Master, TsRefusal> {
        let answer = self.ask_ts(st, show, MASTER, &[("output", output)]).await?;
        answer.json().await.map_err(|e| TsRefusal::new(502, format!("the HLS packager answered oddly ({e})")))
    }

    /// One TS rendition's media playlist as data.
    pub async fn ts_media(&self, st: &Station, show: &str, output: &str, rendition: &str) -> Result<ts::Media, TsRefusal> {
        let answer = self.ask_ts(st, show, MEDIA, &[("output", output), ("rendition", rendition)]).await?;
        answer.json().await.map_err(|e| TsRefusal::new(502, format!("the HLS packager answered oddly ({e})")))
    }

    /// One TS segment's bytes.
    pub async fn ts_segment(&self, st: &Station, show: &str, output: &str, rendition: &str, at: At) -> Result<Bytes, TsRefusal> {
        let at = serde_json::to_string(&at).unwrap_or_default();
        let answer = self.ask_ts(st, show, SEGMENT, &[("output", output), ("rendition", rendition), ("at", &at)]).await?;
        answer.bytes().await.map_err(|e| TsRefusal::new(502, format!("the HLS packager's answer broke off ({e})")))
    }

    async fn ask_ts(&self, st: &Station, show: &str, path: &str, query: &[(&str, &str)]) -> Result<reqwest::Response, TsRefusal> {
        let output = query.first().map(|(_, o)| *o).unwrap_or_default();
        if self.stream(show, output).is_none() {
            return Err(TsRefusal::new(404, format!("show {show} has no HLS output `{output}`")));
        }
        let Some((addr, secret)) = self.packager() else {
            return Err(TsRefusal::new(503, "the HLS packager is starting; try again in a few seconds"));
        };
        let sent = st.http.get(format!("http://{addr}{path}")).bearer_auth(secret).header(SHOW_HEADER, show).query(query);
        let answer = sent.timeout(std::time::Duration::from_secs(5)).send().await.map_err(|e| TsRefusal::new(503, format!("the HLS packager did not answer ({e})")))?;
        if answer.status().is_success() {
            return Ok(answer);
        }
        let status = answer.status().as_u16();
        let body: serde_json::Value = answer.json().await.unwrap_or_default();
        Err(TsRefusal::new(status, body["error"].as_str().unwrap_or("the HLS packager refused")))
    }
}
