//! `gmx ui`: start a whole UI against a running core.
//!
//! A `surface` is the twelfth plugin kind and the least complicated one. It is
//! not placed in the pipeline, it carries no media, and the core does not
//! supervise it. It is a program the operator runs that talks the same public
//! protocol every other client talks, and all the manifest says about it is
//! the command to start and the protocol level it speaks:
//!
//! ```toml
//! [[provides]]
//! kind = "surface"
//! id = "tui"
//! surface = { run = "gmx-tui", api = 1 }
//! ```
//!
//! What this module adds is the two lines of convenience that make that worth
//! having: `gmx ui list` says which surfaces are installed, and `gmx ui <name>`
//! starts one with `GODWINMIX_URL` and `GODWINMIX_TOKEN` already in its
//! environment, so a surface never has to ask the operator for the address of
//! the mixer they are already talking to.
//!
//! Where `run` is looked for, in order: inside the plugin's own directory,
//! then beside the `gmx` binary, then on `PATH`. The first is how a third
//! party surface installed with `gmx plugin add` is found; the second is how
//! the first party TUI is found, because it ships beside `gmx` rather than
//! being installed; the third is how a surface installed by a package manager
//! is found.

use anyhow::{bail, Context, Result};
use godwinmix_protocol::plugin::manifest::Manifest;
use std::collections::BTreeMap;
use std::path::{Path, PathBuf};

/// `gmx ui [name] [-- args...]`.
///
/// One positional rather than a pair of subcommands, because the thing an
/// operator types is the name of their UI: `gmx ui tui`. `gmx ui` with no name
/// and `gmx ui list` both print the list, which means a surface may not be
/// called `list`, and that is a price worth paying for the shorter line.
#[derive(clap::Args, Debug)]
pub struct UiArgs {
    /// The surface to start, as `gmx ui list` prints it. Omit it, or say
    /// `list`, to see what is installed.
    pub name: Option<String>,
    /// Arguments for the surface itself, after `--`.
    ///
    /// `gmx ui tui -- --multiview --fps 8` passes the three of them through
    /// untouched.
    #[arg(trailing_var_arg = true, allow_hyphen_values = true)]
    pub args: Vec<String>,
    /// With no name, print the list as JSON.
    #[arg(long)]
    pub json: bool,
    /// Address of the mixer's control server [default: http://127.0.0.1:8080].
    #[arg(long, env = "GODWINMIX_URL")]
    pub url: Option<String>,
    /// Bearer token, when the mixer has one configured.
    #[arg(long, env = "GODWINMIX_TOKEN", hide_env_values = true)]
    pub token: Option<String>,
}

/// One surface that could be started.
#[derive(Debug, Clone)]
pub struct Surface {
    pub name: String,
    pub version: String,
    pub description: String,
    /// The provide id, so `tui/tui` can be named in full when it matters.
    pub provide: String,
    /// The protocol level the manifest says it speaks.
    pub api: u32,
    /// The plugin directory the manifest was read from.
    pub root: PathBuf,
    /// The command as the manifest spells it.
    pub run: String,
    /// Where that command was found, or why it was not.
    pub found: Result<PathBuf, String>,
}

impl Surface {
    pub fn qualified(&self) -> String {
        format!("{}/{}", self.name, self.provide)
    }
}

pub fn run(url: &str, token: Option<&str>, cmd: UiArgs) -> Result<()> {
    match cmd.name.as_deref() {
        None | Some("list") => list(cmd.json),
        Some(name) => start(name, &cmd.args, url, token),
    }
}

// ---------------------------------------------------------------------------
// Finding surfaces
// ---------------------------------------------------------------------------

/// Every directory a `gmx-plugin.toml` with a surface in it might be under.
///
/// The plugins directory first, because an operator's installed plugin must
/// win over anything a checkout happens to have lying about.
fn search_roots() -> Vec<PathBuf> {
    let mut roots = vec![godwinmix_core::plugin::loader::dir()];
    if let Some(beside) = beside_gmx() {
        // A packaged install puts surfaces next to the binary.
        roots.push(beside.join("surfaces"));
        // ...and a checkout has target/debug/gmx, so the crates are three up.
        if let Some(repo) = beside.parent().and_then(Path::parent) {
            roots.push(repo.join("crates"));
        }
    }
    roots
}

/// The directory holding the running `gmx`.
fn beside_gmx() -> Option<PathBuf> {
    std::env::current_exe()
        .ok()
        .and_then(|path| path.parent().map(Path::to_path_buf))
}

