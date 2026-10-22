//! The typed view: what GodwinMix runs, read from the syntax tree.
//!
//! Every directive this module does not know is listed in `unknown` (with
//! its line) so the server can say plainly which parts of a Flussonic config
//! it ignores, instead of silently running something different.

use std::net::Ipv4Addr;

use serde::Serialize;

use crate::syntax::{Directive, Node};
use crate::Error;

#[derive(Debug, Clone, Serialize, PartialEq)]
pub struct Config {
    pub listeners: Vec<Listen>,
    pub edit_auth: Option<Credentials>,
    pub view_auth: Option<Credentials>,
    pub session_log: Option<String>,
    /// Seconds. Global default for every stream.
    pub segment_duration: Option<f64>,
    pub segment_count: Option<u32>,
    pub dvrs: Vec<Dvr>,
    pub streams: Vec<Stream>,
    pub unknown: Vec<Unknown>,
}

#[derive(Debug, Clone, Serialize, PartialEq)]
pub struct Listen {
    pub addr: Option<String>,
    pub port: u16,
}

#[derive(Clone, Serialize, PartialEq)]
pub struct Credentials {
    pub user: String,
    #[serde(skip)]
    pub password: String,
}

impl std::fmt::Debug for Credentials {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        write!(f, "Credentials({}, <hidden>)", self.user)
    }
}

#[derive(Debug, Clone, Serialize, PartialEq)]
pub struct Dvr {
    pub name: String,
    pub root: Option<String>,
}

#[derive(Debug, Clone, Serialize, PartialEq)]
pub struct Unknown {
    pub scope: String,
    pub name: String,
    pub line: usize,
}

#[derive(Debug, Clone, Serialize, PartialEq)]
pub struct Stream {
    pub name: String,
    pub title: Option<String>,
    pub disabled: bool,
    /// Flussonic's `static` streams run without viewers. Every stream here
    /// runs all the time anyway; kept so it is written back.
    pub is_static: bool,
    pub inputs: Vec<Input>,
    pub transcoder: Option<Transcoder>,
    pub segment_duration: Option<f64>,
    pub segment_count: Option<u32>,
    pub dvr: Option<String>,
    pub pushes: Vec<String>,
    pub line: usize,
}

#[derive(Debug, Clone, Serialize, PartialEq)]
pub struct Input {
    /// The URL exactly as written.
    pub url: String,
    pub source: Source,
}

#[derive(Debug, Clone, Serialize, PartialEq)]
#[serde(tag = "kind", rename_all = "snake_case")]
pub enum Source {
    /// `udp://` or `mpts-udp://`, unicast or multicast. One socket per
    /// group and port is shared by every stream that takes a program from it.
    Udp {
        mpts: bool,
        /// An interface name (`eno1`) or a local address.
        interface: Option<String>,
        group: Ipv4Addr,
        port: u16,
        program: Option<u16>,
    },
    /// Anything FFmpeg can open: rtmp, rtsp, srt, http(s), file.
    Url { scheme: String },
}

#[derive(Debug, Clone, Serialize, PartialEq)]
pub struct Transcoder {
    pub hw: Hw,
    pub device_id: u32,
    pub gop: Option<u32>,
    pub deinterlace: Deinterlace,
    /// `frame` keeps the source frame rate, `field` doubles it from
    /// interlaced sources.
    pub deinterlace_rate: DeinterlaceRate,
    pub video: Vec<VideoTrack>,
    pub audio: Vec<AudioTrack>,
    /// Options read but not acted on, as `key=value`.
    pub unused: Vec<String>,
}

#[derive(Debug, Clone, Copy, Serialize, PartialEq, Eq)]
#[serde(rename_all = "snake_case")]
pub enum Hw {
    Cpu,
    Nvenc,
    Qsv,
    Other,
}

#[derive(Debug, Clone, Copy, Serialize, PartialEq, Eq)]
#[serde(rename_all = "snake_case")]
pub enum Deinterlace {
    Off,
    On,
    /// Only when the source says it is interlaced.
    Auto,
}

#[derive(Debug, Clone, Copy, Serialize, PartialEq, Eq)]
#[serde(rename_all = "snake_case")]
pub enum DeinterlaceRate {
    Frame,
    Field,
}

#[derive(Debug, Clone, Copy, Serialize, PartialEq, Eq)]
#[serde(tag = "mode", content = "bps", rename_all = "snake_case")]
pub enum Bitrate {
    Copy,
    Bps(u64),
}

