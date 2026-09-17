//! The plugins the core can reach but does not have.
//!
//! A node reports every plugin installed on it. The core does not have those
//! plugins and never will: the binary is on the other machine. But the picker
//! has to offer them, `plugin.describe` has to answer about them, and
//! `core.api` has to list their kinds, or a remote plugin is exactly the
//! second class citizen this whole design exists to avoid.
//!
//! So the manifests are interned here, tagged with the node they came from, in
//! a table beside the loader's own. The loader's registry is not touched: an
//! installed plugin and a reachable one are different things and conflating
//! them would make `plugin.remove` ambiguous. Lookups try the loader first, so
//! a plugin that is installed locally always wins and a config written against
//! `ndi/source` means the same thing whichever machine ends up running it.

use super::Manifest;
use godwinmix_protocol::plugin::manifest::Manifest as PluginManifest;
use parking_lot::RwLock;
use std::collections::BTreeMap;
use std::sync::OnceLock;

/// One plugin, on one node.
#[derive(Debug, Clone)]
pub struct Reachable {
    pub node: String,
    pub manifest: PluginManifest,
}

#[derive(Default)]
struct Table {
    /// `<node>` to the plugins on it.
    by_node: BTreeMap<String, Vec<PluginManifest>>,
    /// `<plugin>/<provide>` to the interned manifest, leaked once.
    interned: BTreeMap<String, &'static Manifest>,
    /// `<plugin>/<provide>` to its settings schema, as the node sent it.
    schemas: BTreeMap<String, serde_json::Value>,
}

fn table() -> &'static RwLock<Table> {
    static TABLE: OnceLock<RwLock<Table>> = OnceLock::new();
    TABLE.get_or_init(Default::default)
}

/// A node has said what it has. Replaces whatever it said before.
pub fn learn(
    node: &str,
    plugins: Vec<PluginManifest>,
    schemas: BTreeMap<String, serde_json::Value>,
) {
    let mut table = table().write();
    table.schemas.extend(schemas);
    for plugin in &plugins {
        for decl in &plugin.provides {
            let type_id = format!("{}/{}", plugin.plugin.name, decl.id);
            if table.interned.contains_key(&type_id) {
                continue;
            }
            // Leaked once per provide, for the life of the process, the same
            // way the loader interns a local one. A node reconnecting with the
            // same plugins adds nothing.
            let manifest: &'static Manifest =
                Box::leak(Box::new(super::loader::manifest_of_at(plugin, decl, super::Tier::Node)));
            table.interned.insert(type_id, manifest);
        }
    }
    let offered: Vec<String> = plugins
        .iter()
        .flat_map(|p| p.provides.iter().map(move |d| format!("{}/{}", p.plugin.name, d.id)))
        .collect();
    let had = table.by_node.insert(node.to_string(), plugins).map(|old| old.len());
    tracing::info!(
        node,
        offers = offered.len(),
        offered = %offered.join(", "),
        replaced = had.unwrap_or(0),
        "a node's plugins are reachable"
    );
}

/// A node has gone, or was removed. Its plugins stop being offered.
///
/// The interned manifests stay: they are `&'static` and something may still
/// hold one while a source is torn down. What goes is the node's claim to
/// offer them, which is what the picker and the reconciler read.
pub fn forget(node: &str) {
    let gone = table().write().by_node.remove(node);
    match gone {
        Some(plugins) => tracing::info!(
            node,
            plugins = plugins.len(),
            "a node's plugins stopped being reachable"
        ),
        None => tracing::debug!(node, "a node with nothing reachable was forgotten"),
    }
}

/// The interned manifest for a provide some node has.
pub fn manifest(type_id: &str) -> Option<&'static Manifest> {
    table().read().interned.get(type_id).copied()
}

/// The settings schema for a provide some node has, as that node sent it.
pub fn schema(type_id: &str) -> Option<serde_json::Value> {
    table().read().schemas.get(type_id).cloned()
}

/// Every plugin reachable on any node, with the node it is on, newest listing
/// per plugin name wins. For `plugin.list`.
pub fn plugins() -> Vec<Reachable> {
    let mut out = Vec::new();
    for (node, plugins) in &table().read().by_node {
        for manifest in plugins {
            out.push(Reachable { node: node.clone(), manifest: manifest.clone() });
        }
    }
    out.sort_by(|a, b| a.manifest.plugin.name.cmp(&b.manifest.plugin.name));
    out.dedup_by(|a, b| a.manifest.plugin.name == b.manifest.plugin.name);
    out
}