/// Read every surface under the search roots, nearest first, one entry per
/// plugin name: an installed surface shadows one in a checkout.
pub fn installed() -> Vec<Surface> {
    let mut found: BTreeMap<String, Surface> = BTreeMap::new();
    for root in search_roots() {
        for manifest_path in manifests_under(&root) {
            let Ok(manifest) = Manifest::load(&manifest_path) else {
                continue;
            };
            let Some(dir) = manifest_path.parent() else { continue };
            for provide in manifest.provides.iter().filter(|p| p.kind == "surface") {
                let Some(table) = provide.surface.as_ref() else { continue };
                let run = table
                    .get("run")
                    .and_then(|v| v.as_str())
                    .unwrap_or_default()
                    .to_string();
                if run.is_empty() {
                    continue;
                }
                let surface = Surface {
                    name: manifest.plugin.name.clone(),
                    version: manifest.plugin.version.clone(),
                    description: manifest.plugin.description.clone(),
                    provide: provide.id.clone(),
                    api: table.get("api").and_then(|v| v.as_u64()).unwrap_or(1) as u32,
                    found: locate(&run, dir),
                    root: dir.to_path_buf(),
                    run,
                };
                found.entry(surface.name.clone()).or_insert(surface);
            }
        }
    }
    found.into_values().collect()
}

/// `<root>/gmx-plugin.toml`, `<root>/<name>/gmx-plugin.toml` and
/// `<root>/<name>/<version>/gmx-plugin.toml`, which is the layout the loader
/// installs into plus the two shallower ones a checkout has.
fn manifests_under(root: &Path) -> Vec<PathBuf> {
    let mut out = Vec::new();
    if root.join("gmx-plugin.toml").is_file() {
        out.push(root.join("gmx-plugin.toml"));
    }
    let Ok(entries) = std::fs::read_dir(root) else {
        return out;
    };
    for entry in entries.flatten() {
        let path = entry.path();
        if !path.is_dir() {
            continue;
        }
        if path.join("gmx-plugin.toml").is_file() {
            out.push(path.join("gmx-plugin.toml"));
        }
        let Ok(versions) = std::fs::read_dir(&path) else { continue };
        for version in versions.flatten() {
            let inner = version.path();
            if inner.is_dir() && inner.join("gmx-plugin.toml").is_file() {
                out.push(inner.join("gmx-plugin.toml"));
            }
        }
    }
    out
}

/// Find the command: in the plugin directory, then beside `gmx`, then on PATH.
fn locate(run: &str, plugin_dir: &Path) -> Result<PathBuf, String> {
    let exe = with_exe_suffix(run);
    let mut looked = Vec::new();

    for candidate in [plugin_dir.join(&exe), plugin_dir.join("bin").join(&exe)] {
        if candidate.is_file() {
            return Ok(candidate);
        }
        looked.push(candidate.display().to_string());
    }
    if let Some(beside) = beside_gmx() {
        let candidate = beside.join(&exe);
        if candidate.is_file() {
            return Ok(candidate);
        }
        looked.push(candidate.display().to_string());
    }
    if let Some(on_path) = on_path(&exe) {
        return Ok(on_path);
    }
    looked.push(format!("{exe} on PATH"));
    Err(format!("looked in: {}", looked.join(", ")))
}

fn with_exe_suffix(run: &str) -> String {
    if cfg!(windows) && !run.ends_with(".exe") {
        format!("{run}.exe")
    } else {
        run.to_string()
    }
}

fn on_path(exe: &str) -> Option<PathBuf> {
    let path = std::env::var_os("PATH")?;
    std::env::split_paths(&path)
        .map(|dir| dir.join(exe))
        .find(|candidate| candidate.is_file())
}

// ---------------------------------------------------------------------------
// The subcommands
// ---------------------------------------------------------------------------

/// The surface a preset chose, out of the `[ui]` section of the runtime store.
///
/// `gmx preset apply church` writes `surface = "web"` there. It is a note to
/// whoever starts a UI on this machine, which is this command, so `gmx ui`
/// says which one the preset meant rather than leaving an operator to guess.
pub fn chosen() -> Option<String> {
    let store = godwinmix_core::config::Config::runtime_store_path(
        &godwinmix_core::config::path_in_force(Path::new("")),
    );
    let text = std::fs::read_to_string(store).ok()?;
    let table: toml::Table = toml::from_str(&text).ok()?;
    let name = table
        .get("ui")?
        .as_table()?
        .get("surface")?
        .as_str()?
        .trim()
        .to_string();
    (!name.is_empty()).then_some(name)
}

