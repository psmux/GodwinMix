//! Every sound track of a show's input, as the packager reads them: the one
//! place that knows how the relay hands over the tracks and how the input
//! names them, so a change to either is a change here.
//!
//! * The relay gives every track to a reader that asks with
//!   [`ALL_TRACKS`] after the stream (`GMXHUB <app>/<stream> tracks=all`),
//!   each tag's track in the last byte of its FLV stream id: 0 for the
//!   picture and the first sound, then 1, 2, 3 in PMT order.
//! * `direct.input`'s `tracks` names them: `{track, kind, codec, channels,
//!   sample_rate, lang, pid}`, `lang` an ISO 639 code from the PMT.
//!
//! A sound track's CMAF rung, and its TS rendition when it is not the
//! first, is `audio` for track 0 and `audio<n>` for track n.

use serde::{Deserialize, Serialize};
use serde_json::Value;

/// What a reader of every track says after the stream on its request line.
pub const ALL_TRACKS: &str = "tracks=all";

/// The request line for `path`, every track.
pub fn request_line(path: &str) -> String {
    format!("GMXHUB {path} {ALL_TRACKS}\n")
}

/// The track a tag belongs to, from its 11 byte FLV header.
pub fn track_of(head: &[u8; 11]) -> u8 {
    head[10]
}

/// One sound track as the input names it.
#[derive(Debug, Clone, Default, PartialEq, Eq, Serialize, Deserialize)]
pub struct Sound {
    pub track: u8,
    /// ISO 639 from the PMT, empty when it gave none.
    #[serde(default)]
    pub lang: String,
}

/// The sound tracks `direct.input` lists, in track order.
pub fn sounds(input: &Value) -> Vec<Sound> {
    let Some(list) = input.get("tracks").and_then(Value::as_array) else { return Vec::new() };
    let mut out: Vec<Sound> = list
        .iter()
        .filter(|t| t["kind"] == "audio")
        .filter_map(|t| {
            let track = u8::try_from(t["track"].as_u64()?).ok()?;
            Some(Sound { track, lang: t["lang"].as_str().unwrap_or_default().to_string() })
        })
        .collect();
    out.sort_by_key(|s| s.track);
    out.dedup_by_key(|s| s.track);
    out
}

/// The CMAF rung a sound track is packaged as.
pub fn rung(track: u8) -> String {
    godwinmix_core::hls::ts::sound_id(track)
}

#[cfg(test)]
mod tests {
    use super::*;
    use serde_json::json;

    #[test]
    fn the_inputs_sound_tracks_are_read_in_order_and_named() {
        let input = json!({"tracks": [
            {"track": 0, "kind": "video", "codec": "mpeg2", "pid": 49},
            {"track": 1, "kind": "audio", "codec": "ac3", "channels": 2, "lang": "spa", "pid": 53},
            {"track": 0, "kind": "audio", "codec": "ac3", "channels": 6, "lang": "eng", "pid": 52},
            {"track": 2, "kind": "audio", "codec": "ac3", "pid": 54}
        ]});
        let s = sounds(&input);
        assert_eq!(s.iter().map(|s| (s.track, s.lang.as_str())).collect::<Vec<_>>(), [(0, "eng"), (1, "spa"), (2, "")]);
        assert_eq!(sounds(&json!({"audio": {"codec": "aac"}})), Vec::new(), "an input that lists no tracks");
        assert_eq!((rung(0), rung(2)), ("audio".to_string(), "audio2".to_string()));
        assert_eq!(request_line("direct.ch/main"), "GMXHUB direct.ch/main tracks=all\n");
        let mut head = [0u8; 11];
        head[10] = 3;
        assert_eq!(track_of(&head), 3);
    }
}
