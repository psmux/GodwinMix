//! HEVC in Annex B to the enhanced RTMP bodies `h265parse` and the tagger
//! make: an `hvcC` sequence start whenever the parameter sets change, then
//! each access unit with four byte lengths in front of its NAL units.

use super::annexb::{push_sized, units};
use super::h264::header;
use super::{cts_ms, Parser, Pes, Said, Unit};
use crate::eflv::{prefix, HEVC};
use crate::sps::Bits;

const VPS: u8 = 32;
const SPS: u8 = 33;
const PPS: u8 = 34;

#[derive(Default)]
pub struct Hevc {
    sets: [Option<Vec<u8>>; 3],
    sent: Option<Vec<u8>>,
}

impl Parser for Hevc {
    fn pes(&mut self, pes: &Pes, out: &mut Vec<Unit>) {
        let Some(at) = pes.at() else { return };
        let mut au = Vec::with_capacity(pes.data.len() + 16);
        let mut keyframe = false;
        for nal in units(pes.data) {
            let kind = (nal[0] >> 1) & 0x3F;
            match kind {
                VPS..=PPS => self.sets[usize::from(kind - VPS)] = Some(nal.to_vec()),
                // IRAP pictures: BLA, IDR and CRA, 16 to 23.
                16..=23 => keyframe = true,
                _ => {}
            }
            push_sized(&mut au, nal);
        }
        let Some(config) = hvcc(&self.sets) else { return };
        if au.is_empty() {
            return;
        }
        if self.sent.as_ref() != Some(&config) {
            out.push(header(at, &prefix(HEVC, true, true, 0), &config));
            self.sent = Some(config);
        }
        let mut body = prefix(HEVC, keyframe, false, cts_ms(pes));
        body.extend_from_slice(&au);
        out.push(Unit { at, body, keyframe, header: false });
    }

    fn said(&self) -> Said {
        Said { codec: "h265", ..Said::default() }
    }
}

/// The HEVCDecoderConfigurationRecord (ISO 14496-15, 8.3.3.1.2) for one
/// VPS, SPS and PPS, as `h265parse` writes it.
pub fn hvcc(sets: &[Option<Vec<u8>>; 3]) -> Option<Vec<u8>> {
    let [Some(vps), Some(sps), Some(pps)] = sets else { return None };
    let f = read_sps(sps)?;
    let mut c = vec![1];
    c.extend_from_slice(&f.general);
    c.extend_from_slice(&[0xF0, 0x00, 0xFC, 0xFC | f.chroma, 0xF8 | f.luma, 0xF8 | f.chroma_depth, 0, 0]);
    c.push(((f.sub_layers + 1) << 3) | (f.nesting << 2) | 3);
    c.push(3);
    for (kind, nal) in [(VPS, vps), (SPS, sps), (PPS, pps)] {
        c.push(kind);
        c.extend_from_slice(&1u16.to_be_bytes());
        c.extend_from_slice(&(nal.len() as u16).to_be_bytes());
        c.extend_from_slice(nal);
    }
    Some(c)
}

struct SpsFields {
    /// The general profile, tier and level: 12 bytes, as the record has them.
    general: [u8; 12],
    sub_layers: u8,
    nesting: u8,
    chroma: u8,
    luma: u8,
    chroma_depth: u8,
}

fn read_sps(nal: &[u8]) -> Option<SpsFields> {
    let mut b = Bits::new(nal.get(2..)?);
    b.bits(4)?;
    let sub_layers = b.bits(3)?;
    let nesting = b.bit()?;
    let mut general = [0u8; 12];
    let mut raw = Bits::new(nal.get(2..)?);
    raw.bits(8)?;
    for g in general.iter_mut() {
        *g = raw.bits(8)? as u8;
    }
    crate::hevc::skip_profile_tier_level(&mut b, sub_layers)?;
    b.ue()?; // sps_seq_parameter_set_id
    let chroma = b.ue()?;
    if chroma == 3 {
        b.bit()?;
    }
    b.ue()?;
    b.ue()?; // the picture size
    if b.bit()? == 1 {
        for _ in 0..4 {
            b.ue()?;
        }
    }
    let (luma, chroma_depth) = (b.ue()?, b.ue()?);
    Some(SpsFields { general, sub_layers: sub_layers as u8, nesting: nesting as u8, chroma: chroma as u8, luma: luma as u8, chroma_depth: chroma_depth as u8 })
}
