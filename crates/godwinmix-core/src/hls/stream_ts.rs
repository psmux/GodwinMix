//! A stream's MPEG-TS renditions: kept beside its CMAF tracks, and listed
//! from them.

use super::super::playlist::Variant;
use super::super::ts::{self, listing, At, Master, Media, TsTrack};
use super::Stream;
use bytes::Bytes;
use std::sync::Arc;

impl Stream {
    /// A TS rendition: `main` (sound None) or a further sound alone. One
    /// already under this id is kept, ring and all, as a CMAF track is.
    pub fn add_ts(&self, id: &str, sound: Option<u8>) -> Arc<TsTrack> {
        let mut all = self.ts.write();
        if let Some(t) = all.iter().find(|t| t.id == id && t.sound == sound) {
            return t.clone();
        }
        all.retain(|t| t.id != id);
        let t = Arc::new(TsTrack::new(id, sound, self.params.ring_capacity()));
        all.push(t.clone());
        t
    }

    /// Keep only the TS renditions named in `ids`.
    pub fn keep_ts(&self, ids: &[String]) {
        self.ts.write().retain(|t| ids.contains(&t.id));
    }

    pub fn ts_tracks(&self) -> Vec<Arc<TsTrack>> {
        self.ts.read().clone()
    }

    pub fn ts_track(&self, id: &str) -> Option<Arc<TsTrack>> {
        self.ts.read().iter().find(|t| t.id == id).cloned()
    }

    /// Whether `main` has a whole segment, which is when a TS master can say
    /// anything true.
    pub fn ts_ready(&self) -> bool {
        self.ts_track(ts::MAIN).is_some_and(|t| !t.listing().0.is_empty())
    }

    /// The TS multivariant playlist as data: the picture's codec and size
    /// from the CMAF picture, the bandwidth TS measured, every sound's name
    /// and language. None until `main` has a segment.
    pub fn ts_master(&self) -> Option<Master> {
        let main_ts = self.ts_track(ts::MAIN).filter(|_| self.ts_ready())?;
        let picture = self.leader()?;
        let mut main = Self::variant(&picture);
        let (peak, avg) = main_ts.measured_bps().unwrap_or((main.bandwidth, main.bandwidth));
        (main.id, main.bandwidth, main.average) = (ts::MAIN.into(), peak.max(main.bandwidth).max(1), Some(avg));
        let mut sounds: Vec<Variant> = Vec::new();
        for t in self.ts_tracks() {
            let info = t.sound_info.read().clone();
            if info.codecs.is_empty() {
                continue;
            }
            let (peak, avg) = t.measured_bps().unzip();
            let id = if t.sound.is_some() { t.id.clone() } else { ts::sound_id(0) };
            sounds.push(Variant { id, bandwidth: peak.unwrap_or(0).max(1), average: avg, info });
        }
        sounds.sort_by_key(|v| v.info.sound);
        Some(Master { main: Some(main), sounds })
    }

    /// One TS rendition's media playlist as data.
    pub fn ts_media(&self, id: &str) -> Option<Media> {
        let (entries, longest) = self.ts_track(id)?.listing();
        Some(listing::media(&entries, longest, &self.params))
    }

    /// One TS segment's bytes, shared with every other reader.
    pub fn ts_segment(&self, id: &str, at: At) -> Option<Bytes> {
        self.ts_track(id)?.segment_at(at)
    }
}
