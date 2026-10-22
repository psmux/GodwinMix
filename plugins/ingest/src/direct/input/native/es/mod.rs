//! Elementary streams to tag bodies, byte for byte what the GStreamer path
//! makes of them.
//!
//! | PMT stream type | Module | What a PES becomes |
//! |---|---|---|
//! | 0x1B H.264 | `h264` | one access unit, length prefixed, after an `avcC` header tag |
//! | 0x24 HEVC | `hevc` | the same with an `hvcC` header tag, as enhanced RTMP |
//! | 0x0F AAC in ADTS | `adts` | one raw frame per tag, after its AudioSpecificConfig |
//! | 0x81, 0x87, or 0x06 with an AC-3 descriptor | `ac3` | one syncframe per tag, `exaudio` prefixed |
//! | 0x03, 0x04 MPEG audio | `mpa` | one frame per tag |
//! | 0x01, 0x02 MPEG-2 video | `mp2v` | one picture per tag, as `crate::mp2v` lays it out |
//!
//! Each parser is handed whole PES packets with their times already in
//! nanoseconds, unwrapped, and answers [`Unit`]s. Nothing is decoded.

mod ac3;
mod adts;
mod frames;
pub mod annexb;
mod h264;
mod h264_key;
mod hevc;
mod mp2v;
mod mpa;
pub mod rate;

pub use rate::Ns;

/// One PES packet's payload and its times, in nanoseconds.
pub struct Pes<'a> {
    pub pts: Option<Ns>,
    pub dts: Option<Ns>,
    pub data: &'a [u8],
}

impl Pes<'_> {
    /// The decode time, or the presentation time when there is no other.
    pub fn at(&self) -> Option<Ns> {
        self.dts.or(self.pts)
    }
}

/// One tag body, not yet on the session's timeline.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Unit {
    pub at: Ns,
    pub body: Vec<u8>,
    pub keyframe: bool,
    pub header: bool,
}

/// What a parser has learned of its stream, for `tracks` and the stats.
#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub struct Said {
    pub codec: &'static str,
    pub channels: u32,
    pub sample_rate: u32,
}

pub trait Parser: Send {
    /// Turn one PES packet into tag bodies, appended to `out`.
    fn pes(&mut self, pes: &Pes, out: &mut Vec<Unit>);
    /// What the stream has said about itself so far.
    fn said(&self) -> Said {
        Said::default()
    }
    /// Video tags handed on so far, and how many of them were one field of
    /// a field coded picture: `h264parse` hands on each field as an access
    /// unit of its own, and so does this, so a frame rate counts two of
    /// those as one frame.
    fn pictures(&self) -> (u64, u64) {
        (0, 0)
    }
}

/// What a stream is, from its PMT entry.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Codec {
    H264,
    Hevc,
    Mpeg2,
    Aac,
    Ac3,
    Mpa,
}

impl Codec {
    pub fn video(self) -> bool {
        matches!(self, Codec::H264 | Codec::Hevc | Codec::Mpeg2)
    }

    /// The codec a PMT entry names, from its type and, for private data
    /// (type 6), its descriptors. `None` for anything without a parser.
    pub fn of(stream: &gmx_udp::ts::tables::Stream) -> Option<Codec> {
        Some(match stream.kind() {
            "h264" => Codec::H264,
            "hevc" => Codec::Hevc,
            "mpeg2 video" => Codec::Mpeg2,
            "aac" => Codec::Aac,
            "ac3" | "eac3" => Codec::Ac3,
            "mpeg audio" => Codec::Mpa,
            _ => return None,
        })
    }

    pub fn parser(self) -> Box<dyn Parser> {
        match self {
            Codec::H264 => Box::<h264::H264>::default(),
            Codec::Hevc => Box::<hevc::Hevc>::default(),
            Codec::Mpeg2 => Box::<mp2v::Mpeg2>::default(),
            Codec::Aac => Box::<adts::Adts>::default(),
            Codec::Ac3 => Box::<ac3::Ac3>::default(),
            Codec::Mpa => Box::<mpa::Mpa>::default(),
        }
    }
}

/// The 24 bit composition offset the GStreamer path writes: each time in
/// whole milliseconds, then the difference, as `tagger::tags` does.
pub fn cts_ms(pes: &Pes) -> i64 {
    match (pes.pts, pes.dts) {
        (Some(p), Some(d)) => (p / 1_000_000) as i64 - (d / 1_000_000) as i64,
        _ => 0,
    }
}

