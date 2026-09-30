//! One simulated player, and the counts every player adds to.

use parking_lot::Mutex;
use std::collections::HashSet;
use std::sync::atomic::{AtomicBool, AtomicU64, Ordering};
use std::sync::Arc;
use std::time::{Duration, Instant};

#[derive(Default)]
pub struct Totals {
    requests: AtomicU64,
    bytes: AtomicU64,
    errors: AtomicU64,
    /// Seconds each media fetch took. Playlist waits are not kept: nothing
    /// reports them.
    media: Mutex<Vec<f64>>,
}

impl Totals {
    fn count(&self, bytes: usize, media: Option<f64>, error: bool) {
        self.requests.fetch_add(1, Ordering::Relaxed);
        self.bytes.fetch_add(bytes as u64, Ordering::Relaxed);
        self.errors.fetch_add(u64::from(error), Ordering::Relaxed);
        if let Some(took) = media {
            self.media.lock().push(took);
        }
    }

    pub fn counts(&self) -> (u64, u64, u64) {
        let get = |a: &AtomicU64| a.load(Ordering::Relaxed);
        (get(&self.requests), get(&self.bytes), get(&self.errors))
    }

    pub fn media(&self) -> Vec<f64> {
        self.media.lock().clone()
    }
}

async fn get(client: &reqwest::Client, url: &str) -> Result<(Vec<u8>, f64), reqwest::Error> {
    let started = Instant::now();
    let body = client.get(url).send().await?.error_for_status()?.bytes().await?;
    Ok((body.to_vec(), started.elapsed().as_secs_f64()))
}

/// The quoted `URI="..."` of a tag line.
fn quoted(line: &str) -> Option<&str> {
    let at = line.find("URI=\"")? + 5;
    line[at..].find('"').map(|end| &line[at..at + end])
}

/// What a player fetches from a playlist: the init segment, then the parts
/// when the playlist has them (LL-HLS) or the segments when it does not.
pub fn uris(text: &str) -> Vec<String> {
    let tagged = |tag: &str| text.lines().filter(|l| l.starts_with(tag)).filter_map(quoted).map(String::from).collect::<Vec<_>>();
    let (mut out, parts) = (tagged("#EXT-X-MAP:"), tagged("#EXT-X-PART:"));
    if parts.is_empty() {
        out.extend(text.lines().filter(|l| !l.is_empty() && !l.starts_with('#')).map(String::from));
    } else {
        out.extend(parts);
    }
    out
}

/// A player joins near the live edge: the init, then the last few.
pub fn live_edge(found: &[String]) -> HashSet<String> {
    found[..found.len().saturating_sub(3)].iter().filter(|u| !u.contains("init")).cloned().collect()
}

/// The part the playlist hints, `(msn, part)`.
pub fn hint(text: &str) -> Option<(u64, u64)> {
    let line = text.lines().find(|l| l.starts_with("#EXT-X-PRELOAD-HINT:TYPE=PART"))?;
    let uri = quoted(line)?;
    let (m, rest) = uri.split_once('.')?;
    let (p, _) = rest.split_once(".m4s")?;
    Some((m.parse().ok()?, p.parse().ok()?))
}

fn join(base: &str, rel: &str) -> String {
    reqwest::Url::parse(base).and_then(|b| b.join(rel)).map(String::from).unwrap_or_else(|_| rel.to_string())
}

pub async fn viewer(client: reqwest::Client, master: String, n: usize, t: Arc<Totals>, stop: Arc<AtomicBool>) {
    let Ok((text, _)) = get(&client, &master).await else { return t.count(0, None, true) };
    let text = String::from_utf8_lossy(&text).into_owned();
    let variants: Vec<&str> = text.lines().filter(|l| !l.is_empty() && !l.starts_with('#')).collect();
    let Some(rel) = variants.get(n % variants.len().max(1)) else { return t.count(0, None, true) };
    let rung = join(&master, rel);
    let (mut seen, mut next): (Option<HashSet<String>>, Option<(u64, u64)>) = (None, None);
    while !stop.load(Ordering::Relaxed) {
        let mut url = rung.clone();
        if let Some((m, p)) = next {
            url.push_str(&format!("{}_HLS_msn={m}&_HLS_part={p}", if url.contains('?') { '&' } else { '?' }));
        }
        let body = match get(&client, &url).await {
            Ok((body, _)) => body,
            Err(_) => {
                t.count(0, None, true);
                tokio::time::sleep(Duration::from_secs(1)).await;
                next = None;
                continue;
            }
        };
        t.count(body.len(), None, false);
        let text = String::from_utf8_lossy(&body).into_owned();
        let found = uris(&text);
        let seen = seen.get_or_insert_with(|| live_edge(&found));
        for u in found {
            if seen.insert(u.clone()) {
                match get(&client, &join(&rung, &u)).await {
                    Ok((data, took)) => t.count(data.len(), Some(took), false),
                    Err(_) => t.count(0, None, true),
                }
            }
        }
        next = hint(&text);
        if next.is_none() {
            tokio::time::sleep(Duration::from_secs(1)).await;
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn a_low_latency_playlist_gives_the_init_the_parts_and_the_hint() {
        let text = "#EXTM3U\n#EXT-X-MAP:URI=\"init.mp4?key=k\"\n#EXT-X-PART:DURATION=0.3,URI=\"7.0.m4s?key=k\",INDEPENDENT=YES\n\
                    #EXT-X-PART:DURATION=0.3,URI=\"7.1.m4s?key=k\"\n#EXTINF:2.0,\n7.m4s?key=k\n#EXT-X-PRELOAD-HINT:TYPE=PART,URI=\"8.0.m4s?key=k\"\n";
        assert_eq!(uris(text), ["init.mp4?key=k", "7.0.m4s?key=k", "7.1.m4s?key=k"]);
        assert_eq!(hint(text), Some((8, 0)));
        let plain = "#EXTM3U\n#EXT-X-MAP:URI=\"init.mp4\"\n#EXTINF:2.0,\n1.m4s\n#EXTINF:2.0,\n2.m4s\n";
        assert_eq!(uris(plain), ["init.mp4", "1.m4s", "2.m4s"]);
        assert_eq!(hint(plain), None);
    }
}
