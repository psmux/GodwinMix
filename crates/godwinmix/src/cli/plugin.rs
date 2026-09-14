//! `gmx plugin new|test|add|remove|enable|disable|update|reload|list|describe|stats|bisect`.
//!
//! The developer path. Two of these need no mixer at all (`new` writes a
//! template, `test` runs the harness against a directory) and the rest are
//! thin clients of the `plugin.*` methods, which is the same contract a third
//! party surface calls.
//!
//! `bisect` is the one worth explaining. When a core misbehaves with thirty
//! plugins installed, the question is which one, and the answer is a binary
//! search: disable half, run the check, and keep the half that still fails.
//! Six steps finds a bad plugin among thirty two, which is what VS Code's
//! Extension Bisect does and why it is in the developer path rather than in a
//! support document.

use crate::ctl::Api;
use anyhow::{Context, Result};
use clap::Subcommand;
use serde_json::{json, Value};
use std::path::{Path, PathBuf};

#[derive(Subcommand, Debug)]
pub enum Plugin {
    /// Write a new plugin from a template, ready to run.
    New {
        /// The plugin's name. A slug: it becomes the namespace of every id it
        /// contributes.
        name: String,
        /// What it provides.
        #[arg(long, default_value = "source")]
        kind: String,
        /// What to write it in.
        #[arg(long, default_value = "python")]
        lang: String,
        /// Where to put it. Defaults to a directory named after the plugin.
        #[arg(long)]
        out: Option<PathBuf>,
        /// One sentence saying what it does. It is what a person and a model
        /// both read first in the index.
        #[arg(long)]
        description: Option<String>,
        /// An SPDX id.
        #[arg(long, default_value = "MIT")]
        license: String,
        /// Whose name goes in the manifest.
        #[arg(long)]
        author: Option<String>,
    },
    /// Run the conformance harness against a plugin directory.
    ///
    /// The full run takes about a minute; `--quick` skips the kill test and
    /// the footprint and takes about fifteen seconds. `--offline` needs no
    /// core at all and replays a recorded transcript against the binary.
    Test {
        /// The plugin directory, the one with gmx-plugin.toml at its root.
        #[arg(default_value = ".")]
        dir: PathBuf,
        /// Skip checks 6 and 8, the two that take time.
        #[arg(long)]
        quick: bool,
        /// Replay `tests/transcript.jsonl` against the plugin binary with no
        /// core running, and assert the replies.
        #[arg(long)]
        offline: bool,
        /// Which provide to check. Defaults to the first source it declares.
        #[arg(long)]
        provide: Option<String>,
    },
    /// Install a plugin, while live, from anywhere it can come from.
    ///
    /// Seven forms, and the first one is the one to reach for:
    ///
    ///   gmx plugin add ndi                     a name a marketplace knows
    ///   gmx plugin add psmux/gmx-ndi           a GitHub release, @version to pin
    ///   gmx plugin add https://x/y.git         a git clone, built here
    ///   gmx plugin add cargo:gmx-ndi           a crate
    ///   gmx plugin add npm:@x/gmx-chat         an npm package
    ///   gmx plugin add pypi:gmx-director       a PyPI package
    ///   gmx plugin add ./my-plugin             a directory you are working in
    ///
    /// The signature and the api level are checked before anything is copied.
    Add {
        /// Where the plugin comes from.
        source: String,
    },
    /// Find a plugin in the marketplaces this mixer knows.
    Search {
        /// A word to look for in a name, a description or a kind. Leave it out
        /// to list everything.
        #[arg(default_value = "")]
        term: String,
        #[arg(long)]
        json: bool,
    },
    /// Uninstall a plugin and unwind everything it registered.
    Remove { name: String },
    /// Turn a plugin on.
    Enable { name: String },
    /// Turn a plugin off without uninstalling it.
    Disable { name: String },
    /// Read a plugin's directory again and swap its running instances.
    Reload { name: String },
    /// Fetch a newer build, prove it starts, and swap it in.
    ///
    /// The new build is installed beside the one that is running and has ten
    /// seconds to answer `initialize`. One that does not is rolled back, and
    /// the version that was working is the version that is still working.
    Update {
        name: String,
        /// Where the new version is, in any of the forms `add` takes.
        /// Defaults to wherever this plugin was installed from.
        source: Option<String>,
    },
    /// Every plugin installed, with what each instance costs.
    List {
        #[arg(long)]
        json: bool,
    },
    /// One plugin in full: manifest, schemas and skill descriptions.
    Describe {
        name: String,
        #[arg(long)]
        json: bool,
    },
    /// Per instance cpu, memory, latency, dropped buffers and restarts.
    Stats {
        #[arg(long)]
        json: bool,
    },
    /// Find the plugin that breaks a check, by binary search.
    ///
    /// Runs `--check` with half the plugins disabled and keeps the half that
    /// still fails. A bad one among 32 takes at most 6 steps. Every plugin is
    /// put back the way it was found, whatever happens.
    Bisect {
        /// A command that exits non-zero when the fault is present.
        #[arg(long)]
        check: String,
    },
}

