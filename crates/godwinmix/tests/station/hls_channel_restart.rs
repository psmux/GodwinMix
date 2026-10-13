//! A watch link across a restart of its channel's feed: the encoder stops
//! and publishes again, and the picture and the sound playlists must still
//! agree, segment by segment, on the discontinuity each one is in. When they
//! did not, hls.js loaded nothing for a new viewer.

use super::direct_live::staged_ingest;
use super::hls_channel::{encoder_with, until};
use super::hls_direct::{discover, folder_with_relay, text};
use super::support::*;
use gstreamer::prelude::*;
use serde_json::json;
use std::collections::BTreeMap;
use std::time::{Duration, Instant};

/// One segment as a player numbers it: its discontinuity and its wall clock.
#[derive(Debug, Clone, PartialEq)]
struct Seg {
    disc: u64,
    pdt: String,
}

/// A media playlist's segments by media sequence number, with the
/// discontinuity each one is in, the init segment it needs and its URI.
fn segments(playlist: &str) -> BTreeMap<u64, (Seg, String, String)> {
    let num = |tag: &str| playlist.lines().find_map(|l| l.strip_prefix(tag)).and_then(|n| n.trim().parse().ok()).unwrap_or(0u64);
    let (mut msn, mut disc) = (num("#EXT-X-MEDIA-SEQUENCE:"), num("#EXT-X-DISCONTINUITY-SEQUENCE:"));
    let (mut pdt, mut map, mut out) = (String::new(), String::new(), BTreeMap::new());
    for line in playlist.lines() {
        if line == "#EXT-X-DISCONTINUITY" {
            disc += 1;
        } else if let Some(p) = line.strip_prefix("#EXT-X-PROGRAM-DATE-TIME:") {
            pdt = p.to_string();
        } else if let Some(m) = line.strip_prefix("#EXT-X-MAP:URI=\"") {
            map = m.split('"').next().unwrap_or_default().to_string();
        } else if !line.is_empty() && !line.starts_with('#') {
            assert!(line.starts_with(&format!("{msn}.m4s")), "segment {msn} is named {line}:\n{playlist}");
            out.insert(msn, (Seg { disc, pdt: pdt.clone() }, map.clone(), line.to_string()));
            msn += 1;
        }
    }
    out
}

/// The picture's and the sound's media playlists, by the URIs the
/// multivariant playlist gives.
async fn both(st: &Running, master: &str) -> (String, String, String, String) {
    let base = master.split('?').next().unwrap().trim_end_matches("index.m3u8").to_string();
    let (status, top) = text(st, master).await;
    assert_eq!(status, 200, "{top}");
    assert!(top.starts_with("#EXTM3U") && top.contains("mp4a.") && top.contains("avc1."), "{top}");
    let video = top.lines().find(|l| !l.starts_with('#') && !l.is_empty()).expect("a rung").to_string();
    let audio = top.lines().find_map(|l| l.split("URI=\"").nth(1)).and_then(|r| r.split('"').next()).expect("the sound").to_string();
    let v = text(st, &format!("{base}{video}")).await.1;
    let a = text(st, &format!("{base}{audio}")).await.1;
    (format!("{base}{}", &video[..=video.find('/').unwrap()]), v, format!("{base}{}", &audio[..=audio.find('/').unwrap()]), a)
}

/// Every segment number both playlists list is in the same discontinuity
/// and starts at the same moment, give or take one sound frame and the
/// muxer's rounding. Answers the newest discontinuity.
fn agree(v: &str, a: &str) -> u64 {
    let (sv, sa) = (segments(v), segments(a));
    let mut shared = 0;
    for (msn, (seg, _, _)) in &sv {
        let Some((other, _, _)) = sa.get(msn) else { continue };
        shared += 1;
        assert_eq!(seg.disc, other.disc, "segment {msn} is in discontinuity {} on the picture and {} on the sound:\n{v}\n{a}", seg.disc, other.disc);
        let (pv, pa) = (millis(&seg.pdt), millis(&other.pdt));
        assert!((pv - pa).abs() <= 500, "segment {msn} starts at {} on the picture and {} on the sound:\n{v}\n{a}", seg.pdt, other.pdt);
    }
    assert!(shared >= 3, "the two playlists share {shared} segments:\n{v}\n{a}");
    sv.values().map(|(s, _, _)| s.disc).max().unwrap_or(0)
}

/// Milliseconds of the day in an `EXT-X-PROGRAM-DATE-TIME`.
fn millis(pdt: &str) -> i64 {
    let t = pdt.split('T').nth(1).unwrap_or_default().trim_end_matches('Z');
    let mut parts = t.split(':').map(|p| p.parse::<f64>().unwrap_or(0.0));
    let (h, m, s) = (parts.next().unwrap_or(0.0), parts.next().unwrap_or(0.0), parts.next().unwrap_or(0.0));
    ((h * 3600.0 + m * 60.0 + s) * 1000.0).round() as i64
}

