//! A rendition that was not started, with the code and the `data` a caller
//! builds its buttons from. Carried through the mixer's reply channel as it
//! is, so the control plane answers with the same facts.

use godwinmix_protocol::error::ErrorCode;
use godwinmix_protocol::rendition::{RenditionAdvice, RenditionRefusal, RenditionRequest};
use godwinmix_render::PlanError;
use serde_json::Value;

#[derive(Debug, Clone)]
pub struct Refusal {
    /// `Safety` when the governor said no; `InvalidParams` when the planner
    /// could not make what was asked at all.
    pub code: ErrorCode,
    pub message: String,
    pub data: Value,
}

impl std::fmt::Display for Refusal {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.write_str(&self.message)
    }
}

impl std::error::Error for Refusal {}

impl Refusal {
    pub fn plan(e: &PlanError) -> Refusal {
        Refusal { code: ErrorCode::InvalidParams, message: e.to_string(), data: e.data() }
    }

    pub fn bad_choice(message: String) -> Refusal {
        Refusal { code: ErrorCode::InvalidParams, message, data: serde_json::json!({}) }
    }

    /// The governor's refusal, with each shape that fits offered as the
    /// request to send instead.
    pub fn governor(what: &str, need: godwinmix_protocol::rendition::Cost, have: godwinmix_protocol::rendition::Cost, advice: &godwinmix_govern::Advice, asked: &RenditionRequest) -> Refusal {
        let offers = advice
            .fits
            .iter()
            .take(4)
            .map(|fit| {
                let mut request = asked.clone();
                let mut video = request.video.clone().unwrap_or_default();
                video.codec = Some(fit.slot.codec);
                video.width = Some(fit.width);
                video.height = Some(fit.height);
                video.fps = Some(fit.fps);
                // The bitrate asked for was for the bigger picture; the
                // smaller one gets the planner's own figure for its size.
                video.bitrate_kbps = None;
                request.video = Some(video);
                RenditionAdvice { text: format!("{} fits", fit.label), request }
            })
            .collect();
        let data = RenditionRefusal { need, have, advice: offers };
        let message = format!(
            "The {what} was not started, so nothing on air drops a frame. {}",
            advice.text
        );
        Refusal {
            code: ErrorCode::Safety,
            message,
            data: serde_json::to_value(data).unwrap_or_default(),
        }
    }
}
