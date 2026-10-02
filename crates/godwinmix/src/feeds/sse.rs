//! Server-Sent Events, line by line: `data:` lines gather until a blank
//! line ends the event. A comment line (`:` first) is the server keeping the
//! connection open, and counts as a sign of life. `event:`, `id:` and
//! `retry:` are read past: a binding wants the data.

use godwinmix_protocol::feeds::MAX_BYTES;

#[derive(Debug, PartialEq)]
pub enum Item {
    Event(String),
    Alive,
}

#[derive(Default)]
pub struct Parser {
    line: Vec<u8>,
    data: String,
    has_data: bool,
}

impl Parser {
    /// Feed in bytes as they arrive; answer the events they finished.
    pub fn push(&mut self, bytes: &[u8]) -> Result<Vec<Item>, String> {
        let mut out = Vec::new();
        for &b in bytes {
            if b != b'\n' {
                self.line.push(b);
                if self.line.len() + self.data.len() > MAX_BYTES {
                    return Err(super::fetch::too_big());
                }
                continue;
            }
            let line = String::from_utf8_lossy(&self.line).trim_end_matches('\r').to_string();
            self.line.clear();
            if let Some(item) = self.line_done(&line) {
                out.push(item);
            }
        }
        Ok(out)
    }

    fn line_done(&mut self, line: &str) -> Option<Item> {
        if line.is_empty() {
            if !self.has_data {
                return None;
            }
            self.has_data = false;
            return Some(Item::Event(std::mem::take(&mut self.data)));
        }
        if line.starts_with(':') {
            return Some(Item::Alive);
        }
        let (field, value) = line.split_once(':').unwrap_or((line, ""));
        if field == "data" {
            if self.has_data {
                self.data.push('\n');
            }
            self.data.push_str(value.strip_prefix(' ').unwrap_or(value));
            self.has_data = true;
        }
        None
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn data_lines_gather_until_a_blank_line_across_chunks() {
        let mut p = Parser::default();
        assert_eq!(p.push(b": hello\n\nevent: score\nda").unwrap(), vec![Item::Alive]);
        assert_eq!(p.push(b"ta: {\"a\":\r\ndata: 1}\r\n\r\n").unwrap(), vec![Item::Event("{\"a\":\n1}".into())]);
        assert_eq!(p.push(b"id: 4\n\n").unwrap(), vec![]);
    }
}
