//! Choosing an entry.
//!
//! Selection is by rank among the entries whose elements are actually in the
//! registry. Decode and encode are chosen independently, because a machine
//! with NVDEC and no usable NVENC is a real configuration and should get the
//! fast decoder and the software encoder rather than neither. A pinned
//! `hardware.encode` fails loudly and prints every entry that was available,
//! because an operator who pinned a backend wants to hear that it is missing,
//! not to discover it from the CPU graph.

use super::model::{AudioEntry, GraphicsEntry, PropValue, Role, VideoEntry};
use super::Catalogue;
use crate::config::Accel;
use anyhow::{bail, Result};
use serde::Serialize;
use std::collections::BTreeMap;

/// What answers "is this element installed". The real one asks GStreamer; the
/// CI validation test hands in a set of names so the whole of selection can be
/// exercised on a runner with no plugins at all.
pub trait Registry {
    fn has(&self, factory: &str) -> bool;
}

pub struct GstRegistry;

impl Registry for GstRegistry {
    fn has(&self, factory: &str) -> bool {
        crate::probe::exists(factory)
    }
}

/// A registry made of names, for tests.
pub struct FakeRegistry(pub std::collections::BTreeSet<String>);

impl FakeRegistry {
    pub fn with(names: &[&str]) -> Self {
        Self(names.iter().map(|s| s.to_string()).collect())
    }
}

impl Registry for FakeRegistry {
    fn has(&self, factory: &str) -> bool {
        self.0.contains(factory)
    }
}

/// What the operator asked for, separated from `Config` so a test can ask for
/// one thing without building a whole configuration.
#[derive(Debug, Clone)]
pub struct Request {
    /// The container the programme is going out in. Selection keeps to codecs
    /// this container can carry.
    pub container: Option<String>,
    pub decode: Accel,
    pub encode: Accel,
    pub graphics: Accel,
    /// `os-arch`, matched against a graphics entry's `verified` list.
    pub platform: String,
}

impl Default for Request {
    fn default() -> Self {
        Self {
            container: Some("flv".into()),
            decode: Accel::Auto,
            encode: Accel::Auto,
            graphics: Accel::Auto,
            platform: current_platform(),
        }
    }
}

pub fn current_platform() -> String {
    format!("{}-{}", std::env::consts::OS, std::env::consts::ARCH)
}

/// One element chosen for one role, flattened so the mixer does not have to
/// know whether it came from a video entry or an audio one.
#[derive(Debug, Clone, Serialize)]
pub struct Chosen {
    pub id: String,
    pub role: String,
    pub codec: String,
    pub accel: String,
    pub element: String,
    pub download: Option<String>,
    pub parser: Option<String>,
    pub memory: String,
    pub rank: i32,
    pub license: String,
    #[serde(skip)]
    pub properties: BTreeMap<String, PropValue>,
    #[serde(skip)]
    pub keyframe: Option<super::model::Keyframe>,
    pub priming_delay_ms: Option<u64>,
    pub container: Vec<String>,
}

/// The graphics backend chosen, with the reason, because "software" is the
/// answer most of the time and the operator deserves to know whether that is
/// because there was no GPU or because nobody has verified the GPU path here.
#[derive(Debug, Clone, Serialize)]
pub struct GraphicsChoice {
    pub id: String,
    pub accel: String,
    pub rank: i32,
    pub compositor: String,
    pub convert: String,
    pub upload: Option<String>,
    pub download: Option<String>,
    pub memory: String,
    pub why: String,
}

impl GraphicsChoice {
    pub fn is_gpu(&self) -> bool {
        self.memory != "system"
    }
}

/// One line of `--probe`: an entry that was looked at, and what happened.
#[derive(Debug, Clone, Serialize)]
pub struct Consideration {
    pub role: String,
    pub id: String,
    pub accel: String,
    pub rank: i32,
    pub element: String,
    pub present: bool,
    pub missing: Vec<String>,
    pub chosen: bool,
    pub note: String,
}

#[derive(Debug, Clone, Serialize)]
pub struct Selection {
    pub video_decode: Chosen,
    pub video_encode: Chosen,
    pub audio_decode: Chosen,
    pub audio_encode: Chosen,
    pub graphics: GraphicsChoice,
    pub container: Option<String>,
    pub considered: Vec<Consideration>,
}

