//! A minimal streamable Matroska writer, in pure Rust.
//!
//! This is here so the crate works on a machine with no GStreamer development
//! packages at all. It writes exactly as much of the format as `decodebin`
//! needs to open the stream and no more: an EBML header, a Segment of unknown
//! size, Info, Tracks, and SimpleBlocks inside Clusters of unknown size.
//!
//! Nothing is seekable and nothing is buffered. Every call writes its bytes and
//! returns, which is what a pipe wants: a source that holds a frame back to
//! compute a Cues table is a source that adds latency for a feature nobody
//! reading a pipe can use.
//!
//! The element choice was taken from what `matroskamux streamable=true` emits
//! for `video/x-raw,format=I420` and `audio/x-raw,format=F32LE`, so the same
//! demuxer path in the core reads both.

use std::io::{Result, Write};

// Element ids, as they appear on the wire.
const ID_EBML: &[u8] = &[0x1A, 0x45, 0xDF, 0xA3];
const ID_DOCTYPE: &[u8] = &[0x42, 0x82];
const ID_DOCTYPE_VERSION: &[u8] = &[0x42, 0x87];
const ID_DOCTYPE_READ_VERSION: &[u8] = &[0x42, 0x85];
const ID_SEGMENT: &[u8] = &[0x18, 0x53, 0x80, 0x67];
const ID_INFO: &[u8] = &[0x15, 0x49, 0xA9, 0x66];
const ID_TIMECODE_SCALE: &[u8] = &[0x2A, 0xD7, 0xB1];
const ID_MUXING_APP: &[u8] = &[0x4D, 0x80];
const ID_WRITING_APP: &[u8] = &[0x57, 0x41];
const ID_TRACKS: &[u8] = &[0x16, 0x54, 0xAE, 0x6B];
const ID_TRACK_ENTRY: &[u8] = &[0xAE];
const ID_TRACK_NUMBER: &[u8] = &[0xD7];
const ID_TRACK_UID: &[u8] = &[0x73, 0xC5];
const ID_TRACK_TYPE: &[u8] = &[0x83];
const ID_CODEC_ID: &[u8] = &[0x86];
const ID_DEFAULT_DURATION: &[u8] = &[0x23, 0xE3, 0x83];
const ID_VIDEO: &[u8] = &[0xE0];
const ID_PIXEL_WIDTH: &[u8] = &[0xB0];
const ID_PIXEL_HEIGHT: &[u8] = &[0xBA];
const ID_FLAG_INTERLACED: &[u8] = &[0x9A];
const ID_COLOUR_SPACE: &[u8] = &[0x2E, 0xB5, 0x24];
const ID_AUDIO: &[u8] = &[0xE1];
const ID_SAMPLING_FREQUENCY: &[u8] = &[0xB5];
const ID_CHANNELS: &[u8] = &[0x9F];
const ID_BIT_DEPTH: &[u8] = &[0x62, 0x64];
const ID_CLUSTER: &[u8] = &[0x1F, 0x43, 0xB6, 0x75];
const ID_TIMECODE: &[u8] = &[0xE7];
const ID_SIMPLE_BLOCK: &[u8] = &[0xA3];

/// An element size of unknown length, written as an 8 byte vint of all ones.
const UNKNOWN_SIZE: [u8; 8] = [0x01, 0xFF, 0xFF, 0xFF, 0xFF, 0xFF, 0xFF, 0xFF];

/// One millisecond per timecode unit, which is what every Matroska reader
/// assumes when it sees nothing else.
const TIMECODE_SCALE_NS: u64 = 1_000_000;

/// A new cluster every two seconds. Short enough that a reader joining late
/// starts quickly, long enough that the overhead is nothing.
const CLUSTER_MS: i64 = 2_000;

/// The raw video formats a source may send.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum VideoFormat {
    /// The canvas format.
    I420,
    /// What a provide declaring `alpha = true` sends.
    Ayuv,
}

impl VideoFormat {
    /// The FourCC that goes in ColourSpace, which is how the demuxer names the
    /// format back.
    pub fn fourcc(&self) -> &'static [u8; 4] {
        match self {
            VideoFormat::I420 => b"I420",
            VideoFormat::Ayuv => b"AYUV",
        }
    }

    pub fn as_str(&self) -> &'static str {
        match self {
            VideoFormat::I420 => "I420",
            VideoFormat::Ayuv => "AYUV",
        }
    }
}

/// The video track to declare.
#[derive(Debug, Clone, Copy)]
pub struct VideoTrack {
    pub width: u32,
    pub height: u32,
    pub fps: u32,
    pub format: VideoFormat,
}