pub async fn run(base: &str, token: Option<&str>, cmd: Plugin) -> Result<()> {
    match cmd {
        // The two that need no mixer.
        Plugin::New { name, kind, lang, out, description, license, author } => {
            let at = out.unwrap_or_else(|| PathBuf::from(&name));
            let fields = Fields {
                name: name.clone(),
                kind: kind.clone(),
                description: description.unwrap_or_else(|| {
                    format!("A GodwinMix {kind} called {name}. Say here what it does.")
                }),
                license: license.clone(),
                author: author.unwrap_or_else(whoami),
            };
            return new_plugin(&lang, &at, &fields);
        }
        Plugin::Test { dir, quick, offline, provide } => {
            return test_plugin(&dir, quick, offline, provide.as_deref());
        }
        _ => {}
    }
    let api = Api::new(base, token)?;
    match cmd {
        Plugin::Add { source } => {
            let record = add_and_wait(&api, &source).await?;
            print_added(&record);
        }
        Plugin::Update { name, source } => {
            let updated = update_and_wait(&api, &name, source.as_deref()).await?;
            println!(
                "updated {name}: {} -> {} (said hello in {} ms)",
                updated["from"].as_str().unwrap_or("?"),
                updated["to"].as_str().unwrap_or("?"),
                updated["handshake_ms"].as_u64().unwrap_or(0)
            );
            print_added(&updated["plugin"]);
        }
        Plugin::Search { term, json } => {
            let found: Value = api
                .get("plugin.search", None, &[("term", term.clone())])
                .await?;
            if json {
                println!("{}", serde_json::to_string_pretty(&found)?);
            } else {
                print_search(&term, &found);
            }
        }
        Plugin::Remove { name } => {
            let gone: Value = api.call("plugin.remove", Some(&name), &json!({})).await?;
            println!("removed {}", gone["removed"].as_str().unwrap_or(&name));
            let provides = gone["provides"].as_array().map(Vec::len).unwrap_or(0);
            let tools = gone["tools"].as_array().map(Vec::len).unwrap_or(0);
            println!("  unregistered {provides} provide(s) and {tools} tool(s)");
        }
        Plugin::Enable { name } => {
            api.call::<_, Value>("plugin.enable", Some(&name), &json!({})).await?;
            println!("{name} is on");
        }
        Plugin::Disable { name } => {
            api.call::<_, Value>("plugin.disable", Some(&name), &json!({})).await?;
            println!("{name} is off. Its provides are unregistered and its processes stopped.");
        }
        Plugin::Reload { name } => {
            let record: Value = api.call("plugin.reload", Some(&name), &json!({})).await?;
            println!("reloaded {name} v{}", record["version"].as_str().unwrap_or("?"));
        }
        Plugin::List { json } => {
            let listing: Value = api.get("plugin.list", None, &[]).await?;
            if json {
                println!("{}", serde_json::to_string_pretty(&listing)?);
            } else {
                print_list(&listing);
            }
        }
        Plugin::Describe { name, json } => {
            let described: Value = api.get("plugin.describe", Some(&name), &[]).await?;
            if json {
                println!("{}", serde_json::to_string_pretty(&described)?);
            } else {
                print_described(&described);
            }
        }
        Plugin::Stats { json } => {
            let stats: Value = api.get("plugin.stats", None, &[]).await?;
            if json {
                println!("{}", serde_json::to_string_pretty(&stats)?);
            } else {
                print_stats(stats["instances"].as_array().unwrap_or(&Vec::new()));
            }
        }
        Plugin::Bisect { check } => return bisect(&api, &check).await,
        Plugin::New { .. } | Plugin::Test { .. } => unreachable!("handled above"),
    }
    Ok(())
}

