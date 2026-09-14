//! `gmx preset list|show|apply|save|diff`, and `gmx build`.
//!
//! One command is meant to be the whole of a volunteer's install:
//!
//! ```text
//! gmx preset apply church
//! ```
//!
//! Everything else here serves that. `list` says what there is, `show` and
//! `diff` say what one would do before it does it, `save` turns a working
//! machine back into a preset, and `build` assembles a branded directory.
//!
//! The engine side is `godwinmix_core::preset`. Nothing in this file decides
//! anything; it parses a command line, calls the engine and prints.

use anyhow::{Context, Result};
use clap::Subcommand;
use godwinmix_core::preset::{self, plan::Options};
use std::path::PathBuf;

#[derive(Subcommand, Debug, Clone)]
pub enum Preset {
    /// Every preset this machine can apply, built in and installed.
    List {
        /// Print as JSON, the same shape the `preset.list` RPC method returns.
        #[arg(long)]
        json: bool,
    },
    /// One preset: what it is for, what it needs, and what applying it would do.
    Show {
        /// A preset name, or a path to a directory holding `gmx-plugin.toml`.
        name: String,
        /// The config it would be measured against.
        #[arg(short, long, default_value = "godwinmix.toml")]
        config: PathBuf,
        /// Print the preset's README instead of the plan.
        #[arg(long)]
        readme: bool,
        #[arg(long)]
        json: bool,
    },
    /// What applying it would change in this config, and nothing else.
    Diff {
        name: String,
        #[arg(short, long, default_value = "godwinmix.toml")]
        config: PathBuf,
        #[arg(long)]
        json: bool,
    },
    /// Put a preset on this machine: the config, the scenes, the layout, the
    /// theme and the gallery mode.
    ///
    /// Your own values win over the preset's. `--force` takes the preset's
    /// instead. Sources and outputs are appended by id and never duplicated,
    /// so applying the same preset twice changes nothing the second time.
    Apply {
        /// A preset name, or a path to a directory holding `gmx-plugin.toml`.
        name: String,
        /// Print the plan and write nothing.
        #[arg(long)]
        dry_run: bool,
        #[arg(short, long, default_value = "godwinmix.toml")]
        config: PathBuf,
        /// Take the preset's value wherever you already have one of your own.
        #[arg(long)]
        force: bool,
        /// Leave your sources and outputs alone: config, scenes and surface only.
        #[arg(long)]
        keep_sources: bool,
        #[arg(long)]
        json: bool,
    },
    /// Turn this machine's working setup into a preset somebody else can apply.
    ///
    /// The stream keys and the control token are replaced with placeholders, so
    /// the result is safe to hand over. Read it before you publish it anyway.
    Save {
        /// The new preset's name. A slug: lower case, digits and hyphens.
        name: String,
        #[arg(short, long, default_value = "godwinmix.toml")]
        config: PathBuf,
        /// Where to write it [default: ~/.godwinmix/presets/<name>].
        #[arg(long)]
        out: Option<PathBuf>,
        #[arg(long)]
        json: bool,
    },
}

pub fn run(cmd: Preset) -> Result<()> {
    match cmd {
        Preset::List { json } => list(json),
        Preset::Show { name, config, readme, json } => show(&name, config, readme, json),
        Preset::Diff { name, config, json } => diff(&name, config, json),
        Preset::Apply { name, dry_run, config, force, keep_sources, json } => {
            apply(&name, dry_run, config, force, keep_sources, json)
        }
        Preset::Save { name, config, out, json } => save(&name, config, out, json),
    }
}

fn list(json: bool) -> Result<()> {
    let rows = preset::list();
    if json {
        println!("{}", serde_json::to_string_pretty(&rows)?);
        return Ok(());
    }
    if rows.is_empty() {
        println!("no presets. This build ships six; something has gone wrong with it.");
        return Ok(());
    }
    println!("{:<16} {:<9} {:<10} FOR", "NAME", "THEME", "GALLERY");
    for row in &rows {
        println!(
            "{:<16} {:<9} {:<10} {}",
            row.name,
            row.theme,
            row.gallery.clone().unwrap_or_else(|| "machine".into()),
            first_sentence(&row.description)
        );
    }
    println!();
    println!("`gmx preset show <name>` for one of them, `gmx preset apply <name>` to use it.");
    Ok(())
}