/// The audio track to declare. The media contract fixes these values, but they
/// are here rather than hard coded so a test can prove the writer carries them.
#[derive(Debug, Clone, Copy)]
pub struct AudioTrack {
    pub rate: u32,
    pub channels: u32,
    pub bit_depth: u32,
}

impl Default for AudioTrack {
    fn default() -> Self {
        AudioTrack {
            rate: 48_000,
            channels: 2,
            bit_depth: 32,
        }
    }
}

/// Track numbers are fixed so a reader does not have to guess.
pub const VIDEO_TRACK: u64 = 1;
pub const AUDIO_TRACK: u64 = 2;

/// Writes streamable Matroska to anything that takes bytes.
pub struct MatroskaWriter<W: Write> {
    out: W,
    /// Where the open cluster started, in milliseconds. `None` before the first
    /// block.
    cluster_ms: Option<i64>,
    wrote_header: bool,
    video: Option<VideoTrack>,
    audio: Option<AudioTrack>,
}

impl<W: Write> MatroskaWriter<W> {
    /// A writer that has not yet written anything. At least one track is
    /// required; a stream with neither carries nothing.
    pub fn new(out: W, video: Option<VideoTrack>, audio: Option<AudioTrack>) -> Self {
        MatroskaWriter {
            out,
            cluster_ms: None,
            wrote_header: false,
            video,
            audio,
        }
    }

    /// Write the header now. Calling it twice does nothing the second time.
    ///
    /// Call this from `start` so the core's demuxer has the tracks before the
    /// first frame is drawn; otherwise a slow first draw looks like a stall.
    pub fn write_header(&mut self) -> Result<()> {
        if self.wrote_header {
            return Ok(());
        }
        self.wrote_header = true;

        let mut ebml = Vec::new();
        element(&mut ebml, ID_DOCTYPE, b"matroska\0");
        element(&mut ebml, ID_DOCTYPE_VERSION, &uint(4));
        element(&mut ebml, ID_DOCTYPE_READ_VERSION, &uint(2));
        let mut header = Vec::new();
        element(&mut header, ID_EBML, &ebml);

        // The Segment is open ended: the plugin does not know how long it runs.
        header.extend_from_slice(ID_SEGMENT);
        header.extend_from_slice(&UNKNOWN_SIZE);

        let mut info = Vec::new();
        element(&mut info, ID_TIMECODE_SCALE, &uint(TIMECODE_SCALE_NS));
        element(&mut info, ID_MUXING_APP, b"godwinmix-sdk\0");
        element(&mut info, ID_WRITING_APP, b"godwinmix-sdk\0");
        element(&mut header, ID_INFO, &info);

        let mut tracks = Vec::new();
        if let Some(v) = self.video {
            let mut entry = Vec::new();
            element(&mut entry, ID_TRACK_NUMBER, &uint(VIDEO_TRACK));
            element(&mut entry, ID_TRACK_UID, &uint(VIDEO_TRACK));
            element(&mut entry, ID_TRACK_TYPE, &uint(1));
            element(&mut entry, ID_CODEC_ID, b"V_UNCOMPRESSED\0");
            if v.fps > 0 {
                element(
                    &mut entry,
                    ID_DEFAULT_DURATION,
                    &uint(1_000_000_000 / v.fps as u64),
                );
            }
            let mut video = Vec::new();
            element(&mut video, ID_PIXEL_WIDTH, &uint(v.width as u64));
            element(&mut video, ID_PIXEL_HEIGHT, &uint(v.height as u64));
            // 2 is progressive. Every canvas frame is.
            element(&mut video, ID_FLAG_INTERLACED, &uint(2));
            element(&mut video, ID_COLOUR_SPACE, v.format.fourcc());
            element(&mut entry, ID_VIDEO, &video);
            element(&mut tracks, ID_TRACK_ENTRY, &entry);
        }
        if let Some(a) = self.audio {
            let mut entry = Vec::new();
            element(&mut entry, ID_TRACK_NUMBER, &uint(AUDIO_TRACK));
            element(&mut entry, ID_TRACK_UID, &uint(AUDIO_TRACK));
            element(&mut entry, ID_TRACK_TYPE, &uint(2));
            element(&mut entry, ID_CODEC_ID, b"A_PCM/FLOAT/IEEE\0");
            let mut audio = Vec::new();
            element(&mut audio, ID_SAMPLING_FREQUENCY, &(a.rate as f64).to_be_bytes());
            element(&mut audio, ID_CHANNELS, &uint(a.channels as u64));
            element(&mut audio, ID_BIT_DEPTH, &uint(a.bit_depth as u64));
            element(&mut entry, ID_AUDIO, &audio);
            element(&mut tracks, ID_TRACK_ENTRY, &entry);
        }
        element(&mut header, ID_TRACKS, &tracks);

        self.out.write_all(&header)?;
        self.out.flush()
    }

