//! The codec and graphics catalogue.
//!
//! What codec the programme is encoded in, which element does it, what
//! properties that element wants and in what units, which compositor the
//! canvas is drawn on: all of it is data in `codecs.toml`, not a table in
//! Rust. A new GPU generation, a renamed element or AV1 is an entry somebody
//! sends, not a core release, and that matters because those names change
//! every few months while a core release does not.
//!
//! The file ships embedded in the binary. Three things layer on top of it, in
//! this order:
//!
//! 1. `[codecs]` in the operator's config, which adds entries or replaces
//!    shipped ones by id.
//! 2. `--codecs <file>`, the same shape in a file of its own, for trying a
//!    catalogue update before it is installed.
//! 3. `GMX_CODEC_RANK=id=rank,...` from the environment, which nudges the
//!    order without restating an entry. `GMX_CODEC_DISABLE=id,...` takes one
//!    out.
//!
//! The merged result is what `codec.list` returns over RPC and what
//! `godwinmix --probe` prints.

pub mod apply;
pub mod check;
pub mod cli;
pub mod model;
pub mod select;

use crate::config::Config;
use anyhow::{Context, Result};
use model::{AudioEntry, ContainerEntry, GraphicsEntry, PropValue, Role, VideoEntry};
use select::{Consideration, GstRegistry, Registry, Request, Selection};
use serde::{Deserialize, Serialize};
use std::path::Path;
use std::sync::{Arc, OnceLock};
use tracing::{info, warn};

/// The catalogue that ships with the core.
const SHIPPED: &str = include_str!("../../codecs.toml");

#[derive(Debug, Clone, Default, Deserialize, Serialize, PartialEq)]
pub struct Catalogue {
    #[serde(default)]
    pub video: Vec<VideoEntry>,
    #[serde(default)]
    pub audio: Vec<AudioEntry>,
    #[serde(default)]
    pub graphics: Vec<GraphicsEntry>,
    #[serde(default)]
    pub container: Vec<ContainerEntry>,
    /// The container the programme is muxed into, and so the codecs selection
    /// keeps to. `flv` is what RTMP wants and is the default.
    #[serde(default)]
    pub programme_container: Option<String>,
}

impl Catalogue {
    /// The catalogue compiled into this binary.
    pub fn shipped() -> Result<Self> {
        Self::parse(SHIPPED).context("parsing the built in codecs.toml")
    }

    pub fn parse(text: &str) -> Result<Self> {
        Ok(toml::from_str(text)?)
    }

    pub fn read(path: &Path) -> Result<Self> {
        let text = std::fs::read_to_string(path)
            .with_context(|| format!("reading catalogue {}", path.display()))?;
        Self::parse(&text).with_context(|| format!("parsing catalogue {}", path.display()))
    }

    pub fn container(&self, name: &str) -> Option<&ContainerEntry> {
        self.container.iter().find(|c| c.name == name)
    }

    pub fn video_entry(&self, id: &str) -> Option<&VideoEntry> {
        self.video.iter().find(|e| e.id() == id)
    }

    pub fn audio_entry(&self, id: &str) -> Option<&AudioEntry> {
        self.audio.iter().find(|e| e.id() == id)
    }

    pub fn graphics_entry(&self, id: &str) -> Option<&GraphicsEntry> {
        self.graphics.iter().find(|e| e.id() == id)
    }

    /// Lay another catalogue over this one. An entry whose id is already here
    /// replaces it; anything else is appended. Replacing wholesale rather than
    /// merging field by field is deliberate: half an entry from two places is
    /// harder to reason about than one entry restated.
    pub fn overlay(&mut self, other: Catalogue) {
        for e in other.video {
            replace_or_push(&mut self.video, e, |x| x.id());
        }
        for e in other.audio {
            replace_or_push(&mut self.audio, e, |x| x.id());
        }
        for e in other.graphics {
            replace_or_push(&mut self.graphics, e, |x| x.id());
        }
        for e in other.container {
            let name = e.name.clone();
            match self.container.iter_mut().find(|c| c.name == name) {
                Some(slot) => *slot = e,
                None => self.container.push(e),
            }
        }
        if other.programme_container.is_some() {
            self.programme_container = other.programme_container;
        }
    }

    /// `GMX_CODEC_RANK=h264-nvidia=0,av1-software=300` and
    /// `GMX_CODEC_DISABLE=h264-software-x264`.
    pub fn apply_env(&mut self) {
        self.apply_env_from(|k| std::env::var(k).ok());
    }

