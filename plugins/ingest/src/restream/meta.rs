//! A publisher's `onMetaData`, read back into the fields `rml_rtmp` sends.
//!
//! The client session writes metadata only from a `StreamMetadata`, never
//! from raw bytes, because it owns the chunk stream's header compression. So
//! the script tag is decoded (AMF0 is a few hundred bytes of numbers and
//! strings, nothing like a picture) and the fields a platform reads are
//! carried across. Anything else in it is left behind.

use std::collections::HashMap;

use rml_amf0::Amf0Value;
use rml_rtmp::sessions::StreamMetadata;

/// The metadata in a script tag body, or `None` when it carries none.
pub fn parse(payload: &[u8]) -> Option<StreamMetadata> {
    let values = rml_amf0::deserialize(&mut std::io::Cursor::new(payload)).ok()?;
    let mut rest = values.into_iter().skip_while(|v| {
        matches!(v, Amf0Value::Utf8String(s) if s == "@setDataFrame" || s == "onMetaData")
    });
    let Some(Amf0Value::Object(props)) = rest.next() else { return None };
    Some(from_props(&props))
}

fn from_props(p: &HashMap<String, Amf0Value>) -> StreamMetadata {
    let num = |k: &str| match p.get(k) {
        Some(Amf0Value::Number(n)) if n.is_finite() && *n >= 0.0 => Some(*n),
        _ => None,
    };
    let int = |k: &str| num(k).map(|n| n as u32);
    let mut m = StreamMetadata::new();
    m.video_width = int("width");
    m.video_height = int("height");
    m.video_codec_id = int("videocodecid");
    m.video_frame_rate = num("framerate").map(|n| n as f32);
    m.video_bitrate_kbps = int("videodatarate");
    m.audio_codec_id = int("audiocodecid");
    m.audio_bitrate_kbps = int("audiodatarate");
    m.audio_sample_rate = int("audiosamplerate");
    m.audio_channels = int("audiochannels");
    m.audio_is_stereo = match p.get("stereo") {
        Some(Amf0Value::Boolean(b)) => Some(*b),
        _ => None,
    };
    m.encoder = match p.get("encoder") {
        Some(Amf0Value::Utf8String(s)) => Some(s.clone()),
        _ => None,
    };
    m
}

#[cfg(test)]
mod tests {
    use super::*;

    fn script(with_set_data_frame: bool) -> Vec<u8> {
        let mut props = HashMap::new();
        props.insert("width".to_string(), Amf0Value::Number(1920.0));
        props.insert("height".to_string(), Amf0Value::Number(1080.0));
        props.insert("framerate".to_string(), Amf0Value::Number(30.0));
        props.insert("encoder".to_string(), Amf0Value::Utf8String("obs".into()));
        let mut values = vec![];
        if with_set_data_frame {
            values.push(Amf0Value::Utf8String("@setDataFrame".into()));
        }
        values.push(Amf0Value::Utf8String("onMetaData".into()));
        values.push(Amf0Value::Object(props));
        rml_amf0::serialize(&values).unwrap()
    }

    #[test]
    fn on_metadata_is_read_with_or_without_set_data_frame() {
        for wrapped in [true, false] {
            let m = parse(&script(wrapped)).expect("metadata");
            assert_eq!((m.video_width, m.video_height), (Some(1920), Some(1080)));
            assert_eq!(m.video_frame_rate, Some(30.0));
            assert_eq!(m.encoder.as_deref(), Some("obs"));
            assert_eq!(m.audio_channels, None);
        }
    }

    #[test]
    fn a_script_tag_with_no_metadata_is_none() {
        assert!(parse(&[]).is_none());
        let other = rml_amf0::serialize(&vec![Amf0Value::Utf8String("onCuePoint".into())]).unwrap();
        assert!(parse(&other).is_none());
    }
}
