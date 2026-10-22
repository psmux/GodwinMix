//! The config file as written: directives, blocks, comments and blank lines,
//! in order. Nothing is thrown away, so a file edited from the dashboard is
//! written back with every directive GodwinMix does not understand still in
//! place and Flussonic could read it again.
//!
//! The language is small. A directive is words ended by `;`, or words followed
//! by a `{ ... }` block. `#` starts a comment only at the start of a word: in
//! `size=1280x720:fit:#000000` the `#` is part of the value. Words may be
//! quoted with `"` or `'`.

use std::fmt::Write as _;

use crate::Error;

#[derive(Debug, Clone, PartialEq)]
pub enum Node {
    Comment(String),
    Blank,
    Directive(Directive),
}

#[derive(Debug, Clone, PartialEq)]
pub struct Directive {
    pub name: String,
    pub args: Vec<String>,
    /// `Some` for `name args { ... }`, `None` for `name args;`.
    pub block: Option<Vec<Node>>,
    /// 1 based line the directive starts on, for error messages. 0 when the
    /// directive was made in code.
    pub line: usize,
}

impl Directive {
    pub fn new(name: impl Into<String>, args: Vec<String>) -> Self {
        Directive { name: name.into(), args, block: None, line: 0 }
    }

    pub fn block(name: impl Into<String>, args: Vec<String>, body: Vec<Node>) -> Self {
        Directive { name: name.into(), args, block: Some(body), line: 0 }
    }

    pub fn arg(&self, i: usize) -> Option<&str> {
        self.args.get(i).map(String::as_str)
    }

    /// The directives directly inside this block, in order.
    pub fn children(&self) -> impl Iterator<Item = &Directive> {
        self.block.iter().flatten().filter_map(|n| match n {
            Node::Directive(d) => Some(d),
            _ => None,
        })
    }

    pub fn children_mut(&mut self) -> impl Iterator<Item = &mut Directive> {
        self.block.iter_mut().flatten().filter_map(|n| match n {
            Node::Directive(d) => Some(d),
            _ => None,
        })
    }
}

#[derive(Debug, Clone, PartialEq)]
enum Tok {
    Word(String, usize),
    Semi(usize),
    Open(usize),
    Close(usize),
    Comment(String, usize),
    Blank(usize),
}

fn lex(src: &str) -> Result<Vec<Tok>, Error> {
    let mut out = Vec::new();
    let chars: Vec<char> = src.chars().collect();
    let mut i = 0;
    let mut line = 1;
    // A blank line is two newlines with only spaces between them. Kept so
    // the written file keeps the grouping a person gave it.
    let mut newlines_in_gap = 0;
    while i < chars.len() {
        let c = chars[i];
        match c {
            '\n' => {
                newlines_in_gap += 1;
                if newlines_in_gap == 2 {
                    out.push(Tok::Blank(line));
                }
                line += 1;
                i += 1;
            }
            c if c.is_whitespace() => i += 1,
            '#' => {
                let start = i + 1;
                while i < chars.len() && chars[i] != '\n' {
                    i += 1;
                }
                let text: String = chars[start..i].iter().collect();
                out.push(Tok::Comment(text.trim_end().to_string(), line));
                newlines_in_gap = 0;
            }
            ';' => {
                out.push(Tok::Semi(line));
                i += 1;
                newlines_in_gap = 0;
            }
            '{' => {
                out.push(Tok::Open(line));
                i += 1;
                newlines_in_gap = 0;
            }
            '}' => {
                out.push(Tok::Close(line));
                i += 1;
                newlines_in_gap = 0;
            }
            '"' | '\'' => {
                let quote = c;
                let start_line = line;
                let mut word = String::new();
                i += 1;
                loop {
                    match chars.get(i) {
                        None => return Err(Error::Syntax { line: start_line, msg: "unclosed quote".into() }),
                        Some('\\') if chars.get(i + 1).is_some() => {
                            word.push(chars[i + 1]);
                            i += 2;
                        }
                        Some(&q) if q == quote => {
                            i += 1;
                            break;
                        }
                        Some(&ch) => {
                            if ch == '\n' {
                                line += 1;
                            }
                            word.push(ch);
                            i += 1;
                        }
                    }
                }
                out.push(Tok::Word(word, start_line));
                newlines_in_gap = 0;
            }
            _ => {
                let start = i;
                while i < chars.len() {
                    let ch = chars[i];
                    if ch.is_whitespace() || ch == ';' || ch == '{' || ch == '}' {
                        break;
                    }
                    i += 1;
                }
                out.push(Tok::Word(chars[start..i].iter().collect(), line));
                newlines_in_gap = 0;
            }
        }
    }
    Ok(out)
}

/// Parse a whole config file.
pub fn parse(src: &str) -> Result<Vec<Node>, Error> {
    let toks = lex(src)?;
    let mut pos = 0;
    let nodes = parse_body(&toks, &mut pos, false)?;
    Ok(nodes)
}

