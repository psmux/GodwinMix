//! The switcher itself: version, product, topology, inputs, aux and tally.

use super::read::{text_at, u16_at, u8_at};
use super::{slot, Input, State, Tally, Topology, Version};

/// `_ver`: protocol major and minor, two u16.
pub fn version(state: &mut State, b: &[u8]) -> Option<()> {
    state.protocol = Some(Version { major: u16_at(b, 0)?, minor: u16_at(b, 2)? });
    Some(())
}

/// `_pin`: the product name in 40 bytes, then the model number.
pub fn product(state: &mut State, b: &[u8]) -> Option<()> {
    state.product = Some(text_at(b, 0, 40)?);
    state.model = u8_at(b, 40);
    Some(())
}

/// `_top`: the first six counts, which every protocol version since 2.27
/// puts in the same place.
pub fn topology(state: &mut State, b: &[u8]) -> Option<()> {
    state.topology = Topology {
        mix_effects: u8_at(b, 0)?,
        sources: u8_at(b, 1)?,
        downstream_keyers: u8_at(b, 2)?,
        aux: u8_at(b, 3)?,
        mix_minus_outputs: u8_at(b, 4)?,
        media_players: u8_at(b, 5)?,
    };
    Some(())
}

/// `InPr`: source number, a 20 byte long name and a 4 byte short name.
pub fn input(state: &mut State, b: &[u8]) -> Option<()> {
    let source = u16_at(b, 0)?;
    let input = Input { long_name: text_at(b, 2, 20)?, short_name: text_at(b, 22, 4)? };
    state.inputs.insert(source, input);
    Some(())
}

/// `AuxS`: aux number, a pad byte, the source.
pub fn aux(state: &mut State, b: &[u8]) -> Option<()> {
    let aux = u8_at(b, 0)?;
    let source = u16_at(b, 2)?;
    *slot(&mut state.aux, usize::from(aux)) = source;
    Some(())
}

/// `TlSr`: a count, then three bytes per source: its number and a flag byte
/// with bit 0 programme and bit 1 preview. The whole table every time.
pub fn tally(state: &mut State, b: &[u8]) -> Option<()> {
    let count = usize::from(u16_at(b, 0)?);
    let mut table = std::collections::BTreeMap::new();
    for i in 0..count {
        let at = 2 + i * 3;
        let source = u16_at(b, at)?;
        let flags = u8_at(b, at + 2)?;
        table.insert(source, Tally { program: flags & 1 != 0, preview: flags & 2 != 0 });
    }
    state.tally = table;
    Some(())
}
