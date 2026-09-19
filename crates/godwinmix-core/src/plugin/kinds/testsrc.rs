//! `test/source`: a colour bar and a tone, for the conformance harness.
//!
//! It exists so the harness has something to check that needs no network, no
//! file and no browser: `test://smpte` produces video and audio at canvas caps
//! on any machine that has GStreamer at all. Every check the harness makes of a
//! real kind it makes of this one first, so a failure says whether the fault is
//! in the kind or in the harness.

use super::{assemble, BuildCtx, Ingest, KindParts, Wiring};
use crate::caps::CanvasCaps;
use crate::config::Params;
use crate::gstutil::make;
use crate::plugin::source::{unknown_method, Provide, Source, SourceRequest};
use crate::plugin::{
    Capability, CapabilitySet, Configure, Health, Hello, Manifest, MediaDecl, MediaEnds,
    PluginState, ProvideKind, Ready, StreamMode, Tier, API_LEVEL,
};
use anyhow::{Context, Result};
use gstreamer::prelude::*;
use serde_json::Value;

pub const MANIFEST: Manifest = Manifest {
    plugin: "test",
    id: "source",
    kind: ProvideKind::Source,
    api: API_LEVEL,
    description: "A test pattern and a tone, for the conformance harness",
    uri_schemes: &["test://"],
    rank: 256,
    media: MediaDecl {
        video: StreamMode::Raw,
        audio: StreamMode::Raw,
        alpha: false,
        thumb: true,
    },
    capabilities: CapabilitySet::new()
        .with(Capability::RestartInPlace)
        .with(Capability::ProgrammeTimeline)
        .with(Capability::Health),
    latency_ms: 0,
    tier: Tier::Core,
};

pub const PROVIDE: Provide = Provide { manifest: MANIFEST, claims, make: new };

fn claims(uri: &str) -> Option<u16> {
    uri.trim().to_lowercase().starts_with("test://").then_some(MANIFEST.rank)
}

/// The pattern after `test://`, defaulting to colour bars.
fn pattern(uri: &str) -> String {
    let rest = uri.trim().trim_start_matches("test://").trim_start_matches("TEST://");
    let name = rest.split(['?', '#', '/']).next().unwrap_or("");
    if name.is_empty() { "smpte".to_string() } else { name.to_string() }
}

fn new(req: SourceRequest<'_>) -> Result<Box<dyn Source>> {
    Ok(Box::new(TestSource { pattern: pattern(&req.cfg.uri), ctx: req.ctx(), running: false }))
}

pub struct TestSource {
    ctx: BuildCtx,
    pattern: String,
    running: bool,
}

impl Source for TestSource {
    fn manifest(&self) -> &Manifest {
        &MANIFEST
    }

    fn initialize(&mut self, hello: Hello) -> Result<Ready> {
        self.ctx.canvas = hello.canvas;
        Ok(Ready {
            manifest: MANIFEST,
            latency_ms: MANIFEST.latency_ms,
            capabilities: MANIFEST.capabilities,
        })
    }

    fn start(&mut self, canvas: &CanvasCaps, thumb: bool) -> Result<MediaEnds> {
        self.ctx.canvas = canvas.clone();
        let id = &self.ctx.id;
        let vsrc = make("videotestsrc", &format!("{id}-src-test"))?;
        vsrc.set_property("is-live", true);
        // Not `set_property_from_str`: that panics on a name the enum has
        // never heard of, and this one comes off a URI an operator typed. A
        // panic here is a panic on the mixer thread, which is the programme.
        crate::probe::try_set_enum(&vsrc, "pattern", &self.pattern)
            .with_context(|| format!("{}: {}", self.ctx.id, pattern_help()))?;
        let asrc = make("audiotestsrc", &format!("{id}-src-tone"))?;
        asrc.set_property("is-live", true);
        // Ten millisecond buffers, which is what the canvas contract asks a
        // plugin for on the audio side.
        crate::probe::set_int(&asrc, "samplesperbuffer", (canvas.sample_rate / 100) as i64);
        let ends = assemble(
            &self.ctx,
            thumb,
            Ingest::default().with([vsrc.clone(), asrc.clone()]).livesync(false),
            |w: &Wiring| {
                vsrc.link(&w.norm.video_entry()).context("linking the test pattern")?;
                asrc.link(&w.norm.audio_entry()).context("linking the test tone")?;
                w.has_video.store(true, std::sync::atomic::Ordering::Relaxed);
                w.has_audio.store(true, std::sync::atomic::Ordering::Relaxed);
                Ok(KindParts::default())
            },
        )?;
        self.running = true;
        Ok(ends)
    }

