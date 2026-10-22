//! [`Segments`] over an output's MPEG-TS rings, through the station's
//! `ts_master`, `ts_media` and `ts_segment` (`station::direct::hls::facade`).
//! An output that makes no TS (`segments = "cmaf"`) is answered by the CMAF
//! bridge instead, so swapping this in changes nothing for one.
//!
//! | Selection | TS rendition |
//! |---|---|
//! | `tracks-v1a1`, and `tracks-v1` | `main`: the picture with the first sound muxed in |
//! | `tracks-a<n>`, n from 2 | `audio<n-1>`: that sound alone |
//! | `tracks-a1` | none: the first sound rides in `main` |

use super::bridge::Cmaf;
use crate::station::direct::hls::facade::TsRefusal;
use crate::station::state::Station;
use godwinmix_core::hls::playlist::Variant;
use godwinmix_core::hls::ts::{self, At as TsAt};
use godwinmix_flussonic::facade::access::*;
use std::sync::Arc;

pub struct Ts {
    st: Arc<Station>,
    cmaf: Cmaf,
}

fn missing(r: TsRefusal) -> Missing {
    match r.status {
        404 => Missing::Gone(r.message),
        _ => Missing::NotYet(r.message),
    }
}

fn track(v: &Variant, kind: Kind) -> Track {
    let i = &v.info;
    let number = if kind == Kind::Video { 1 } else { i.sound + 1 };
    let name = (!i.name.is_empty()).then(|| i.name.clone()).or_else(|| Some(godwinmix_core::hls::lang::name(&i.lang, i.sound)));
    Track {
        kind,
        number,
        codecs: i.codecs.clone(),
        bandwidth: v.bandwidth,
        width: i.width,
        height: i.height,
        fps: i.fps.filter(|(n, d)| *n > 0 && *d > 0).map(|(n, d)| f64::from(n) / f64::from(d)),
        channels: (i.channels > 0).then_some(i.channels),
        lang: (!i.lang.is_empty()).then(|| i.lang.clone()),
        name,
    }
}

/// The TS rendition a selection is.
fn rendition(sel: Select) -> Result<String, Missing> {
    match (sel.video, sel.audio) {
        (true, None | Some(1)) => Ok(ts::MAIN.to_string()),
        (false, Some(n)) if n > 1 => Ok(ts::sound_id(n - 1)),
        _ => Err(Missing::Unsupported("in MPEG-TS the first sound rides with the picture: ask for tracks-v1a1".into())),
    }
}

impl Ts {
    pub fn new(st: Arc<Station>) -> Ts {
        Ts { cmaf: Cmaf::new(st.clone()), st }
    }

    /// Whether the output makes TS. An output the station does not know
    /// goes to the CMAF bridge, which says so.
    fn makes_ts(&self, at: &At) -> bool {
        self.st.direct.hls.stream(&at.show, &at.output).is_some_and(|s| s.params.segments.ts())
    }
}

impl Segments for Ts {
    fn tracks<'a>(&'a self, at: &'a At) -> Fut<'a, Result<Tracks, Missing>> {
        if !self.makes_ts(at) {
            return self.cmaf.tracks(at);
        }
        Box::pin(async move {
            let m = self.st.direct.hls.ts_master(&self.st, &at.show, &at.output).await.map_err(missing)?;
            let mut tracks: Vec<Track> = m.main.iter().map(|v| track(v, Kind::Video)).collect();
            tracks.extend(m.sounds.iter().map(|v| track(v, Kind::Audio)));
            Ok(Tracks { tracks, muxed: true })
        })
    }

    fn window<'a>(&'a self, at: &'a At, sel: Select) -> Fut<'a, Result<Window, Missing>> {
        if !self.makes_ts(at) {
            return self.cmaf.window(at, sel);
        }
        Box::pin(async move {
            let media = self.st.direct.hls.ts_media(&self.st, &at.show, &at.output, &rendition(sel)?).await.map_err(missing)?;
            let sequence = media.segments.first().map_or(0, |e| e.msn);
            let mut disc = media.segments.first().map(|e| e.disc);
            let segments = media
                .segments
                .iter()
                .map(|e| {
                    let broke = disc != Some(e.disc);
                    disc = Some(e.disc);
                    Segment { start_ms: e.start_ms, duration_ms: e.duration_ms, init: None, discontinuity: broke }
                })
                .collect();
            Ok(Window { container: Container::Ts, sequence, segments })
        })
    }

    fn segment<'a>(&'a self, at: &'a At, sel: Select, start_s: i64) -> Fut<'a, Result<Vec<u8>, Missing>> {
        if !self.makes_ts(at) {
            return self.cmaf.segment(at, sel, start_s);
        }
        Box::pin(async move {
            let r = rendition(sel)?;
            let bytes = self.st.direct.hls.ts_segment(&self.st, &at.show, &at.output, &r, TsAt::Second(start_s)).await.map_err(missing)?;
            Ok(bytes.to_vec())
        })
    }

    fn init<'a>(&'a self, at: &'a At, sel: Select, id: &'a str) -> Fut<'a, Result<Vec<u8>, Missing>> {
        if !self.makes_ts(at) {
            return self.cmaf.init(at, sel, id);
        }
        Box::pin(async move { Err(Missing::Gone("MPEG-TS segments need no init segment".into())) })
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn a_flussonic_selection_is_a_ts_rendition() {
        assert_eq!(rendition(Select { video: true, audio: Some(1) }).unwrap(), "main");
        assert_eq!(rendition(Select { video: true, audio: None }).unwrap(), "main");
        assert_eq!(rendition(Select { video: false, audio: Some(3) }).unwrap(), "audio2");
        assert!(matches!(rendition(Select { video: false, audio: Some(1) }), Err(Missing::Unsupported(_))));
    }
}