/// Install, and wait for the task to finish.
///
/// `plugin.add` answers with a task handle because a plugin with a virtual
/// environment to build can take longer than a call is allowed to. A person at
/// a terminal wants the answer, so this polls until it has one.
async fn add_and_wait(api: &Api, source: &str) -> Result<Value> {
    let started: Value =
        api.call("plugin.add", None, &json!({ "source": resolve_locally(source)? })).await?;
    wait_for(api, started, "install").await
}

/// The same, for `plugin.update`.
async fn update_and_wait(api: &Api, name: &str, source: Option<&str>) -> Result<Value> {
    let mut params = json!({ "id": name });
    if let Some(source) = source {
        params["source"] = Value::String(resolve_locally(source)?);
    }
    let started: Value = api.call("plugin.update", Some(name), &params).await?;
    wait_for(api, started, "update").await
}

/// Poll a task handle until it has an answer.
async fn wait_for(api: &Api, started: Value, what: &str) -> Result<Value> {
    // A core that answered outright rather than with a handle: take it.
    let Some(task_id) = started["task_id"].as_str().map(str::to_string) else {
        return Ok(started);
    };
    let wait = started["poll_interval_ms"].as_u64().unwrap_or(200).clamp(50, 2_000);
    let deadline = std::time::Instant::now() + std::time::Duration::from_secs(600);
    loop {
        tokio::time::sleep(std::time::Duration::from_millis(wait)).await;
        // `task.get` is a singleton route with no `{id}` in its path, so the
        // id travels as a query parameter rather than as a path segment.
        let task: Value =
            api.get("task.get", None, &[("task_id", task_id.clone())]).await?;
        match task["state"].as_str().unwrap_or("running") {
            "completed" => return Ok(task["result"].clone()),
            "failed" => anyhow::bail!(
                "{}",
                task["error"]
                    .as_str()
                    .unwrap_or("it failed and said nothing, which is a bug worth reporting")
            ),
            "cancelled" => anyhow::bail!("the {what} was cancelled"),
            _ => {}
        }
        anyhow::ensure!(
            std::time::Instant::now() < deadline,
            "the {what} is still running after ten minutes. It has not been cancelled; \
             read it back with `gmx ctl ... task.get {task_id}`."
        );
    }
}

/// Make a path source absolute, and leave every other form alone.
///
/// The core resolves the source itself, and it may be on another machine, so a
/// relative path has to become the one the operator meant before it is sent. A
/// `owner/repo` or a `cargo:` spec means the same thing on both ends and
/// travels unchanged.
fn resolve_locally(source: &str) -> Result<String> {
    let parsed = godwinmix_host::sources::Source::parse(source);
    let Ok(godwinmix_host::sources::Source::Path(path)) = parsed else {
        return Ok(source.to_string());
    };
    let full = if path.is_absolute() {
        path
    } else {
        std::env::current_dir().context("reading the working directory")?.join(path)
    };
    Ok(full.to_string_lossy().into_owned())
}