impl Catalogue {
    /// Pick a decoder, an encoder and a compositor for this machine.
    pub fn select(&self, req: &Request, reg: &dyn Registry) -> Result<Selection> {
        let mut seen = Vec::new();
        let video_decode = self.pick_video(Role::Decode, req, reg, &mut seen)?;
        let video_encode = self.pick_video(Role::Encode, req, reg, &mut seen)?;
        let audio_decode = self.pick_audio(Role::Decode, req, reg, &mut seen)?;
        let audio_encode = self.pick_audio(Role::Encode, req, reg, &mut seen)?;
        let graphics = self.pick_graphics(req, reg, &mut seen)?;
        Ok(Selection {
            video_decode,
            video_encode,
            audio_decode,
            audio_encode,
            graphics,
            container: req.container.clone(),
            considered: seen,
        })
    }

    fn carries_video(&self, req: &Request, codec: &str) -> bool {
        match req.container.as_deref().and_then(|c| self.container(c)) {
            Some(c) => c.carries_video(codec),
            None => true,
        }
    }

    fn carries_audio(&self, req: &Request, codec: &str) -> bool {
        match req.container.as_deref().and_then(|c| self.container(c)) {
            Some(c) => c.carries_audio(codec),
            None => true,
        }
    }

    fn pick_video(
        &self,
        role: Role,
        req: &Request,
        reg: &dyn Registry,
        seen: &mut Vec<Consideration>,
    ) -> Result<Chosen> {
        let pin = if role == Role::Encode { req.encode } else { req.decode };
        let label = format!("video.{}", role.as_str());
        let rows: Vec<(&VideoEntry, Consideration)> = self
            .video
            .iter()
            .filter(|e| !e.disabled && e.element(role).is_some())
            .filter(|e| self.carries_video(req, &e.codec))
            .map(|e| {
                let missing = missing_of(&e.needs(role), reg);
                (e, row(&label, &e.id(), &e.accel, e.rank, e.element(role).unwrap(), missing))
            })
            .collect();
        let idx = choose(&label, pin, &rows.iter().map(|(_, c)| c.clone()).collect::<Vec<_>>())?;
        let (entry, _) = &rows[idx];
        for (i, (_, c)) in rows.iter().enumerate() {
            let mut c = c.clone();
            c.chosen = i == idx;
            seen.push(c);
        }
        Ok(Chosen {
            id: entry.id(),
            role: label,
            codec: entry.codec.clone(),
            accel: entry.accel.clone(),
            element: entry.element(role).unwrap().to_string(),
            download: entry.download.clone(),
            parser: entry.parser.clone(),
            memory: entry.memory.clone(),
            rank: entry.rank,
            license: entry.license.clone().unwrap_or_else(|| "unstated".into()),
            properties: if role == Role::Encode {
                entry.properties.clone()
            } else {
                BTreeMap::new()
            },
            keyframe: entry.keyframe.clone(),
            priming_delay_ms: None,
            container: entry.container.clone(),
        })
    }

    fn pick_audio(
        &self,
        role: Role,
        req: &Request,
        reg: &dyn Registry,
        seen: &mut Vec<Consideration>,
    ) -> Result<Chosen> {
        let label = format!("audio.{}", role.as_str());
        let rows: Vec<(&AudioEntry, Consideration)> = self
            .audio
            .iter()
            .filter(|e| !e.disabled && e.element(role).is_some())
            .filter(|e| self.carries_audio(req, &e.codec))
            .map(|e| {
                let missing = missing_of(&e.needs(role), reg);
                (e, row(&label, &e.id(), &e.accel, e.rank, e.element(role).unwrap(), missing))
            })
            .collect();
        // Audio has no hardware backends worth pinning, so the accel pin does
        // not apply here: forcing `hardware.encode = "nvidia"` must not take
        // the AAC encoder away.
        let idx =
            choose(&label, Accel::Auto, &rows.iter().map(|(_, c)| c.clone()).collect::<Vec<_>>())?;
        let (entry, _) = &rows[idx];
        for (i, (_, c)) in rows.iter().enumerate() {
            let mut c = c.clone();
            c.chosen = i == idx;
            seen.push(c);
        }
        Ok(Chosen {
            id: entry.id(),
            role: label,
            codec: entry.codec.clone(),
            accel: entry.accel.clone(),
            element: entry.element(role).unwrap().to_string(),
            download: None,
            parser: entry.parser.clone(),
            memory: "system".into(),
            rank: entry.rank,
            license: entry.license.clone().unwrap_or_else(|| "unstated".into()),
            properties: if role == Role::Encode {
                entry.properties.clone()
            } else {
                BTreeMap::new()
            },
            keyframe: None,
            priming_delay_ms: entry.priming_delay_ms,
            container: entry.container.clone(),
        })
    }

