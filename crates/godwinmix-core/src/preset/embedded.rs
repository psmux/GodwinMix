//! The six official presets, compiled into the binary.
//!
//! `gmx preset apply church` has to work on a machine that downloaded one
//! file, so the presets ship inside it. The table is written out one row at a
//! time rather than pulled in with a directory macro, for the reason `ui.rs`
//! gives about its own: the list is the manifest, and a stray file left in the
//! tree does not silently become part of the product.
//!
//! A directory of the same name in a search path wins over what is here, so an
//! operator editing `presets/church/` sees their edit without a rebuild.

/// `(preset, path relative to the preset root, contents)`.
const FILES: &[(&str, &str, &str)] = &[
    ("default", "README.md", include_str!("../../../../presets/default/README.md")),
    ("default", "config/godwinmix.toml", include_str!("../../../../presets/default/config/godwinmix.toml")),
    ("default", "config/layout.json", include_str!("../../../../presets/default/config/layout.json")),
    ("default", "gmx-plugin.toml", include_str!("../../../../presets/default/gmx-plugin.toml")),
    ("default", "scenes/full.json", include_str!("../../../../presets/default/scenes/full.json")),
    ("default", "scenes/two-box.json", include_str!("../../../../presets/default/scenes/two-box.json")),
    ("church", "README.md", include_str!("../../../../presets/church/README.md")),
    ("church", "config/godwinmix.toml", include_str!("../../../../presets/church/config/godwinmix.toml")),
    ("church", "config/layout.json", include_str!("../../../../presets/church/config/layout.json")),
    ("church", "gmx-plugin.toml", include_str!("../../../../presets/church/gmx-plugin.toml")),
    ("church", "scenes/full.json", include_str!("../../../../presets/church/scenes/full.json")),
    ("church", "scenes/l-shape.json", include_str!("../../../../presets/church/scenes/l-shape.json")),
    ("church", "scenes/pip-bottom-right.json", include_str!("../../../../presets/church/scenes/pip-bottom-right.json")),
    ("church", "scenes/two-box.json", include_str!("../../../../presets/church/scenes/two-box.json")),
    ("church", "theme.css", include_str!("../../../../presets/church/theme.css")),
    ("classroom", "README.md", include_str!("../../../../presets/classroom/README.md")),
    ("classroom", "config/godwinmix.toml", include_str!("../../../../presets/classroom/config/godwinmix.toml")),
    ("classroom", "config/layout.json", include_str!("../../../../presets/classroom/config/layout.json")),
    ("classroom", "gmx-plugin.toml", include_str!("../../../../presets/classroom/gmx-plugin.toml")),
    ("classroom", "scenes/full.json", include_str!("../../../../presets/classroom/scenes/full.json")),
    ("classroom", "scenes/pip-bottom-right.json", include_str!("../../../../presets/classroom/scenes/pip-bottom-right.json")),
    ("classroom", "scenes/split.json", include_str!("../../../../presets/classroom/scenes/split.json")),
    ("classroom", "theme.css", include_str!("../../../../presets/classroom/theme.css")),
    ("esports", "README.md", include_str!("../../../../presets/esports/README.md")),
    ("esports", "config/godwinmix.toml", include_str!("../../../../presets/esports/config/godwinmix.toml")),
    ("esports", "config/layout.json", include_str!("../../../../presets/esports/config/layout.json")),
    ("esports", "gmx-plugin.toml", include_str!("../../../../presets/esports/gmx-plugin.toml")),
    ("esports", "scenes/full.json", include_str!("../../../../presets/esports/scenes/full.json")),
    ("esports", "scenes/multiview.json", include_str!("../../../../presets/esports/scenes/multiview.json")),
    ("esports", "scenes/pip-top-right.json", include_str!("../../../../presets/esports/scenes/pip-top-right.json")),
    ("esports", "scenes/quad.json", include_str!("../../../../presets/esports/scenes/quad.json")),
    ("esports", "theme.css", include_str!("../../../../presets/esports/theme.css")),
    ("headless-agent", "README.md", include_str!("../../../../presets/headless-agent/README.md")),
    ("headless-agent", "config/godwinmix.toml", include_str!("../../../../presets/headless-agent/config/godwinmix.toml")),
    ("headless-agent", "config/layout.json", include_str!("../../../../presets/headless-agent/config/layout.json")),
    ("headless-agent", "gmx-plugin.toml", include_str!("../../../../presets/headless-agent/gmx-plugin.toml")),
    ("headless-agent", "scenes/full.json", include_str!("../../../../presets/headless-agent/scenes/full.json")),
    ("headless-agent", "scenes/pip-bottom-right.json", include_str!("../../../../presets/headless-agent/scenes/pip-bottom-right.json")),
    ("broadcast", "README.md", include_str!("../../../../presets/broadcast/README.md")),
    ("broadcast", "config/godwinmix.toml", include_str!("../../../../presets/broadcast/config/godwinmix.toml")),
    ("broadcast", "config/layout.json", include_str!("../../../../presets/broadcast/config/layout.json")),
    ("broadcast", "gmx-plugin.toml", include_str!("../../../../presets/broadcast/gmx-plugin.toml")),
    ("broadcast", "scenes/full.json", include_str!("../../../../presets/broadcast/scenes/full.json")),
    ("broadcast", "scenes/l-shape.json", include_str!("../../../../presets/broadcast/scenes/l-shape.json")),
    ("broadcast", "scenes/two-box.json", include_str!("../../../../presets/broadcast/scenes/two-box.json")),
    ("broadcast", "theme.css", include_str!("../../../../presets/broadcast/theme.css")),
];

/// One file of one preset, or `None`.
pub fn file(preset: &str, relative: &str) -> Option<&'static str> {
    FILES.iter().find(|(p, f, _)| *p == preset && *f == relative).map(|(_, _, body)| *body)
}

/// Every file directly inside one directory of a preset, as `(name, body)`.
pub fn entries<'a>(
    preset: &'a str,
    directory: &'a str,
) -> impl Iterator<Item = (&'static str, &'static str)> + 'a {
    let prefix = format!("{}/", directory.trim_end_matches('/'));
    FILES.iter().filter_map(move |(p, f, body)| {
        if *p != preset {
            return None;
        }
        let rest = f.strip_prefix(&prefix)?;
        (!rest.contains('/')).then_some((rest, *body))
    })
}

/// Every preset name in the table.
pub fn names() -> Vec<&'static str> {
    let mut out: Vec<&'static str> = Vec::new();
    for (p, _, _) in FILES {
        if !out.contains(p) {
            out.push(p);
        }
    }
    out
}

/// How many bytes of preset the binary carries. Printed by `gmx preset list
/// --json` so the cost of shipping them is visible rather than assumed.
pub fn bytes() -> usize {
    FILES.iter().map(|(_, _, body)| body.len()).sum()
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn every_official_preset_brought_its_manifest_config_layout_and_scenes() {
        for name in super::super::manifest::NAMES {
            assert!(file(name, "gmx-plugin.toml").is_some(), "{name} has no manifest");
            assert!(file(name, "README.md").is_some(), "{name} has no README");
            assert!(file(name, "config/godwinmix.toml").is_some(), "{name} has no config");
            assert!(file(name, "config/layout.json").is_some(), "{name} has no layout");
            assert!(entries(name, "scenes").count() > 0, "{name} ships no scenes");
        }
    }

    #[test]
    fn the_presets_cost_the_binary_less_than_a_hundred_kilobytes() {
        assert!(bytes() < 100 * 1024, "the presets are {} bytes of binary", bytes());
    }
}