fn print_search(term: &str, found: &Value) {
    let results = found["results"].as_array().cloned().unwrap_or_default();
    let markets = found["marketplaces"]
        .as_array()
        .map(|m| m.iter().filter_map(Value::as_str).collect::<Vec<_>>().join(", "))
        .unwrap_or_default();
    if markets.is_empty() {
        println!("no marketplaces are configured on this mixer, so there is nothing to search.");
        println!("\nAdd one:  gmx marketplace add psmux/godwinmix-plugins");
        return;
    }
    if results.is_empty() {
        println!("nothing in {markets} matches `{term}`.");
        println!("\nThe listing is a cached copy; `gmx marketplace refresh` fetches it again.");
        return;
    }
    println!("{:<16} {:<9} {:<8} {:<26} DESCRIPTION", "PLUGIN", "VERSION", "TIER", "SOURCE");
    for r in &results {
        let mark = if r["installed"].as_bool().unwrap_or(false) { " (installed)" } else { "" };
        println!(
            "{:<16} {:<9} {:<8} {:<26} {}{}",
            r["name"].as_str().unwrap_or("?"),
            r["version"].as_str().unwrap_or("-"),
            r["tier"].as_str().unwrap_or("custom"),
            r["source"].as_str().unwrap_or("?"),
            r["description"].as_str().unwrap_or(""),
            mark
        );
    }
    println!("\nsearched {markets}");
    println!("Install one with:  gmx plugin add <plugin>");
}

fn print_added(record: &Value) {
    println!(
        "installed {} v{}",
        record["name"].as_str().unwrap_or("?"),
        record["version"].as_str().unwrap_or("?")
    );
    if let Some(provides) = record["provides"].as_array() {
        for id in provides {
            println!("  provides {}", id.as_str().unwrap_or(""));
        }
        if !provides.is_empty() {
            println!(
                "\nAdd one with:  gmx source add <id> --type {}",
                provides[0].as_str().unwrap_or("")
            );
        }
    }
}

fn print_list(listing: &Value) {
    let plugins = listing["plugins"].as_array().cloned().unwrap_or_default();
    if plugins.is_empty() {
        println!("no plugins installed");
        println!("they are read from {}", listing["plugins_dir"].as_str().unwrap_or("?"));
        println!("write one with:  gmx plugin new --lang python my-plugin");
        return;
    }
    println!(
        "{:<16} {:<9} {:<8} {:<20} PROVIDES",
        "PLUGIN", "VERSION", "STATE", "TRUST"
    );
    for p in &plugins {
        let state = if p["problem"].is_string() {
            "broken"
        } else if p["enabled"].as_bool().unwrap_or(false) {
            "on"
        } else {
            "off"
        };
        println!(
            "{:<16} {:<9} {:<8} {:<20} {}",
            p["name"].as_str().unwrap_or("?"),
            p["version"].as_str().unwrap_or("?"),
            state,
            p["trust"].as_str().unwrap_or("custom, unreviewed"),
            p["provides"]
                .as_array()
                .map(|v| v.iter().filter_map(Value::as_str).collect::<Vec<_>>().join(", "))
                .unwrap_or_default()
        );
        if let Some(problem) = p["problem"].as_str() {
            for line in problem.lines() {
                println!("    {line}");
            }
        }
        let instances = p["instances"].as_array().cloned().unwrap_or_default();
        if !instances.is_empty() {
            print_stats(&instances);
        }
    }
    println!("\nread from {}", listing["plugins_dir"].as_str().unwrap_or("?"));
}

fn print_stats(instances: &[Value]) {
    for i in instances {
        println!(
            "    {:<14} {:<10} cpu {:<6} rss {:<8} latency {:<7} dropped {:<5} restarts {}",
            i["instance"].as_str().unwrap_or("?"),
            i["state"].as_str().unwrap_or("?"),
            i["cpu_percent"].as_f64().map(|v| format!("{v:.0}%")).unwrap_or_else(|| "-".into()),
            i["rss_bytes"]
                .as_u64()
                .map(|v| format!("{} MB", v / (1024 * 1024)))
                .unwrap_or_else(|| "-".into()),
            i["media_latency_ms"]
                .as_u64()
                .map(|v| format!("{v} ms"))
                .unwrap_or_else(|| "-".into()),
            i["buffers_dropped"].as_u64().unwrap_or(0),
            i["restarts"].as_u64().unwrap_or(0),
        );
    }
}

