//! `chroma/filter`: a green or blue screen key, built on GStreamer's `alpha`.
//!
//! The bin is `videoconvert ! alpha ! videoconvert ! capsfilter`. Two
//! conversions because `alpha` works in AYUV and the canvas is I420, and a
//! capsfilter on the way out because a filter must hand back exactly the caps
//! it was given: anything downstream of a source's capsfilter is
//! interchangeable, and a filter that changed that would break the take.
//!
//! The alpha it produces is flattened against whatever is behind it by the
//! compositor the filter's pad belongs to. On the programme that is the
//! programme compositor, so a keyed source shows the source under it; on a
//! source's own input side there is nothing behind it yet and the key reads as
//! black, which is why the useful attachment is the programme side and why the
//! config spells the side out.

use crate::caps::CanvasCaps;
use crate::config::Params;
use crate::gstutil::{self, make};
use crate::plugin::filter::{Filter, Stream};
use crate::plugin::{
    CapabilitySet, Configure, Manifest, MediaDecl, ProvideKind, StreamMode, Tier, API_LEVEL,
};
use anyhow::{Context, Result};
use gstreamer as gst;
use gstreamer::prelude::*;

pub const MANIFEST: Manifest = Manifest {
    plugin: "chroma",
    id: "filter",
    kind: ProvideKind::Filter,
    api: API_LEVEL,
    description: "A green, blue or custom colour key",
    uri_schemes: &[],
    rank: 128,
    media: MediaDecl {
        video: StreamMode::Raw,
        audio: StreamMode::None,
        alpha: true,
        thumb: false,
    },
    capabilities: CapabilitySet::new(),
    // Two conversions and a per pixel test, all within one frame. Declared as
    // zero because it adds no queue: nothing here holds a buffer.
    latency_ms: 0,
    tier: Tier::Core,
};

/// What `method` may be, and what the `alpha` element calls each one.
const METHODS: &[(&str, &str)] = &[("green", "green"), ("blue", "blue"), ("custom", "custom")];

#[derive(Default)]
pub struct ChromaKey {
    alpha: Option<gst::Element>,
}

impl Filter for ChromaKey {
    fn manifest(&self) -> &Manifest {
        &MANIFEST
    }

    fn build(&mut self, canvas: &CanvasCaps, params: &Params) -> Result<gst::Element> {
        validate(params)?;
        let name = params.get("id").and_then(|v| v.as_str()).unwrap_or("chroma").to_string();
        let bin = gst::Bin::with_name(&format!("filter-{name}"));
        let pre = make("videoconvert", &format!("filter-{name}-pre"))?;
        let alpha = make("alpha", &format!("filter-{name}-alpha"))?;
        let post = make("videoconvert", &format!("filter-{name}-post"))?;
        let caps = gstutil::capsfilter(&format!("filter-{name}-caps"), &canvas.video())?;
        apply(&alpha, params);

        bin.add_many([&pre, &alpha, &post, &caps]).context("adding chroma key elements")?;
        gst::Element::link_many([&pre, &alpha, &post, &caps]).context("linking the chroma key")?;
        let sink = pre.static_pad("sink").context("chroma key has no sink pad")?;
        let src = caps.static_pad("src").context("chroma key has no src pad")?;
        bin.add_pad(&gst::GhostPad::with_target(&sink)?).context("ghosting the sink pad")?;
        bin.add_pad(&gst::GhostPad::with_target(&src)?).context("ghosting the src pad")?;
        self.alpha = Some(alpha);
        Ok(bin.upcast())
    }

    fn configure(&mut self, params: &Params) -> Result<Configure> {
        validate(params)?;
        let Some(alpha) = &self.alpha else {
            return Ok(Configure::RestartRequired("the filter is not built yet".into()));
        };
        apply(alpha, params);
        Ok(Configure::Applied)
    }

    fn stream(&self) -> Stream {
        Stream::Video
    }
}

/// Write every setting the params carry onto the element, defensively: a
/// property this GStreamer version does not have is a warning, not a crash.
fn apply(alpha: &gst::Element, params: &Params) {
    let method = params.get("method").and_then(|v| v.as_str()).unwrap_or("green");
    let element_method = METHODS.iter().find(|(k, _)| *k == method).map(|(_, v)| *v).unwrap_or("green");
    crate::probe::set_enum(alpha, "method", element_method);
    if let Some(v) = params.get("target_r").and_then(|v| v.as_integer()) {
        crate::probe::set_int(alpha, "target-r", v.clamp(0, 255));
    }
    if let Some(v) = params.get("target_g").and_then(|v| v.as_integer()) {
        crate::probe::set_int(alpha, "target-g", v.clamp(0, 255));
    }
    if let Some(v) = params.get("target_b").and_then(|v| v.as_integer()) {
        crate::probe::set_int(alpha, "target-b", v.clamp(0, 255));
    }
    if let Some(v) = number(params.get("angle")) {
        set_float(alpha, "angle", v.clamp(0.0, 90.0));
    }
    if let Some(v) = number(params.get("noise")) {
        set_float(alpha, "noise-level", v.max(0.0));
    }
    if let Some(v) = number(params.get("spread")) {
        crate::probe::set_int(alpha, "black-sensitivity", v.max(0.0) as i64);
    }
}

