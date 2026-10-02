//! The little XML a template needs read and written: escaping a value,
//! reading an attribute, and finding a tag by name. A template is SVG a
//! person or an agent wrote, so this reads it as text and touches only what
//! it is asked about, leaving everything else exactly as written.

/// `s` safe inside an element's text and inside a quoted attribute alike.
pub fn escape(s: &str) -> String {
    let mut out = String::with_capacity(s.len());
    for c in s.chars() {
        match c {
            '&' => out.push_str("&amp;"),
            '<' => out.push_str("&lt;"),
            '>' => out.push_str("&gt;"),
            '"' => out.push_str("&quot;"),
            '\'' => out.push_str("&apos;"),
            // A control character other than a tab or a new line is not
            // allowed in XML at all, and would stop the picture rendering.
            c if c.is_control() && c != '\t' && c != '\n' => out.push(' '),
            c => out.push(c),
        }
    }
    out
}

/// The five named entities and numeric ones, back to their characters.
pub fn unescape(s: &str) -> String {
    let mut out = String::with_capacity(s.len());
    let mut rest = s;
    while let Some(at) = rest.find('&') {
        out.push_str(&rest[..at]);
        let tail = &rest[at..];
        let Some(end) = tail.find(';').filter(|e| *e <= 10) else {
            out.push('&');
            rest = &tail[1..];
            continue;
        };
        let entity = &tail[1..end];
        let c = match entity {
            "amp" => Some('&'),
            "lt" => Some('<'),
            "gt" => Some('>'),
            "quot" => Some('"'),
            "apos" => Some('\''),
            e if e.starts_with("#x") => u32::from_str_radix(&e[2..], 16).ok().and_then(char::from_u32),
            e if e.starts_with('#') => e[1..].parse().ok().and_then(char::from_u32),
            _ => None,
        };
        match c {
            Some(c) => {
                out.push(c);
                rest = &tail[end + 1..];
            }
            None => {
                out.push('&');
                rest = &tail[1..];
            }
        }
    }
    out.push_str(rest);
    out
}

/// One start tag found in a document: where it is and its text, from `<`
/// to `>` inclusive.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct Tag<'a> {
    pub start: usize,
    pub end: usize,
    pub text: &'a str,
}

/// Every start tag named `name` (`text`, `gmx:field`), in document order.
pub fn tags<'a>(doc: &'a str, name: &str) -> Vec<Tag<'a>> {
    let open = format!("<{name}");
    let mut found = Vec::new();
    let mut from = 0;
    while let Some(at) = doc[from..].find(&open).map(|i| i + from) {
        let after = doc[at + open.len()..].chars().next();
        from = at + open.len();
        if !matches!(after, Some(c) if c.is_whitespace() || c == '>' || c == '/') {
            continue;
        }
        let Some(close) = tag_end(&doc[at..]) else { break };
        found.push(Tag { start: at, end: at + close, text: &doc[at..at + close] });
        from = at + close;
    }
    found
}

/// The length of the tag at the start of `s`, `>` included, skipping a `>`
/// inside a quoted attribute.
fn tag_end(s: &str) -> Option<usize> {
    let mut quote: Option<char> = None;
    for (i, c) in s.char_indices() {
        match (quote, c) {
            (Some(q), c) if c == q => quote = None,
            (Some(_), _) => {}
            (None, '"' | '\'') => quote = Some(c),
            (None, '>') => return Some(i + 1),
            _ => {}
        }
    }
    None
}

/// The value of attribute `name` on a start tag, unescaped.
pub fn attr(tag: &str, name: &str) -> Option<String> {
    attr_span(tag, name).map(|(s, e)| unescape(&tag[s..e]))
}

/// `tag` with attribute `name` set to `value`: replaced where it is, added
/// before the end of the tag where it is not.
pub fn set_attr(tag: &str, name: &str, value: &str) -> String {
    let v = escape(value);
    if let Some((s, e)) = attr_span(tag, name) {
        return format!("{}{v}{}", &tag[..s], &tag[e..]);
    }
    let end = tag.len() - if tag.ends_with("/>") { 2 } else { 1 };
    format!("{} {name}=\"{v}\"{}", tag[..end].trim_end(), &tag[end..])
}

/// Where the raw value of attribute `name` sits in `tag`, quotes excluded.
fn attr_span(tag: &str, name: &str) -> Option<(usize, usize)> {
    let mut from = 0;
    while let Some(at) = tag[from..].find(name).map(|i| i + from) {
        from = at + name.len();
        if !matches!(tag[..at].chars().next_back(), Some(c) if c.is_whitespace()) {
            continue;
        }
        let rest = &tag[from..];
        let eq = rest.len() - rest.trim_start().len();
        let Some(after_eq) = rest.trim_start().strip_prefix('=') else { continue };
        let skip = after_eq.len() - after_eq.trim_start().len();
        let value_at = from + eq + 1 + skip;
        let quote = tag[value_at..].chars().next().filter(|q| *q == '"' || *q == '\'')?;
        let len = tag[value_at + 1..].find(quote)?;
        return Some((value_at + 1, value_at + 1 + len));
    }
    None
}

#[cfg(test)]
#[path = "xml_tests.rs"]
mod tests;