#[derive(Debug, Clone, Serialize, PartialEq)]
pub struct VideoTrack {
    pub bitrate: Bitrate,
    pub codec: String,
    pub size: Option<Size>,
    pub preset: Option<String>,
    pub profile: Option<String>,
    pub level: Option<String>,
    pub bframes: Option<u32>,
    pub fps: Option<f64>,
}

#[derive(Debug, Clone, Copy, Serialize, PartialEq)]
pub struct Size {
    /// -1 means "keep the aspect ratio".
    pub width: i32,
    pub height: i32,
    pub mode: SizeMode,
    /// Pad colour as 0xRRGGBB, only for `fit`.
    pub pad: Option<u32>,
}

#[derive(Debug, Clone, Copy, Serialize, PartialEq, Eq)]
#[serde(rename_all = "snake_case")]
pub enum SizeMode {
    /// Shrink to fit inside the box, keep the aspect ratio, pad the rest.
    Fit,
    /// Fill the box, keep the aspect ratio, cut the rest.
    Crop,
    /// Stretch to the box.
    Scale,
}

#[derive(Debug, Clone, Serialize, PartialEq)]
pub struct AudioTrack {
    pub bitrate: Bitrate,
    pub codec: String,
    pub sample_rate: Option<u32>,
    pub channels: Option<u32>,
}

/// Bitrates as Flussonic writes them: `1500k`, `2m`, `128000`, `copy`.
pub fn parse_bitrate(s: &str) -> Option<Bitrate> {
    if s.eq_ignore_ascii_case("copy") {
        return Some(Bitrate::Copy);
    }
    let s = s.trim();
    let (num, mul) = match s.chars().last()? {
        'k' | 'K' => (&s[..s.len() - 1], 1_000.0),
        'm' | 'M' => (&s[..s.len() - 1], 1_000_000.0),
        _ => (s, 1.0),
    };
    let v: f64 = num.parse().ok()?;
    (v > 0.0).then(|| Bitrate::Bps((v * mul).round() as u64))
}

/// `1280x720`, `1280x720:fit:#000000`, `-1x720`, `1280x720:crop`.
pub fn parse_size(s: &str) -> Option<Size> {
    let mut parts = s.split(':');
    let dims = parts.next()?;
    let (w, h) = dims.split_once('x')?;
    let width: i32 = w.parse().ok()?;
    let height: i32 = h.parse().ok()?;
    if width == 0 || height == 0 || width < -1 || height < -1 || (width == -1 && height == -1) {
        return None;
    }
    let mut mode = SizeMode::Scale;
    let mut pad = None;
    for p in parts {
        match p {
            "fit" => mode = SizeMode::Fit,
            "crop" => mode = SizeMode::Crop,
            "scale" => mode = SizeMode::Scale,
            colour if colour.starts_with('#') && colour.len() == 7 => {
                pad = Some(u32::from_str_radix(&colour[1..], 16).ok()?);
            }
            _ => return None,
        }
    }
    if mode == SizeMode::Fit && pad.is_none() {
        pad = Some(0x000000);
    }
    Some(Size { width, height, mode, pad })
}

/// `mpts-udp://eno1@224.2.2.2:2001?program=7`, `udp://224.2.2.2:2001`,
/// `udp://10.0.0.5@239.1.1.1:5000`, or any other URL.
pub fn parse_source(url: &str) -> Result<Source, String> {
    let (scheme, rest) = url.split_once("://").ok_or_else(|| format!("'{url}' is not a URL"))?;
    let scheme = scheme.to_ascii_lowercase();
    if scheme != "udp" && scheme != "mpts-udp" {
        return Ok(Source::Url { scheme });
    }
    let (addr, query) = rest.split_once('?').unwrap_or((rest, ""));
    let (interface, hostport) = match addr.rsplit_once('@') {
        Some((i, hp)) => (Some(i.to_string()), hp),
        None => (None, addr),
    };
    let (host, port) = hostport.rsplit_once(':').ok_or_else(|| format!("'{url}' has no port"))?;
    let group: Ipv4Addr = host.parse().map_err(|_| format!("'{host}' in '{url}' is not an IPv4 address"))?;
    let port: u16 = port.trim_end_matches('/').parse().map_err(|_| format!("bad port in '{url}'"))?;
    let mut program = None;
    for kv in query.split('&').filter(|s| !s.is_empty()) {
        if let Some(v) = kv.strip_prefix("program=") {
            program = Some(v.parse().map_err(|_| format!("bad program number in '{url}'"))?);
        }
    }
    Ok(Source::Udp { mpts: scheme == "mpts-udp", interface, group, port, program })
}

