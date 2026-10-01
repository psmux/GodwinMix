//! What a set of destinations would cost, priced and planned the way a
//! replan would, without holding a ticket or keeping anything. For a dry
//! run that has to say whether a batch fits before anything starts.

use std::collections::BTreeMap;

use godwinmix_protocol::destination::{DestinationRefusal, StoredDestination};
use godwinmix_protocol::rendition::{AudioCodec, AudioShape, Cost, Fps, StreamInfo, VideoCodec, VideoShape};

use super::model::{rooms, Model};
use super::outcome::Outcome;
use super::plan::{self, Stream};
use super::Transcode;

/// What a rendition is priced against when its input has not arrived yet:
/// a broadcast HD feed, H.264 1080p30 with stereo AAC.
pub fn assumed_input() -> (StreamInfo, &'static str) {
    let video = VideoShape { codec: VideoCodec::H264, width: 1920, height: 1080, fps: Fps::whole(30), bitrate_kbps: 6000, keyframe_ms: 0 };
    let audio = AudioShape { codec: AudioCodec::Aac, channels: 2, sample_rate: 48_000, bitrate_kbps: 128 };
    (StreamInfo { video: Some(video), audio: Some(audio), encoded: true }, "H.264 1920x1080 30 fps with stereo AAC")
}

impl Transcode {
    /// The cost of every rendition `outputs` ask for, all reading one stream
    /// called `main` shaped as `input`. Copies cost nothing. Err is the
    /// planner's refusal of the first output it could not serve.
    pub fn price(&self, outputs: &[StoredDestination], input: &StreamInfo) -> Result<Cost, DestinationRefusal> {
        let wants = super::state::wants(outputs);
        if wants.is_empty() {
            return Ok(Cost::default());
        }
        let gov = self.governor.get();
        let machine = self.machine();
        let model = Model::new(machine, gov.profile(), rooms(&gov, &BTreeMap::new(), machine));
        let streams = [Stream { name: "main".into(), since_ms: 0, info: Some(input.clone()) }];
        let planned = plan::plan(&wants, &streams, &model, &BTreeMap::new());
        let refused = planned.outcomes.values().find_map(|o| match o {
            Outcome::Refused(no) => Some(no.clone()),
            _ => None,
        });
        match refused {
            Some(no) => Err(no),
            None => Ok(planned.plan.total),
        }
    }
}
