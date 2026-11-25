//! Where a new filter goes in an item's chain.
//!
//! A key or a cutout hands the compositor the camera's own frame with an
//! alpha beside it, and keeps a reference to that frame. A filter after it
//! that changes the picture in place is given a copy, grades the copy, and
//! the original goes to air: a look or a colour correction added to a
//! presenter in a virtual set did nothing on air. So a filter that changes
//! the picture goes in front of the first key or cutout, and anything else
//! goes on the end as before.

use super::document::Filter;

/// Filters that make the alpha the compositor draws with.
const KEYS: &[&str] = &["chroma/filter", "matte/filter"];
/// Filters that change the picture's colour in place.
const COLOUR: &[&str] = &["color/filter", "lut/filter"];

/// Put `filter` where it will be seen: before the first key or cutout when
/// it changes colour, at the end otherwise. Answers where it went.
pub fn insert(filters: &mut Vec<Filter>, filter: Filter) -> usize {
    let at = if COLOUR.contains(&filter.kind.as_str()) {
        filters.iter().position(|f| KEYS.contains(&f.kind.as_str())).unwrap_or(filters.len())
    } else {
        filters.len()
    };
    filters.insert(at, filter);
    at
}

#[cfg(test)]
mod tests {
    use super::*;

    fn f(kind: &str) -> Filter {
        Filter { kind: kind.into(), name: None, enabled: true, params: serde_json::json!({}) }
    }

    fn kinds(v: &[Filter]) -> Vec<&str> {
        v.iter().map(|f| f.kind.as_str()).collect()
    }

    #[test]
    fn a_look_goes_in_front_of_a_cutout() {
        let mut v = vec![f("matte/filter")];
        assert_eq!(insert(&mut v, f("lut/filter")), 0);
        assert_eq!(kinds(&v), ["lut/filter", "matte/filter"]);
    }

    #[test]
    fn colour_goes_in_front_of_a_key_but_after_other_colour() {
        let mut v = vec![f("color/filter"), f("chroma/filter")];
        insert(&mut v, f("lut/filter"));
        assert_eq!(kinds(&v), ["color/filter", "lut/filter", "chroma/filter"]);
    }

    #[test]
    fn anything_else_goes_on_the_end() {
        let mut v = vec![f("matte/filter")];
        insert(&mut v, f("chroma/filter"));
        let mut plain = vec![f("color/filter")];
        insert(&mut plain, f("lut/filter"));
        assert_eq!(kinds(&v), ["matte/filter", "chroma/filter"]);
        assert_eq!(kinds(&plain), ["color/filter", "lut/filter"]);
    }
}