fn parse_bool(s: Option<&str>) -> bool {
    !matches!(s, Some("false") | Some("off") | Some("no") | Some("0"))
}

/// Flussonic writes the transcoder on one line. Options before the first
/// `vb=` are global; each `vb=` starts a video track and the video options
/// after it belong to that track, and the same for `ab=` and audio.
pub fn parse_transcoder(args: &[String]) -> Result<Transcoder, String> {
    let mut t = Transcoder {
        hw: Hw::Cpu,
        device_id: 0,
        gop: None,
        deinterlace: Deinterlace::Off,
        deinterlace_rate: DeinterlaceRate::Frame,
        video: Vec::new(),
        audio: Vec::new(),
        unused: Vec::new(),
    };
    // Codec and size given before any vb= apply to every video track.
    let mut default_vcodec = "h264".to_string();
    let mut default_acodec = "aac".to_string();
    let mut default_size = None;
    let mut default_preset = None;
    let mut default_bframes = None;
    let mut pending_video: Vec<(String, String)> = Vec::new();
    let mut last_was_audio = false;

    for arg in args {
        let (k, v) = arg.split_once('=').unwrap_or((arg.as_str(), ""));
        let bad = || format!("transcoder option '{arg}' has a value GodwinMix cannot read");
        match k {
            "hw" => {
                t.hw = match v {
                    "nvenc" | "cuda" | "nvidia" => Hw::Nvenc,
                    "qsv" => Hw::Qsv,
                    "cpu" | "" => Hw::Cpu,
                    _ => Hw::Other,
                }
            }
            "deviceid" => t.device_id = v.parse().map_err(|_| bad())?,
            "gop" => t.gop = Some(v.parse().map_err(|_| bad())?),
            "deinterlace" => {
                t.deinterlace = match v {
                    "false" | "off" | "0" | "no" => Deinterlace::Off,
                    "auto" => Deinterlace::Auto,
                    _ => Deinterlace::On,
                }
            }
            "deinterlace_rate" => {
                t.deinterlace_rate = match v {
                    "field" => DeinterlaceRate::Field,
                    _ => DeinterlaceRate::Frame,
                }
            }
            "vb" => {
                t.video.push(VideoTrack {
                    bitrate: parse_bitrate(v).ok_or_else(bad)?,
                    codec: default_vcodec.clone(),
                    size: default_size,
                    preset: default_preset.clone(),
                    profile: None,
                    level: None,
                    bframes: default_bframes,
                    fps: None,
                });
                for (pk, pv) in pending_video.drain(..) {
                    t.unused.push(format!("{pk}={pv}"));
                }
                last_was_audio = false;
            }
            "ab" => {
                t.audio.push(AudioTrack {
                    bitrate: parse_bitrate(v).ok_or_else(bad)?,
                    codec: default_acodec.clone(),
                    sample_rate: None,
                    channels: None,
                });
                last_was_audio = true;
            }
            "vcodec" | "size" | "preset" | "vprofile" | "profile" | "level" | "bframes" | "fps" | "r" => {
                match t.video.last_mut() {
                    Some(track) => match k {
                        "vcodec" => track.codec = v.to_string(),
                        "size" => track.size = Some(parse_size(v).ok_or_else(bad)?),
                        "preset" => track.preset = Some(v.to_string()),
                        "vprofile" | "profile" => track.profile = Some(v.to_string()),
                        "level" => track.level = Some(v.to_string()),
                        "bframes" => track.bframes = Some(v.parse().map_err(|_| bad())?),
                        _ => track.fps = Some(v.parse().map_err(|_| bad())?),
                    },
                    None => match k {
                        "vcodec" => default_vcodec = v.to_string(),
                        "size" => default_size = Some(parse_size(v).ok_or_else(bad)?),
                        "preset" => default_preset = Some(v.to_string()),
                        "bframes" => default_bframes = Some(v.parse().map_err(|_| bad())?),
                        _ => pending_video.push((k.to_string(), v.to_string())),
                    },
                }
            }
            "acodec" | "ar" | "ach" | "ac" => match (t.audio.last_mut(), last_was_audio) {
                (Some(track), true) => match k {
                    "acodec" => track.codec = v.to_string(),
                    "ar" => track.sample_rate = Some(v.parse().map_err(|_| bad())?),
                    _ => track.channels = Some(v.parse().map_err(|_| bad())?),
                },
                _ => match k {
                    "acodec" => default_acodec = v.to_string(),
                    _ => t.unused.push(arg.clone()),
                },
            },
            _ => t.unused.push(arg.clone()),
        }
    }
    for (pk, pv) in pending_video {
        t.unused.push(format!("{pk}={pv}"));
    }
    Ok(t)
}

