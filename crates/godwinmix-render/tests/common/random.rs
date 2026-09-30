//! Random shows from a seed, repeatable, with no dependency.

use godwinmix_render::*;

/// xorshift64. Seed with anything but 0.
pub struct Rng(pub u64);

/// Sources and the requests on them, as `plan` takes them.
pub type Show = (
    Vec<(SourceId, StreamInfo)>,
    Vec<(SourceId, RenditionRequest)>,
);

const CODECS: [VideoCodec; 4] = [
    VideoCodec::H264,
    VideoCodec::H265,
    VideoCodec::Vp9,
    VideoCodec::Av1,
];
const CONTAINERS: [Container; 6] = [
    Container::Flv,
    Container::Webrtc,
    Container::MpegTs,
    Container::Hls,
    Container::Mkv,
    Container::Mp4Fragmented,
];
const HEIGHTS: [u32; 5] = [1080, 720, 540, 480, 360];
const RATES: [u32; 5] = [800, 1200, 3000, 4500, 6000];

impl Rng {
    pub fn next(&mut self) -> u64 {
        self.0 ^= self.0 << 13;
        self.0 ^= self.0 >> 7;
        self.0 ^= self.0 << 17;
        self.0
    }

    pub fn below(&mut self, n: u64) -> u64 {
        self.next() % n
    }

    pub fn pick<T: Copy>(&mut self, list: &[T]) -> T {
        list[self.below(list.len() as u64) as usize]
    }

    /// Half the time nothing, otherwise one of `list`.
    fn maybe<T: Copy>(&mut self, list: &[T]) -> Option<T> {
        if self.below(2) == 0 {
            return None;
        }
        Some(self.pick(list))
    }

    pub fn source(&mut self) -> StreamInfo {
        let raw = self.below(4) == 0;
        let h = self.pick(&[1080, 720, 2160]);
        let info = super::encoded(
            self.pick(&CODECS),
            h * 16 / 9,
            h,
            self.pick(&[30, 60, 25]),
            self.pick(&RATES),
        );
        if raw {
            return StreamInfo {
                encoded: false,
                ..info
            };
        }
        info
    }

    pub fn request(&mut self, id: String) -> RenditionRequest {
        let container = self.pick(&CONTAINERS);
        // Mostly codecs the container carries; a refusal now and then is fine.
        let allowed: Vec<VideoCodec> = container::video_codecs(container)
            .iter()
            .copied()
            .filter(|c| *c != VideoCodec::Mpeg2)
            .collect();
        let codec = if self.below(50) == 0 {
            VideoCodec::Vp8
        } else {
            self.pick(&allowed)
        };
        let video = VideoWant {
            codec: self.maybe(&[codec]),
            height: self.maybe(&HEIGHTS),
            fps: self.maybe(&[30, 60]).map(Fps::whole),
            bitrate_kbps: self.maybe(&RATES),
            keyframe_ms: self.maybe(&[1000, 2000, 4000]),
            ..VideoWant::default()
        };
        let audio = AudioWant {
            channels: self.maybe(&[1, 2]),
            bitrate_kbps: self.maybe(&[96, 128, 160]),
            ..AudioWant::default()
        };
        RenditionRequest {
            id,
            container,
            video: self.maybe(&[()]).map(|()| video),
            audio: self.maybe(&[()]).map(|()| audio),
            no_video: false,
            no_audio: self.below(8) == 0,
        }
    }

    /// `sources` sources and `requests` requests spread over them.
    pub fn show(&mut self, sources: usize, requests: usize) -> Show {
        let srcs: Vec<(SourceId, StreamInfo)> = (0..sources)
            .map(|i| (format!("src-{i}"), self.source()))
            .collect();
        let reqs = (0..requests)
            .map(|i| {
                let on = srcs[self.below(sources as u64) as usize].0.clone();
                (on, self.request(format!("out-{i}")))
            })
            .collect();
        (srcs, reqs)
    }
}