fn list(json: bool) -> Result<()> {
    let surfaces = installed();
    let chose = chosen();
    if json {
        let rows: Vec<serde_json::Value> = surfaces
            .iter()
            .map(|s| {
                serde_json::json!({
                    "name": s.name,
                    "provide": s.qualified(),
                    "version": s.version,
                    "description": s.description,
                    "api": s.api,
                    "run": s.run,
                    "root": s.root.display().to_string(),
                    "command": s.found.as_ref().ok().map(|p| p.display().to_string()),
                    "problem": s.found.as_ref().err(),
                    "chosen_by_preset": chose.as_deref() == Some(s.name.as_str()),
                })
            })
            .collect();
        println!(
            "{}",
            serde_json::to_string_pretty(&serde_json::json!({
                "surfaces": rows,
                "chosen_by_preset": chose,
            }))?
        );
        return Ok(());
    }

    if surfaces.is_empty() {
        println!(
            "No surfaces are installed.\n\n\
             A surface is a whole UI: a manifest with kind = \"surface\" and \
             surface = {{ run, api }}.\n\
             Install one with `gmx plugin add <directory>`, or see \
             docs/reference/surfaces.md."
        );
        return Ok(());
    }

    let width = surfaces.iter().map(|s| s.name.len()).max().unwrap_or(4).max(4);
    println!("  {:<width$}  {:<5} COMMAND", "NAME", "API", width = width);
    for surface in &surfaces {
        let command = match &surface.found {
            Ok(path) => path.display().to_string(),
            Err(problem) => format!("not found ({problem})"),
        };
        // A star on the one the applied preset chose.
        let mark = if chose.as_deref() == Some(surface.name.as_str()) { "*" } else { " " };
        println!(
            "{mark} {:<width$}  {:<5} {}",
            surface.name,
            surface.api,
            command,
            width = width
        );
        if !surface.description.is_empty() {
            println!("  {:<width$}  {:<5} {}", " ", " ", surface.description, width = width);
        }
    }
    match chose {
        Some(name) if surfaces.iter().any(|s| s.name == name) => {
            println!("\n* is the surface the applied preset chose. Start it with `gmx ui {name}`.")
        }
        // `web` and `none` are preset answers that name no plugin: the web UI
        // is served by the core itself, and `none` means a headless install.
        Some(name) if name == "web" => println!(
            "\nThe applied preset chose the web UI, which the core serves itself: \
             open the mixer's address in a browser."
        ),
        Some(name) if name == "none" => {
            println!("\nThe applied preset chose no UI. Start one anyway with `gmx ui <name>`.")
        }
        Some(name) => println!(
            "\nThe applied preset chose the surface '{name}', which is not installed here. \
             Install it with `gmx plugin add`, or start one of the above."
        ),
        None => println!("\nStart one with `gmx ui <name>`."),
    }
    Ok(())
}