    fn apply_env_from(&mut self, get: impl Fn(&str) -> Option<String>) {
        if let Some(spec) = get("GMX_CODEC_RANK") {
            for (id, rank) in parse_rank_spec(&spec) {
                if !self.set_rank(&id, rank) {
                    warn!(entry = %id, "GMX_CODEC_RANK names an entry the catalogue does not have");
                }
            }
        }
        if let Some(spec) = get("GMX_CODEC_DISABLE") {
            for id in spec.split(',').map(str::trim).filter(|s| !s.is_empty()) {
                if !self.set_disabled(id) {
                    warn!(entry = %id, "GMX_CODEC_DISABLE names an entry the catalogue does not have");
                }
            }
        }
    }

    fn set_rank(&mut self, id: &str, rank: i32) -> bool {
        if let Some(e) = self.video.iter_mut().find(|e| e.id() == id) {
            e.rank = rank;
            return true;
        }
        if let Some(e) = self.audio.iter_mut().find(|e| e.id() == id) {
            e.rank = rank;
            return true;
        }
        if let Some(e) = self.graphics.iter_mut().find(|e| e.id() == id) {
            e.rank = rank;
            return true;
        }
        false
    }

    fn set_disabled(&mut self, id: &str) -> bool {
        if let Some(e) = self.video.iter_mut().find(|e| e.id() == id) {
            e.disabled = true;
            return true;
        }
        if let Some(e) = self.audio.iter_mut().find(|e| e.id() == id) {
            e.disabled = true;
            return true;
        }
        if let Some(e) = self.graphics.iter_mut().find(|e| e.id() == id) {
            e.disabled = true;
            return true;
        }
        false
    }
}

fn replace_or_push<T>(v: &mut Vec<T>, item: T, id: impl Fn(&T) -> String) {
    let key = id(&item);
    match v.iter_mut().find(|x| id(x) == key) {
        Some(slot) => *slot = item,
        None => v.push(item),
    }
}

fn parse_rank_spec(spec: &str) -> Vec<(String, i32)> {
    spec.split(',')
        .filter_map(|pair| {
            let (id, rank) = pair.split_once('=').or_else(|| pair.split_once(':'))?;
            Some((id.trim().to_string(), rank.trim().parse().ok()?))
        })
        .collect()
}

// ---------------------------------------------------------------------------
// Loading
// ---------------------------------------------------------------------------

/// Build the merged catalogue: shipped, then the operator's `[codecs]`, then
/// `--codecs`, then the environment.
pub fn load(cfg: Option<&Config>, extra: Option<&Path>) -> Result<Catalogue> {
    let mut cat = Catalogue::shipped()?;
    if let Some(cfg) = cfg {
        cat.overlay(cfg.codecs.clone());
    }
    if let Some(path) = extra {
        cat.overlay(Catalogue::read(path)?);
    }
    cat.apply_env();
    for problem in cat.validate() {
        warn!(%problem, "catalogue entry is not well formed and may not behave");
    }
    Ok(cat)
}

static GLOBAL: OnceLock<Arc<Catalogue>> = OnceLock::new();

/// Load the catalogue once for this process. The first caller wins, which is
/// `Mixer::build` in the daemon and the subcommand in the CLI.
pub fn init(cfg: Option<&Config>, extra: Option<&Path>) -> Result<Arc<Catalogue>> {
    if let Some(c) = GLOBAL.get() {
        return Ok(c.clone());
    }
    let cat = Arc::new(load(cfg, extra)?);
    let _ = GLOBAL.set(cat);
    Ok(GLOBAL.get().expect("just set").clone())
}

/// The catalogue in force. Falls back to the shipped one, so a caller that
/// runs before any configuration is read (a unit test, `codec.list` on a
/// core used as a library) still gets an answer.
pub fn global() -> Arc<Catalogue> {
    GLOBAL
        .get_or_init(|| {
            Arc::new(Catalogue::shipped().unwrap_or_else(|e| {
                warn!(error = %e, "the built in catalogue did not parse; running with an empty one");
                Catalogue::default()
            }))
        })
        .clone()
}

/// The muxer and codec compatibility for a container, for an output that needs
/// to know what to mux into. `output.rs` calls this rather than naming a muxer.
pub fn container_for(name: &str) -> Option<ContainerEntry> {
    global().container(name).cloned()
}

/// The request `Mixer::build` makes, from the configuration.
pub fn request_from(cfg: &Config, cat: &Catalogue) -> Request {
    Request {
        container: cat
            .programme_container
            .clone()
            .or_else(|| cfg.codecs.programme_container.clone())
            .or_else(|| Some("flv".into())),
        decode: cfg.hardware.decode,
        encode: cfg.hardware.encode,
        graphics: cfg.hardware.graphics,
        platform: select::current_platform(),
    }
}

/// Select against the live GStreamer registry.
pub fn select(cfg: &Config, extra: Option<&Path>) -> Result<Selection> {
    let cat = init(Some(cfg), extra)?;
    let req = request_from(cfg, &cat);
    cat.select(&req, &GstRegistry)
}

