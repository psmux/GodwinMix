//! One connection's MPEG-TS renditions: the hub's tags turned into the
//! engine's TS units and handed to a cutter each.
//!
//! ```text
//!   picture, sound track 0 ─► TsCut ─► "main"     (the picture and the first sound in one)
//!   sound track n          ─► TsCut ─► "audio<n>" (that sound alone)
//! ```
//!
//! A tag goes to one cutter, as a pointer to the bytes the relay read: the
//! CMAF buffer of the same frame points at the same allocation.

use super::caps::Read;
use super::flv::{Kind, Tag};
use super::tracks::{self, Sound};
use godwinmix_core::hls::parse::info_of;
use godwinmix_core::hls::ts::{self as ts, TsCut, Unit, UnitKind};
use godwinmix_core::hls::Stream;
use gstreamer as gst;
use std::sync::Arc;

pub struct Cutters {
    main: TsCut,
    alone: Vec<(u8, TsCut)>,
}

impl Cutters {
    /// The renditions for a picture and the sound tracks `sounds` (track,
    /// caps), on the pipeline whose zero is tag time `base`. `named` is
    /// what the input says of each track.
    pub fn new(stream: &Arc<Stream>, video: &gst::Caps, sounds: &[(u8, gst::Caps)], base: u32, named: &[Sound]) -> Cutters {
        let main = stream.add_ts(ts::MAIN, None);
        let mut main_cut = TsCut::new(stream, main.clone(), base);
        // The headers came before the pipeline was built: hand them on now.
        let hevc = video.structure(0).is_some_and(|s| s.name() == "video/x-h265");
        main_cut.extend(config(video, if hevc { UnitKind::HevcConfig } else { UnitKind::AvcConfig }));
        let mut ids = vec![ts::MAIN.to_string()];
        let mut alone = Vec::new();
        for (track, caps) in sounds {
            let t = if *track == 0 {
                main.clone()
            } else {
                let id = tracks::rung(*track);
                ids.push(id.clone());
                let t = stream.add_ts(&id, Some(*track));
                let mut cut = TsCut::new(stream, t.clone(), base);
                cut.extend(config(caps, UnitKind::AacConfig));
                alone.push((*track, cut));
                t
            };
            if *track == 0 {
                main_cut.extend(config(caps, UnitKind::AacConfig));
            }
            let mut info = info_of(caps).unwrap_or_default();
            info.sound = *track;
            info.lang = named.iter().find(|s| s.track == *track).map(|s| s.lang.clone()).unwrap_or_default();
            *t.sound_info.write() = info;
        }
        stream.keep_ts(&ids);
        Cutters { main: main_cut, alone }
    }

    /// Hand one tag to the cutter it belongs to.
    pub fn push(&mut self, tag: &Tag, read: &Read) {
        let Some(u) = unit(tag, read) else { return };
        if tag.kind == Kind::Video || tag.track == 0 {
            return self.main.push(u);
        }
        if let Some((_, cut)) = self.alone.iter_mut().find(|(t, _)| *t == tag.track) {
            cut.push(u);
        }
    }
}

/// The configuration record in a header's caps, as a unit.
fn config(caps: &gst::Caps, kind: UnitKind) -> Option<Unit> {
    let record = caps.structure(0)?.get::<gst::Buffer>("codec_data").ok()?;
    let map = record.map_readable().ok()?;
    Some(Unit { kind, ms: 0, cts_ms: 0, key: false, data: bytes::Bytes::copy_from_slice(&map) })
}

/// A tag as a TS unit: a configuration record or a frame, no bytes copied.
fn unit(tag: &Tag, read: &Read) -> Option<Unit> {
    let (kind, skip, cts_ms, key) = match (read, tag.kind) {
        (Read::Header(caps), Kind::Video) => {
            let hevc = caps.structure(0)?.name() == "video/x-h265";
            (if hevc { UnitKind::HevcConfig } else { UnitKind::AvcConfig }, 5, 0, false)
        }
        (Read::Header(_), Kind::Audio) => (UnitKind::AacConfig, 2, 0, false),
        (Read::Frame { skip, cts, key }, Kind::Video) => (UnitKind::Video, *skip, *cts, *key),
        (Read::Frame { skip, .. }, Kind::Audio) => (UnitKind::Audio, *skip, 0, true),
        _ => return None,
    };
    (tag.body.len() > skip).then(|| Unit { kind, ms: tag.ms, cts_ms, key, data: tag.body.slice(skip..) })
}

#[cfg(test)]
mod tests {
    use super::*;
    use bytes::Bytes;

    #[test]
    fn a_tag_becomes_a_unit_without_its_flv_framing() {
        gst::init().unwrap();
        let tag = |kind, body: &'static [u8]| Tag { kind, track: 0, ms: 40, body: Bytes::from_static(body) };
        let v = tag(Kind::Video, &[0x17, 1, 0, 0, 40, 9, 9]);
        let u = unit(&v, &super::super::caps::video(&v.body)).unwrap();
        assert_eq!((u.kind, u.cts_ms, u.key, &u.data[..]), (UnitKind::Video, 40, true, &[9, 9][..]));
        let h = tag(Kind::Audio, &[0xaf, 0, 0x11, 0x90]);
        let u = unit(&h, &super::super::caps::audio(&h.body)).unwrap();
        assert_eq!((u.kind, &u.data[..]), (UnitKind::AacConfig, &[0x11, 0x90][..]));
    }
}