    /// One video frame at the caps the track declared.
    pub fn write_video(&mut self, pts_ns: u64, data: &[u8], keyframe: bool) -> Result<()> {
        self.write_block(VIDEO_TRACK, pts_ns, data, keyframe)
    }

    /// One audio buffer, 10 ms of interleaved F32LE.
    pub fn write_audio(&mut self, pts_ns: u64, data: &[u8]) -> Result<()> {
        self.write_block(AUDIO_TRACK, pts_ns, data, true)
    }

    fn write_block(&mut self, track: u64, pts_ns: u64, data: &[u8], keyframe: bool) -> Result<()> {
        self.write_header()?;
        let ms = (pts_ns / TIMECODE_SCALE_NS) as i64;
        let base = match self.cluster_ms {
            Some(base) if ms - base < CLUSTER_MS && ms >= base => base,
            _ => {
                self.open_cluster(ms)?;
                ms
            }
        };
        let relative = ms - base;
        // Relative timecodes are a signed 16 bit field. A block that cannot
        // reach its cluster opens a new one rather than writing a wrong time.
        let relative = if !(-32768..=32767).contains(&relative) {
            self.open_cluster(ms)?;
            0
        } else {
            relative
        };

        let mut block = Vec::with_capacity(data.len() + 8);
        write_vint(&mut block, track);
        block.extend_from_slice(&(relative as i16).to_be_bytes());
        block.push(if keyframe { 0x80 } else { 0x00 });
        block.extend_from_slice(data);

        // Written straight out rather than through a Vec, so a 1080p frame is
        // not copied a second time on its way to the pipe.
        self.out.write_all(ID_SIMPLE_BLOCK)?;
        let mut size = Vec::with_capacity(8);
        write_size(&mut size, block.len() as u64);
        self.out.write_all(&size)?;
        self.out.write_all(&block)?;
        self.out.flush()
    }

    fn open_cluster(&mut self, ms: i64) -> Result<()> {
        self.cluster_ms = Some(ms);
        let mut head = Vec::with_capacity(16);
        head.extend_from_slice(ID_CLUSTER);
        head.extend_from_slice(&UNKNOWN_SIZE);
        element(&mut head, ID_TIMECODE, &uint(ms.max(0) as u64));
        self.out.write_all(&head)
    }

    /// Flush and give the sink back. A Matroska stream of unknown size needs no
    /// closing element, so there is nothing else to do.
    pub fn finish(mut self) -> Result<W> {
        self.write_header()?;
        self.out.flush()?;
        Ok(self.out)
    }

    /// Flush without consuming the writer.
    pub fn flush(&mut self) -> Result<()> {
        self.out.flush()
    }
}

// ---------------------------------------------------------------------------
// EBML primitives
// ---------------------------------------------------------------------------

/// Append `id`, the size of `payload` as a vint, then the payload.
fn element(out: &mut Vec<u8>, id: &[u8], payload: &[u8]) {
    out.extend_from_slice(id);
    write_size(out, payload.len() as u64);
    out.extend_from_slice(payload);
}

/// An unsigned integer in the shortest big endian form EBML allows.
fn uint(value: u64) -> Vec<u8> {
    if value == 0 {
        return vec![0];
    }
    let bytes = value.to_be_bytes();
    let first = bytes.iter().position(|b| *b != 0).unwrap_or(7);
    bytes[first..].to_vec()
}

/// A data size, as an EBML variable length integer.
///
/// The length marker is a leading one bit; the value fills what is left. A
/// value whose every bit is one at a given width is the "unknown" marker, so
/// this steps up a width rather than writing one by accident.
fn write_size(out: &mut Vec<u8>, value: u64) {
    for length in 1..=8u32 {
        let bits = 7 * length;
        let max = (1u64 << bits) - 1;
        if value < max {
            let marker = 1u64 << bits;
            let encoded = (marker | value).to_be_bytes();
            out.extend_from_slice(&encoded[8 - length as usize..]);
            return;
        }
    }
    // Nothing in this format reaches 2^56 bytes, but write the unknown size
    // rather than a wrong one if it ever did.
    out.extend_from_slice(&UNKNOWN_SIZE);
}

