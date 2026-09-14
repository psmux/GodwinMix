//! The `Source` trait and the table that picks an implementation.
//!
//! A source produces video and audio for the canvas. Everything the core does
//! with one goes through this trait, so a built in kind, a crate compiled in by
//! a custom build and (later) a process on another machine are the same thing
//! to the mixer.

use super::kinds::{self, BuildCtx};
use super::{Capability, CapabilitySet, Configure, Health, Hello, Manifest, MediaEnds, Ready};
use crate::caps::CanvasCaps;
use crate::config::{BrowserConfig, Params, SourceConfig};
use crate::input::MediaReport;
use crate::probe::Backends;
use anyhow::Result;
use serde_json::Value;
use std::time::Instant;

/// What the core asks of every source, whatever tier it runs at.
///
/// `start` is the one that matters: it is exactly what `build_kind` used to
/// produce, named. A sidecar host will be one more implementation that spawns
/// a process, runs the handshake, and wires `fdsrc` into the same `MediaEnds`.
pub trait Source: Send {
    fn manifest(&self) -> &Manifest;

    /// Settle what this instance is before anything is built. Validates the
    /// params and answers with the capabilities that actually apply: a file
    /// declares `seek`, the same code opening an HLS playlist does not.
    fn initialize(&mut self, hello: Hello) -> Result<Ready>;

    /// Build the pipeline in NULL state and hand back its ends. The thumbnail
    /// end is built only when it is asked for: the core does no work for a
    /// picture nobody is looking at.
    fn start(&mut self, canvas: &CanvasCaps, thumb: bool) -> Result<MediaEnds>;

    /// Release whatever the instance holds outside the pipeline: a child
    /// process, a cached clip, a profile directory. The core NULLs the
    /// pipeline itself.
    fn stop(&mut self) -> Result<()>;

    fn configure(&mut self, params: &Params) -> Result<Configure>;

    fn health(&self) -> Health;

    /// Everything else: `restart`, `seek`, `audio.set`, a tool a plugin
    /// contributes. Unknown methods answer with an error naming the ones this
    /// source does take.
    fn call(&mut self, method: &str, params: Value) -> Result<Value>;
}

/// The default answer for a source with nothing to say.
pub fn unknown_method(manifest: &Manifest, method: &str, known: &[&str]) -> anyhow::Error {
    anyhow::anyhow!(
        "{} does not answer `{method}`. It answers: {}",
        manifest.provide_id(),
        if known.is_empty() { "nothing".to_string() } else { known.join(", ") }
    )
}

/// Everything a factory needs to make one instance.
pub struct SourceRequest<'a> {
    pub cfg: &'a SourceConfig,
    pub canvas: &'a CanvasCaps,
    pub backends: &'a Backends,
    pub browser: &'a BrowserConfig,
    pub allow_exec: bool,
    pub thumb_fps: i32,
    pub origin: Instant,
    /// What the page said it plays, for a website being built as layers.
    pub overlay: Option<MediaReport>,
}

impl SourceRequest<'_> {
    pub fn ctx(&self) -> BuildCtx {
        BuildCtx {
            id: self.cfg.id.clone(),
            cfg: self.cfg.clone(),
            canvas: self.canvas.clone(),
            backends: *self.backends,
            thumb_fps: self.thumb_fps.max(1),
            browser: self.browser.clone(),
            allow_exec: self.allow_exec,
            origin: self.origin,
            tier: super::Tier::Core,
        }
    }
}

/// One entry in the registry: a manifest, how strongly it claims a bare URI,
/// and how to make one.
pub struct Provide {
    pub manifest: Manifest,
    /// The rank this provide claims a URI at, or `None` when it does not claim
    /// it at all. This is `SourceKind::detect` turned into a table: the old
    /// prefix rules live in the kinds now, one rule per kind, and the highest
    /// rank wins.
    pub claims: fn(&str) -> Option<u16>,
    pub make: fn(SourceRequest<'_>) -> Result<Box<dyn Source>>,
}

/// Every source the core ships with, in no particular order. The rank in each
/// manifest, not the position here, decides who wins a URI.
pub fn registry() -> &'static [Provide] {
    REGISTRY
}

static REGISTRY: &[Provide] = &[
    kinds::rtmp::PROVIDE,
    kinds::live::PROVIDE,
    kinds::file::PROVIDE,
    kinds::exec::PROVIDE,
    kinds::browser::PROVIDE,
    kinds::layered::PROVIDE,
    kinds::testsrc::PROVIDE,
];

