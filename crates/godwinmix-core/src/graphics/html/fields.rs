//! The checks on a template's fields and its way in and out: what the page
//! shows against what it declares, and whether it waits for `.gmx-in`.

use super::meta::Meta;
use super::problem::{error, warning, Problems};
use crate::graphics::template::FieldType;

/// Fields shown and not declared, and declared and never shown.
pub fn fields(page: &str, m: &Meta, out: &mut Problems) {
    for name in data_fields(page) {
        if !m.fields.iter().any(|f| f.name == name) {
            let names: Vec<&str> = m.fields.iter().map(|f| f.name.as_str()).collect();
            out.push(error(
                &format!("data-field=\"{name}\" is on the page but the gmx-template block declares no field {name:?} (it declares: {})", names.join(", ")),
                &format!("add \"{name}\": {{\"label\": \"...\", \"default\": \"...\"}} to \"fields\", or change the attribute to a field that is declared"),
            ));
        }
    }
    for f in &m.fields {
        let n = &f.name;
        let used = [format!("data-field=\"{n}\""), format!("data-field='{n}'"), format!("--{n}"), format!("[\"{n}\"]"), format!("['{n}']")];
        if !used.iter().any(|u| page.contains(u.as_str())) && !property(page, n) {
            let how = if f.kind == FieldType::Color { format!("use it as var(--{n}) in the CSS") } else { format!("show it with data-field=\"{n}\" on the element that holds it") };
            out.push(warning(&format!("the field {n} is declared and the page never uses it"), &how));
        }
    }
}

/// Whether a script reads `name` as a property: `f.name`, `e.detail.name`.
fn property(page: &str, name: &str) -> bool {
    let pat = format!(".{name}");
    page.match_indices(&pat).any(|(at, _)| {
        let after = page[at + pat.len()..].chars().next();
        !after.is_some_and(|c| c.is_ascii_alphanumeric() || c == '_')
    })
}

/// Every `data-field` value on the page, each once.
pub fn data_fields(page: &str) -> Vec<String> {
    let mut names: Vec<String> = Vec::new();
    for quote in ['"', '\''] {
        let pat = format!("data-field={quote}");
        let mut from = 0;
        while let Some(at) = page[from..].find(&pat).map(|i| i + from + pat.len()) {
            let end = page[at..].find(quote).map(|i| i + at).unwrap_or(page.len());
            let name = page[at..end].to_string();
            if !names.contains(&name) {
                names.push(name);
            }
            from = end;
        }
    }
    names
}

/// A graphic with no way in, or a way out with no length.
pub fn motion(page: &str, m: &Meta, out: &mut Problems) {
    let way_in = page.contains("gmx-in") || page.contains("gmx:in");
    let way_out = page.contains("gmx-out") || page.contains("gmx:out");
    if !way_in && !m.opaque {
        out.push(warning(
            "nothing on the page waits for .gmx-in, so the graphic shows the moment it loads, before anyone takes it",
            "hide it by default and show it under .gmx-in, such as .panel { transform: translateX(-110%); transition: transform .5s } .gmx-in .panel { transform: none }",
        ));
    }
    if (way_out || way_in) && m.out_ms.is_none() && !m.opaque {
        out.push(warning(
            "the gmx-template block has no out_ms, so the mixer does not know how long the way out takes",
            "add \"out_ms\": the length of the longest transition or animation under .gmx-out (or of the transition back from .gmx-in), in milliseconds, such as 600",
        ));
    }
}
