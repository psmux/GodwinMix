//! Reading a WHEP offer: which payload type the viewer gave each codec.
//!
//! The answer has to use the viewer's numbers, so the payloaders are set to
//! them before the offer is answered. Pure text work on the SDP, so it is
//! tested without a network or a browser.

/// One `a=rtpmap` line with its `a=fmtp`, from one media section.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Format {
    pub pt: u8,
    /// `H264`, `opus`, as written.
    pub encoding: String,
    pub fmtp: String,
}

/// Every format offered in the sections of `media` (`video` or `audio`).
pub fn formats(offer: &str, media: &str) -> Vec<Format> {
    let mut out: Vec<Format> = Vec::new();
    let mut inside = false;
    for line in offer.lines().map(str::trim) {
        if let Some(rest) = line.strip_prefix("m=") {
            inside = rest.split_whitespace().next() == Some(media);
            continue;
        }
        if !inside {
            continue;
        }
        if let Some((pt, encoding)) = attribute(line, "a=rtpmap:") {
            out.push(Format { pt, encoding: encoding.split('/').next().unwrap_or("").to_string(), fmtp: String::new() });
        } else if let Some((pt, params)) = attribute(line, "a=fmtp:") {
            if let Some(f) = out.iter_mut().find(|f| f.pt == pt) {
                f.fmtp = params.to_string();
            }
        }
    }
    out
}

fn attribute<'a>(line: &'a str, prefix: &str) -> Option<(u8, &'a str)> {
    let rest = line.strip_prefix(prefix)?;
    let (pt, value) = rest.split_once(' ')?;
    Some((pt.parse().ok()?, value.trim()))
}

/// The payload type to send `encoding` as, or `None` when the viewer did not
/// offer it. For H.264 the non interleaved mode is preferred, and among those
/// the entry for the profile being sent (`profile` is GStreamer's name for
/// it, from the stream's caps), because a payloader told one profile will
/// not take another. Constrained baseline is the choice when nothing is known.
pub fn pick(offer: &str, media: &str, encoding: &str, profile: Option<&str>) -> Option<u8> {
    let offered: Vec<Format> =
        formats(offer, media).into_iter().filter(|f| f.encoding.eq_ignore_ascii_case(encoding)).collect();
    if !encoding.eq_ignore_ascii_case("H264") {
        return offered.first().map(|f| f.pt);
    }
    let mode1 = |f: &&Format| f.fmtp.contains("packetization-mode=1");
    let idc = profile.map_or("42e0", profile_idc);
    offered
        .iter()
        .filter(mode1)
        .find(|f| profile_level_id(&f.fmtp).is_some_and(|p| p.starts_with(idc)))
        .or_else(|| offered.iter().find(mode1))
        .or_else(|| offered.first())
        .map(|f| f.pt)
}

/// The leading hex of `profile-level-id` for a GStreamer H.264 profile name.
fn profile_idc(profile: &str) -> &'static str {
    match profile {
        "constrained-baseline" => "42e0",
        "baseline" => "42",
        "main" => "4d",
        "high" => "64",
        "high-10" => "6e",
        "high-4:2:2" => "7a",
        "high-4:4:4" => "f4",
        _ => "42e0",
    }
}

fn profile_level_id(fmtp: &str) -> Option<String> {
    fmtp.split(';').find_map(|kv| kv.trim().strip_prefix("profile-level-id=")).map(str::to_ascii_lowercase)
}

#[cfg(test)]
mod tests {
    use super::*;

    /// Trimmed from what Chrome 140 offers for a recvonly video and audio.
    const CHROME: &str = "v=0\r\no=- 1 2 IN IP4 127.0.0.1\r\ns=-\r\nt=0 0\r\n\
        m=audio 9 UDP/TLS/RTP/SAVPF 111 63\r\na=mid:0\r\na=recvonly\r\n\
        a=rtpmap:111 opus/48000/2\r\na=fmtp:111 minptime=10;useinbandfec=1\r\na=rtpmap:63 red/48000/2\r\n\
        m=video 9 UDP/TLS/RTP/SAVPF 96 102 103 104 106 45\r\na=mid:1\r\na=recvonly\r\n\
        a=rtpmap:96 VP8/90000\r\n\
        a=rtpmap:102 H264/90000\r\na=fmtp:102 level-asymmetry-allowed=1;packetization-mode=0;profile-level-id=42001f\r\n\
        a=rtpmap:103 H264/90000\r\na=fmtp:103 level-asymmetry-allowed=1;packetization-mode=1;profile-level-id=42001f\r\n\
        a=rtpmap:104 H264/90000\r\na=fmtp:104 level-asymmetry-allowed=1;packetization-mode=1;profile-level-id=42e01f\r\n\
        a=rtpmap:106 H264/90000\r\na=fmtp:106 level-asymmetry-allowed=1;packetization-mode=1;profile-level-id=64001f\r\n\
        a=rtpmap:45 AV1/90000\r\n";

    #[test]
    fn h264_prefers_mode_one_in_the_profile_being_sent() {
        assert_eq!(pick(CHROME, "video", "H264", None), Some(104), "constrained baseline when unknown");
        assert_eq!(pick(CHROME, "video", "H264", Some("high")), Some(106));
        assert_eq!(pick(CHROME, "video", "H264", Some("main")), Some(103), "not offered: the first in mode 1");
        assert_eq!(pick(CHROME, "video", "AV1", None), Some(45));
        assert_eq!(pick(CHROME, "video", "H265", None), None);
    }

    #[test]
    fn opus_is_found_in_the_audio_section_only() {
        assert_eq!(pick(CHROME, "audio", "OPUS", None), Some(111));
        assert_eq!(pick(CHROME, "video", "OPUS", None), None);
        assert_eq!(formats(CHROME, "audio").len(), 2);
    }
}
