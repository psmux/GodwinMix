//! `gmx agent cost`: what an agent pays to look at this mixer.
//!
//! Three tables, because 09 section 5 budgets three things and a budget
//! nobody measures is a hope:
//!
//! 1. `agent.state`, in bytes and tokens, at 2, 6 and 16 sources.
//! 2. The hot tool list per profile, which is charged for on every single
//!    call and which CI fails on a five percent increase of.
//! 3. One snapshot at 320, 640 and 1280 wide, so a director can work out
//!    what looking costs against what reading costs.
//!
//! The tool list needs no mixer: it is a property of the build. The snapshot
//! table needs a running core, because the answer is a real JPEG of a real
//! picture and a made up number would be worse than no number.

use anyhow::{Context, Result};
use godwinmix_protocol::mcp_tools;
use godwinmix_protocol::scope::Profile;
use serde::{Deserialize, Serialize};
use serde_json::{json, Value};

/// Four bytes to a token, which is the proxy the plan uses throughout.
pub const BYTES_PER_TOKEN: usize = 4;
/// Claude's patch based image accounting: about `ceil(w/28) * ceil(h/28)`.
const PATCH: u32 = 28;
/// The widths a snapshot is measured at, with 16:9 heights.
const WIDTHS: [(u32, u32); 3] = [(320, 180), (640, 360), (1280, 720)];
/// The source counts `agent.state` is budgeted at.
const COUNTS: [usize; 3] = [2, 6, 16];
/// The budgets from the brief, in bytes, in the same order.
const BUDGETS: [usize; 3] = [250, 500, 1_200];
/// Where the committed baseline lives, relative to the repository root.
pub const BASELINE: &str = "bench/agent-cost.json";
/// How much the hot list may grow before CI fails. Grafana's number.
pub const TOLERANCE: f64 = 0.05;

#[derive(Debug, Clone, clap::Args)]
pub struct AgentArgs {
    #[command(subcommand)]
    pub cmd: AgentCmd,
}

#[derive(Debug, Clone, clap::Subcommand)]
pub enum AgentCmd {
    /// Print what an agent pays: the state document, the tool list, a picture.
    Cost {
        /// Address of the mixer, for the live document and the snapshots.
        #[arg(long, env = "GODWINMIX_URL")]
        url: Option<String>,
        #[arg(long, env = "GODWINMIX_TOKEN", hide_env_values = true)]
        token: Option<String>,
        /// Write the tool list sizes to `bench/agent-cost.json` as the new
        /// baseline CI compares against. Do this on purpose, never to make a
        /// failing build pass.
        #[arg(long)]
        write_baseline: bool,
        /// Print the tables as JSON instead of as text.
        #[arg(long)]
        json: bool,
    },
}

/// Tokens for a JSON document, at the four bytes to a token the plan uses.
pub fn tokens(bytes: usize) -> usize {
    bytes.div_ceil(BYTES_PER_TOKEN)
}

/// Tokens for an image, by Claude's patch accounting.
pub fn image_tokens(width: u32, height: u32) -> u32 {
    width.div_ceil(PATCH) * height.div_ceil(PATCH)
}

/// The hot tool list size per profile. A pure function of the build, which is
/// what lets CI compare it against a committed number.
pub fn tool_sizes() -> Vec<(Profile, usize, usize)> {
    let registry = crate::control::methods::registry();
    [Profile::Standard, Profile::Minimal]
        .into_iter()
        .map(|p| {
            let tools = mcp_tools::tools(&registry, p);
            (p, tools.len(), mcp_tools::wire_size(&tools))
        })
        .collect()
}

/// The committed baseline: profile name to the hot list's size in bytes.
#[derive(Debug, Clone, Default, Serialize, Deserialize)]
pub struct Baseline {
    /// What the numbers are and how to move them, for whoever opens the file.
    #[serde(default)]
    pub note: String,
    pub tools: std::collections::BTreeMap<String, ToolBaseline>,
}

#[derive(Debug, Clone, Copy, Serialize, Deserialize)]
pub struct ToolBaseline {
    pub count: usize,
    pub bytes: usize,
}