// ---------------------------------------------------------------------------
// Validation, for CI with no hardware at all
// ---------------------------------------------------------------------------

/// A plausible GStreamer element factory name: lower case, starting with a
/// letter or digit. This catches a typo like `NvH264Enc` or `nvh264enc `
/// without needing a registry to look it up in.
pub fn plausible_factory(name: &str) -> bool {
    !name.is_empty()
        && name.chars().next().is_some_and(|c| c.is_ascii_lowercase() || c.is_ascii_digit())
        && name
            .chars()
            .all(|c| c.is_ascii_lowercase() || c.is_ascii_digit() || matches!(c, '_' | '-' | '+' | '.'))
}

impl Catalogue {
    /// Everything wrong with this catalogue, in plain words. Empty is good.
    pub fn validate(&self) -> Vec<String> {
        let mut out = Vec::new();
        let mut ids: Vec<String> = Vec::new();
        for e in &self.video {
            let id = e.id();
            check_id(&mut out, &mut ids, &id);
            if e.codec.is_empty() {
                out.push(format!("{id}: no codec"));
            }
            if e.license.is_none() {
                out.push(format!("{id}: no license. Every entry carries one."));
            }
            if e.encoder.is_none() && e.decoder.is_none() {
                out.push(format!("{id}: neither an encoder nor a decoder"));
            }
            let mut names: Vec<String> = e.needs(Role::Encode);
            names.extend(e.needs(Role::Decode));
            names.extend(e.download.clone());
            names.extend(e.parser.clone());
            check_names(&mut out, &id, &names);
            check_props(&mut out, &id, &e.properties);
            if let Some(kf) = &e.keyframe {
                if !matches!(kf.unit.as_str(), "frames" | "seconds") {
                    out.push(format!("{id}: keyframe unit {:?} is not frames or seconds", kf.unit));
                }
            }
        }
        for e in &self.audio {
            let id = e.id();
            check_id(&mut out, &mut ids, &id);
            if e.license.is_none() {
                out.push(format!("{id}: no license. Every entry carries one."));
            }
            let mut names: Vec<String> = e.needs(Role::Encode);
            names.extend(e.needs(Role::Decode));
            names.extend(e.parser.clone());
            check_names(&mut out, &id, &names);
            check_props(&mut out, &id, &e.properties);
        }
        for e in &self.graphics {
            let id = e.id();
            check_id(&mut out, &mut ids, &id);
            if e.license.is_none() {
                out.push(format!("{id}: no license. Every entry carries one."));
            }
            if e.compositor.is_empty() || e.convert.is_empty() {
                out.push(format!("{id}: a graphics entry needs a compositor and a convert"));
            }
            let mut names = e.needs();
            names.extend(e.upload.clone());
            names.extend(e.download.clone());
            check_names(&mut out, &id, &names);
        }
        for c in &self.container {
            if !plausible_factory(&c.muxer) {
                out.push(format!("container {}: {:?} is not an element name", c.name, c.muxer));
            }
            check_props(&mut out, &c.name, &c.properties);
        }
        out
    }
}

fn check_id(out: &mut Vec<String>, ids: &mut Vec<String>, id: &str) {
    if ids.iter().any(|x| x == id) {
        out.push(format!("{id}: two entries share this id"));
    }
    ids.push(id.to_string());
}

fn check_names(out: &mut Vec<String>, id: &str, names: &[String]) {
    for n in names {
        if !plausible_factory(n) {
            out.push(format!("{id}: {n:?} is not a plausible element factory name"));
        }
    }
}

fn check_props(out: &mut Vec<String>, id: &str, props: &std::collections::BTreeMap<String, PropValue>) {
    for (name, v) in props {
        if name.is_empty() {
            out.push(format!("{id}: a property with no name"));
        }
        if let PropValue::Derived(d) = v {
            if let Some(why) = apply::check_derived(d) {
                out.push(format!("{id}: property {name}: {why}"));
            }
        }
    }
}

// ---------------------------------------------------------------------------
// codec.list
// ---------------------------------------------------------------------------

#[derive(Debug, Clone, Serialize)]
pub struct EntryInfo {
    pub id: String,
    pub kind: String,
    pub codec: String,
    pub accel: String,
    pub rank: i32,
    pub encoder: Option<String>,
    pub decoder: Option<String>,
    pub parser: Option<String>,
    pub license: String,
    pub present: bool,
    pub missing: Vec<String>,
    pub verified: Vec<model::Verified>,
    pub comment: Option<String>,
}

#[derive(Debug, Clone, Serialize)]
pub struct Listing {
    pub platform: String,
    pub gstreamer: String,
    pub entries: Vec<EntryInfo>,
    pub containers: Vec<ContainerEntry>,
    /// What would be selected on this machine right now, and why.
    pub selected: Option<Selection>,
    pub considered: Vec<Consideration>,
}

