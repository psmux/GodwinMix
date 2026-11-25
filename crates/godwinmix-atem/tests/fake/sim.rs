//! What the fake switcher does with a command: just enough of a switcher
//! to answer each command with the state blocks a real one sends back.

use godwinmix_atem::wire::block;

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Sim {
    pub program: u16,
    pub preview: u16,
    pub dsk_on: bool,
    pub ftb: bool,
    pub aux: u16,
    pub macro_running: Option<u16>,
}

impl Default for Sim {
    fn default() -> Sim {
        Sim { program: 1, preview: 2, dsk_on: false, ftb: false, aux: 10010, macro_running: None }
    }
}

/// `TlSr` for one programme and one preview input, out of inputs 1 to 4.
pub fn tally(program: u16, preview: u16) -> Vec<u8> {
    let mut body = 4u16.to_be_bytes().to_vec();
    for input in 1..=4u16 {
        body.extend(input.to_be_bytes());
        body.push(u8::from(input == program) | (u8::from(input == preview) << 1));
    }
    block(b"TlSr", &body)
}

fn word(body: &[u8], at: usize) -> u16 {
    u16::from_be_bytes([body[at], body[at + 1]])
}

impl Sim {
    fn buses(&self) -> Vec<Vec<u8>> {
        let p = self.program.to_be_bytes();
        let v = self.preview.to_be_bytes();
        vec![
            block(b"PrgI", &[0, 0, p[0], p[1]]),
            block(b"PrvI", &[0, 0, v[0], v[1], 0, 0, 0, 0]),
            tally(self.program, self.preview),
        ]
    }

    /// Fold one command block in; answer with the blocks that changed.
    pub fn apply(&mut self, name: &[u8; 4], body: &[u8]) -> Vec<Vec<u8>> {
        match name {
            b"CPgI" => self.program = word(body, 2),
            b"CPvI" => self.preview = word(body, 2),
            b"DCut" | b"DAut" => std::mem::swap(&mut self.program, &mut self.preview),
            b"CDsL" => self.dsk_on = body[1] != 0,
            b"DDsA" => self.dsk_on = !self.dsk_on,
            b"FtbA" => self.ftb = !self.ftb,
            b"CAuS" => {
                self.aux = word(body, 2);
                let a = self.aux.to_be_bytes();
                return vec![block(b"AuxS", &[body[1], 0, a[0], a[1]])];
            }
            b"MAct" if body[2] == 0 => {
                self.macro_running = Some(word(body, 0));
                return vec![block(b"MRPr", &[1, 0, body[0], body[1]])];
            }
            b"MAct" => {
                self.macro_running = None;
                return vec![block(b"MRPr", &[0, 0, 0xff, 0xff])];
            }
            _ => return Vec::new(),
        }
        match name {
            b"CDsL" | b"DDsA" => vec![block(b"DskS", &[0, u8::from(self.dsk_on), 0, 0, 0, 0, 0, 0])],
            b"FtbA" => vec![block(b"FtbS", &[0, u8::from(self.ftb), 0, 0])],
            _ => self.buses(),
        }
    }
}