impl Baseline {
    /// What this build measures now.
    pub fn measure() -> Self {
        Self {
            note: format!(
                "The hot MCP tool list, measured by `gmx agent cost`. CI fails on a growth \
                 of more than {} percent against these numbers, because a tool list is \
                 charged for on every call. Move them with `gmx agent cost \
                 --write-baseline` when the growth is deliberate.",
                (TOLERANCE * 100.0) as u64
            ),
            tools: tool_sizes()
                .into_iter()
                .map(|(p, count, bytes)| {
                    (p.as_str().to_string(), ToolBaseline { count, bytes })
                })
                .collect(),
        }
    }

    /// Every profile that has grown past the tolerance, as a sentence each.
    pub fn regressions(&self, now: &Self) -> Vec<String> {
        let mut out = Vec::new();
        for (name, was) in &self.tools {
            let Some(is) = now.tools.get(name) else {
                out.push(format!("the {name} profile is gone from this build"));
                continue;
            };
            let ceiling = (was.bytes as f64 * (1.0 + TOLERANCE)) as usize;
            if is.bytes > ceiling {
                out.push(format!(
                    "the {name} tool list grew from {} to {} bytes, which is over the {} \
                     percent ceiling of {ceiling}. Shorten a description, or move a tool \
                     behind search_tools, or write a new baseline on purpose.",
                    was.bytes,
                    is.bytes,
                    (TOLERANCE * 100.0) as u64
                ));
            }
        }
        out
    }
}

pub async fn run(cmd: AgentCmd) -> Result<()> {
    let AgentCmd::Cost { url, token, write_baseline, json: as_json } = cmd;
    let url = url
        .or_else(|| godwinmix_core::config::env_var("URL"))
        .unwrap_or_else(|| crate::DEFAULT_URL.to_string());
    let token = token.or_else(|| godwinmix_core::config::env_var("TOKEN"));
    let client = reqwest::Client::new();

    let state = state_table(&client, &url, token.as_deref()).await;
    let tools = tool_table();
    let pictures = picture_table(&client, &url, token.as_deref()).await;

    if as_json {
        println!(
            "{}",
            serde_json::to_string_pretty(&json!({
                "agent_state": state,
                "tools": tools,
                "snapshots": pictures,
            }))?
        );
    } else {
        print_tables(&state, &tools, &pictures);
    }

    if write_baseline {
        let path = std::path::Path::new(BASELINE);
        if let Some(dir) = path.parent() {
            std::fs::create_dir_all(dir).ok();
        }
        let text = serde_json::to_string_pretty(&Baseline::measure())? + "\n";
        std::fs::write(path, text).with_context(|| format!("writing {BASELINE}"))?;
        println!("\nwrote {BASELINE}");
    }
    Ok(())
}

/// `agent.state` at 2, 6 and 16 sources, and against the live core.
async fn state_table(client: &reqwest::Client, url: &str, token: Option<&str>) -> Vec<Value> {
    let mut rows: Vec<Value> = COUNTS
        .iter()
        .zip(BUDGETS)
        .map(|(n, budget)| {
            let bytes = synthetic_state(*n).len();
            json!({
                "sources": n,
                "bytes": bytes,
                "tokens": tokens(bytes),
                "budget_bytes": budget,
                "within_budget": bytes < budget,
                "measured": "synthetic",
            })
        })
        .collect();
    if let Some(body) = get(client, url, token, "/api/v1/agent/state").await {
        let sources = serde_json::from_slice::<Value>(&body)
            .ok()
            .and_then(|v| v["sources"].as_array().map(Vec::len))
            .unwrap_or(0);
        rows.push(json!({
            "sources": sources,
            "bytes": body.len(),
            "tokens": tokens(body.len()),
            "measured": "live",
        }));
    }
    rows
}