/// The data behind the `codec.list` RPC method and `gmx codec list`. A plain
/// function so the API layer has nothing to do but serialise it.
pub fn list() -> Listing {
    listing(&global(), &GstRegistry, &Request::default())
}

pub fn listing(cat: &Catalogue, reg: &dyn Registry, req: &Request) -> Listing {
    let mut entries = Vec::new();
    for e in &cat.video {
        entries.push(info_of(
            "video",
            &e.id(),
            &e.codec,
            &e.accel,
            e.rank,
            e.encoder.clone(),
            e.decoder.clone(),
            e.parser.clone(),
            e.license.clone(),
            present_missing(&e.needs(Role::Encode), &e.needs(Role::Decode), reg),
            e.verified.clone(),
            e.comment.clone(),
        ));
    }
    for e in &cat.audio {
        entries.push(info_of(
            "audio",
            &e.id(),
            &e.codec,
            &e.accel,
            e.rank,
            e.encoder.clone(),
            e.decoder.clone(),
            e.parser.clone(),
            e.license.clone(),
            present_missing(&e.needs(Role::Encode), &e.needs(Role::Decode), reg),
            e.verified.clone(),
            e.comment.clone(),
        ));
    }
    for e in &cat.graphics {
        let missing: Vec<String> = e.needs().into_iter().filter(|n| !reg.has(n)).collect();
        entries.push(info_of(
            "graphics",
            &e.id(),
            "",
            &e.accel,
            e.rank,
            Some(e.compositor.clone()),
            None,
            None,
            e.license.clone(),
            missing,
            e.verified.clone(),
            e.comment.clone(),
        ));
    }
    let selected = cat.select(req, reg).ok();
    let considered = selected.as_ref().map(|s| s.considered.clone()).unwrap_or_default();
    Listing {
        platform: select::current_platform(),
        gstreamer: gstreamer_version(),
        entries,
        containers: cat.container.clone(),
        selected,
        considered,
    }
}

#[allow(clippy::too_many_arguments)]
fn info_of(
    kind: &str,
    id: &str,
    codec: &str,
    accel: &str,
    rank: i32,
    encoder: Option<String>,
    decoder: Option<String>,
    parser: Option<String>,
    license: Option<String>,
    missing: Vec<String>,
    verified: Vec<model::Verified>,
    comment: Option<String>,
) -> EntryInfo {
    EntryInfo {
        id: id.into(),
        kind: kind.into(),
        codec: codec.into(),
        accel: accel.into(),
        rank,
        encoder,
        decoder,
        parser,
        license: license.unwrap_or_else(|| "unstated".into()),
        present: missing.is_empty(),
        missing,
        verified,
        comment,
    }
}

/// An entry counts as present when either of its roles is fillable, and the
/// missing list is what is short for both.
fn present_missing(encode: &[String], decode: &[String], reg: &dyn Registry) -> Vec<String> {
    let missing_enc: Vec<String> = encode.iter().filter(|n| !reg.has(n)).cloned().collect();
    let missing_dec: Vec<String> = decode.iter().filter(|n| !reg.has(n)).cloned().collect();
    let enc_ok = !encode.is_empty() && missing_enc.is_empty();
    let dec_ok = !decode.is_empty() && missing_dec.is_empty();
    if enc_ok || dec_ok {
        return Vec::new();
    }
    let mut all = missing_enc;
    for m in missing_dec {
        if !all.contains(&m) {
            all.push(m);
        }
    }
    all
}

pub fn gstreamer_version() -> String {
    let (major, minor, micro, _) = gstreamer::version();
    format!("{major}.{minor}.{micro}")
}

/// One line per entry whose elements exist, each backed by a one second encode,
/// for `gmx doctor` to print. Entries that are absent get a line too, because
/// "the NVIDIA entry is not installed here" is exactly what the operator of a
/// laptop needs to read.
pub fn doctor_lines() -> Vec<String> {
    check::doctor_lines(&global(), &GstRegistry)
}

/// Log what was chosen, once, at startup.
pub fn log_selection(sel: &Selection) {
    info!(
        video_decoder = %sel.video_decode.element,
        video_encoder = %sel.video_encode.element,
        video_codec = %sel.video_encode.codec,
        audio_encoder = %sel.audio_encode.element,
        audio_codec = %sel.audio_encode.codec,
        graphics = %sel.graphics.id,
        container = sel.container.as_deref().unwrap_or("none"),
        "selected codec backends from the catalogue"
    );
    if sel.video_encode.accel == "software" {
        warn!(
            entry = %sel.video_encode.id,
            "encoding H.264 on the CPU; expect roughly one core per 1080p30 output"
        );
    }
}

#[cfg(test)]
mod tests;
