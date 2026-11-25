//! Command blocks: the payload of a packet is zero or more of these.
//!
//! ```text
//! byte 0..2  block length, these eight header bytes included
//! byte 2..4  zero
//! byte 4..8  the command name, four ASCII bytes such as PrgI
//! byte 8..   the body, laid out per command
//! ```

/// One block, borrowed from the packet it came in.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct Block<'a> {
    pub name: [u8; 4],
    pub body: &'a [u8],
}

impl Block<'_> {
    /// The name as text, for logs and errors.
    pub fn name_str(&self) -> &str {
        std::str::from_utf8(&self.name).unwrap_or("????")
    }
}

/// The blocks in a payload, in order. Stops at the first block whose length
/// is impossible, since everything after it would be read out of step.
pub struct Blocks<'a> {
    rest: &'a [u8],
}

impl<'a> Blocks<'a> {
    pub fn new(payload: &'a [u8]) -> Blocks<'a> {
        Blocks { rest: payload }
    }
}

impl<'a> Iterator for Blocks<'a> {
    type Item = Block<'a>;

    fn next(&mut self) -> Option<Block<'a>> {
        if self.rest.len() < 8 {
            return None;
        }
        let length = usize::from(u16::from_be_bytes([self.rest[0], self.rest[1]]));
        if length < 8 || length > self.rest.len() {
            self.rest = &[];
            return None;
        }
        let name = [self.rest[4], self.rest[5], self.rest[6], self.rest[7]];
        let body = &self.rest[8..length];
        self.rest = &self.rest[length..];
        Some(Block { name, body })
    }
}

/// Build one block from a name and a body.
pub fn block(name: &[u8; 4], body: &[u8]) -> Vec<u8> {
    let length = (8 + body.len()) as u16;
    let mut out = Vec::with_capacity(usize::from(length));
    out.extend_from_slice(&length.to_be_bytes());
    out.extend_from_slice(&[0, 0]);
    out.extend_from_slice(name);
    out.extend_from_slice(body);
    out
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn two_blocks_come_back_out_in_order() {
        let mut payload = block(b"PrgI", &[0, 0, 0, 1]);
        payload.extend(block(b"InCm", &[]));
        let names: Vec<_> = Blocks::new(&payload).map(|b| b.name_str().to_string()).collect();
        assert_eq!(names, ["PrgI", "InCm"]);
    }

    #[test]
    fn a_block_longer_than_the_packet_ends_the_walk() {
        let mut payload = block(b"PrgI", &[0, 0, 0, 1]);
        payload[1] = 200;
        assert_eq!(Blocks::new(&payload).count(), 0);
    }
}
