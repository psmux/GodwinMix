//! `whep/output` params, and what each video codec is sent with.

use crate::config::Params;
use anyhow::Result;
use godwinmix_protocol::rendition::VideoCodec;

/// The most viewers one output takes unless its params say otherwise. Every
/// viewer is one more packetiser and one more SRTP session over the same
/// encode, so the cost is network and a little CPU, never an encoder.
pub const DEFAULT_VIEWERS: u32 = 10;

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct WhepParams {
    pub max_viewers: u32,
    /// A STUN URI, or empty for host candidates only.
    pub stun: String,
    pub turn: Vec<String>,
    /// `params.viewer_key`, when the person set one; otherwise the output's
    /// key is derived from its id, the same way an HLS output's is.
    pub viewer_key: Option<String>,
}

impl Default for WhepParams {
    fn default() -> Self {
        let ice = crate::preview::whep::IceConfig::default();
        WhepParams { max_viewers: DEFAULT_VIEWERS, stun: ice.stun, turn: ice.turn, viewer_key: None }
    }
}

impl WhepParams {
    pub fn from_params(params: &Params) -> Result<WhepParams> {
        let mut out = WhepParams::default();
        if let Some(v) = params.get("max_viewers") {
            let n = v.as_integer().filter(|n| (1..=500).contains(n)).ok_or_else(|| {
                anyhow::anyhow!("whep/output params.max_viewers must be a whole number from 1 to 500, got {v}")
            })?;
            out.max_viewers = n as u32;
        }
        if let Some(v) = params.get("stun") {
            let s = v.as_str().ok_or_else(|| anyhow::anyhow!("whep/output params.stun must be a string such as stun://stun.example.com:3478, or empty for this network only"))?;
            anyhow::ensure!(s.is_empty() || s.starts_with("stun://"), "whep/output params.stun must start with stun://, or be empty for this network only; got {s}");
            out.stun = s.to_string();
        }
        if let Some(v) = params.get("turn") {
            let list = v.as_array().ok_or_else(|| anyhow::anyhow!("whep/output params.turn must be a list of turn:// or turns:// addresses"))?;
            out.turn = list.iter().filter_map(|t| t.as_str()).map(str::to_string).collect();
            anyhow::ensure!(
                out.turn.iter().all(|t| t.starts_with("turn://") || t.starts_with("turns://")),
                "whep/output params.turn takes turn://user:password@host:port addresses"
            );
        }
        if let Some(v) = params.get("viewer_key") {
            let k = v.as_str().filter(|k| k.len() >= 16).ok_or_else(|| {
                anyhow::anyhow!("whep/output params.viewer_key must be at least 16 characters, or left out for the one this machine makes")
            })?;
            out.viewer_key = Some(k.to_string());
        }
        Ok(out)
    }
}

/// How one video codec travels over WebRTC.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct VideoSend {
    pub parser: Option<&'static str>,
    pub payloader: &'static str,
    /// The name in `a=rtpmap`.
    pub encoding: &'static str,
}

/// What `codec` is parsed and packetised with, or `None` for a codec WebRTC
/// has no mapping for.
pub fn video_send(codec: VideoCodec) -> Option<VideoSend> {
    let (parser, payloader, encoding) = match codec {
        VideoCodec::H264 => (Some("h264parse"), "rtph264pay", "H264"),
        VideoCodec::H265 => (Some("h265parse"), "rtph265pay", "H265"),
        VideoCodec::Av1 => (Some("av1parse"), "rtpav1pay", "AV1"),
        VideoCodec::Vp8 => (None, "rtpvp8pay", "VP8"),
        VideoCodec::Vp9 => (None, "rtpvp9pay", "VP9"),
        _ => return None,
    };
    Some(VideoSend { parser, payloader, encoding })
}

#[cfg(test)]
mod tests {
    use super::*;

    fn params(text: &str) -> Params {
        toml::from_str(text).unwrap()
    }

    #[test]
    fn defaults_and_limits() {
        let p = WhepParams::from_params(&params("")).unwrap();
        assert_eq!(p.max_viewers, DEFAULT_VIEWERS);
        let lan = WhepParams::from_params(&params("stun = \"\"\nmax_viewers = 3")).unwrap();
        assert_eq!((lan.stun.as_str(), lan.max_viewers), ("", 3));
        let err = WhepParams::from_params(&params("max_viewers = 0")).unwrap_err().to_string();
        assert!(err.contains("1 to 500"), "{err}");
        assert!(WhepParams::from_params(&params("stun = \"stun.example.com\"")).is_err());
    }

    #[test]
    fn every_webrtc_codec_has_a_payloader_and_prores_has_none() {
        assert_eq!(video_send(VideoCodec::H264).unwrap().payloader, "rtph264pay");
        assert_eq!(video_send(VideoCodec::Av1).unwrap().encoding, "AV1");
        assert!(video_send(VideoCodec::Prores).is_none());
    }
}