fn start(name: &str, args: &[String], url: &str, token: Option<&str>) -> Result<()> {
    let surfaces = installed();
    let Some(surface) = surfaces.iter().find(|s| s.name == name || s.qualified() == name) else {
        let known: Vec<&str> = surfaces.iter().map(|s| s.name.as_str()).collect();
        bail!(
            "there is no surface called '{name}'. {}\n\
             A surface is a plugin whose manifest has kind = \"surface\"; \
             `gmx ui list` shows the ones this machine can start.",
            if known.is_empty() {
                "No surfaces are installed.".to_string()
            } else {
                format!("Installed: {}.", known.join(", "))
            }
        );
    };

    let command = match &surface.found {
        Ok(path) => path.clone(),
        Err(problem) => bail!(
            "'{name}' declares surface.run = \"{}\" and that command is not there. {problem}.\n\
             Build or reinstall the surface, or put its binary on PATH.",
            surface.run
        ),
    };

    if surface.api > godwinmix_protocol::API_LEVEL {
        bail!(
            "'{name}' speaks protocol level {} and this core is level {}. \
             Upgrade GodwinMix, or install a build of the surface for this level.",
            surface.api,
            godwinmix_protocol::API_LEVEL
        );
    }

    let mut child = std::process::Command::new(&command);
    child.args(args);
    // The two variables every first party client already reads. A surface that
    // wants them under other names reads them in its own entry point; these
    // are the ones `gmx ctl`, `gmx mcp` and `gmx-tui` use.
    child.env("GODWINMIX_URL", url);
    if let Some(token) = token {
        child.env("GODWINMIX_TOKEN", token);
    } else {
        // An inherited token for another mixer is worse than none: the surface
        // would send it to this one.
        child.env_remove("GODWINMIX_TOKEN");
    }
    child.env("GMX_PLUGIN", &surface.name);
    child.env("GMX_PROVIDE", &surface.provide);
    child.env("GMX_PLUGIN_ROOT", &surface.root);
    child.env("GMX_API_LEVEL", godwinmix_protocol::API_LEVEL.to_string());
    // stdin, stdout and stderr are inherited: a surface owns the terminal.

    let status = child
        .status()
        .with_context(|| format!("starting {}", command.display()))?;
    if !status.success() {
        // The surface has already said why on its own stderr. Carry its exit
        // code out rather than wrapping it in one of ours.
        std::process::exit(status.code().unwrap_or(1));
    }
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;

    fn write(dir: &Path, name: &str, body: &str) -> PathBuf {
        let plugin = dir.join(name);
        std::fs::create_dir_all(&plugin).unwrap();
        std::fs::write(plugin.join("gmx-plugin.toml"), body).unwrap();
        plugin
    }

    fn manifest(name: &str, run: &str) -> String {
        format!(
            r#"
[plugin]
name = "{name}"
version = "0.1.0"
api = 1
description = "a surface for testing"
license = "Apache-2.0"
platforms = ["linux-x86_64", "macos-aarch64", "macos-x86_64", "windows-x86_64"]
placements = ["in-process"]

[[provides]]
kind = "surface"
id = "main"
surface = {{ run = "{run}", api = 1 }}
"#
        )
    }

    #[test]
    fn a_surface_manifest_validates() {
        let parsed = Manifest::parse(&manifest("panel", "gmx-panel")).expect("parses");
        let problems = parsed.validate(None);
        assert!(problems.is_empty(), "{problems:#?}");
    }

    #[test]
    fn a_surface_without_a_run_command_is_refused_with_the_key() {
        let text = manifest("panel", "gmx-panel").replace(
            r#"surface = { run = "gmx-panel", api = 1 }"#,
            "surface = { api = 1 }",
        );
        let parsed = Manifest::parse(&text).expect("parses");
        let problems = parsed.validate(None);
        assert!(
            problems.iter().any(|p| p.path.ends_with("surface.run")),
            "{problems:#?}"
        );
    }

    #[test]
    fn a_surface_without_an_api_level_is_refused_with_the_key() {
        let text = manifest("panel", "gmx-panel").replace(
            r#"surface = { run = "gmx-panel", api = 1 }"#,
            r#"surface = { run = "gmx-panel" }"#,
        );
        let parsed = Manifest::parse(&text).expect("parses");
        let problems = parsed.validate(None);
        assert!(
            problems.iter().any(|p| p.path.ends_with("surface.api")),
            "{problems:#?}"
        );
    }

    #[test]
    fn manifests_are_found_at_all_three_depths() {
        let temp = std::env::temp_dir().join(format!("gmx-ui-depths-{}", std::process::id()));
        let _ = std::fs::remove_dir_all(&temp);
        std::fs::create_dir_all(&temp).unwrap();

        std::fs::write(temp.join("gmx-plugin.toml"), manifest("here", "x")).unwrap();
        write(&temp, "one", &manifest("one", "x"));
        write(&temp.join("two"), "0.1.0", &manifest("two", "x"));

        let found = manifests_under(&temp);
        assert_eq!(found.len(), 3, "{found:#?}");

        let _ = std::fs::remove_dir_all(&temp);
    }

    #[test]
    fn a_command_inside_the_plugin_directory_is_found_before_anything_else() {
        let temp = std::env::temp_dir().join(format!("gmx-ui-locate-{}", std::process::id()));
        let _ = std::fs::remove_dir_all(&temp);
        std::fs::create_dir_all(temp.join("bin")).unwrap();

        let run = if cfg!(windows) { "surf.exe" } else { "surf" };
        std::fs::write(temp.join("bin").join(run), b"#!/bin/sh\n").unwrap();
        let found = locate("surf", &temp).expect("found in bin/");
        assert!(found.ends_with(run), "{}", found.display());

        let _ = std::fs::remove_dir_all(&temp);
    }

    #[test]
    fn a_command_that_is_nowhere_says_everywhere_it_looked() {
        let nowhere = std::env::temp_dir().join("gmx-ui-nothing-here");
        let problem = locate("definitely-not-a-real-surface", &nowhere).unwrap_err();
        assert!(problem.contains("looked in:"), "{problem}");
        assert!(problem.contains("on PATH"), "{problem}");
    }

    #[test]
    fn the_shipped_tui_manifest_is_a_valid_surface() {
        // The first surface, and the one `gmx ui tui` starts. It lives in the
        // checkout rather than the plugins directory, which is exactly the
        // case the search roots exist for.
        let path = Path::new(env!("CARGO_MANIFEST_DIR"))
            .join("../godwinmix-tui/gmx-plugin.toml");
        let manifest = Manifest::load(&path).expect("the TUI's manifest loads");
        let problems = manifest.validate(path.parent());
        assert!(problems.is_empty(), "{problems:#?}");

        let surface = manifest
            .provides
            .iter()
            .find(|p| p.kind == "surface")
            .expect("a surface provide");
        let table = surface.surface.as_ref().expect("the surface table");
        assert_eq!(table.get("run").and_then(|v| v.as_str()), Some("gmx-tui"));
        assert_eq!(table.get("api").and_then(|v| v.as_u64()), Some(1));
    }
}
