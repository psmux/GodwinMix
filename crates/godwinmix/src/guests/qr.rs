//! A QR code for an invite link, as an SVG a page can show as it is.
//!
//! The same encoder as the channels page's `ui/panels/channels/qr.js`, in
//! Rust so `guest.invite.create` can answer with the code itself: byte mode,
//! error correction level M, versions 1 to 10, which is up to 213 bytes and
//! more than any invite link. Written from the ISO 18004 layout in the shape
//! of Project Nayuki's reference encoder. `qr_grid.rs` lays the modules out.

use super::qr_ecc::interleave;
use super::qr_grid::Grid;

/// Error correction codewords per block, and the block count, for level M.
pub(super) const ECC_M: [usize; 10] = [10, 16, 26, 18, 24, 16, 18, 22, 22, 26];
pub(super) const BLOCKS_M: [usize; 10] = [1, 1, 1, 2, 2, 4, 4, 4, 5, 5];
const MAX_VERSION: usize = 10;

/// The modules for `text`, rows of booleans with true dark, or None when it
/// does not fit in version 10.
pub fn matrix(text: &str) -> Option<Vec<Vec<bool>>> {
    let bytes = text.as_bytes();
    (1..=MAX_VERSION).find_map(|ver| {
        let used = 4 + if ver < 10 { 8 } else { 16 } + bytes.len() * 8;
        (used <= data_codewords(ver) * 8).then(|| build(ver, bytes))
    })
}

/// The code as an SVG document, four modules of quiet zone around it.
pub fn svg(text: &str) -> Option<String> {
    let m = matrix(text)?;
    let size = m.len() + 8;
    let mut d = String::new();
    for (y, row) in m.iter().enumerate() {
        for (x, dark) in row.iter().enumerate() {
            if *dark {
                d.push_str(&format!("M{} {}h1v1h-1z", x + 4, y + 4));
            }
        }
    }
    Some(format!(
        "<svg xmlns=\"http://www.w3.org/2000/svg\" viewBox=\"0 0 {size} {size}\" shape-rendering=\"crispEdges\">\
         <rect width=\"{size}\" height=\"{size}\" fill=\"#fff\"/><path fill=\"#000\" d=\"{d}\"/></svg>"
    ))
}

pub(super) fn raw_modules(ver: usize) -> usize {
    let mut n = (16 * ver + 128) * ver + 64;
    if ver >= 2 {
        let align = ver / 7 + 2;
        n -= (25 * align - 10) * align - 55;
        if ver >= 7 {
            n -= 36;
        }
    }
    n
}

fn data_codewords(ver: usize) -> usize {
    raw_modules(ver) / 8 - ECC_M[ver - 1] * BLOCKS_M[ver - 1]
}

fn build(ver: usize, bytes: &[u8]) -> Vec<Vec<bool>> {
    let mut bits: Vec<u8> = Vec::new();
    let put = |bits: &mut Vec<u8>, value: usize, len: usize| {
        for i in (0..len).rev() {
            bits.push(((value >> i) & 1) as u8);
        }
    };
    put(&mut bits, 4, 4);
    put(&mut bits, bytes.len(), if ver < 10 { 8 } else { 16 });
    for b in bytes {
        put(&mut bits, *b as usize, 8);
    }
    let capacity = data_codewords(ver) * 8;
    let end = 4.min(capacity - bits.len());
    put(&mut bits, 0, end);
    let pad = (8 - bits.len() % 8) % 8;
    put(&mut bits, 0, pad);
    let mut data: Vec<u8> = bits.chunks(8).map(|c| c.iter().fold(0u8, |a, b| (a << 1) | b)).collect();
    let mut filler = 0xecu8;
    while data.len() < capacity / 8 {
        data.push(filler);
        filler ^= 0xec ^ 0x11;
    }
    let size = ver * 4 + 17;
    let mut grid = Grid::new(size);
    grid.functions(ver);
    grid.codewords(&interleave(ver, &data));
    let best = (0..8)
        .map(|mask| {
            let mut trial = grid.clone();
            trial.mask(mask);
            trial.format(mask);
            (trial.penalty(), trial)
        })
        .min_by_key(|(score, _)| *score)
        .expect("eight masks");
    best.1.dark
}
