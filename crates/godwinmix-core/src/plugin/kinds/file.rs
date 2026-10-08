//! `file/source`: a finite clip. The kind an ad break is built from.
//!
//! It ends with EOS, which is how the break knows to return; anywhere else
//! its end is `params.at_end`'s to decide, see `mixer::clip_act`. And it can be
//! scrubbed, which is why it is the one built in kind declaring `seek`. It gets
//! no `livesync`: a file has neither drift nor gaps, and its timestamps start
//! at zero while the programme's running time is minutes in, so livesync judged
//! every early frame late and an eight second ad lost its first 1.4 seconds.

use super::BuildCtx;
use crate::caps::CanvasCaps;
use crate::config::Params;
use crate::plugin::source::{unknown_method, Provide, Source, SourceRequest};
use crate::plugin::{
    Capability, CapabilitySet, Configure, Health, Hello, Manifest, MediaDecl, MediaEnds,
    PluginState, ProvideKind, Ready, StreamMode, Tier, API_LEVEL,
};
use anyhow::Result;
use gstreamer as gst;
use serde_json::Value;

pub const MANIFEST: Manifest = Manifest {
    plugin: "file",
    id: "source",
    kind: ProvideKind::Source,
    api: API_LEVEL,
    description: "A finite clip on disk or over HTTP, scrubbable, ending with EOS",
    uri_schemes: &["file://"],
    // The permissive fallback: anything nothing else claims is treated as a
    // file, which is what `SourceKind::detect` did with its last line.
    rank: 1,
    media: MediaDecl {
        video: StreamMode::Container,
        audio: StreamMode::Container,
        alpha: false,
        thumb: true,
    },
    capabilities: CapabilitySet::new()
        .with(Capability::RestartInPlace)
        .with(Capability::Seek)
        .with(Capability::Health),
    latency_ms: 0,
    tier: Tier::Core,
};

pub const PROVIDE: Provide = Provide { manifest: MANIFEST, claims, make: new };

fn claims(uri: &str) -> Option<u16> {
    let lower = uri.trim().to_lowercase();
    if lower.starts_with("file://") {
        // An explicit file URI is claimed properly, not as the fallback.
        return Some(200);
    }
    // Anything else, including a plain file over HTTP, is finite. Claimed at
    // rank 1 so every other kind that recognises the scheme wins first.
    (!uri.trim().is_empty()).then_some(MANIFEST.rank)
}

fn new(req: SourceRequest<'_>) -> Result<Box<dyn Source>> {
    Ok(Box::new(FileSource { ctx: req.ctx(), pipeline: None, params: Params::new() }))
}

pub struct FileSource {
    ctx: BuildCtx,
    pipeline: Option<gst::Pipeline>,
    /// The params it runs with, so a change to `at_end` alone can be taken in
    /// place: the mixer reads it at the clip's end, and nothing in the
    /// pipeline depends on it.
    params: Params,
}

impl Source for FileSource {
    fn manifest(&self) -> &Manifest {
        &MANIFEST
    }

    fn initialize(&mut self, hello: Hello) -> Result<Ready> {
        validate(&hello.params)?;
        self.params = hello.params.clone();
        self.ctx.canvas = hello.canvas;
        Ok(Ready {
            manifest: MANIFEST,
            latency_ms: MANIFEST.latency_ms,
            capabilities: MANIFEST.capabilities,
        })
    }

    fn start(&mut self, canvas: &CanvasCaps, thumb: bool) -> Result<MediaEnds> {
        self.ctx.canvas = canvas.clone();
        if let Some(missing) = missing_file(&self.ctx.cfg.uri) {
            return Err(missing.into());
        }
        let ends = super::clip_start::open(&self.ctx, thumb)?;
        self.pipeline = Some(ends.pipeline.clone());
        Ok(ends)
    }

    fn stop(&mut self) -> Result<()> {
        Ok(())
    }

    fn configure(&mut self, params: &Params) -> Result<Configure> {
        validate(params)?;
        if !same_but_at_end(&self.params, params, &self.ctx.cfg.uri) {
            return Ok(Configure::RestartRequired("a clip takes a new path by being reopened".into()));
        }
        self.params = params.clone();
        Ok(Configure::Applied)
    }

    fn health(&self) -> Health {
        Health::of(if self.pipeline.is_some() { PluginState::Running } else { PluginState::Starting })
    }