/// Set a float valued property whether the element spells it f32 or f64. The
/// same defensive shape as `probe::set_int`, kept here because it is the only
/// place in the core that writes a float property.
fn set_float(el: &gst::Element, prop: &str, v: f32) {
    let Some(pspec) = el.find_property(prop) else {
        tracing::debug!(element = %el.name(), prop, "no such property on this element version");
        return;
    };
    let t = pspec.value_type();
    if t == f32::static_type() {
        el.set_property(prop, v);
    } else if t == f64::static_type() {
        el.set_property(prop, v as f64);
    } else {
        tracing::warn!(element = %el.name(), prop, ?t, "unexpected property type, skipping");
    }
}

fn number(v: Option<&toml::Value>) -> Option<f32> {
    match v? {
        toml::Value::Float(f) => Some(*f as f32),
        toml::Value::Integer(i) => Some(*i as f32),
        _ => None,
    }
}

/// A bad param names the field and what it accepts.
pub fn validate(params: &Params) -> Result<()> {
    for (key, value) in params {
        match key.as_str() {
            "method" => {
                let s = value.as_str().unwrap_or_default();
                anyhow::ensure!(
                    METHODS.iter().any(|(k, _)| *k == s),
                    "chroma/filter params.method must be green, blue or custom, not `{s}`"
                );
            }
            "target_r" | "target_g" | "target_b" => {
                let n = value.as_integer().unwrap_or(-1);
                anyhow::ensure!(
                    (0..=255).contains(&n),
                    "chroma/filter params.{key} must be 0 to 255, not `{value}`"
                );
            }
            "angle" => {
                let n = number(Some(value)).unwrap_or(-1.0);
                anyhow::ensure!(
                    (0.0..=90.0).contains(&n),
                    "chroma/filter params.angle must be 0 to 90 degrees, not `{value}`"
                );
            }
            "noise" | "spread" => {
                anyhow::ensure!(
                    number(Some(value)).is_some_and(|n| n >= 0.0),
                    "chroma/filter params.{key} must be a number of 0 or more, not `{value}`"
                );
            }
            "id" => {}
            other => {
                anyhow::ensure!(
                    false,
                    "chroma/filter has no setting `{other}`. It takes: method, target_r, \
                     target_g, target_b, angle, noise, spread"
                );
            }
        }
    }
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;

    fn params(pairs: &[(&str, toml::Value)]) -> Params {
        pairs.iter().map(|(k, v)| (k.to_string(), v.clone())).collect()
    }

    #[test]
    fn a_bad_method_names_the_field_and_the_choices() {
        let err = validate(&params(&[("method", toml::Value::String("pink".into()))]))
            .expect_err("pink is not a keying method");
        let text = format!("{err}");
        assert!(text.contains("params.method"), "{text}");
        assert!(text.contains("green"), "{text}");
    }

    #[test]
    fn an_unknown_setting_lists_the_ones_that_exist() {
        let err = validate(&params(&[("colour", toml::Value::String("green".into()))]))
            .expect_err("colour is not a setting");
        assert!(format!("{err}").contains("method"), "{err}");
    }

    /// The other half of the acceptance: a chroma key written in config, on one
    /// source, built into that source's own pipeline before it starts, with the
    /// canvas contract unchanged below it.
    #[test]
    fn a_configured_chroma_key_goes_on_one_source_and_keeps_the_canvas_contract() {
        let _ = gstreamer::init();
        if !crate::probe::exists("alpha") {
            println!("skipping: this build of GStreamer has no `alpha` element");
            return;
        }
        let canvas = crate::plugin::harness::test_canvas();
        let backends =
            crate::probe::Backends::probe(crate::config::Accel::Auto, crate::config::Accel::Auto)
                .unwrap();
        let cfg = crate::config::SourceConfig::bare("keyed", "test://smpte");
        let input = crate::input::InputPipeline::build_kind(
            &cfg,
            &canvas,
            &backends,
            8,
            std::time::Instant::now(),
            false,
            &crate::config::BrowserConfig::default(),
            None,
            false,
        )
        .expect("a test source builds");

        let mut params = Params::new();
        params.insert("method".into(), toml::Value::String("green".into()));
        let filter = crate::config::FilterConfig {
            place: None,
            id: "key".into(),
            type_id: MANIFEST.provide_id(),
            attach: crate::config::FilterAttach {
                source: Some("keyed".into()),
                side: crate::config::FilterAttachSide::Input,
                programme: false,
            },
            params,
        };
        input.attach_filter(&filter, &canvas, false).expect("a filter goes on at build time");
        assert_eq!(input.filter_ids(), vec!["key".to_string()]);

        input.start().expect("the source starts with the key on it");
        std::thread::sleep(std::time::Duration::from_millis(800));

        // The contract below the filter is untouched: what leaves this source
        // is still exactly what every other source produces.
        let pad = input.video_proxy.static_pad("sink").unwrap();
        let have = pad.current_caps().expect("caps reached the proxy sink");
        assert!(
            have.is_subset(&canvas.video()),
            "a filter changed the canvas contract: {have}"
        );
        assert!(input.health.saw_video(), "no frames came through the key");

        input.remove_filter("key").expect("and comes off again");
        assert!(input.filter_ids().is_empty());
        input.stop();
    }

    #[test]
    fn the_defaults_and_the_ranges_are_accepted() {
        validate(&params(&[
            ("method", toml::Value::String("blue".into())),
            ("angle", toml::Value::Float(25.0)),
            ("target_g", toml::Value::Integer(255)),
            ("noise", toml::Value::Integer(2)),
        ]))
        .expect("every one of these is in range");
    }
}