    fn pick_graphics(
        &self,
        req: &Request,
        reg: &dyn Registry,
        seen: &mut Vec<Consideration>,
    ) -> Result<GraphicsChoice> {
        let pinned = req.graphics != Accel::Auto;
        let mut rows: Vec<(&GraphicsEntry, Consideration)> = Vec::new();
        for e in self.graphics.iter().filter(|e| !e.disabled) {
            let mut c =
                row("graphics", &e.id(), &e.accel, e.rank, &e.compositor, missing_of(&e.needs(), reg));
            // Honesty about upstream state. Every GPU compositor is rank none
            // in GStreamer 1.28 with open bugs on dynamic pad add and remove,
            // and a mixer adds and removes pads all day. So a GPU entry is
            // only taken automatically once somebody has run the test on this
            // platform and written it into `verified`.
            if c.present && e.memory != "system" && !pinned && !verified_here(e, &req.platform) {
                c.present = false;
                c.note = format!("present but not verified on {}", req.platform);
            }
            rows.push((e, c));
        }
        let idx = choose(
            "graphics",
            req.graphics,
            &rows.iter().map(|(_, c)| c.clone()).collect::<Vec<_>>(),
        )?;
        let (entry, _) = &rows[idx];
        let why = if pinned {
            format!("pinned by [hardware] graphics = \"{}\"", entry.accel)
        } else if entry.memory == "system" {
            "highest ranked entry present; GPU entries are taken only once verified here".into()
        } else {
            format!("highest ranked entry present and verified on {}", req.platform)
        };
        for (i, (_, c)) in rows.iter().enumerate() {
            let mut c = c.clone();
            c.chosen = i == idx;
            seen.push(c);
        }
        Ok(GraphicsChoice {
            id: entry.id(),
            accel: entry.accel.clone(),
            rank: entry.rank,
            compositor: entry.compositor.clone(),
            convert: entry.convert.clone(),
            upload: entry.upload.clone(),
            download: entry.download.clone(),
            memory: entry.memory.clone(),
            why,
        })
    }
}

fn verified_here(e: &GraphicsEntry, platform: &str) -> bool {
    e.verified.iter().any(|v| v.platform == platform || v.platform == "any")
}

fn missing_of(needs: &[String], reg: &dyn Registry) -> Vec<String> {
    needs.iter().filter(|n| !reg.has(n)).cloned().collect()
}

fn row(role: &str, id: &str, accel: &str, rank: i32, element: &str, missing: Vec<String>) -> Consideration {
    let present = missing.is_empty();
    let note = if present { String::new() } else { format!("missing {}", missing.join(", ")) };
    Consideration {
        role: role.into(),
        id: id.into(),
        accel: accel.into(),
        rank,
        element: element.into(),
        present,
        missing,
        chosen: false,
        note,
    }
}

/// The rank decision itself, over rows that already know whether they are
/// present. Returns the index into `rows`.
fn choose(role: &str, pin: Accel, rows: &[Consideration]) -> Result<usize> {
    let want = pin.name();
    let eligible = |c: &Consideration| c.present && (want.is_none() || want == Some(c.accel.as_str()));
    let best = rows
        .iter()
        .enumerate()
        .filter(|(_, c)| eligible(c))
        .max_by_key(|(i, c)| (c.rank, std::cmp::Reverse(*i)));
    match best {
        Some((i, _)) => Ok(i),
        None => bail!("{}", no_entry_message(role, want, rows)),
    }
}

fn no_entry_message(role: &str, want: Option<&str>, rows: &[Consideration]) -> String {
    let mut s = match want {
        Some(w) => format!(
            "no {role} entry for accel \"{w}\" is installed. \
             Set [hardware] back to \"auto\", or install the elements. \
             The entries the catalogue knows for this role:"
        ),
        None => format!("no {role} entry is installed at all. The entries the catalogue knows:"),
    };
    if rows.is_empty() {
        s.push_str("\n  (none: the catalogue has no entry for this role)");
        return s;
    }
    for c in rows {
        let state = if c.present { "installed".to_string() } else { format!("missing {}", c.missing.join(", ")) };
        s.push_str(&format!("\n  {:<26} accel {:<16} rank {:<4} {}", c.id, c.accel, c.rank, state));
    }
    s
}
