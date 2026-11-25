//! The core's MJPEG stream of one source: `multipart/x-mixed-replace`, one
//! JPEG per part. Pictures are cut out of the byte stream at their start and
//! end markers, which is all a part is, and when several have arrived since
//! the last read only the newest is decoded.

use std::io::Read;
use std::net::TcpStream;
use std::time::{Duration, Instant};

use super::{Frames, Picture};
use crate::core::open;

pub struct MjpegFrames {
    url: String,
    source: String,
    stream: Option<TcpStream>,
    pending: Vec<u8>,
}

impl MjpegFrames {
    pub fn new(url: String, source: &str) -> MjpegFrames {
        MjpegFrames { url, source: source.to_string(), stream: None, pending: Vec::new() }
    }

    fn connect(&mut self) -> Result<(), String> {
        let mut s = open(&self.url, Duration::from_secs(3))?;
        let mut head = Vec::new();
        let mut byte = [0u8; 1];
        while !head.ends_with(b"\r\n\r\n") {
            match s.read(&mut byte) {
                Ok(1) => head.push(byte[0]),
                _ => return Err(format!("the mixer closed the picture stream of '{}' before answering", self.source)),
            }
            if head.len() > 16_384 {
                return Err("the mixer's answer had no end to its headers".into());
            }
        }
        let head = String::from_utf8_lossy(&head).to_string();
        let status = head.split_whitespace().nth(1).unwrap_or("").to_string();
        if status != "200" {
            let mut body = String::new();
            let _ = s.take(600).read_to_string(&mut body);
            return Err(format!("the mixer answered {status} for the pictures of '{}': {}", self.source, body.trim()));
        }
        self.stream = Some(s);
        self.pending.clear();
        Ok(())
    }
}

/// The newest whole JPEG in `buf`, and how much of `buf` it used up.
pub fn newest_jpeg(buf: &[u8]) -> Option<(std::ops::Range<usize>, usize)> {
    let mut found = None;
    let mut at = 0;
    while let Some(start) = find(&buf[at..], &[0xFF, 0xD8]).map(|i| i + at) {
        let Some(end) = find(&buf[start + 2..], &[0xFF, 0xD9]).map(|i| i + start + 4) else { break };
        found = Some((start..end, end));
        at = end;
    }
    found
}

fn find(hay: &[u8], needle: &[u8]) -> Option<usize> {
    hay.windows(needle.len()).position(|w| w == needle)
}

pub fn decode(jpeg: &[u8]) -> Result<Picture, String> {
    let img = image::load_from_memory_with_format(jpeg, image::ImageFormat::Jpeg).map_err(|e| format!("a picture from the mixer would not decode: {e}"))?;
    let rgb = img.to_rgb8();
    Ok(Picture { width: rgb.width(), height: rgb.height(), format: godwinmix_track::PixelFormat::Rgb8, data: rgb.into_raw() })
}

impl Frames for MjpegFrames {
    fn next(&mut self, timeout: Duration) -> Result<Option<Picture>, String> {
        if self.stream.is_none() {
            self.connect()?;
        }
        let until = Instant::now() + timeout;
        let mut buf = [0u8; 65_536];
        loop {
            let left = until.saturating_duration_since(Instant::now());
            if left.is_zero() {
                return Ok(None);
            }
            let s = self.stream.as_mut().expect("connected above");
            s.set_read_timeout(Some(left)).ok();
            match s.read(&mut buf) {
                Ok(n) if n > 0 => self.pending.extend_from_slice(&buf[..n]),
                Err(e) if matches!(e.kind(), std::io::ErrorKind::WouldBlock | std::io::ErrorKind::TimedOut) => return Ok(None),
                _ => {
                    self.stream = None;
                    return Err(format!("the picture stream of '{}' ended. It is opened again on the next frame.", self.source));
                }
            }
            if let Some((range, used)) = newest_jpeg(&self.pending) {
                let picture = decode(&self.pending[range]);
                self.pending.drain(..used);
                return picture.map(Some);
            }
            if self.pending.len() > 8 << 20 {
                self.pending.clear();
            }
        }
    }

    fn describe(&self) -> String {
        format!("the mixer's MJPEG stream of '{}'", self.source)
    }
}
