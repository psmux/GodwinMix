//! What the catalogue takes from gst-libav, and keeping `decodebin` from
//! taking more.
//!
//! GodwinMix runs without gst-libav. The catalogue ranks every libav element
//! below the alternatives that exist (`codecs.toml` says which), so selection
//! takes FFmpeg's code only for a codec nothing else on the machine handles.
//! Selection is half of it. A source is opened with `decodebin`, which picks
//! by GStreamer's own ranks, and gst-libav registers `avdec_h264` and
//! `avdec_aac` at primary, above openh264, faad and the operating system's
//! AAC decoders. So for each codec where the catalogue prefers a software
//! decoder to a libav one, the two swap GStreamer ranks: the preferred one
//! takes the libav decoder's rank and the libav decoder drops one under it.
//! Nothing else moves, so a hardware decoder that already beat libav still
//! wins, and libav stays pluggable for a stream the preferred decoder's caps
//! refuse (High 10 H.264 into openh264, say).

use super::model::Role;
use super::select::Registry;
use super::Catalogue;
use std::collections::BTreeMap;

/// Whether an element is FFmpeg's code, reached through gst-libav.
pub fn is_libav(element: &str) -> bool {
    ["avdec_", "avenc_", "avmux_", "avdemux_"].iter().any(|p| element.starts_with(p))
        || matches!(element, "avdeinterlace" | "avvideocompare")
}

/// For one codec and one role, the elements the catalogue offers that are
/// installed here, best first, each with its accel.
pub type Ladder = Vec<(String, String)>;

/// Every codec's installed elements for `role`, best first, video and audio
/// together. Disabled entries are left out, as selection leaves them out.
pub fn ladders(cat: &Catalogue, role: Role, reg: &dyn Registry) -> BTreeMap<String, Ladder> {
    let mut rows: Vec<(String, i32, String, String)> = Vec::new();
    for e in cat.video.iter().filter(|e| !e.disabled) {
        if let Some(el) = e.element(role).filter(|_| e.needs(role).iter().all(|n| reg.has(n))) {
            rows.push((e.codec.clone(), e.rank, el.to_string(), e.accel.clone()));
        }
    }
    for e in cat.audio.iter().filter(|e| !e.disabled) {
        if let Some(el) = e.element(role).filter(|_| e.needs(role).iter().all(|n| reg.has(n))) {
            rows.push((e.codec.clone(), e.rank, el.to_string(), e.accel.clone()));
        }
    }
    rows.sort_by(|a, b| a.0.cmp(&b.0).then(b.1.cmp(&a.1)));
    let mut out: BTreeMap<String, Ladder> = BTreeMap::new();
    for (codec, _, element, accel) in rows {
        let ladder = out.entry(codec).or_default();
        if !ladder.iter().any(|(e, _)| *e == element) {
            ladder.push((element, accel));
        }
    }
    out
}

/// The GStreamer rank changes that make `decodebin` agree with the
/// catalogue about libav, as (element, new rank). `rank_of` answers an
/// element's current GStreamer rank.
pub fn rank_plan(cat: &Catalogue, reg: &dyn Registry, rank_of: &dyn Fn(&str) -> Option<i32>) -> Vec<(String, i32)> {
    let mut plan: Vec<(String, i32)> = Vec::new();
    for ladder in ladders(cat, Role::Decode, reg).values() {
        let software: Vec<&str> =
            ladder.iter().filter(|(_, accel)| accel == "software").map(|(e, _)| e.as_str()).collect();
        let Some(pos) = software.iter().position(|e| !is_libav(e)) else { continue };
        let preferred = software[pos];
        let Some(own) = rank_of(preferred) else { continue };
        for libav in software[pos + 1..].iter().filter(|e| is_libav(e)) {
            let Some(theirs) = rank_of(libav) else { continue };
            if theirs >= own && !plan.iter().any(|(e, _)| e == preferred || e == libav) {
                plan.push((preferred.to_string(), theirs));
                plan.push((libav.to_string(), theirs - 1));
                break;
            }
        }
    }
    plan
}

/// Apply [`rank_plan`] to this process's GStreamer registry. Called once,
/// before the first source is built.
pub fn prefer_over_libav(cat: &Catalogue) {
    use gstreamer::glib::translate::IntoGlib;
    use gstreamer::prelude::*;
    let rank_of = |name: &str| gstreamer::ElementFactory::find(name).map(|f| f.rank().into_glib());
    for (element, rank) in rank_plan(cat, &super::select::GstRegistry, &rank_of) {
        if let Some(f) = gstreamer::ElementFactory::find(&element) {
            f.set_rank(gstreamer::Rank::NONE + rank);
            tracing::debug!(element, rank, "decoder rank set so decodebin follows the catalogue over libav");
        }
    }
}

/// What this machine would take from libav, as (role, codec, element): the
/// codecs whose best installed element for a role is a libav one.
pub fn in_use(cat: &Catalogue, reg: &dyn Registry) -> Vec<(&'static str, String, String)> {
    let mut out = Vec::new();
    for (role, label) in [(Role::Encode, "encode"), (Role::Decode, "decode")] {
        for (codec, ladder) in ladders(cat, role, reg) {
            if let Some((element, _)) = ladder.first().filter(|(e, _)| is_libav(e)) {
                out.push((label, codec, element.clone()));
            }
        }
    }
    out
}

/// The codecs the catalogue knows a decoder for and this machine has none of.
pub fn undecodable(cat: &Catalogue, reg: &dyn Registry) -> Vec<String> {
    let have = ladders(cat, Role::Decode, reg);
    let mut known: Vec<String> = cat
        .video
        .iter()
        .filter(|e| !e.disabled && e.decoder.is_some())
        .map(|e| e.codec.clone())
        .chain(cat.audio.iter().filter(|e| !e.disabled && e.decoder.is_some()).map(|e| e.codec.clone()))
        .collect();
    known.sort();
    known.dedup();
    known.retain(|c| !have.contains_key(c));
    known
}

#[cfg(test)]
#[path = "libav_tests.rs"]
mod tests;
