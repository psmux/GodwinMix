//! `gmx marketplace add|list|remove`.
//!
//! A marketplace is a repository with `godwinmix-marketplace.json` (or
//! `index.json`) at its root, listing plugins and where each one comes from.
//! Adding one fetches that document and caches it under `~/.godwinmix`; from
//! then on `gmx plugin search` reads the cache and `gmx plugin add <name>`
//! resolves a bare name through it.
//!
//! These commands need no running mixer, the way `gmx plugin new` does not:
//! the list is state belonging to the machine, beside the config file, and the
//! core reads it when it installs. A core on another machine reads that
//! machine's list, so run these where the core runs.

use anyhow::{Context, Result};
use clap::Subcommand;
use godwinmix_host::marketplace;

#[derive(Subcommand, Debug)]
pub enum Marketplace {
    /// Add a marketplace and fetch its listing.
    ///
    /// Takes `owner/repo`, a URL, or a path to a directory with a marketplace
    /// document in it. The official one, listing the first party plugins, is
    /// this repository itself.
    Add {
        /// `owner/repo`, `https://.../index.json`, or a directory.
        source: String,
    },
    /// Every marketplace this machine knows, and how many plugins each lists.
    List {
        #[arg(long)]
        json: bool,
    },
    /// Forget a marketplace. Plugins installed from it stay installed.
    Remove { name: String },
    /// Fetch every added marketplace again.
    Refresh,
}

pub fn run(cmd: Marketplace) -> Result<()> {
    match cmd {
        Marketplace::Add { source } => add(&source),
        Marketplace::List { json } => list(json),
        Marketplace::Remove { name } => remove(&name),
        Marketplace::Refresh => refresh(),
    }
}

fn add(source: &str) -> Result<()> {
    let added = marketplace::add(source)
        .with_context(|| format!("adding the marketplace at {source}"))?;
    println!("added {} ({} plugin(s))", added.name, added.plugins);
    println!("  from {}", added.url);
    println!("  cached at {}", marketplace::cache_path(&added.name).display());
    println!("\nNext:  gmx plugin search <word>");
    Ok(())
}

fn list(json: bool) -> Result<()> {
    let store = marketplace::load_store();
    if json {
        println!("{}", serde_json::to_string_pretty(&store)?);
        return Ok(());
    }
    if store.marketplaces.is_empty() {
        println!("no marketplaces added");
        // The same two the API offers a surface with no terminal, off the one
        // list, so the shell and the web page never disagree about which
        // marketplace the project runs.
        for offer in marketplace::recommendations(&[]) {
            println!("\n{} ({})", offer.title, offer.name);
            println!("  gmx marketplace add {}", offer.source);
        }
        return Ok(());
    }
    println!("{:<22} {:<8} {:<36} SOURCE", "MARKETPLACE", "PLUGINS", "TITLE");
    for entry in &store.marketplaces {
        let doc = marketplace::Marketplace::read(&marketplace::cache_path(&entry.name)).ok();
        let title = doc.as_ref().map(|d| d.title.clone()).unwrap_or_default();
        println!(
            "{:<22} {:<8} {:<36} {}",
            entry.name,
            entry.plugins,
            if title.is_empty() { "-".into() } else { title },
            entry.source
        );
    }
    println!("\nread from {}", marketplace::store_path().display());
    Ok(())
}

fn remove(name: &str) -> Result<()> {
    let gone = marketplace::remove(name)?;
    println!("removed {} ({})", gone.name, gone.source);
    println!("Plugins installed from it are untouched; `gmx plugin list` still shows them.");
    Ok(())
}

fn refresh() -> Result<()> {
    let results = marketplace::refresh();
    if results.is_empty() {
        println!("no marketplaces added, so there is nothing to refresh");
        return Ok(());
    }
    let mut failed = 0;
    for (name, outcome) in &results {
        match outcome {
            Ok(count) => println!("{name:<22} {count} plugin(s)"),
            Err(e) => {
                failed += 1;
                println!("{name:<22} not refreshed: {e:#}");
            }
        }
    }
    anyhow::ensure!(
        failed == 0,
        "{failed} of {} marketplaces could not be refreshed. The cached copy of each is \
         still there, so `gmx plugin search` goes on working.",
        results.len()
    );
    Ok(())
}
