//! [`Segments`] over the HLS a direct show serves today: CMAF from the HLS
//! packager, asked on loopback the way the station's own `/hls` door asks
//! it. This stands in until the HLS branch's open TS access lands; the
//! facade names everything in Flussonic's shape either way, so swapping it
//! changes this file and nothing that reads the facade.
//!
//! CMAF keeps the video and each sound in playlists of their own, so this
//! offers no muxed `tracks-v1a1` and no `/mpegts`.

use super::bridge_parse::{self, Seg};
use crate::station::packager::wire::{PEER_HEADER, SHOW_HEADER};
use crate::station::state::Station;
use godwinmix_flussonic::facade::access::*;
use parking_lot::Mutex;
use std::collections::HashMap;
use std::hash::{Hash, Hasher};
use std::sync::Arc;
use std::time::{Duration, Instant};

const INIT_FOR: Duration = Duration::from_secs(5);

pub struct Cmaf {
    st: Arc<Station>,
    /// Init segments by path, with the id made from their bytes.
    inits: Mutex<HashMap<String, (Vec<u8>, String, Instant)>>,
}

fn not_yet(why: String) -> Missing {
    Missing::NotYet(why)
}

impl Cmaf {
    pub fn new(st: Arc<Station>) -> Cmaf {
        Cmaf { st, inits: Mutex::new(HashMap::new()) }
    }

    async fn get(&self, at: &At, path: &str) -> Result<Vec<u8>, Missing> {
        let Some((addr, secret)) = self.st.direct.hls.packager() else {
            return Err(not_yet(format!("show {}'s HLS packager is starting", at.show)));
        };
        let url = format!("http://{addr}/hls/{}/{path}", at.output);
        let asked = self.st.http.get(&url).bearer_auth(secret).header(SHOW_HEADER, &at.show).header(PEER_HEADER, "127.0.0.1:0").timeout(Duration::from_secs(10)).send().await;
        match asked {
            Ok(r) if r.status().is_success() => r.bytes().await.map(|b| b.to_vec()).map_err(|e| not_yet(format!("the packager's answer was cut off: {e}"))),
            Ok(r) if r.status() == reqwest::StatusCode::NOT_FOUND => Err(Missing::Gone(format!("show {} has no {path} in its HLS output", at.show))),
            Ok(r) => Err(not_yet(format!("the packager answered {} for {path}", r.status()))),
            Err(e) => Err(not_yet(format!("the packager did not answer: {e}"))),
        }
    }

    async fn text(&self, at: &At, path: &str) -> Result<String, Missing> {
        Ok(String::from_utf8_lossy(&self.get(at, path).await?).into_owned())
    }

    /// The tracks, each with its playlist's path under `/hls/<output>/`.
    async fn listed(&self, at: &At) -> Result<Vec<(Track, String)>, Missing> {
        let listed = bridge_parse::master(&self.text(at, "master.m3u8").await?);
        if listed.is_empty() {
            return Err(not_yet(format!("show {}'s HLS output lists nothing yet", at.show)));
        }
        Ok(listed)
    }

    /// The playlist of `sel`, its folder, and its whole segments.
    async fn playlist(&self, at: &At, sel: Select) -> Result<(String, u64, Vec<Seg>), Missing> {
        let listed = self.listed(at).await?;
        let want = match (sel.video, sel.audio) {
            (true, None) => listed.iter().find(|(t, _)| t.kind == Kind::Video),
            (false, Some(n)) => listed.iter().find(|(t, _)| t.kind == Kind::Audio && t.number == n),
            _ => return Err(Missing::Unsupported("CMAF segments carry the video and each sound apart; ask for tracks-v1 or tracks-a<n>".into())),
        };
        let (_, uri) = want.ok_or_else(|| Missing::Gone(format!("show {} has no such track", at.show)))?;
        let folder = uri.rsplit_once('/').map(|(f, _)| format!("{f}/")).unwrap_or_default();
        let (sequence, segs) = bridge_parse::media(&self.text(at, uri).await?);
        Ok((folder, sequence, segs))
    }

    /// An init segment and the id its bytes give it.
    async fn init_at(&self, at: &At, path: &str) -> Result<(Vec<u8>, String), Missing> {
        let key = format!("{}/{}/{path}", at.show, at.output);
        if let Some((b, id, when)) = self.inits.lock().get(&key) {
            if when.elapsed() < INIT_FOR {
                return Ok((b.clone(), id.clone()));
            }
        }
        let bytes = self.get(at, path).await?;
        let mut hasher = std::collections::hash_map::DefaultHasher::new();
        bytes.hash(&mut hasher);
        let id = format!("{:016x}", hasher.finish());
        let mut inits = self.inits.lock();
        inits.retain(|_, (_, _, when)| when.elapsed() < INIT_FOR * 4);
        inits.insert(key, (bytes.clone(), id.clone(), Instant::now()));
        Ok((bytes, id))
    }
}

impl Segments for Cmaf {
    fn tracks<'a>(&'a self, at: &'a At) -> Fut<'a, Result<Tracks, Missing>> {
        Box::pin(async move { Ok(Tracks { tracks: self.listed(at).await?.into_iter().map(|(t, _)| t).collect(), muxed: false }) })
    }

    fn window<'a>(&'a self, at: &'a At, sel: Select) -> Fut<'a, Result<Window, Missing>> {
        Box::pin(async move {
            let (folder, sequence, segs) = self.playlist(at, sel).await?;
            let mut segments = Vec::new();
            for s in segs {
                let init = match &s.init {
                    Some(i) => Some(self.init_at(at, &format!("{folder}{i}")).await?.1),
                    None => None,
                };
                segments.push(Segment { start_ms: s.start_ms, duration_ms: s.duration_ms, init, discontinuity: s.discontinuity });
            }
            Ok(Window { container: Container::Cmaf, sequence, segments })
        })
    }

    fn segment<'a>(&'a self, at: &'a At, sel: Select, start_s: i64) -> Fut<'a, Result<Vec<u8>, Missing>> {
        Box::pin(async move {
            let (folder, _, segs) = self.playlist(at, sel).await?;
            let s = segs.iter().find(|s| s.start_ms.div_euclid(1000) == start_s).ok_or_else(|| Missing::Gone("that segment has left the live window".into()))?;
            self.get(at, &format!("{folder}{}", s.uri)).await
        })
    }

    fn init<'a>(&'a self, at: &'a At, sel: Select, id: &'a str) -> Fut<'a, Result<Vec<u8>, Missing>> {
        Box::pin(async move {
            let (folder, _, segs) = self.playlist(at, sel).await?;
            let mut paths: Vec<String> = segs.iter().filter_map(|s| s.init.clone()).collect();
            paths.dedup();
            for p in paths {
                let (bytes, got) = self.init_at(at, &format!("{folder}{p}")).await?;
                if got == id {
                    return Ok(bytes);
                }
            }
            Err(Missing::Gone("that init segment is no longer served".into()))
        })
    }
}