fn print_described(described: &Value) {
    println!(
        "{} v{}",
        described["name"].as_str().unwrap_or("?"),
        described["version"].as_str().unwrap_or("?")
    );
    if let Some(d) = described["description"].as_str() {
        println!("{d}");
    }
    println!("\ninstalled at {}", described["root"].as_str().unwrap_or("?"));
    if let Some(source) = described["source"].as_str().filter(|s| !s.is_empty()) {
        println!("from         {source}");
    }
    println!("trust        {}", described["trust"].as_str().unwrap_or("custom, unreviewed"));
    if let Some(detail) = described["trust_detail"].as_str() {
        println!("             {detail}");
    }
    if let Some(schemas) = described["schemas"].as_object() {
        for (id, schema) in schemas {
            println!("\n{id}");
            if let Some(properties) = schema["properties"].as_object() {
                for (key, property) in properties {
                    println!(
                        "  {key:<16} {:<10} {}",
                        property["type"].as_str().unwrap_or(""),
                        property["description"].as_str().unwrap_or("")
                    );
                }
            }
        }
    }
    if let Some(skills) = described["skills"].as_object() {
        for (id, skill) in skills {
            println!("\nskill for {id}: {}", skill["description"].as_str().unwrap_or(""));
        }
    }
    if let Some(tools) = described["tools"].as_array().filter(|t| !t.is_empty()) {
        println!("\ntools (find them with search_tools; they are never in the hot list):");
        for t in tools {
            println!("  {}", t.as_str().unwrap_or(""));
        }
    }
}

// ---------------------------------------------------------------------------
// The two that need no mixer
// ---------------------------------------------------------------------------

/// `gmx plugin test`. Runs in this process, against a directory.
fn test_plugin(dir: &Path, quick: bool, offline: bool, provide: Option<&str>) -> Result<()> {
    let dir = dir.canonicalize().with_context(|| format!("there is no {}", dir.display()))?;
    if offline {
        return test_offline(&dir, provide);
    }
    let _ = gstreamer::init();
    println!("godwinmix test core: 1280x720x30, no outputs, no multiview");
    println!("{}\n", dir.display());
    let report = godwinmix_core::plugin::harness::check_plugin(&dir, quick)?;
    for line in report.lines() {
        println!("  {line}");
    }
    println!();
    if !report.passed() {
        anyhow::bail!(
            "{} did not pass. Each FAIL above names what was wanted; \
             docs/how-to/test-a-plugin.md says what to do about each check.",
            report.type_id
        );
    }
    println!("{} is conformant", report.type_id);
    Ok(())
}

/// `gmx plugin test --offline`: a transcript, the plugin binary, no core.
fn test_offline(dir: &Path, provide: Option<&str>) -> Result<()> {
    let transcript_path = dir.join("tests").join("transcript.jsonl");
    let transcript = std::fs::read_to_string(&transcript_path).with_context(|| {
        format!(
            "there is no {}. An offline test is a recorded transcript: one JSON object a \
             line, each with a single key, 'core' for a line the core sends and 'plugin' \
             for one the plugin must send. `gmx plugin new` writes a starting one.",
            transcript_path.display()
        )
    })?;
    let manifest =
        godwinmix_protocol::plugin::manifest::Manifest::load(dir.join("gmx-plugin.toml"))
            .map_err(|e| anyhow::anyhow!("{e}"))?;
    let provide = provide
        .map(str::to_string)
        .or_else(|| manifest.provides.first().map(|p| p.id.clone()))
        .context("the manifest declares no provides")?;
    let ctx = godwinmix_host::launch::LaunchCtx {
        root: dir.to_path_buf(),
        provide,
        instance: "offline".into(),
        api_level: godwinmix_protocol::API_LEVEL,
        token: String::new(),
        rpc: String::new(),
        media: String::new(),
    };
    let launch = godwinmix_host::launch::plan(&manifest, &ctx)?;
    println!("offline replay of {}", transcript_path.display());
    println!("  {}\n", launch.command_line());
    let replay = godwinmix_host::offline::replay(&launch.argv, dir, &launch.env, &transcript)?;
    for line in &replay.steps {
        println!("{line}");
    }
    println!();
    match replay.failure {
        None => {
            println!(
                "{} line(s) sent, {} matched; no core, no sockets, no clock",
                replay.sent, replay.matched
            );
            Ok(())
        }
        Some(why) => anyhow::bail!("the transcript did not replay: {why}"),
    }
}