fn credentials(d: &Directive) -> Result<Credentials, Error> {
    match (d.arg(0), d.arg(1)) {
        (Some(u), Some(p)) => Ok(Credentials { user: u.to_string(), password: p.to_string() }),
        _ => Err(Error::Invalid { line: d.line, msg: format!("{} needs a user and a password", d.name) }),
    }
}

fn listen(d: &Directive) -> Result<Listen, Error> {
    let a = d.arg(0).ok_or_else(|| Error::Invalid { line: d.line, msg: "http needs a port".into() })?;
    let bad = || Error::Invalid { line: d.line, msg: format!("'{a}' is not a port") };
    match a.rsplit_once(':') {
        Some((addr, port)) => Ok(Listen { addr: Some(addr.to_string()), port: port.parse().map_err(|_| bad())? }),
        None => Ok(Listen { addr: None, port: a.parse().map_err(|_| bad())? }),
    }
}

fn seconds(d: &Directive) -> Result<f64, Error> {
    d.arg(0)
        .and_then(|s| s.parse::<f64>().ok())
        .filter(|v| *v > 0.0 && *v <= 60.0)
        .ok_or_else(|| Error::Invalid { line: d.line, msg: format!("{} needs seconds between 0 and 60", d.name) })
}

fn count(d: &Directive) -> Result<u32, Error> {
    d.arg(0)
        .and_then(|s| s.parse::<u32>().ok())
        .filter(|v| (1..=100).contains(v))
        .ok_or_else(|| Error::Invalid { line: d.line, msg: format!("{} needs a number from 1 to 100", d.name) })
}

/// Names a stream may have: they become URL path segments.
pub fn valid_stream_name(name: &str) -> bool {
    !name.is_empty()
        && name.len() <= 128
        && name.chars().all(|c| c.is_ascii_alphanumeric() || matches!(c, '_' | '-' | '.'))
        && !name.starts_with('.')
        && name != "streamer"
        && name != "godwinstream"
}

fn stream(d: &Directive, unknown: &mut Vec<Unknown>) -> Result<Stream, Error> {
    let name = d.arg(0).ok_or_else(|| Error::Invalid { line: d.line, msg: "stream needs a name".into() })?;
    if !valid_stream_name(name) {
        return Err(Error::Invalid { line: d.line, msg: format!("'{name}' cannot be a stream name") });
    }
    let mut s = Stream {
        name: name.to_string(),
        title: None,
        disabled: false,
        is_static: false,
        inputs: Vec::new(),
        transcoder: None,
        segment_duration: None,
        segment_count: None,
        dvr: None,
        pushes: Vec::new(),
        line: d.line,
    };
    for c in d.children() {
        match c.name.as_str() {
            "title" => s.title = Some(c.args.join(" ")),
            "disabled" => s.disabled = parse_bool(c.arg(0)),
            "static" => s.is_static = parse_bool(c.arg(0)),
            "input" => {
                let url = c.arg(0).ok_or_else(|| Error::Invalid { line: c.line, msg: "input needs a URL".into() })?;
                let source = parse_source(url).map_err(|msg| Error::Invalid { line: c.line, msg })?;
                s.inputs.push(Input { url: url.to_string(), source });
            }
            "transcoder" => {
                s.transcoder = Some(parse_transcoder(&c.args).map_err(|msg| Error::Invalid { line: c.line, msg })?)
            }
            "segment_duration" => s.segment_duration = Some(seconds(c)?),
            "segment_count" => s.segment_count = Some(count(c)?),
            "dvr" => s.dvr = c.arg(0).map(str::to_string),
            "push" => s.pushes.extend(c.arg(0).map(str::to_string)),
            other => unknown.push(Unknown { scope: format!("stream {name}"), name: other.to_string(), line: c.line }),
        }
    }
    Ok(s)
}

