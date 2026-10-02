//! Putting a source's field values into its template.
//!
//! Each `{{name}}` is replaced by the value escaped for XML, so a headline
//! with `&`, `<` or quotes in it shows those characters and cannot change the
//! picture's structure. A value comes from the source's `params.fields`,
//! then for `accent`, `text` and `panel` from the station's brand, then from
//! the template's default.

use super::brand::BrandConfig;
use super::template::{FieldType, Template, TemplateField};
use super::xml::escape;
use std::collections::BTreeMap;

/// The longest value one field takes, in characters.
pub const MAX_VALUE: usize = 2000;

/// Field values a source set, by name.
pub type Values = BTreeMap<String, String>;

/// A field the template does not have. Carries the template's fields, so a
/// caller can say which it meant.
#[derive(Debug, Clone)]
pub struct UnknownField {
    pub template: String,
    pub field: String,
    pub fields: Vec<String>,
}

impl std::fmt::Display for UnknownField {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        write!(
            f,
            "template {} has no field {:?}. Its fields are: {}. Set one of those in params.fields",
            self.template,
            self.field,
            self.fields.join(", ")
        )
    }
}

impl std::error::Error for UnknownField {}

/// Refuse a value the template could not draw: a field it does not have, a
/// colour that is not one, or a value longer than any screen.
pub fn check(t: &Template, values: &Values) -> anyhow::Result<()> {
    for (name, value) in values {
        let Some(field) = t.field(name) else {
            return Err(UnknownField { template: t.info.name.clone(), field: name.clone(), fields: t.field_names() }.into());
        };
        if value.chars().count() > MAX_VALUE {
            anyhow::bail!("params.fields.{name} is {} characters; a field takes at most {MAX_VALUE}", value.chars().count());
        }
        if field.kind == FieldType::Color {
            crate::plugin::kinds::text::style::colour(value).map_err(|e| anyhow::anyhow!("params.fields.{name}: {e}"))?;
        }
    }
    Ok(())
}

/// What `field` shows, and whether the source set it.
pub fn value_of(field: &TemplateField, values: &Values, brand: &BrandConfig) -> (String, bool) {
    if let Some(v) = values.get(&field.name) {
        return (v.clone(), true);
    }
    let fallback = brand.get(&field.name).map(str::to_string).unwrap_or_else(|| field.default.clone());
    (fallback, false)
}

/// The template's SVG with every marker replaced by its field's value.
pub fn fill(t: &Template, values: &Values, brand: &BrandConfig) -> String {
    let mut out = String::with_capacity(t.svg.len() + 256);
    let mut at = 0;
    for m in super::template::markers(&t.svg) {
        out.push_str(&t.svg[at..m.start]);
        if let Some(field) = t.field(&m.name) {
            out.push_str(&escape(&value_of(field, values, brand).0));
        }
        at = m.end;
    }
    out.push_str(&t.svg[at..]);
    out
}