/// A document of the shape the concise format produces, at `n` sources, with
/// ids of the length real ones have.
fn synthetic_state(n: usize) -> String {
    let sources: Vec<Value> = (1..=n)
        .map(|i| json!({ "id": format!("cam{i}"), "state": "live", "motion": 0.1 }))
        .collect();
    json!({
        "program": "cam1",
        "program_motion": 0.12,
        "uptime_secs": 942,
        "sources": sources,
        "outputs": [{ "id": "yt", "state": "live" }],
        "snapshot": crate::control::methods::agent::SNAPSHOT_URL,
    })
    .to_string()
}

fn tool_table() -> Vec<Value> {
    let baseline = read_baseline();
    tool_sizes()
        .into_iter()
        .map(|(p, count, bytes)| {
            let was = baseline.as_ref().and_then(|b| b.tools.get(p.as_str()).copied());
            json!({
                "profile": p.as_str(),
                "tools": count,
                "bytes": bytes,
                "tokens": tokens(bytes),
                "baseline_bytes": was.map(|w| w.bytes),
                "change_percent": was.map(|w| {
                    ((bytes as f64 - w.bytes as f64) / w.bytes as f64 * 1000.0).round() / 10.0
                }),
            })
        })
        .collect()
}

/// The committed baseline, from the working directory or from the repository
/// this binary was built in, so `gmx agent cost` prints the comparison
/// wherever it is run from inside a checkout.
pub fn read_baseline() -> Option<Baseline> {
    let here = std::path::PathBuf::from(BASELINE);
    let built = std::path::Path::new(env!("CARGO_MANIFEST_DIR"))
        .join("../..")
        .join(BASELINE);
    let text = std::fs::read_to_string(&here)
        .or_else(|_| std::fs::read_to_string(&built))
        .ok()?;
    serde_json::from_str(&text).ok()
}

/// One snapshot at each width, from the live core.
async fn picture_table(client: &reqwest::Client, url: &str, token: Option<&str>) -> Vec<Value> {
    let mut rows = Vec::new();
    for (width, height) in WIDTHS {
        // `force` because the rate limit exists to stop an agent looking every
        // frame, and this is measuring rather than directing.
        let path = format!("/api/v1/snapshot/program?width={width}&force=true");
        let Some(bytes) = get(client, url, token, &path).await else {
            rows.push(json!({
                "width": width,
                "height": height,
                "image_tokens": image_tokens(width, height),
                "bytes": Value::Null,
                "note": "no picture: the core is not answering, or snapshots are off",
            }));
            continue;
        };
        rows.push(json!({
            "width": width,
            "height": height,
            "bytes": bytes.len(),
            "image_tokens": image_tokens(width, height),
        }));
    }
    rows
}

async fn get(
    client: &reqwest::Client,
    url: &str,
    token: Option<&str>,
    path: &str,
) -> Option<Vec<u8>> {
    let mut req = client.get(format!("{}{path}", url.trim_end_matches('/')));
    if let Some(token) = token.filter(|t| !t.is_empty()) {
        req = req.bearer_auth(token);
    }
    let response = req.send().await.ok()?;
    if !response.status().is_success() {
        return None;
    }
    response.bytes().await.ok().map(|b| b.to_vec())
}

