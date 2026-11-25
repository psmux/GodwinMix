//! Just enough XML reading for SOAP answers: the text of an element and an
//! attribute of one, whatever namespace prefix the device chose. Cameras
//! differ in prefixes far more than in structure, so matching local names is
//! what makes the same code read every maker's answers.

/// The text inside the first element whose local name is `name`.
pub fn element<'a>(xml: &'a str, name: &str) -> Option<&'a str> {
    let (tag, after) = open_tag(xml, name)?;
    if tag.ends_with('/') {
        return Some("");
    }
    Some(&after[..after.find("</")?])
}

/// The value of `attr` on the first element whose local name is `name`.
pub fn attr<'a>(xml: &'a str, name: &str, attr: &str) -> Option<&'a str> {
    let (tag, _) = open_tag(xml, name)?;
    let needle = format!("{attr}=");
    let mut rest = tag;
    loop {
        let at = rest.find(&needle)?;
        let before = rest[..at].chars().last();
        rest = &rest[at + needle.len()..];
        if before.is_some_and(|c| c.is_whitespace() || c == ':') {
            let quote = rest.chars().next()?;
            let body = &rest[1..];
            return Some(&body[..body.find(quote)?]);
        }
    }
}

/// Every part of `xml` from one opening `name` element to the next, for a
/// list such as the presets in GetPresetsResponse.
pub fn each<'a>(xml: &'a str, name: &str) -> Vec<&'a str> {
    let mut out = Vec::new();
    let mut rest = xml;
    while let Some((_, after)) = open_tag(rest, name) {
        let start = rest.len() - after.len();
        let tag_start = rest[..start].rfind('<').unwrap_or(0);
        let next = open_tag(after, name).map(|(_, a)| after.len() - a.len()).and_then(|e| after[..e].rfind('<'));
        let end = next.map(|n| start + n).unwrap_or(rest.len());
        out.push(&rest[tag_start..end]);
        rest = &rest[end..];
    }
    out
}

/// The inside of the first opening tag named `name`, and what follows it.
fn open_tag<'a>(xml: &'a str, name: &str) -> Option<(&'a str, &'a str)> {
    let mut rest = xml;
    loop {
        let at = rest.find('<')?;
        rest = &rest[at + 1..];
        let tag_end = rest.find('>')?;
        let tag = &rest[..tag_end];
        let local = tag.split_whitespace().next().unwrap_or("").trim_end_matches('/').rsplit(':').next().unwrap_or("");
        if local == name && !tag.starts_with('/') && !tag.starts_with('?') {
            return Some((tag, &rest[tag_end + 1..]));
        }
    }
}

pub fn escape(s: &str) -> String {
    s.replace('&', "&amp;").replace('<', "&lt;").replace('>', "&gt;").replace('"', "&quot;")
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn elements_and_attributes_read_whatever_the_prefix() {
        assert_eq!(element("<a:B x=\"1\">hi</a:B>", "B"), Some("hi"));
        assert_eq!(element("<B/>", "B"), Some(""));
        let status = "<tt:Position><tt:PanTilt x=\"0.25\" y='-0.5' space=\"s\"/><tt:Zoom x=\"0.1\"/></tt:Position>";
        assert_eq!(attr(status, "PanTilt", "x"), Some("0.25"));
        assert_eq!(attr(status, "PanTilt", "y"), Some("-0.5"));
        assert_eq!(attr(status, "Zoom", "x"), Some("0.1"));
        assert_eq!(attr("<P token=\"t\" xtoken=\"no\"/>", "P", "token"), Some("t"));
    }

    #[test]
    fn a_list_splits_into_one_part_per_item() {
        let xml = "<r><tptz:Preset token=\"1\"><tt:Name>A</tt:Name></tptz:Preset><tptz:Preset token=\"2\"><tt:Name>B</tt:Name></tptz:Preset></r>";
        let parts = each(xml, "Preset");
        assert_eq!(parts.len(), 2);
        assert_eq!(attr(parts[1], "Preset", "token"), Some("2"));
        assert_eq!(element(parts[1], "Name"), Some("B"));
    }
}