/// What a template's placeholders are filled with.
///
/// Every `{{key}}` a template can carry is here, so a template that grows one
/// fails to substitute rather than shipping the braces to an author.
pub struct Fields {
    pub name: String,
    pub kind: String,
    pub description: String,
    pub license: String,
    pub author: String,
}

impl Fields {
    /// The substitutions, in the order a reader would expect them.
    fn pairs(&self) -> Vec<(String, String)> {
        vec![
            ("{{name}}".into(), self.name.clone()),
            // A Rust module and a Python package cannot have a hyphen in them.
            ("{{name_snake}}".into(), self.name.replace('-', "_")),
            ("{{kind}}".into(), self.kind.clone()),
            ("{{description}}".into(), self.description.clone()),
            ("{{license}}".into(), self.license.clone()),
            ("{{author}}".into(), self.author.clone()),
            ("{{year}}".into(), year()),
            // Templates written against a published SDK version.
            ("{{sdk}}".into(), env!("CARGO_PKG_VERSION").to_string()),
        ]
    }

    fn fill(&self, text: &str) -> String {
        let mut out = text.to_string();
        for (key, value) in self.pairs() {
            out = out.replace(&key, &value);
        }
        out
    }
}

/// The current year, for a licence header. Read off the clock rather than
/// pulled from a date crate: one number does not justify a dependency.
fn year() -> String {
    let secs = std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .map(|d| d.as_secs())
        .unwrap_or(0);
    // Good enough for a copyright line, and it needs no leap year table.
    (1970 + secs / 31_556_952).to_string()
}

fn whoami() -> String {
    std::env::var("GMX_AUTHOR")
        .or_else(|_| std::env::var("USER"))
        .or_else(|_| std::env::var("USERNAME"))
        .unwrap_or_else(|_| "unknown".into())
}

/// `gmx plugin new`. Copies `templates/<lang>/` and fills its placeholders.
fn new_plugin(lang: &str, out: &Path, fields: &Fields) -> Result<()> {
    anyhow::ensure!(
        godwinmix_protocol::plugin::manifest::is_slug(&fields.name),
        "`{}` is not a slug. Use lower case letters, digits and hyphens, starting with \
         a letter: the name becomes the namespace of every id this plugin contributes.",
        fields.name
    );
    let kinds = godwinmix_protocol::plugin::manifest::KINDS;
    anyhow::ensure!(
        kinds.contains(&fields.kind.as_str()),
        "`{}` is not a plugin kind. Known: {}.",
        fields.kind,
        kinds.join(", ")
    );
    const LANGS: &[&str] = &["rust", "python", "node", "go", "shell"];
    anyhow::ensure!(
        LANGS.contains(&lang),
        "`{lang}` has no template. Known: {}.",
        LANGS.join(", ")
    );
    let template = find_template(lang)?;
    anyhow::ensure!(
        !out.exists(),
        "{} already exists. Pick another name, or --out somewhere else.",
        out.display()
    );
    let written = copy_substituting(&template, out, fields)?;
    println!("wrote {written} file(s) to {}", out.display());
    println!("\nNext:");
    println!("  cd {}", out.display());
    println!("  gmx plugin test . --quick");
    println!("  gmx plugin add .");
    println!("  gmx source add {} --type {}/{}", fields.name, fields.name, fields.kind);
    Ok(())
}

