//! Which encoder makes a shape: hardware of the right codec first, then
//! software, never one whose device is full. The reason is kept so the page
//! can say "using h264-software-x264 because the GPU nvidia0 is full".

use std::collections::HashMap;

use godwinmix_protocol::rendition::{Cost, EncoderSlot, VideoShape};

use crate::container::{shape_text, video_name};
use crate::error::Skip;
use crate::graph::{Reason, ReasonCode};
use crate::model::{device_of, CostModel};

/// What the plan so far holds on each hardware device.
pub type Used = HashMap<String, Cost>;

pub struct Choice {
    pub encoder: EncoderSlot,
    pub cost: Cost,
    pub reason: Reason,
}

/// The encoders of `shape.codec`, hardware first, otherwise in model order.
fn candidates<'a>(encoders: &'a [EncoderSlot], shape: &VideoShape) -> Vec<&'a EncoderSlot> {
    let mut out: Vec<&EncoderSlot> = encoders.iter().filter(|e| e.codec == shape.codec).collect();
    out.sort_by_key(|e| !e.hardware);
    out
}

pub fn choose(
    model: &dyn CostModel,
    encoders: &[EncoderSlot],
    shape: &VideoShape,
    used: &Used,
) -> Result<Choice, Vec<Skip>> {
    let list = candidates(encoders, shape);
    let mut skips: Vec<(Skip, ReasonCode)> = Vec::new();
    for enc in &list {
        let Some(cost) = model.encode_cost(shape, enc) else {
            let why = format!("{} cannot make {}", enc.id, shape_text(shape));
            skips.push((Skip { encoder: enc.id.clone(), why }, ReasonCode::ShapeUnsupported));
            continue;
        };
        if enc.hardware {
            let device = device_of(enc);
            let room = model.room(device);
            let held = used.get(device).copied().unwrap_or_default();
            if !room.fits(&held, &cost) {
                let why = format!("the GPU {device} is full");
                skips.push((Skip { encoder: enc.id.clone(), why }, ReasonCode::DeviceFull));
                continue;
            }
        }
        let reason = reason_for(enc, shape, skips.first());
        return Ok(Choice { encoder: (*enc).clone(), cost, reason });
    }
    Err(skips.into_iter().map(|(s, _)| s).collect())
}

fn reason_for(
    enc: &EncoderSlot,
    shape: &VideoShape,
    first_skip: Option<&(Skip, ReasonCode)>,
) -> Reason {
    let codec = video_name(shape.codec);
    if let Some((skip, code)) = first_skip {
        let text = format!("using {} because {}", enc.id, skip.why);
        return Reason { code: *code, text };
    }
    if enc.hardware {
        let text = format!("{} is a hardware {codec} encoder on {}", enc.id, device_of(enc));
        return Reason { code: ReasonCode::Hardware, text };
    }
    // Hardware sorts first, so software with nothing skipped means none.
    let text = format!("using {} because this machine has no hardware {codec} encoder", enc.id);
    Reason { code: ReasonCode::SoftwareOnly, text }
}

/// Records an encoder's cost against its device.
pub fn hold(used: &mut Used, enc: &EncoderSlot, cost: &Cost) {
    if enc.hardware {
        let entry = used.entry(device_of(enc).to_string()).or_default();
        *entry = entry.plus(*cost);
    }
}