/// A track number, as an EBML variable length integer. Same encoding as a size.
fn write_vint(out: &mut Vec<u8>, value: u64) {
    write_size(out, value);
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn sizes_use_the_shortest_form() {
        let mut out = Vec::new();
        write_size(&mut out, 0);
        assert_eq!(out, vec![0x80]);

        let mut out = Vec::new();
        write_size(&mut out, 1);
        assert_eq!(out, vec![0x81]);

        let mut out = Vec::new();
        write_size(&mut out, 126);
        assert_eq!(out, vec![0xFE]);

        // 127 is the one byte "unknown" marker, so it steps up to two bytes.
        let mut out = Vec::new();
        write_size(&mut out, 127);
        assert_eq!(out, vec![0x40, 0x7F]);

        // 6148, the size matroskamux writes for a 64x64 I420 SimpleBlock.
        let mut out = Vec::new();
        write_size(&mut out, 6148);
        assert_eq!(out, vec![0x58, 0x04]);
    }

    #[test]
    fn integers_lose_their_leading_zeroes() {
        assert_eq!(uint(0), vec![0]);
        assert_eq!(uint(1), vec![1]);
        assert_eq!(uint(256), vec![1, 0]);
        assert_eq!(uint(1_000_000), vec![0x0F, 0x42, 0x40]);
    }

    fn video_writer() -> MatroskaWriter<Vec<u8>> {
        MatroskaWriter::new(
            Vec::new(),
            Some(VideoTrack {
                width: 64,
                height: 64,
                fps: 30,
                format: VideoFormat::I420,
            }),
            None,
        )
    }

    #[test]
    fn the_header_starts_the_way_every_matroska_file_does() {
        let mut w = video_writer();
        w.write_header().unwrap();
        let bytes = w.finish().unwrap();
        assert_eq!(&bytes[..4], ID_EBML);
        let text = String::from_utf8_lossy(&bytes);
        assert!(text.contains("matroska"));
        assert!(text.contains("V_UNCOMPRESSED"));
        assert!(text.contains("I420"));
        // The Segment is open ended.
        let segment = find(&bytes, ID_SEGMENT).expect("no Segment");
        assert_eq!(&bytes[segment + 4..segment + 12], &UNKNOWN_SIZE);
    }

    #[test]
    fn writing_the_header_twice_writes_it_once() {
        let mut w = video_writer();
        w.write_header().unwrap();
        w.write_header().unwrap();
        let bytes = w.finish().unwrap();
        assert_eq!(count(&bytes, ID_EBML), 1);
    }

    #[test]
    fn a_frame_opens_a_cluster_and_carries_the_bytes() {
        let mut w = video_writer();
        let frame = vec![0x42u8; 64 * 64 * 3 / 2];
        w.write_video(0, &frame, true).unwrap();
        let bytes = w.finish().unwrap();
        let stream = Walk::of(&bytes);
        assert_eq!(stream.clusters, 1);
        assert_eq!(stream.blocks.len(), 1);
        let block = &stream.blocks[0];
        assert_eq!(block.track, VIDEO_TRACK);
        assert_eq!(block.relative_ms, 0);
        assert!(block.keyframe);
        assert_eq!(block.payload, frame);
    }

    #[test]
    fn frames_within_two_seconds_share_one_cluster() {
        let mut w = video_writer();
        let frame = vec![0u8; 16];
        for i in 0..30u64 {
            w.write_video(i * 33_333_333, &frame, true).unwrap();
        }
        let stream = Walk::of(&w.finish().unwrap());
        assert_eq!(stream.clusters, 1);
        assert_eq!(stream.blocks.len(), 30);
        assert_eq!(stream.blocks[29].relative_ms, 966);
    }

    #[test]
    fn a_new_cluster_opens_after_two_seconds() {
        let mut w = video_writer();
        let frame = vec![0u8; 16];
        for i in 0..90u64 {
            w.write_video(i * 33_333_333, &frame, true).unwrap();
        }
        let stream = Walk::of(&w.finish().unwrap());
        assert_eq!(stream.clusters, 2, "three seconds is two clusters");
        assert_eq!(stream.blocks.len(), 90);
        // Every block sits inside the range a signed 16 bit field can hold.
        for block in &stream.blocks {
            assert!(block.relative_ms < CLUSTER_MS as i16);
        }
    }

    #[test]
    fn audio_and_video_share_a_cluster_and_keep_their_track_numbers() {
        let mut w = MatroskaWriter::new(
            Vec::new(),
            Some(VideoTrack {
                width: 32,
                height: 32,
                fps: 30,
                format: VideoFormat::I420,
            }),
            Some(AudioTrack::default()),
        );
        w.write_video(0, &vec![0u8; 32 * 32 * 3 / 2], true).unwrap();
        w.write_audio(0, &vec![0u8; 480 * 2 * 4]).unwrap();
        let bytes = w.finish().unwrap();
        assert!(String::from_utf8_lossy(&bytes).contains("A_PCM/FLOAT/IEEE"));
        let stream = Walk::of(&bytes);
        assert_eq!(stream.clusters, 1);
        assert_eq!(stream.blocks.len(), 2);
        assert_eq!(stream.blocks[0].track, VIDEO_TRACK);
        assert_eq!(stream.blocks[1].track, AUDIO_TRACK);
        assert_eq!(stream.blocks[1].payload.len(), 480 * 2 * 4);
    }

    #[test]
    fn an_alpha_source_declares_ayuv() {
        let mut w = MatroskaWriter::new(
            Vec::new(),
            Some(VideoTrack {
                width: 8,
                height: 8,
                fps: 25,
                format: VideoFormat::Ayuv,
            }),
            None,
        );
        w.write_header().unwrap();
        let bytes = w.finish().unwrap();
        assert!(String::from_utf8_lossy(&bytes).contains("AYUV"));
    }

    fn find(haystack: &[u8], needle: &[u8]) -> Option<usize> {
        haystack.windows(needle.len()).position(|w| w == needle)
    }

    fn count(haystack: &[u8], needle: &[u8]) -> usize {
        haystack.windows(needle.len()).filter(|w| *w == needle).count()
    }

    // ---------------------------------------------------------------------
    // A reader, so the tests check the structure rather than byte patterns.
    // Searching for a one byte element id in a stream full of pixels finds
    // pixels, which is how these tests first lied to me.
    // ---------------------------------------------------------------------

    struct Block {
        track: u64,
        relative_ms: i16,
        keyframe: bool,
        payload: Vec<u8>,
    }

    struct Walk {
        clusters: usize,
        blocks: Vec<Block>,
    }

    impl Walk {
        fn of(bytes: &[u8]) -> Walk {
            let mut walk = Walk {
                clusters: 0,
                blocks: Vec::new(),
            };
            walk.elements(bytes, &mut 0);
            walk
        }

        /// Walk a run of elements, descending into the ones with children.
        fn elements(&mut self, bytes: &[u8], at: &mut usize) {
            while *at < bytes.len() {
                let id = read_id(bytes, at);
                let size = read_size_field(bytes, at);
                let known = size.unwrap_or(u64::MAX);
                match id.as_slice() {
                    ID_SEGMENT => continue, // unknown size: its children follow
                    ID_CLUSTER => {
                        self.clusters += 1;
                        continue;
                    }
                    ID_SIMPLE_BLOCK => {
                        let end = *at + known as usize;
                        let body = &bytes[*at..end];
                        let mut i = 0usize;
                        let track = read_size_field(body, &mut i).unwrap_or(0);
                        let relative_ms = i16::from_be_bytes([body[i], body[i + 1]]);
                        let flags = body[i + 2];
                        self.blocks.push(Block {
                            track,
                            relative_ms,
                            keyframe: flags & 0x80 != 0,
                            payload: body[i + 3..].to_vec(),
                        });
                        *at = end;
                    }
                    _ => {
                        // Everything else in this writer has a known size.
                        *at += known as usize;
                    }
                }
            }
        }
    }

    fn read_id(bytes: &[u8], at: &mut usize) -> Vec<u8> {
        let first = bytes[*at];
        let length = match first {
            0x80..=0xFF => 1,
            0x40..=0x7F => 2,
            0x20..=0x3F => 3,
            _ => 4,
        };
        let id = bytes[*at..*at + length].to_vec();
        *at += length;
        id
    }

    /// A vint. `None` when it is the all ones "unknown" marker.
    fn read_size_field(bytes: &[u8], at: &mut usize) -> Option<u64> {
        let first = bytes[*at];
        let length = first.leading_zeros() as usize + 1;
        // u16 so the eight byte "unknown" marker does not shift a u8 by 8.
        let mask = (0xFFu16 >> length) as u8;
        let mut value = u64::from(first & mask);
        let mut all_ones = (first & mask) == mask;
        for byte in &bytes[*at + 1..*at + length] {
            value = (value << 8) | u64::from(*byte);
            all_ones &= *byte == 0xFF;
        }
        *at += length;
        if all_ones {
            None
        } else {
            Some(value)
        }
    }
}