/// Where the templates are: beside the executable in a release, at the root of
/// a checkout, or wherever `GMX_TEMPLATES` says.
fn find_template(lang: &str) -> Result<PathBuf> {
    let mut tried = Vec::new();
    for base in template_roots() {
        let path = base.join(lang);
        if path.is_dir() {
            return Ok(path);
        }
        tried.push(path.display().to_string());
    }
    anyhow::bail!(
        "there is no template for {lang}. Looked in: {}. Set GMX_TEMPLATES to the directory \
         the templates live in, or copy one of the worked examples from the repository's \
         examples/ directory and edit its gmx-plugin.toml.",
        tried.join(", ")
    )
}

fn template_roots() -> Vec<PathBuf> {
    let mut roots = Vec::new();
    if let Ok(explicit) = std::env::var("GMX_TEMPLATES") {
        roots.push(PathBuf::from(explicit));
    }
    if let Ok(exe) = std::env::current_exe() {
        if let Some(dir) = exe.parent() {
            roots.push(dir.join("templates"));
            // A cargo target directory is three deep from the checkout root.
            if let Some(root) = dir.ancestors().nth(3) {
                roots.push(root.join("templates"));
            }
        }
    }
    roots.push(PathBuf::from("templates"));
    roots
}

/// Copy a tree, filling every placeholder in every text file and in every file
/// name.
fn copy_substituting(from: &Path, to: &Path, fields: &Fields) -> Result<usize> {
    std::fs::create_dir_all(to).with_context(|| format!("making {}", to.display()))?;
    let mut written = 0;
    for entry in std::fs::read_dir(from)
        .with_context(|| format!("reading {}", from.display()))?
        .flatten()
    {
        let path = entry.path();
        let target_name = fields.fill(&entry.file_name().to_string_lossy());
        let target = to.join(&target_name);
        if path.is_dir() {
            written += copy_substituting(&path, &target, fields)?;
            continue;
        }
        match std::fs::read_to_string(&path) {
            Ok(text) => {
                std::fs::write(&target, fields.fill(&text))
                    .with_context(|| format!("writing {}", target.display()))?;
            }
            // Not text: an icon, a font, a prebuilt binary. Copied as it is.
            Err(_) => {
                std::fs::copy(&path, &target)
                    .with_context(|| format!("copying {}", path.display()))?;
            }
        }
        copy_mode(&path, &target);
        written += 1;
    }
    Ok(written)
}

#[cfg(unix)]
fn copy_mode(from: &Path, to: &Path) {
    use std::os::unix::fs::PermissionsExt;
    if let Ok(meta) = std::fs::metadata(from) {
        let mode = meta.permissions().mode();
        let _ = std::fs::set_permissions(to, std::fs::Permissions::from_mode(mode));
    }
}

#[cfg(not(unix))]
fn copy_mode(_from: &Path, _to: &Path) {}

// ---------------------------------------------------------------------------
// Bisect
// ---------------------------------------------------------------------------

/// Binary search the enabled plugins for the one that breaks a check.
///
/// The invariant: the check fails with the whole set and passes with none. If
/// either is untrue the search says so rather than picking a plugin at random,
/// because "it is plugin seven" when plugin seven is innocent costs more time
/// than no answer at all.
async fn bisect(api: &Api, check: &str) -> Result<()> {
    let listing: Value = api.get("plugin.list", None, &[]).await?;
    let names: Vec<String> = listing["plugins"]
        .as_array()
        .cloned()
        .unwrap_or_default()
        .iter()
        .filter(|p| p["enabled"].as_bool().unwrap_or(false))
        .filter_map(|p| p["name"].as_str().map(str::to_string))
        .collect();
    anyhow::ensure!(!names.is_empty(), "no plugins are enabled, so none of them is the cause.");
    println!("bisecting {} plugin(s) with: {check}", names.len());
    let steps = (names.len() as f64).log2().ceil() as usize;
    println!("at most {steps} step(s)\n");

    let outcome = search(api, check, &names).await;
    // Put everything back the way it was found, whatever happened above.
    for name in &names {
        let _ = api.call::<_, Value>("plugin.enable", Some(name), &json!({})).await;
    }
    let culprit = outcome?;
    match culprit {
        Some(name) => {
            println!("\n{name} is the plugin that breaks the check.");
            println!("Disable it with:  gmx plugin disable {name}");
            println!("Every other plugin has been put back the way it was.");
        }
        None => println!("\nno single plugin breaks the check on its own."),
    }
    Ok(())
}