fn print_tables(state: &[Value], tools: &[Value], pictures: &[Value]) {
    // `serde_json::Value` ignores a format width, so every cell becomes a
    // string first and the columns line up.
    let cell = |v: &Value| match v {
        Value::String(s) => s.clone(),
        Value::Null => "-".to_string(),
        other => other.to_string(),
    };

    println!("agent.state");
    println!("  {:<9} {:>8} {:>8} {:>8}", "sources", "bytes", "tokens", "budget");
    for row in state {
        let note = match row["measured"].as_str() {
            Some("live") => "live core",
            _ if row["within_budget"] == Value::Bool(false) => "OVER BUDGET",
            _ => "",
        };
        println!(
            "  {:<9} {:>8} {:>8} {:>8}  {note}",
            cell(&row["sources"]),
            cell(&row["bytes"]),
            cell(&row["tokens"]),
            cell(&row["budget_bytes"]),
        );
    }

    println!("\nMCP hot tool list");
    println!("  {:<9} {:>6} {:>8} {:>8} {:>9}", "profile", "tools", "bytes", "tokens", "vs base");
    for row in tools {
        let change = row["change_percent"]
            .as_f64()
            .map(|p| format!("{p:+.1}%"))
            .unwrap_or_else(|| "-".into());
        println!(
            "  {:<9} {:>6} {:>8} {:>8} {:>9}",
            cell(&row["profile"]),
            cell(&row["tools"]),
            cell(&row["bytes"]),
            cell(&row["tokens"]),
            change
        );
    }

    println!("\nOne snapshot");
    println!("  {:<11} {:>8} {:>12}", "size", "bytes", "image tokens");
    for row in pictures {
        println!(
            "  {:<11} {:>8} {:>12}  {}",
            format!("{}x{}", cell(&row["width"]), cell(&row["height"])),
            cell(&row["bytes"]),
            cell(&row["image_tokens"]),
            row["note"].as_str().unwrap_or_default()
        );
    }
    println!(
        "\nA look at 1280 costs about {} times a read of agent.state at 6 sources.",
        image_tokens(1280, 720) as usize / tokens(synthetic_state(6).len()).max(1)
    );
}

#[cfg(test)]
mod tests {
    use super::*;

    /// The CI gate from 07 Phase 2: the hot list may not grow by more than
    /// five percent against the committed baseline without somebody saying so.
    ///
    /// The baseline is included at compile time rather than read from disk,
    /// so the test runs the same in CI, in a package and from a worktree.
    #[test]
    fn the_hot_tool_list_has_not_grown_past_its_baseline() {
        const COMMITTED: &str = include_str!("../../../../bench/agent-cost.json");
        let baseline: Baseline = serde_json::from_str(COMMITTED).expect("bench/agent-cost.json");
        let now = Baseline::measure();
        let regressions = baseline.regressions(&now);
        assert!(regressions.is_empty(), "{}", regressions.join("\n"));
    }

    #[test]
    fn a_growth_inside_the_tolerance_passes_and_one_over_it_does_not() {
        let was = Baseline {
            note: String::new(),
            tools: [("standard".to_string(), ToolBaseline { count: 12, bytes: 10_000 })]
                .into_iter()
                .collect(),
        };
        let small = Baseline {
            note: String::new(),
            tools: [("standard".to_string(), ToolBaseline { count: 12, bytes: 10_400 })]
                .into_iter()
                .collect(),
        };
        assert!(was.regressions(&small).is_empty(), "four percent is inside the tolerance");

        let big = Baseline {
            note: String::new(),
            tools: [("standard".to_string(), ToolBaseline { count: 13, bytes: 11_000 })]
                .into_iter()
                .collect(),
        };
        let found = was.regressions(&big);
        assert_eq!(found.len(), 1);
        assert!(found[0].contains("10000") && found[0].contains("11000"), "{}", found[0]);
        assert!(found[0].contains("search_tools"), "the message names a way out: {}", found[0]);

        // A profile that disappeared is a regression too.
        let gone = Baseline { note: String::new(), tools: Default::default() };
        assert_eq!(was.regressions(&gone).len(), 1);
    }

    #[test]
    fn a_picture_is_counted_the_way_the_model_counts_it() {
        // 09 section 5 item 12 and 05 section 6: 320x180 is 84 tokens, 640x360
        // about 300, 1280x720 about 1,200.
        assert_eq!(image_tokens(320, 180), 84);
        assert_eq!(image_tokens(640, 360), 299);
        assert_eq!(image_tokens(1280, 720), 1_196);
        assert_eq!(tokens(400), 100);
        assert_eq!(tokens(401), 101, "a part token is a token");
    }

    /// The synthetic document is the shape the concise format produces, so
    /// the table is not measuring something nobody sends.
    #[test]
    fn the_measured_document_is_inside_the_budget_at_every_count() {
        for (n, budget) in COUNTS.iter().zip(BUDGETS) {
            let bytes = synthetic_state(*n).len();
            assert!(bytes < budget, "at {n} sources: {bytes} bytes, budget {budget}");
        }
    }
}