    fn stop(&mut self) -> Result<()> {
        self.running = false;
        Ok(())
    }

    fn configure(&mut self, _params: &Params) -> Result<Configure> {
        Ok(Configure::Applied)
    }

    fn health(&self) -> Health {
        Health::of(if self.running { PluginState::Running } else { PluginState::Starting })
    }

    fn call(&mut self, method: &str, _params: Value) -> Result<Value> {
        match method {
            "restart" => Ok(Value::Null),
            other => Err(unknown_method(&MANIFEST, other, &["restart"])),
        }
    }
}

/// The patterns this machine's `videotestsrc` accepts, read off the element.
fn patterns() -> Vec<String> {
    crate::probe::factory_enum_nicks("videotestsrc", "pattern")
}

/// One line naming what to write instead, for the error an operator reads.
fn pattern_help() -> String {
    let names = patterns();
    if names.is_empty() {
        return "videotestsrc is not installed, so test:// sources cannot be built here. \
                Install gst-plugins-base."
            .to_string();
    }
    format!("write test://<pattern>, where <pattern> is one of: {}", names.join(", "))
}

/// Check the pattern before anything is built.
///
/// The name comes out of the URI, which means it comes from whoever typed the
/// URI, and `videotestsrc` panics rather than complains about a name it does
/// not know. Checking here turns a dead mixer thread into a 400 that lists
/// the names that would have worked.
pub fn validate(params: &Params) -> Result<()> {
    let Some(uri) = params.get("uri").and_then(|v| v.as_str()) else {
        return Ok(());
    };
    let want = pattern(uri);
    let names = patterns();
    // An empty list means videotestsrc is missing. That is a real fault, but
    // it is the build's fault rather than this URI's, and `start` reports it
    // with the same words.
    if names.is_empty() || names.contains(&want) {
        return Ok(());
    }
    anyhow::bail!("`{want}` is not a videotestsrc pattern. {}", pattern_help())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn the_pattern_comes_off_the_uri_and_defaults_to_bars() {
        assert_eq!(pattern("test://"), "smpte");
        assert_eq!(pattern("test://ball"), "ball");
        assert_eq!(pattern("test://snow?x=1"), "snow");
    }

    fn params_for(uri: &str) -> Params {
        let mut p = Params::new();
        p.insert("uri".into(), toml::Value::String(uri.into()));
        p
    }

    /// `test://bars` looks right and is not a pattern videotestsrc has. It
    /// used to panic the mixer thread inside `set_property_from_str`, which
    /// took the programme off air. It has to be a refusal that says what to
    /// write instead.
    #[test]
    fn a_pattern_videotestsrc_does_not_have_is_refused_by_name() {
        gstreamer::init().expect("gstreamer");
        let e = validate(&params_for("test://bars")).expect_err("bars is not a pattern");
        let message = format!("{e:#}");
        assert!(message.contains("bars"), "the error has to name what was asked for: {message}");
        assert!(message.contains("smpte"), "and list what would work: {message}");
        assert!(message.contains("ball"), "the whole list, not the default: {message}");
    }

    #[test]
    fn the_patterns_that_do_exist_pass_and_so_does_a_source_with_no_uri() {
        gstreamer::init().expect("gstreamer");
        validate(&params_for("test://smpte")).expect("smpte is the default pattern");
        validate(&params_for("test://ball")).expect("ball has been there since 2009");
        validate(&params_for("test://")).expect("no pattern means the default");
        validate(&Params::new()).expect("a source with no uri is not this kind's problem");
    }

    /// The names are read off the element, not typed into this file, so a
    /// build with more patterns than we know about accepts them.
    #[test]
    fn the_list_comes_off_the_element() {
        gstreamer::init().expect("gstreamer");
        let names = patterns();
        assert!(names.iter().any(|n| n == "smpte"), "read from videotestsrc: {names:?}");
        assert!(names.len() > 5, "videotestsrc has more than five patterns: {names:?}");
    }

    /// And the setter refuses instead of panicking, which is the fault that
    /// started this.
    #[test]
    fn the_setter_refuses_rather_than_panicking() {
        gstreamer::init().expect("gstreamer");
        let el = crate::gstutil::make("videotestsrc", "testsrc-refusal").expect("videotestsrc");
        crate::probe::try_set_enum(&el, "pattern", "bars").expect_err("bars is not a pattern");
        crate::probe::try_set_enum(&el, "pattern", "ball").expect("ball is");
    }
}