/// The newest whole segment of one rung behind the init segment it names,
/// as a new player would fetch them.
async fn newest(st: &Running, dir: &str, playlist: &str, file: &std::path::Path) -> String {
    let (_, (_, map, uri)) = segments(playlist).into_iter().last().expect("a segment");
    let get = |p: String| async move {
        let answer = reqwest::get(format!("http://{}{dir}{p}", st.url)).await.unwrap();
        assert!(answer.status().is_success(), "{dir}{p}: {}", answer.status());
        answer.bytes().await.unwrap().to_vec()
    };
    let mut body = get(map).await;
    body.extend(get(uri).await);
    std::fs::write(file, body).unwrap();
    discover(file)
}

#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
async fn a_watch_link_marks_a_feed_restart_the_same_on_picture_and_sound() {
    if !godwinmix_core::probe::have_or_skip("cmafmux") || !godwinmix_core::probe::have_or_skip("rtmp2sink") {
        return;
    }
    let (dir, port) = folder_with_relay("hls-restart");
    let rtmp: u16 = std::fs::read_to_string(dir.join("godwinmix.toml")).unwrap().rsplit("rtmp_port = ").next().unwrap().trim().parse().unwrap();
    let source = staged_ingest(&dir);
    let st = start(dir.clone(), port, &[]).await;
    let mut ws = rpc(&st, "").await;
    let added = call(&mut ws, 1, "channel.add", json!({"name": "Sunday"})).await;
    let secret = added["result"]["key"]["secret"].as_str().unwrap_or_else(|| panic!("{added}")).to_string();
    let watch = call(&mut ws, 3, "channel.destination.add", json!({"id": "sunday", "platform": "hls", "server": "hls://?segment_ms=1000&window=8"})).await;
    assert!(watch.get("error").is_none(), "{watch}");
    let asked = call(&mut ws, 4, "plugin.add", json!({"source": source.to_string_lossy()})).await;
    assert!(asked.get("error").is_none(), "{asked}");
    tokio::time::sleep(Duration::from_secs(3)).await;
    let url = format!("rtmp://127.0.0.1:{rtmp}/sunday/main?psk={secret}");
    // A keyframe every two seconds against one second segments, as an
    // encoder left at its defaults sends: the picture is cut every two.
    let first = encoder_with(&url, 60);
    let link = until(&mut ws, "watch-link", |d| d["state"] == "live", Duration::from_secs(90)).await;
    let master = link["playback"]["master_url_path"].as_str().unwrap().to_string();
    tokio::time::sleep(Duration::from_secs(6)).await;
    let (_, v, _, a) = both(&st, &master).await;
    let before = agree(&v, &a);

    first.set_state(gstreamer::State::Null).unwrap();
    tokio::time::sleep(Duration::from_secs(4)).await;
    let second = encoder_with(&url, 60);
    // Until the break is inside both windows and the two windows start at
    // the same segment, which they do once the sound's newest segment is
    // as whole as the picture's.
    let limit = Instant::now() + Duration::from_secs(60);
    let after = |p: &str| segments(p).into_iter().filter(|(_, (s, _, _))| s.disc > before).map(|(m, _)| m).collect::<Vec<_>>();
    let (vdir, v, adir, a) = loop {
        let (vd, v, ad, a) = both(&st, &master).await;
        let same_window = segments(&v).keys().next() == segments(&a).keys().next();
        if after(&v).len() >= 3 && after(&a).len() >= 3 && same_window && after(&v).len() < segments(&v).len() {
            break (vd, v, ad, a);
        }
        assert!(Instant::now() < limit, "the two playlists never listed the same window across the restart:\n{v}\n{a}");
        tokio::time::sleep(Duration::from_millis(250)).await;
    };
    assert_eq!(agree(&v, &a), before + 1, "one restart is one discontinuity:\n{v}\n{a}");
    assert_eq!(after(&v).first(), after(&a).first(), "the break falls before the same segment:\n{v}\n{a}");
    let seq = |p: &str| p.lines().find(|l| l.starts_with("#EXT-X-DISCONTINUITY-SEQUENCE")).map(str::to_string);
    let count = |p: &str| p.matches("#EXT-X-DISCONTINUITY\n").count();
    assert_eq!((seq(&v), count(&v)), (seq(&a), count(&a)), "the same window says the same:\n{v}\n{a}");
    // A player arriving now plays both from the newest segments.
    let seen = newest(&st, &vdir, &v, &dir.join("after-v.mp4")).await;
    assert!(seen.contains("H.264") && !seen.contains("rror"), "{seen}");
    let seen = newest(&st, &adir, &a, &dir.join("after-a.mp4")).await;
    assert!((seen.contains("AAC") || seen.contains("MPEG-4")) && !seen.contains("rror"), "{seen}");
    second.set_state(gstreamer::State::Null).unwrap();
}
