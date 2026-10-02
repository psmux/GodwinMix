//! Just enough XML to read a news feed.
//!
//! Elements, attributes, text, CDATA and the five entities plus numeric ones.
//! Comments, processing instructions and a doctype are skipped. It is
//! forgiving rather than validating: a close tag that does not match closes
//! back to the one that does, which is what a feed written by hand needs.
//! No crate in the workspace parses XML, and a feed reader does not justify
//! adding one.

#[derive(Debug, Default, Clone)]
pub struct Node {
    pub name: String,
    pub attrs: Vec<(String, String)>,
    pub children: Vec<Node>,
    pub text: String,
}

impl Node {
    /// The name without a namespace prefix.
    pub fn local(&self) -> &str {
        self.name.rsplit(':').next().unwrap_or(&self.name)
    }

    pub fn child(&self, name: &str) -> Option<&Node> {
        self.children.iter().find(|c| c.name == name || c.local() == name)
    }

    pub fn all<'a>(&'a self, name: &'a str) -> impl Iterator<Item = &'a Node> + 'a {
        self.children.iter().filter(move |c| c.local() == name)
    }

    pub fn attr(&self, name: &str) -> Option<&str> {
        self.attrs.iter().find(|(k, _)| k == name).map(|(_, v)| v.as_str())
    }
}

/// Read a document and answer its root element.
pub fn parse(text: &str) -> Result<Node, String> {
    let mut stack: Vec<Node> = vec![Node::default()];
    let mut rest = text;
    while let Some(lt) = rest.find('<') {
        push_text(&mut stack, &decode(&rest[..lt]));
        rest = &rest[lt..];
        if let Some(body) = rest.strip_prefix("<![CDATA[") {
            let end = body.find("]]>").ok_or("a CDATA section is never closed")?;
            push_text(&mut stack, &body[..end]);
            rest = &body[end + 3..];
        } else if let Some(body) = rest.strip_prefix("<!--") {
            rest = &body[body.find("-->").ok_or("a comment is never closed")? + 3..];
        } else if rest.starts_with("<?") || rest.starts_with("<!") {
            rest = &rest[skip_declaration(rest)?..];
        } else if let Some(body) = rest.strip_prefix("</") {
            let end = body.find('>').ok_or("a close tag is never finished")?;
            close(&mut stack, body[..end].trim());
            rest = &body[end + 1..];
        } else {
            let end = tag_end(rest).ok_or("a tag is never finished")?;
            let inner = &rest[1..end];
            let (inner, empty) = match inner.strip_suffix('/') {
                Some(i) => (i, true),
                None => (inner, false),
            };
            let node = open(inner);
            if empty {
                attach(&mut stack, node);
            } else {
                stack.push(node);
            }
            rest = &rest[end + 1..];
        }
    }
    while stack.len() > 1 {
        let node = stack.pop().unwrap_or_default();
        attach(&mut stack, node);
    }
    let root = stack.pop().unwrap_or_default();
    root.children.into_iter().next().ok_or_else(|| "there is no element in it".to_string())
}

fn push_text(stack: &mut [Node], text: &str) {
    if let Some(top) = stack.last_mut() {
        top.text.push_str(text);
    }
}

fn attach(stack: &mut [Node], node: Node) {
    if let Some(top) = stack.last_mut() {
        top.children.push(node);
    }
}

fn close(stack: &mut Vec<Node>, name: &str) {
    if !stack.iter().skip(1).any(|n| n.name == name) {
        return;
    }
    while stack.len() > 1 {
        let node = stack.pop().unwrap_or_default();
        let done = node.name == name;
        attach(stack, node);
        if done {
            return;
        }
    }
}

/// `<?xml ...?>` or `<!DOCTYPE ... [ ... ]>`: where it ends.
fn skip_declaration(rest: &str) -> Result<usize, String> {
    let mut depth = 0i32;
    for (i, c) in rest.char_indices() {
        match c {
            '[' => depth += 1,
            ']' => depth -= 1,
            '>' if depth <= 0 => return Ok(i + 1),
            _ => {}
        }
    }
    Err("a declaration is never finished".into())
}

/// The `>` that ends a tag, past any `>` inside a quoted attribute.
fn tag_end(rest: &str) -> Option<usize> {
    let mut quote = None;
    for (i, c) in rest.char_indices() {
        match (quote, c) {
            (None, '"' | '\'') => quote = Some(c),
            (Some(q), c) if c == q => quote = None,
            (None, '>') => return Some(i),
            _ => {}
        }
    }
    None
}

fn open(inner: &str) -> Node {
    let inner = inner.trim();
    let name_end = inner.find(|c: char| c.is_whitespace()).unwrap_or(inner.len());
    let mut node = Node { name: inner[..name_end].to_string(), ..Node::default() };
    let mut rest = &inner[name_end..];
    while let Some(eq) = rest.find('=') {
        let key = rest[..eq].trim().to_string();
        let after = rest[eq + 1..].trim_start();
        let Some(q) = after.chars().next().filter(|c| *c == '"' || *c == '\'') else { break };
        let Some(end) = after[1..].find(q) else { break };
        node.attrs.push((key, decode(&after[1..1 + end])));
        rest = &after[end + 2..];
    }
    node
}

/// The XML entities, the HTML space and numeric references.
pub fn decode(text: &str) -> String {
    if !text.contains('&') {
        return text.to_string();
    }
    let mut out = String::with_capacity(text.len());
    let mut rest = text;
    while let Some(amp) = rest.find('&') {
        out.push_str(&rest[..amp]);
        rest = &rest[amp..];
        let semi = rest.char_indices().take_while(|(i, _)| *i < 12).find(|(_, c)| *c == ';').map(|(i, _)| i);
        let Some(semi) = semi else {
            out.push('&');
            rest = &rest[1..];
            continue;
        };
        match entity(&rest[1..semi]) {
            Some(c) => out.push(c),
            None => out.push_str(&rest[..=semi]),
        }
        rest = &rest[semi + 1..];
    }
    out.push_str(rest);
    out
}

fn entity(name: &str) -> Option<char> {
    match name {
        "amp" => Some('&'),
        "lt" => Some('<'),
        "gt" => Some('>'),
        "quot" => Some('"'),
        "apos" => Some('\''),
        "nbsp" => Some('\u{a0}'),
        _ => {
            let n = name.strip_prefix('#')?;
            let code = match n.strip_prefix('x').or_else(|| n.strip_prefix('X')) {
                Some(hex) => u32::from_str_radix(hex, 16).ok()?,
                None => n.parse().ok()?,
            };
            char::from_u32(code)
        }
    }
}
