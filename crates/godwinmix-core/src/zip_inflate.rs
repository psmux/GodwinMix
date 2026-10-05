//! Deflate (RFC 1951), read only, so a pack of transitions zipped by any
//! tool on any platform imports without unpacking it first.
//!
//! The decoder is the plain one from the RFC, in the shape of zlib's `puff`:
//! canonical Huffman tables counted per bit length and walked a bit at a
//! time. Not fast, and it does not need to be: a pack is read once, at
//! import, on a blocking worker.

/// Bits in, least significant first.
struct Bits<'a> {
    data: &'a [u8],
    at: usize,
    bit: u32,
}

impl Bits<'_> {
    fn take(&mut self, n: u32) -> Option<u32> {
        let mut v = 0u32;
        for i in 0..n {
            let byte = *self.data.get(self.at)?;
            v |= (((byte >> self.bit) & 1) as u32) << i;
            self.bit += 1;
            if self.bit == 8 {
                self.bit = 0;
                self.at += 1;
            }
        }
        Some(v)
    }
}

/// A canonical Huffman code: how many codes of each length, and the symbols
/// in code order.
struct Code {
    counts: [u16; 16],
    symbols: Vec<u16>,
}

impl Code {
    fn new(lengths: &[u8]) -> Code {
        let mut counts = [0u16; 16];
        for &l in lengths {
            counts[l as usize] += 1;
        }
        counts[0] = 0;
        let mut offsets = [0u16; 16];
        for i in 1..16 {
            offsets[i] = offsets[i - 1] + counts[i - 1];
        }
        let mut symbols = vec![0u16; lengths.len()];
        for (s, &l) in lengths.iter().enumerate() {
            if l != 0 {
                symbols[offsets[l as usize] as usize] = s as u16;
                offsets[l as usize] += 1;
            }
        }
        Code { counts, symbols }
    }

    fn decode(&self, bits: &mut Bits<'_>) -> Option<u16> {
        let (mut code, mut first, mut index) = (0i32, 0i32, 0i32);
        for len in 1..16 {
            code |= bits.take(1)? as i32;
            let count = self.counts[len] as i32;
            if code - count < first {
                return self.symbols.get((index + code - first) as usize).copied();
            }
            index += count;
            first = (first + count) << 1;
            code <<= 1;
        }
        None
    }
}

const LEN_BASE: [u16; 29] = [3, 4, 5, 6, 7, 8, 9, 10, 11, 13, 15, 17, 19, 23, 27, 31, 35, 43, 51, 59, 67, 83, 99, 115, 131, 163, 195, 227, 258];
const LEN_EXTRA: [u8; 29] = [0, 0, 0, 0, 0, 0, 0, 0, 1, 1, 1, 1, 2, 2, 2, 2, 3, 3, 3, 3, 4, 4, 4, 4, 5, 5, 5, 5, 0];
const DIST_BASE: [u16; 30] = [1, 2, 3, 4, 5, 7, 9, 13, 17, 25, 33, 49, 65, 97, 129, 193, 257, 385, 513, 769, 1025, 1537, 2049, 3073, 4097, 6145, 8193, 12289, 16385, 24577];
const DIST_EXTRA: [u8; 30] = [0, 0, 0, 0, 1, 1, 2, 2, 3, 3, 4, 4, 5, 5, 6, 6, 7, 7, 8, 8, 9, 9, 10, 10, 11, 11, 12, 12, 13, 13];

/// Inflate a raw deflate stream. `None` for a stream that is not one.
pub fn inflate(data: &[u8], size_hint: usize) -> Option<Vec<u8>> {
    let mut bits = Bits { data, at: 0, bit: 0 };
    let mut out = Vec::with_capacity(size_hint);
    loop {
        let last = bits.take(1)?;
        match bits.take(2)? {
            0 => stored(&mut bits, &mut out)?,
            1 => {
                let mut l = [8u8; 288];
                l[144..256].fill(9);
                l[256..280].fill(7);
                block(&mut bits, &mut out, &Code::new(&l), &Code::new(&[5u8; 30]))?
            }
            2 => {
                let (lit, dist) = dynamic(&mut bits)?;
                block(&mut bits, &mut out, &lit, &dist)?
            }
            _ => return None,
        }
        if last == 1 {
            return Some(out);
        }
    }
}

fn stored(bits: &mut Bits<'_>, out: &mut Vec<u8>) -> Option<()> {
    if bits.bit != 0 {
        bits.bit = 0;
        bits.at += 1;
    }
    let d = bits.data.get(bits.at..bits.at + 4)?;
    let len = u16::from_le_bytes([d[0], d[1]]) as usize;
    out.extend_from_slice(bits.data.get(bits.at + 4..bits.at + 4 + len)?);
    bits.at += 4 + len;
    Some(())
}

fn dynamic(bits: &mut Bits<'_>) -> Option<(Code, Code)> {
    let (nlen, ndist, ncode) = (bits.take(5)? as usize + 257, bits.take(5)? as usize + 1, bits.take(4)? as usize + 4);
    const ORDER: [usize; 19] = [16, 17, 18, 0, 8, 7, 9, 6, 10, 5, 11, 4, 12, 3, 13, 2, 14, 1, 15];
    let mut cl = [0u8; 19];
    for &i in ORDER.iter().take(ncode) {
        cl[i] = bits.take(3)? as u8;
    }
    let lencode = Code::new(&cl);
    let mut lengths = Vec::with_capacity(nlen + ndist);
    while lengths.len() < nlen + ndist {
        let (value, repeat) = match lencode.decode(bits)? {
            s @ 0..=15 => (s as u8, 1),
            16 => (*lengths.last()?, 3 + bits.take(2)?),
            17 => (0, 3 + bits.take(3)?),
            _ => (0, 11 + bits.take(7)?),
        };
        lengths.extend(std::iter::repeat_n(value, repeat as usize));
    }
    lengths.truncate(nlen + ndist);
    Some((Code::new(&lengths[..nlen]), Code::new(&lengths[nlen..])))
}

fn block(bits: &mut Bits<'_>, out: &mut Vec<u8>, lit: &Code, dist: &Code) -> Option<()> {
    loop {
        let s = lit.decode(bits)? as usize;
        if s < 256 {
            out.push(s as u8);
            continue;
        }
        if s == 256 {
            return Some(());
        }
        let i = s.checked_sub(257).filter(|i| *i < 29)?;
        let len = LEN_BASE[i] as usize + bits.take(LEN_EXTRA[i] as u32)? as usize;
        let d = dist.decode(bits)? as usize;
        let back = *DIST_BASE.get(d)? as usize + bits.take(*DIST_EXTRA.get(d)? as u32)? as usize;
        let from = out.len().checked_sub(back)?;
        for k in 0..len {
            out.push(out[from + k]);
        }
    }
}
