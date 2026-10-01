//! What a show's `hls://` output says: its name, its params, and the one
//! rendition it may ask for.
//!
//! The params are kept on the output's sealed address as its query
//! (`hls://viewers?segment_ms=1000&window=6`), so a viewer key given in
//! them is sealed with it and nothing new is written to the list of shows.

use godwinmix_core::config::Params;
use godwinmix_core::hls::HlsParams;
use godwinmix_protocol::error::RpcError;
use godwinmix_protocol::rendition::RenditionChoice;
use godwinmix_protocol::shows::HlsOutputParams;

pub const SCHEME: &str = "hls";

/// An HLS output's name and params, read back from its address.
#[derive(Debug, Clone, PartialEq)]
pub struct HlsSpec {
    pub params: HlsParams,
    pub viewer_key: Option<String>,
}

/// The name in `hls://viewers`, which is what an id is made from.
pub fn name_of(uri: &str) -> Option<String> {
    let rest = uri.trim().strip_prefix("hls://")?;
    let name = rest.split(['?', '/']).next().unwrap_or_default();
    let slug = crate::channels::keys::slug(name);
    (!slug.is_empty()).then_some(slug)
}

/// The address to keep: the name, and the params as its query.
pub fn address(name: &str, params: Option<&HlsOutputParams>) -> Result<String, RpcError> {
    let Some(p) = params else { return Ok(format!("hls://{name}")) };
    check(p)?;
    let mut q: Vec<String> = Vec::new();
    let mut put = |k: &str, v: Option<String>| {
        if let Some(v) = v {
            q.push(format!("{k}={v}"));
        }
    };
    put("segment_ms", p.segment_ms.map(|v| v.to_string()));
    put("part_ms", p.part_ms.map(|v| v.to_string()));
    put("low_latency", p.low_latency.map(|v| v.to_string()));
    put("window", p.window.map(|v| v.to_string()));
    put("viewer_key", p.viewer_key.clone());
    Ok(if q.is_empty() { format!("hls://{name}") } else { format!("hls://{name}?{}", q.join("&")) })
}

/// Refuse params an `hls/output` would refuse, with its own sentence.
pub fn check(p: &HlsOutputParams) -> Result<(), RpcError> {
    if let Some(k) = &p.viewer_key {
        if k.len() < 16 || !k.chars().all(|c| c.is_ascii_alphanumeric() || c == '-' || c == '_') {
            let msg = "params.viewer_key must be at least 16 letters, digits, - or _, or left out for the one this machine makes";
            return Err(RpcError::invalid_params(msg).with("field", "params.viewer_key"));
        }
    }
    HlsParams::from_params(&table(p)).map(|_| ()).map_err(|e| RpcError::invalid_params(format!("{e:#}")).with("field", "params"))
}

fn table(p: &HlsOutputParams) -> Params {
    let mut t = Params::new();
    let mut num = |k: &str, v: Option<u32>| {
        if let Some(v) = v {
            t.insert(k.into(), toml::Value::Integer(i64::from(v)));
        }
    };
    num("segment_ms", p.segment_ms);
    num("part_ms", p.part_ms);
    num("window", p.window);
    if let Some(on) = p.low_latency {
        t.insert("low_latency".into(), toml::Value::Boolean(on));
    }
    t
}

/// Read a kept address back. An address written by [`address`] always reads.
pub fn read(uri: &str) -> HlsSpec {
    let query = uri.split_once('?').map(|(_, q)| q).unwrap_or_default();
    let mut p = HlsOutputParams::default();
    for (k, v) in query.split('&').filter_map(|kv| kv.split_once('=')) {
        match k {
            "segment_ms" => p.segment_ms = v.parse().ok(),
            "part_ms" => p.part_ms = v.parse().ok(),
            "window" => p.window = v.parse().ok(),
            "low_latency" => p.low_latency = v.parse().ok(),
            "viewer_key" => p.viewer_key = Some(v.to_string()),
            _ => {}
        }
    }
    let params = HlsParams::from_params(&table(&p)).unwrap_or_default();
    HlsSpec { params, viewer_key: p.viewer_key }
}

/// A rendition an HLS output of a direct show can carry: one, not a ladder.
pub fn check_rendition(id: &str, choice: &RenditionChoice) -> Result<(), RpcError> {
    let expanded = godwinmix_render::presets::expand(id, choice).map_err(|e| RpcError::invalid_params(e).with("field", "rendition"))?;
    if expanded.is_some_and(|r| r.len() > 1) {
        let msg = "an HLS output of a show without compositing carries one rendition, and that is a ladder. \
                   Ask for one, such as {\"preset\": \"youtube-720p30\"} or {\"audio\": {\"codec\": \"aac\"}}, or turn \
                   compositing on with show.set and add an hls/output with the ladder inside the show.";
        return Err(RpcError::invalid_params(msg).with("field", "rendition").with("ladder", true));
    }
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;
    use serde_json::json;

    #[test]
    fn the_params_ride_on_the_address_and_read_back() {
        let p: HlsOutputParams = serde_json::from_value(json!({"segment_ms": 1000, "part_ms": 250, "window": 6})).unwrap();
        let a = address("viewers", Some(&p)).unwrap();
        assert_eq!(a, "hls://viewers?segment_ms=1000&part_ms=250&window=6");
        assert_eq!(read(&a).params, HlsParams { segment_ms: 1000, part_ms: 250, window_s: 6 });
        assert_eq!(read("hls://viewers").params, HlsParams::default());
        assert_eq!(name_of("hls://Front Door?x=1").as_deref(), Some("front-door"));
        assert_eq!(name_of("udp://h:1"), None);
    }

    #[test]
    fn params_an_hls_output_would_refuse_are_refused_with_its_words() {
        let p: HlsOutputParams = serde_json::from_value(json!({"segment_ms": 100})).unwrap();
        let e = address("v", Some(&p)).unwrap_err();
        assert!(e.message.contains("500 to 10000"), "{e:?}");
        assert_eq!(e.data["field"], "params");
        let short: HlsOutputParams = serde_json::from_value(json!({"viewer_key": "abc"})).unwrap();
        assert_eq!(check(&short).unwrap_err().data["field"], "params.viewer_key");
        let ladder: RenditionChoice = serde_json::from_value(json!({"preset": "abr-ladder-4"})).unwrap();
        assert_eq!(check_rendition("v", &ladder).unwrap_err().data["ladder"], true);
    }
}