/// A description is one or two sentences; a table row wants the first.
fn first_sentence(text: &str) -> String {
    match text.find(". ") {
        Some(at) if at < 92 => text[..=at].to_string(),
        _ if text.len() > 92 => format!("{}...", &text[..89]),
        _ => text.to_string(),
    }
}

fn show(name: &str, config: PathBuf, readme: bool, json: bool) -> Result<()> {
    let found = preset::resolve(name)?;
    if readme {
        print!("{}", found.read("README.md").unwrap_or_else(|_| {
            format!("{} ships no README.\n", found.name)
        }));
        return Ok(());
    }
    let plan = preset::plan::build(&found, &Options::new(config))?;
    if json {
        println!("{}", serde_json::to_string_pretty(&plan)?);
        return Ok(());
    }
    for line in plan.report() {
        println!("{line}");
    }
    println!();
    println!("`gmx preset show {name} --readme` is the page written for the person using it.");
    Ok(())
}

fn diff(name: &str, config: PathBuf, json: bool) -> Result<()> {
    let found = preset::resolve(name)?;
    let plan = preset::plan::build(&found, &Options::new(&config))?;
    if json {
        println!("{}", serde_json::to_string_pretty(&plan.config)?);
        return Ok(());
    }
    if plan.config.is_empty() {
        println!("{} would change nothing in {}.", name, config.display());
        return Ok(());
    }
    for change in &plan.config {
        match change.action {
            preset::plan::Action::Set => println!("+ {} = {}", change.key, change.to),
            preset::plan::Action::Keep => println!(
                "  {} = {} (the preset wanted {})",
                change.key,
                change.from.clone().unwrap_or_default(),
                change.to
            ),
            preset::plan::Action::Override => println!(
                "~ {} = {} (was {})",
                change.key,
                change.to,
                change.from.clone().unwrap_or_default()
            ),
        }
    }
    for addition in plan.sources.iter().chain(&plan.outputs) {
        if !addition.already_there {
            println!("+ {} {} = {}", addition.type_id, addition.id, addition.uri);
        }
    }
    Ok(())
}

fn apply(
    name: &str,
    dry_run: bool,
    config: PathBuf,
    force: bool,
    keep_sources: bool,
    json: bool,
) -> Result<()> {
    let found = preset::resolve(name)?;
    let options = Options { config_path: config, force, keep_sources };
    let plan = preset::plan::build(&found, &options)?;
    if dry_run {
        if json {
            println!("{}", serde_json::to_string_pretty(&plan)?);
        } else {
            for line in plan.report() {
                println!("{line}");
            }
            println!();
            println!("Nothing was written. Run it again without --dry-run.");
        }
        return Ok(());
    }
    let applied = preset::apply::run(&found, &plan)
        .with_context(|| format!("applying the preset {name}"))?;
    if json {
        println!("{}", serde_json::to_string_pretty(&applied)?);
        return Ok(());
    }
    for line in applied.report() {
        println!("{line}");
    }
    println!();
    println!("Run `gmx` to start it, then open http://localhost:8080.");
    Ok(())
}

fn save(name: &str, config: PathBuf, out: Option<PathBuf>, json: bool) -> Result<()> {
    let dir = out.unwrap_or_else(|| preset::save::default_dir(name));
    let saved = preset::save::run(name, &config, &dir)?;
    if json {
        println!("{}", serde_json::to_string_pretty(&saved)?);
        return Ok(());
    }
    println!("saved {} to {}", saved.name, saved.dir.display());
    for path in &saved.wrote {
        println!("  wrote    {}", path.display());
    }
    if !saved.redacted.is_empty() {
        println!();
        println!("taken out");
        for note in &saved.redacted {
            println!("  {note}");
        }
    }
    println!();
    println!("Rewrite README.md and the description in gmx-plugin.toml before you share it.");
    println!("`gmx preset apply {}` applies it from here.", saved.dir.display());
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn a_long_description_is_cut_to_a_table_row() {
        let long = "A".repeat(200);
        assert!(first_sentence(&long).len() <= 92);
        assert_eq!(first_sentence("Short one."), "Short one.");
        assert_eq!(first_sentence("First. Second."), "First.");
    }

    #[test]
    fn every_official_preset_shows_a_plan_from_the_command_line() {
        let _ = gstreamer::init();
        for name in preset::NAMES {
            let found = preset::resolve(name).unwrap();
            let plan =
                preset::plan::build(&found, &Options::new("/nowhere/godwinmix.toml")).unwrap();
            assert!(!plan.report().is_empty(), "{name} prints nothing");
        }
    }
}
