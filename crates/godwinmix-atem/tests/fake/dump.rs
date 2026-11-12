//! The state a fake switcher sends when a session opens, block by block, in
//! the order a one M/E switcher with four inputs sends it. Written from the
//! layouts in docs/reference/atem-protocol.md rather than captured, and with
//! no serial number or address in it.

use godwinmix_atem::wire::block;

fn text(value: &str, width: usize) -> Vec<u8> {
    let mut out = value.as_bytes().to_vec();
    out.resize(width, 0);
    out
}

fn input(id: u16, long: &str, short: &str) -> Vec<u8> {
    let mut body = id.to_be_bytes().to_vec();
    body.extend(text(long, 20));
    body.extend(text(short, 4));
    body.extend([1, 0, 0, 1, 0, 1, 0, 0, 0x01, 0x01]);
    block(b"InPr", &body)
}

fn macro_slot(index: u16, name: &str, description: &str) -> Vec<u8> {
    let mut body = index.to_be_bytes().to_vec();
    body.extend([1, 0]);
    body.extend((name.len() as u16).to_be_bytes());
    body.extend((description.len() as u16).to_be_bytes());
    body.extend(name.as_bytes());
    body.extend(description.as_bytes());
    while body.len() % 4 != 0 {
        body.push(0);
    }
    block(b"MPrp", &body)
}

/// The blocks of the dump, the last one `InCm`.
pub fn blocks() -> Vec<Vec<u8>> {
    let mut product = text("ATEM Mini Pro", 40);
    product.extend([13, 0, 0, 0]);
    let mut out = vec![
        block(b"_ver", &[0, 2, 0, 30]),
        block(b"_pin", &product),
        block(b"_top", &[1, 14, 1, 1, 0, 1, 1, 0, 0, 1, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0]),
        block(b"_MeC", &[0, 1, 0, 0]),
        input(0, "Black", "BLK"),
        input(1, "Camera 1", "CAM1"),
        input(2, "Camera 2", "CAM2"),
        input(3, "Camera 3", "CAM3"),
        input(4, "Slides", "SLDS"),
        input(1000, "Color Bars", "BARS"),
        block(b"PrgI", &[0, 0, 0, 1]),
        block(b"PrvI", &[0, 0, 0, 2, 0, 0, 0, 0]),
        block(b"TrSS", &[0, 0, 1, 0, 1, 0, 0, 0]),
        block(b"TrPs", &[0, 0, 0, 0, 0, 0, 0, 0]),
        block(b"TMxP", &[0, 25, 0, 0]),
        block(b"TDpP", &[0, 25, 0, 0]),
        block(b"TWpP", &[0, 25, 0, 0]),
        block(b"KeOn", &[0, 0, 0, 0]),
        block(b"FtbP", &[0, 25, 0, 0]),
        block(b"FtbS", &[0, 0, 0, 25]),
        block(b"DskB", &[0, 0, 0x0b, 0xc2, 0x0b, 0xc3]),
        block(b"DskP", &[0, 0, 25, 0, 0, 0, 0, 0]),
        block(b"DskS", &[0, 0, 0, 0, 0, 0, 0, 0]),
        block(b"AuxS", &[0, 0, 0x27, 0x1a]),
        macro_slot(0, "Opening", "Wide, then the title"),
        macro_slot(3, "Closing", ""),
        block(b"MRPr", &[0, 0, 0xff, 0xff]),
        block(b"MPCE", &[0, 1, 0, 0]),
    ];
    out.push(super::sim::tally(1, 2));
    out.push(block(b"InCm", &[1, 0, 0, 0]));
    out
}

/// The blocks packed into payloads of at most `limit` bytes each, the way
/// a switcher packs its dump.
pub fn payloads(limit: usize) -> Vec<Vec<u8>> {
    let mut out: Vec<Vec<u8>> = vec![Vec::new()];
    for b in blocks() {
        if out.last().unwrap().len() + b.len() > limit {
            out.push(Vec::new());
        }
        out.last_mut().unwrap().extend(b);
    }
    out
}
