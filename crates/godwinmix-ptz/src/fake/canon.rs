//! A fake Canon CR-N camera: `control.cgi`, `info.cgi` and `preset/set`
//! behind a Digest login, answering in `name:=value` lines with a
//! `Livescope-Status` header, as the XC protocol does.

use std::net::SocketAddr;

use super::http::{serve, Reply};
use super::{Head, Shared};
use crate::canon::auth::{authorization, Challenge};
use crate::canon::Ranges;
use crate::types::{ExposureMode, Focus, Velocity};

pub const USER: &str = "operator";
pub const PASSWORD: &str = "canon-pw";
const REALM: &str = "fake-canon";
const NONCE: &str = "0123456789abcdef";

fn authorised(request: &str, uri: &str) -> bool {
    let Some(line) = request.lines().find(|l| l.to_ascii_lowercase().starts_with("authorization:")) else { return false };
    let field = |k: &str| line.split(&format!("{k}=")).nth(1).map(|v| v.trim_start_matches('"').split(['"', ',']).next().unwrap_or("").to_string());
    let (Some(nc), Some(cnonce)) = (field("nc"), field("cnonce")) else { return false };
    let c = Challenge::Digest { realm: REALM.into(), nonce: NONCE.into(), opaque: None, qop: true };
    let expected = authorization(&c, USER, PASSWORD, "GET", uri, u32::from_str_radix(&nc, 16).unwrap_or(0), &cnonce);
    field("response") == expected.split("response=\"").nth(1).map(|v| v.split('"').next().unwrap_or("").to_string())
}

fn speed(pairs: &[(&str, &str)], name: &str, max: i32) -> f32 {
    pairs.iter().find(|(k, _)| *k == format!("{name}.speed.dir")).and_then(|(_, v)| v.parse::<f32>().ok()).unwrap_or(0.0) / max as f32
}

/// Apply one control query and answer its echo lines, or `None` for a key
/// this camera does not know.
fn control(head: &mut Head, pairs: &[(&str, &str)]) -> Option<String> {
    let r = Ranges::default();
    let mut v = head.velocity;
    let mut echo = String::new();
    for (k, val) in pairs {
        let mut ok = true;
        match (*k, *val) {
            ("pan", "right" | "left") => v.pan = speed(pairs, "pan", r.pan_speed) * if *val == "right" { 1.0 } else { -1.0 },
            ("tilt", "up" | "down") => v.tilt = speed(pairs, "tilt", r.tilt_speed) * if *val == "up" { 1.0 } else { -1.0 },
            ("zoom", "tele" | "wide") => v.zoom = speed(pairs, "zoom", r.zoom_speed) * if *val == "tele" { 1.0 } else { -1.0 },
            ("pan", "stop") => v.pan = 0.0,
            ("tilt", "stop") => v.tilt = 0.0,
            ("zoom", "stop") => v.zoom = 0.0,
            ("pan" | "tilt" | "zoom", n) => {
                let n: f32 = n.parse().ok()?;
                let (mut p, mut t, mut z) = (head.pan, head.tilt, head.zoom);
                match *k {
                    "pan" => p = if n >= 0.0 { n / r.pan.1 as f32 } else { -n / r.pan.0 as f32 },
                    "tilt" => t = if n >= 0.0 { n / r.tilt.1 as f32 } else { -n / r.tilt.0 as f32 },
                    _ => z = (r.zoom.1 as f32 - n) / (r.zoom.1 - r.zoom.0) as f32,
                }
                head.jump(p, t, z);
                v = Velocity::STOP;
            }
            ("focus", m) => head.focus(if m == "auto" { Focus::Auto } else { Focus::Manual }),
            ("focus.action", a) => {
                ok = !head.focus_auto;
                if ok {
                    head.focus(Focus::Drive(match a { "far" => 1.0, "near" => -1.0, _ => 0.0 }));
                }
            }
            ("c.1.focus.action", "one_shot") => head.focus(Focus::OnePush),
            ("p", n) => {
                head.recall(n.parse().ok()?);
                v = Velocity::STOP;
            }
            ("c.1.exp", m) => head.exposure.mode = Some(match m { "manual" => ExposureMode::Manual, "tv" => ExposureMode::ShutterPriority, "av" => ExposureMode::IrisPriority, _ => ExposureMode::Auto }),
            ("c.1.me.shutter", n) => head.exposure.shutter = n.parse().ok(),
            ("c.1.me.diaphragm", n) => head.exposure.iris = n.parse().ok(),
            ("c.1.me.gain", n) => head.exposure.gain = n.parse().ok(),
            (k, _) if k.ends_with(".speed.dir") || k.ends_with(".mode") || k == "focus.speed" => {}
            _ => return None,
        }
        echo.push_str(&if ok { format!("{k}:={val}\n") } else { format!("{k}==?\n") });
    }
    if v != head.velocity {
        head.set_velocity(v);
    }
    Some(echo)
}

fn info(head: &mut Head) -> String {
    let r = Ranges::default();
    let p = head.position();
    let pan = if p.pan >= 0.0 { p.pan * r.pan.1 as f32 } else { -p.pan * r.pan.0 as f32 };
    let tilt = if p.tilt >= 0.0 { p.tilt * r.tilt.1 as f32 } else { -p.tilt * r.tilt.0 as f32 };
    let zoom = r.zoom.1 as f32 - p.zoom * (r.zoom.1 - r.zoom.0) as f32;
    format!("c.1.pan:={}\nc.1.tilt:={}\nc.1.zoom:={}\n", pan.round(), tilt.round(), zoom.round())
}

/// The camera on a loopback port. Answers `host:port` for the driver.
pub fn serve_canon(head: Shared) -> SocketAddr {
    serve(move |request| {
        let uri = request.split_whitespace().nth(1).unwrap_or("/").to_string();
        if !authorised(request, &uri) {
            let challenge = format!("WWW-Authenticate: Digest realm=\"{REALM}\", nonce=\"{NONCE}\", qop=\"auth\"");
            return Reply { status: 401, headers: vec![challenge], body: String::new() };
        }
        let (path, query) = uri.split_once('?').unwrap_or((&uri, ""));
        let pairs: Vec<(&str, &str)> = query.split('&').filter_map(|kv| kv.split_once('=')).collect();
        let mut h = head.lock().unwrap();
        let body = match path.trim_start_matches("/-wvhttp-01-/") {
            "control.cgi" => control(&mut h, &pairs),
            "info.cgi" => Some(info(&mut h)),
            "preset/set" => pairs.iter().find(|(k, _)| *k == "p").and_then(|(_, n)| n.parse().ok()).map(|n| {
                h.save(n);
                format!("p:={n}\n")
            }),
            _ => None,
        };
        let status = if body.is_some() { "0" } else { "401" };
        Reply { status: 200, headers: vec![format!("Livescope-Status: {status}")], body: body.unwrap_or_default() }
    })
}
