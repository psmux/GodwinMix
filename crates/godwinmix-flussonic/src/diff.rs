//! What a changed config asks for: only the streams that changed restart.

use crate::import::ShowPlan;
use std::collections::BTreeMap;

/// The steps from the shows that run to the shows a config wants.
#[derive(Debug, Default, PartialEq)]
pub struct Diff {
    /// New streams.
    pub add: Vec<ShowPlan>,
    /// Streams gone from the config, by name.
    pub remove: Vec<String>,
    /// Streams whose input or output changed: made again.
    pub restart: Vec<ShowPlan>,
    /// Only `disabled` moved: stopped or started, nothing else.
    pub toggle: Vec<ShowPlan>,
    /// Only the title moved: renamed in place.
    pub retitle: Vec<ShowPlan>,
}

impl Diff {
    pub fn is_empty(&self) -> bool {
        self.add.is_empty() && self.remove.is_empty() && self.restart.is_empty() && self.toggle.is_empty() && self.retitle.is_empty()
    }
}

/// From `running` (by name) to `wanted`.
pub fn diff(running: &BTreeMap<String, ShowPlan>, wanted: &[ShowPlan]) -> Diff {
    let mut d = Diff::default();
    for w in wanted {
        let Some(r) = running.get(&w.name) else {
            d.add.push(w.clone());
            continue;
        };
        if !r.same_media(w) {
            d.restart.push(w.clone());
            continue;
        }
        if r.disabled != w.disabled {
            d.toggle.push(w.clone());
        }
        if r.title != w.title {
            d.retitle.push(w.clone());
        }
    }
    d.remove = running.keys().filter(|n| !wanted.iter().any(|w| &w.name == *n)).cloned().collect();
    d
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::import::import;
    use crate::Document;

    fn shows(text: &str) -> Vec<ShowPlan> {
        import(&Document::parse(text).unwrap().config).shows
    }

    #[test]
    fn only_what_changed_moves() {
        let before = "stream A { input rtmp://h/a; }\nstream B { input rtmp://h/b; }\nstream C { input rtmp://h/c; }\nstream D { input rtmp://h/d; }\n";
        let after = "stream A { input rtmp://h/a; }\nstream B { input rtmp://h/b2; }\nstream C { disabled true; title Sea; input rtmp://h/c; }\nstream E { input rtmp://h/e; }\n";
        let running: BTreeMap<String, ShowPlan> = shows(before).into_iter().map(|s| (s.name.clone(), s)).collect();
        let d = diff(&running, &shows(after));
        let names = |v: &[ShowPlan]| v.iter().map(|s| s.name.clone()).collect::<Vec<_>>();
        assert_eq!(names(&d.add), ["E"]);
        assert_eq!(d.remove, ["D"]);
        assert_eq!(names(&d.restart), ["B"]);
        assert_eq!(names(&d.toggle), ["C"]);
        assert_eq!(names(&d.retitle), ["C"]);
        assert!(diff(&running, &shows(before)).is_empty());
    }
}