/// Build the typed view from a parsed file.
pub fn from_nodes(nodes: &[Node]) -> Result<Config, Error> {
    let mut cfg = Config {
        listeners: Vec::new(),
        edit_auth: None,
        view_auth: None,
        session_log: None,
        segment_duration: None,
        segment_count: None,
        dvrs: Vec::new(),
        streams: Vec::new(),
        unknown: Vec::new(),
    };
    for node in nodes {
        let Node::Directive(d) = node else { continue };
        match d.name.as_str() {
            "http" => cfg.listeners.push(listen(d)?),
            "edit_auth" => cfg.edit_auth = Some(credentials(d)?),
            "view_auth" => cfg.view_auth = Some(credentials(d)?),
            "session_log" => cfg.session_log = d.arg(0).map(str::to_string),
            "segment_duration" => cfg.segment_duration = Some(seconds(d)?),
            "segment_count" => cfg.segment_count = Some(count(d)?),
            "dvr" => cfg.dvrs.push(Dvr {
                name: d.arg(0).unwrap_or_default().to_string(),
                root: d.children().find(|c| c.name == "root").and_then(|c| c.arg(0)).map(str::to_string),
            }),
            "stream" => {
                let s = stream(d, &mut cfg.unknown)?;
                if cfg.streams.iter().any(|o| o.name == s.name) {
                    return Err(Error::Invalid { line: d.line, msg: format!("stream {} is defined twice", s.name) });
                }
                cfg.streams.push(s);
            }
            other => cfg.unknown.push(Unknown { scope: "global".into(), name: other.to_string(), line: d.line }),
        }
    }
    if cfg.listeners.is_empty() {
        cfg.listeners.push(Listen { addr: None, port: 80 });
    }
    Ok(cfg)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn bitrates() {
        assert_eq!(parse_bitrate("1500k"), Some(Bitrate::Bps(1_500_000)));
        assert_eq!(parse_bitrate("2.5m"), Some(Bitrate::Bps(2_500_000)));
        assert_eq!(parse_bitrate("128000"), Some(Bitrate::Bps(128_000)));
        assert_eq!(parse_bitrate("copy"), Some(Bitrate::Copy));
        assert_eq!(parse_bitrate("fast"), None);
    }

    #[test]
    fn sizes() {
        let s = parse_size("1280x720:fit:#000000").unwrap();
        assert_eq!((s.width, s.height, s.mode, s.pad), (1280, 720, SizeMode::Fit, Some(0)));
        assert_eq!(parse_size("-1x720").unwrap().width, -1);
        assert_eq!(parse_size("1280x720:fit").unwrap().pad, Some(0));
        assert_eq!(parse_size("1280x720:crop").unwrap().mode, SizeMode::Crop);
        assert!(parse_size("-1x-1").is_none());
        assert!(parse_size("wide").is_none());
    }

    #[test]
    fn sources() {
        assert_eq!(
            parse_source("mpts-udp://eno1@224.2.2.2:2001?program=7").unwrap(),
            Source::Udp {
                mpts: true,
                interface: Some("eno1".into()),
                group: "224.2.2.2".parse().unwrap(),
                port: 2001,
                program: Some(7)
            }
        );
        assert!(matches!(parse_source("udp://239.1.1.1:5000").unwrap(), Source::Udp { interface: None, program: None, .. }));
        assert_eq!(parse_source("rtmp://127.0.0.1/live/x").unwrap(), Source::Url { scheme: "rtmp".into() });
        assert!(parse_source("udp://eno1@nowhere:1").is_err());
    }

    #[test]
    fn the_btn36_transcoder_line() {
        let args: Vec<String> = "deviceid=0 gop=50 hw=nvenc deinterlace=false deinterlace_rate=frame vb=1500k vcodec=h264 size=1280x720:fit:#000000 ab=128k"
            .split(' ')
            .map(String::from)
            .collect();
        let t = parse_transcoder(&args).unwrap();
        assert_eq!(t.hw, Hw::Nvenc);
        assert_eq!(t.gop, Some(50));
        assert_eq!(t.deinterlace, Deinterlace::Off);
        assert_eq!(t.video.len(), 1);
        assert_eq!(t.video[0].bitrate, Bitrate::Bps(1_500_000));
        assert_eq!(t.video[0].size.unwrap().mode, SizeMode::Fit);
        assert_eq!(t.audio.len(), 1);
        assert_eq!(t.audio[0].bitrate, Bitrate::Bps(128_000));
        assert!(t.unused.is_empty(), "{:?}", t.unused);
    }

    #[test]
    fn options_before_vb_apply_to_every_track() {
        let args: Vec<String> = "size=-1x720 vb=2m vb=800k size=640x360 ab=copy".split(' ').map(String::from).collect();
        let t = parse_transcoder(&args).unwrap();
        assert_eq!(t.video[0].size.unwrap().height, 720);
        assert_eq!(t.video[1].size.unwrap().height, 360);
        assert_eq!(t.audio[0].bitrate, Bitrate::Copy);
    }
}