fn parse_body(toks: &[Tok], pos: &mut usize, nested: bool) -> Result<Vec<Node>, Error> {
    let mut nodes = Vec::new();
    while *pos < toks.len() {
        match &toks[*pos] {
            Tok::Comment(text, _) => {
                nodes.push(Node::Comment(text.clone()));
                *pos += 1;
            }
            Tok::Blank(_) => {
                // Never two blanks in a row, and none straight after an open brace.
                if !matches!(nodes.last(), None | Some(Node::Blank)) {
                    nodes.push(Node::Blank);
                }
                *pos += 1;
            }
            Tok::Close(line) => {
                if !nested {
                    return Err(Error::Syntax { line: *line, msg: "'}' without a block to close".into() });
                }
                *pos += 1;
                while matches!(nodes.last(), Some(Node::Blank)) {
                    nodes.pop();
                }
                return Ok(nodes);
            }
            Tok::Semi(line) => {
                return Err(Error::Syntax { line: *line, msg: "';' with no directive before it".into() })
            }
            Tok::Open(line) => {
                return Err(Error::Syntax { line: *line, msg: "'{' with no directive name before it".into() })
            }
            Tok::Word(_, line) => {
                let line = *line;
                let mut words = Vec::new();
                loop {
                    match toks.get(*pos) {
                        Some(Tok::Word(w, _)) => {
                            words.push(w.clone());
                            *pos += 1;
                        }
                        // A comment in the middle of a directive belongs to no one;
                        // Flussonic does not write them and dropping it is harmless.
                        Some(Tok::Comment(..)) | Some(Tok::Blank(_)) => *pos += 1,
                        Some(Tok::Semi(_)) => {
                            *pos += 1;
                            let name = words.remove(0);
                            nodes.push(Node::Directive(Directive { name, args: words, block: None, line }));
                            break;
                        }
                        Some(Tok::Open(_)) => {
                            *pos += 1;
                            let body = parse_body(toks, pos, true)?;
                            let name = words.remove(0);
                            nodes.push(Node::Directive(Directive { name, args: words, block: Some(body), line }));
                            break;
                        }
                        Some(Tok::Close(l)) => {
                            return Err(Error::Syntax { line: *l, msg: format!("'{}' is missing its ';'", words[0]) })
                        }
                        None => return Err(Error::Syntax { line, msg: format!("'{}' is missing its ';'", words[0]) }),
                    }
                }
            }
        }
    }
    if nested {
        return Err(Error::Syntax { line: 0, msg: "a block is missing its '}'".into() });
    }
    while matches!(nodes.last(), Some(Node::Blank)) {
        nodes.pop();
    }
    Ok(nodes)
}

fn needs_quotes(w: &str) -> bool {
    w.is_empty() || w.starts_with('#') || w.chars().any(|c| c.is_whitespace() || matches!(c, ';' | '{' | '}' | '"' | '\''))
}

fn write_word(out: &mut String, w: &str) {
    if needs_quotes(w) {
        out.push('"');
        for c in w.chars() {
            if c == '"' || c == '\\' {
                out.push('\\');
            }
            out.push(c);
        }
        out.push('"');
    } else {
        out.push_str(w);
    }
}

/// Write nodes back as config text, two spaces per level, the way Flussonic
/// writes its own file.
pub fn print(nodes: &[Node]) -> String {
    let mut out = String::new();
    print_into(&mut out, nodes, 0);
    out
}

fn print_into(out: &mut String, nodes: &[Node], depth: usize) {
    let pad = "  ".repeat(depth);
    for node in nodes {
        match node {
            Node::Blank => out.push('\n'),
            Node::Comment(text) => {
                let _ = writeln!(out, "{pad}#{text}");
            }
            Node::Directive(d) => {
                out.push_str(&pad);
                out.push_str(&d.name);
                for a in &d.args {
                    out.push(' ');
                    write_word(out, a);
                }
                match &d.block {
                    None => out.push_str(";\n"),
                    Some(body) => {
                        out.push_str(" {\n");
                        print_into(out, body, depth + 1);
                        let _ = writeln!(out, "{pad}}}");
                    }
                }
            }
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn hash_inside_a_word_is_not_a_comment() {
        let nodes = parse("transcoder size=1280x720:fit:#000000 ab=128k; # trailing\n").unwrap();
        let Node::Directive(d) = &nodes[0] else { panic!() };
        assert_eq!(d.args, vec!["size=1280x720:fit:#000000", "ab=128k"]);
        assert_eq!(nodes[1], Node::Comment(" trailing".into()));
    }

    #[test]
    fn blocks_nest_and_print_back() {
        let src = "# Global settings:\nhttp 8090;\n\nstream CH1 {\n  disabled false;\n  input udp://eth1@239.0.0.1:2001?program=7;\n}\n";
        let nodes = parse(src).unwrap();
        assert_eq!(print(&nodes), src);
    }

    #[test]
    fn quotes_round_trip() {
        let nodes = parse("stream A { title \"Fox News; HD\"; }").unwrap();
        let text = print(&nodes);
        assert_eq!(text, "stream A {\n  title \"Fox News; HD\";\n}\n");
        assert_eq!(print(&parse(&text).unwrap()), text);
    }

    #[test]
    fn errors_name_the_line() {
        let err = parse("http 8090;\nstream A {\n  input x\n}\n").unwrap_err();
        assert!(matches!(err, Error::Syntax { line: 4, .. }), "{err:?}");
        assert!(parse("stream A {\n").is_err());
        assert!(parse("}").is_err());
    }
}