async fn search(api: &Api, check: &str, names: &[String]) -> Result<Option<String>> {
    if !run_check(api, check, names).await? {
        anyhow::bail!(
            "the check passes with every plugin enabled, so there is nothing to find. \
             Make `--check` a command that exits non-zero when the fault is present."
        );
    }
    if run_check(api, check, &[]).await? {
        anyhow::bail!(
            "the check fails with every plugin disabled too, so a plugin is not the cause. \
             Look at the core, the config or the machine."
        );
    }
    let mut candidates: Vec<String> = names.to_vec();
    let mut step = 1;
    while candidates.len() > 1 {
        let half = candidates.len() / 2;
        let left: Vec<String> = candidates[..half].to_vec();
        println!("step {step}: trying {} of {}", left.len(), candidates.len());
        candidates = if run_check(api, check, &left).await? {
            left
        } else {
            candidates[half..].to_vec()
        };
        step += 1;
    }
    Ok(candidates.into_iter().next())
}

/// Enable exactly `on`, disable the rest, and say whether the check failed.
async fn run_check(api: &Api, check: &str, on: &[String]) -> Result<bool> {
    let listing: Value = api.get("plugin.list", None, &[]).await?;
    for plugin in listing["plugins"].as_array().cloned().unwrap_or_default() {
        let Some(name) = plugin["name"].as_str() else { continue };
        let method = if on.iter().any(|n| n == name) { "plugin.enable" } else { "plugin.disable" };
        api.call::<_, Value>(method, Some(name), &json!({})).await?;
    }
    let status = std::process::Command::new(shell())
        .arg(shell_flag())
        .arg(check)
        .status()
        .with_context(|| format!("running the check: {check}"))?;
    Ok(!status.success())
}

fn shell() -> &'static str {
    if cfg!(windows) {
        "cmd"
    } else {
        "sh"
    }
}

fn shell_flag() -> &'static str {
    if cfg!(windows) {
        "/C"
    } else {
        "-c"
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn a_template_substitutes_the_name_in_the_text_and_in_the_file_names() {
        let root = std::env::temp_dir().join(format!("gmx-template-{}", std::process::id()));
        let _ = std::fs::remove_dir_all(&root);
        let template = root.join("python");
        std::fs::create_dir_all(template.join("src")).expect("the template");
        std::fs::write(
            template.join("gmx-plugin.toml"),
            "[plugin]\nname = \"{{name}}\"\n\n[[provides]]\nkind = \"{{kind}}\"\n",
        )
        .expect("the manifest");
        std::fs::write(template.join("src").join("{{name}}.py"), "# {{name}}\n")
            .expect("the entry");

        let out = root.join("made");
        let fields = Fields {
            name: "clock".into(),
            kind: "source".into(),
            description: "A clock".into(),
            license: "MIT".into(),
            author: "nobody".into(),
        };
        let written = copy_substituting(&template, &out, &fields).expect("it copies");
        assert_eq!(written, 2);
        let manifest =
            std::fs::read_to_string(out.join("gmx-plugin.toml")).expect("the manifest");
        assert!(manifest.contains("name = \"clock\""), "{manifest}");
        assert!(manifest.contains("kind = \"source\""), "{manifest}");
        assert!(
            out.join("src").join("clock.py").is_file(),
            "the file name is substituted too"
        );
        let _ = std::fs::remove_dir_all(&root);
    }

    #[test]
    fn a_missing_template_names_where_it_looked_and_what_to_do() {
        let err = find_template("cobol").expect_err("there is no cobol template");
        let text = format!("{err}");
        assert!(text.contains("GMX_TEMPLATES"), "{text}");
        assert!(text.contains("examples"), "{text}");
    }
}
