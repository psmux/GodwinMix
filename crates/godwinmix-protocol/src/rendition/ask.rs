//! What an output or a destination says it wants: a whole request, or a
//! preset by name.

use schemars::JsonSchema;
use serde::{Deserialize, Serialize};

use super::{rendition_preset, rendition_presets, RenditionRequest};

/// `rendition` on an output or a channel destination: `{"preset":
/// "youtube-720p30"}`, or a [`RenditionRequest`] written out. The preset
/// `copy`, or a request that asks for nothing, is a copy.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize, JsonSchema)]
#[serde(untagged)]
pub enum RenditionAsk {
    Preset { preset: String },
    Request(RenditionRequest),
}

impl RenditionAsk {
    /// The request this asks for, with `id` as its id, or why there is none:
    /// a preset nobody has heard of, or a ladder where one rendition goes.
    pub fn request(&self, id: &str) -> Result<RenditionRequest, String> {
        let mut request = match self {
            RenditionAsk::Request(r) => r.clone(),
            RenditionAsk::Preset { preset } => {
                let Some(p) = rendition_preset(preset) else {
                    let ids: Vec<String> = rendition_presets().into_iter().map(|p| p.id).collect();
                    return Err(format!("there is no preset '{preset}'. Pick one of {}.", ids.join(", ")));
                };
                if p.is_ladder() {
                    return Err(format!(
                        "'{preset}' is a ladder of renditions, and one destination sends one. Pick a \
                         single rendition such as youtube-720p30, or send the ladder to an HLS output."
                    ));
                }
                p.request
            }
        };
        request.id = id.to_string();
        Ok(request)
    }

    /// Asks for nothing to change: the `copy` preset, or an empty request.
    pub fn is_copy(&self) -> bool {
        match self {
            RenditionAsk::Preset { preset } => preset == "copy",
            RenditionAsk::Request(r) => r.video.is_none() && r.audio.is_none() && !r.no_video && !r.no_audio,
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use serde_json::json;

    #[test]
    fn a_preset_or_a_request_without_an_id_both_read() {
        let p: RenditionAsk = serde_json::from_value(json!({"preset": "youtube-720p30"})).unwrap();
        let r = p.request("yt").unwrap();
        assert_eq!(r.id, "yt");
        assert_eq!(r.video.unwrap().height, Some(720));
        let q: RenditionAsk = serde_json::from_value(json!({"video": {"height": 480}})).unwrap();
        assert_eq!(q.request("fb").unwrap().video.unwrap().height, Some(480));
        assert!(!q.is_copy());
    }

    #[test]
    fn copy_a_ladder_and_an_unknown_preset() {
        let copy: RenditionAsk = serde_json::from_value(json!({"preset": "copy"})).unwrap();
        assert!(copy.is_copy());
        let empty: RenditionAsk = serde_json::from_value(json!({})).unwrap();
        assert!(empty.is_copy());
        let ladder = RenditionAsk::Preset { preset: "abr-ladder-4".into() };
        assert!(ladder.request("x").unwrap_err().contains("HLS"));
        let odd = RenditionAsk::Preset { preset: "vhs".into() };
        assert!(odd.request("x").unwrap_err().contains("youtube-720p30"));
    }
}