    fn call(&mut self, method: &str, _params: Value) -> Result<Value> {
        match method {
            "restart" => Ok(Value::Null),
            other => Err(unknown_method(&MANIFEST, other, &["restart"])),
        }
    }
}

pub fn validate(params: &Params) -> Result<()> {
    if let Some(v) = params.get("uri") {
        anyhow::ensure!(v.is_str(), "file/source params.uri must be a string");
    }
    if let Some(v) = params.get("at_end") {
        anyhow::ensure!(
            v.as_str().is_some_and(|s| AT_END.contains(&s)),
            "file/source params.at_end must be \"repeat\", \"hold\" or \"leave\", not {v}. Repeat plays \
             the clip again from the start, hold keeps its last frame up, leave holds it while the \
             programme moves off it"
        );
    }
    if let Some(v) = params.get("loop") {
        anyhow::ensure!(v.is_bool(), "file/source params.loop must be true or false, not {v}. Write at_end = \"repeat\" instead");
    }
    Ok(())
}

/// What `params.at_end` may say. Read by `mixer::clip_end::AtEnd`.
const AT_END: [&str; 3] = ["repeat", "hold", "leave"];

/// Whether two sets of params differ in what the clip does at its end at
/// most. A missing `uri` is the source's own address, which is how a client
/// that sends only the field it changed writes it.
fn same_but_at_end(was: &Params, now: &Params, uri: &str) -> bool {
    let strip = |p: &Params| {
        let mut p = p.clone();
        p.remove("at_end");
        p.remove("loop");
        p.entry("uri").or_insert_with(|| toml::Value::String(uri.to_string()));
        p
    };
    strip(was) == strip(now)
}

/// A path with nothing at it, said before the pipeline is built. Left to
/// GStreamer the answer was "Element failed to change its state", which a
/// preset's placeholder clip ("media/slides.mp4", to be dropped in later)
/// turned into the first thing a person read after picking a tile.
fn missing_file(uri: &str) -> Option<godwinmix_protocol::Actionable> {
    if uri.contains("://") && !uri.starts_with("file://") {
        return None;
    }
    let path = match gst::glib::filename_from_uri(uri) {
        Ok((path, _)) => path,
        Err(_) => std::path::PathBuf::from(uri),
    };
    if path.exists() {
        return None;
    }
    Some(godwinmix_protocol::Actionable::new(
        format!(
            "there is no file at {} yet. Upload one with that name in the Media tab, or drop \
             it on the window, and the mixer tries this source again.",
            path.display()
        ),
        godwinmix_protocol::ErrorAction::open_panel("Open Media", "core/media"),
    ))
}

#[cfg(test)]
mod tests {
    use super::*;

    fn params(s: &str) -> Params {
        toml::from_str(s).unwrap()
    }

    #[test]
    fn at_end_takes_three_words_and_names_them_when_refused() {
        for word in AT_END {
            validate(&params(&format!("at_end = \"{word}\""))).unwrap();
        }
        let refused = validate(&params("at_end = \"loop\"")).unwrap_err().to_string();
        assert!(refused.contains("\"repeat\", \"hold\" or \"leave\""), "{refused}");
        assert!(validate(&params("at_end = true")).is_err());
    }

    /// Changing what a clip does at its end is taken in place, so a person
    /// flicking Repeat on a clip that is on air does not restart it.
    #[test]
    fn only_at_end_changing_needs_no_restart() {
        let uri = "/clips/intro.mp4";
        let was = params("uri = \"/clips/intro.mp4\"");
        assert!(same_but_at_end(&was, &params("at_end = \"repeat\""), uri));
        assert!(same_but_at_end(&params("at_end = \"leave\""), &params("at_end = \"hold\""), uri));
        assert!(!same_but_at_end(&was, &params("at_end = \"repeat\"\nalpha = true"), uri));
        assert!(!same_but_at_end(&was, &params("uri = \"/clips/other.mp4\""), uri));
    }

    #[test]
    fn a_clip_that_is_not_there_yet_says_so_and_opens_media() {
        let refusal = missing_file("media/definitely-not-here.mp4").expect("refused");
        assert!(refusal.message.contains("no file at"), "{}", refusal.message);
        assert_eq!(refusal.action.panel.as_deref(), Some("core/media"));
        assert!(missing_file("https://example.com/a.m3u8").is_none(), "a stream is not a file");
        let here = std::env::current_dir().unwrap().join("Cargo.toml");
        assert!(missing_file(&here.to_string_lossy()).is_none());
        assert!(missing_file(&crate::input::file_uri(&here)).is_none());
    }
}