/// The whole `gmx-plugin.toml` for a provide, from whichever node has it.
///
/// `node` narrows it to one machine, which is what a source with
/// `place = "node:studio-b"` wants: two nodes may have different versions of
/// the same plugin and the answer has to be the one that will actually run.
pub fn plugin_manifest(type_id: &str, node: Option<&str>) -> Option<PluginManifest> {
    let (name, _) = type_id.split_once('/')?;
    let table = table().read();
    let search: Box<dyn Iterator<Item = (&String, &Vec<PluginManifest>)>> = match node {
        Some(node) => Box::new(table.by_node.get_key_value(node).into_iter()),
        None => Box::new(table.by_node.iter()),
    };
    for (_, plugins) in search {
        if let Some(found) = plugins.iter().find(|p| p.plugin.name == name) {
            return Some(found.clone());
        }
    }
    None
}

/// Which nodes can run this provide.
pub fn nodes_with(type_id: &str) -> Vec<String> {
    let Some((name, id)) = type_id.split_once('/') else { return Vec::new() };
    table()
        .read()
        .by_node
        .iter()
        .filter(|(_, plugins)| {
            plugins
                .iter()
                .any(|p| p.plugin.name == name && p.provides.iter().any(|d| d.id == id))
        })
        .map(|(node, _)| node.clone())
        .collect()
}

/// Every provide reachable on any node, as `<plugin>/<provide>` with the node.
pub fn reachable() -> Vec<(String, String)> {
    let mut out = Vec::new();
    for (node, plugins) in &table().read().by_node {
        for plugin in plugins {
            for decl in &plugin.provides {
                out.push((format!("{}/{}", plugin.plugin.name, decl.id), node.clone()));
            }
        }
    }
    out.sort();
    out.dedup();
    out
}

/// Everything one node offers, for `node.get`.
pub fn on_node(node: &str) -> Vec<PluginManifest> {
    table().read().by_node.get(node).cloned().unwrap_or_default()
}

/// Forget everything. Tests only: the table is process wide.
#[cfg(test)]
pub fn clear() {
    let mut table = table().write();
    table.by_node.clear();
    table.schemas.clear();
}

#[cfg(test)]
mod tests {
    use super::*;

    fn manifest_toml(name: &str) -> PluginManifest {
        toml::from_str(&format!(
            r#"
[plugin]
name = "{name}"
version = "1.0.0"
api = 1
description = "A plugin that lives on another machine, for the test."
license = "Apache-2.0"
platforms = ["linux-x86_64"]
placements = ["sidecar", "node"]

[run]
bin = {{ "linux-x86_64" = "bin/x" }}

[[provides]]
kind = "source"
id = "source"
media = {{ video = "container", audio = "container" }}
transports = ["container"]
"#
        ))
        .unwrap()
    }

    /// One test, not two. The table is process wide, as the loader's is, so
    /// two tests writing to it in parallel would each see the other's nodes.
    #[test]
    fn a_nodes_plugins_become_reachable_and_stop_being_so() {
        clear();
        learn("studio-b", vec![manifest_toml("ndi")], Default::default());
        assert_eq!(nodes_with("ndi/source"), vec!["studio-b".to_string()]);
        assert!(manifest("ndi/source").is_some());
        assert_eq!(plugin_manifest("ndi/source", Some("studio-b")).unwrap().plugin.name, "ndi");
        assert!(plugin_manifest("ndi/source", Some("elsewhere")).is_none());
        assert_eq!(reachable(), vec![("ndi/source".to_string(), "studio-b".to_string())]);
        assert_eq!(on_node("studio-b").len(), 1);

        // A second node with the same plugin is a second place to run it.
        learn("graphics-pc", vec![manifest_toml("ndi")], Default::default());
        let mut both = nodes_with("ndi/source");
        both.sort();
        assert_eq!(both, vec!["graphics-pc".to_string(), "studio-b".to_string()]);

        forget("studio-b");
        assert_eq!(nodes_with("ndi/source"), vec!["graphics-pc".to_string()]);
        forget("graphics-pc");
        assert!(reachable().is_empty());
        // The interned manifest survives, because something may still hold it.
        assert!(manifest("ndi/source").is_some());
        assert_eq!(manifest("ndi/source").unwrap().tier, super::super::Tier::Node);
        clear();
    }
}
