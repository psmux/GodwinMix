//! Finding a plugin's tool from what an agent wants to do.
//!
//! The listing names each plugin's tools; `plugin.describe` has what each one
//! does. A tool is scored on the query's words: a word in its name counts
//! most, one in its own description next, and one only in the plugin's name
//! or description least, so "cut the ATEM" finds `gmx_atem_cut` rather than
//! the first five tools of the ATEM plugin.

use serde_json::{json, Value};

/// Words that say nothing about which tool is meant.
const STOP: &[&str] = &[
    "a", "an", "and", "the", "to", "of", "on", "in", "it", "is", "be", "for", "with", "make", "more", "less", "my", "me", "this", "that", "please", "can", "how", "do", "use",
];

/// The query's words worth matching, cut to a stem so "smoothly" meets
/// "smoother" and "follows" meets "follow".
pub fn words(query: &str) -> Vec<String> {
    query
        .to_lowercase()
        .split(|c: char| !c.is_alphanumeric())
        .filter(|w| w.len() > 1 && !STOP.contains(w))
        .map(|w| w.chars().take(6).collect())
        .collect()
}

/// Where a word sits in one tool: 4 in its name, 2 in its description, 1
/// only in its plugin's name or description, 0 nowhere.
fn place(w: &str, tool: &str, description: &str, plugin: &str) -> f32 {
    if tool.contains(w) {
        4.0
    } else if description.contains(w) {
        2.0
    } else if plugin.contains(w) {
        1.0
    } else {
        0.0
    }
}

/// The best `limit` tools of the plugins described in `plugins`, each given
/// as (plugin name, plugin description, manifest tools), best first. A word
/// that few tools mention counts for more than one most of them do, so
/// "follow the speaker more smoothly" finds the tool about smoothness.
pub fn rank(query: &str, plugins: &[(String, String, Vec<Value>)], limit: usize) -> Vec<Value> {
    let words = words(query);
    let tools: Vec<(String, String, String, String)> = plugins
        .iter()
        .flat_map(|(name, about, tools)| {
            tools.iter().map(move |t| {
                let tool = t["name"].as_str().unwrap_or("").to_string();
                let description = t["description"].as_str().unwrap_or("").to_string();
                (name.clone(), tool.to_lowercase(), description, format!("{name} {about}").to_lowercase())
            })
        })
        .collect();
    let n = tools.len() as f32;
    let weight: Vec<f32> = words
        .iter()
        .map(|w| {
            let hits = tools.iter().filter(|t| place(w, &t.1, &t.2.to_lowercase(), &t.3) > 0.0).count() as f32;
            if hits == 0.0 { 0.0 } else { 1.0 + (n / hits).ln() }
        })
        .collect();
    let mut scored: Vec<(f32, Value)> = tools
        .iter()
        .map(|(name, tool, description, plugin)| {
            let lower = description.to_lowercase();
            let s: f32 = words.iter().zip(&weight).map(|(w, k)| k * place(w, tool, &lower, plugin)).sum();
            let mcp = format!("gmx_{}_{}", name.replace('-', "_"), tool);
            (s, json!({"name": mcp, "description": description, "plugin": name}))
        })
        .filter(|(s, _)| *s > 0.0)
        .collect();
    scored.sort_by(|a, b| b.0.total_cmp(&a.0));
    scored.into_iter().take(limit).map(|(_, v)| v).collect()
}

#[cfg(test)]
mod tests {
    use super::*;

    fn rig() -> Vec<(String, String, Vec<Value>)> {
        let atem = vec![
            json!({"name": "list", "description": "Every switcher this plugin knows."}),
            json!({"name": "state", "description": "One switcher's buses and keyers."}),
            json!({"name": "connect", "description": "Connect to a switcher."}),
            json!({"name": "cut", "description": "Cut programme and preview on an M/E."}),
        ];
        let ptz = vec![
            json!({"name": "camera_list", "description": "Every PTZ camera."}),
            json!({"name": "track_follow", "description": "Follow a person with a camera."}),
            json!({"name": "track_tune", "description": "Read or change how a camera follows: top speed, smoothness, headroom."}),
        ];
        vec![("atem".into(), "Blackmagic ATEM switchers".into(), atem), ("ptz".into(), "PTZ cameras".into(), ptz)]
    }

    #[test]
    fn the_tool_that_does_it_comes_first_not_the_plugins_first_tools() {
        let cut = rank("cut the ATEM switcher", &rig(), 3);
        assert_eq!(cut[0]["name"], "gmx_atem_cut", "{cut:?}");
        let smooth = rank("make a PTZ camera follow the speaker more smoothly", &rig(), 3);
        assert_eq!(smooth[0]["name"], "gmx_ptz_track_tune", "{smooth:?}");
        assert!(rank("render a lower third", &rig(), 3).is_empty(), "nothing matches, nothing is offered");
    }
}