/// The provide named by `type`, if this core has one.
///
/// The built in registry first, then whatever the loader has installed. In
/// that order on purpose: a plugin can add a kind and can never shadow one
/// that ships with the core, so a config written against `file/source` means
/// the same thing on every machine.
pub fn by_type(type_id: &str) -> Option<&'static Provide> {
    registry()
        .iter()
        .find(|p| p.manifest.is(type_id))
        .or_else(|| super::loader::source_provide(type_id))
}

/// The provide a bare URI resolves to, by scheme and rank.
///
/// This is what keeps `uri = "rtmp://..."` working with no `type` written down:
/// every kind says what it claims and how strongly, and the operator overrides
/// the outcome by writing `type` explicitly.
pub fn resolve(uri: &str) -> Option<&'static Provide> {
    let built_in = registry()
        .iter()
        .filter_map(|p| (p.claims)(uri).map(|rank| (rank, p)))
        .max_by_key(|(rank, _)| *rank);
    let loaded = super::loader::source_for_uri(uri).map(|p| (p.manifest.rank, p));
    // One table, ranked together. A plugin that claims `rtmp://` at 240 beats
    // the built in kind at 200, which is exactly what `rank` is for and what
    // lets an operator install a better RTMP source without editing a config.
    match (built_in, loaded) {
        (Some((a, p)), Some((b, q))) => Some(if b > a { q } else { p }),
        (Some((_, p)), None) => Some(p),
        (None, Some((_, q))) => Some(q),
        (None, None) => None,
    }
}

/// The `type` a config entry means, whether it wrote one or only a URI.
///
/// A `type` that names nothing is an error that lists what this build has,
/// because "unknown source type: ndi/source" with no list sends an operator
/// looking in the wrong place.
pub fn resolve_config(cfg: &SourceConfig) -> Result<&'static Provide> {
    if let Some(t) = cfg.type_id.as_deref().filter(|t| !t.trim().is_empty()) {
        return by_type(t.trim()).ok_or_else(|| {
            anyhow::anyhow!(
                "no source type `{t}` in this build. It has: {}",
                available().join(", ")
            )
        });
    }
    resolve(&cfg.uri).ok_or_else(|| {
        anyhow::anyhow!(
            "nothing in this build opens `{}`. Write `type` to say what it is; this build has: {}",
            cfg.uri,
            available().join(", ")
        )
    })
}

/// Every source type id this build carries, for an error message or a listing.
pub fn available() -> Vec<String> {
    let mut all: Vec<String> = registry().iter().map(|p| p.manifest.provide_id()).collect();
    all.extend(super::loader::available());
    all
}

/// Every source kind this build carries, with what it is and what it claims.
pub fn described() -> Vec<super::KindInfo> {
    let mut all: Vec<super::KindInfo> = registry().iter().map(|p| p.manifest.describe()).collect();
    // A loaded plugin's provides are in the same table a picker reads, so a
    // build with a plugin installed offers its tile without the page being
    // redeployed.
    all.extend(super::loader::described());
    all
}

/// The capabilities a source ends up with, given what its manifest declared
/// and what this particular instance turned out to be.
pub fn instance_capabilities(manifest: &Manifest, seekable: bool) -> CapabilitySet {
    let mut caps = manifest.capabilities;
    caps.set(Capability::Seek, seekable && caps.has(Capability::Seek));
    caps
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn a_bare_uri_still_picks_the_kind_it_always_did() {
        let cases = [
            ("rtmp://host/app/key", "rtmp/source"),
            ("rtmps://host/app/key", "rtmp/source"),
            ("https://host/live.m3u8", "hls/source"),
            ("rtsp://host/stream", "hls/source"),
            ("srt://host:9000", "hls/source"),
            ("/clips/ad.mp4", "file/source"),
            ("https://host/clip.mp4", "file/source"),
            ("web+https://example.com", "browser/source"),
            ("web://example.com", "browser/source"),
            ("exec:ffmpeg -i x -f mpegts -", "exec/source"),
            ("EXEC://something", "exec/source"),
            ("test://smpte", "test/source"),
        ];
        for (uri, want) in cases {
            let got = resolve(uri).expect("every uri resolves to something");
            assert_eq!(got.manifest.provide_id(), want, "for {uri}");
        }
    }

    #[test]
    fn an_unknown_type_names_what_the_build_has() {
        let mut cfg = SourceConfig::bare("x", "ndi://CAM 1");
        cfg.type_id = Some("ndi/source".into());
        let err = match resolve_config(&cfg) {
            Ok(p) => panic!("this build has no ndi, but {} claimed it", p.manifest.provide_id()),
            Err(e) => e,
        };
        let text = format!("{err}");
        assert!(text.contains("ndi/source"), "{text}");
        assert!(text.contains("rtmp/source"), "{text}");
    }
}
