//! `gmx codec list`, `gmx codec test <entry>`, and what `--probe` prints.
//!
//! All three answer the same question from different distances: what will this
//! machine encode with, and why that one. `--probe` is the whole decision on
//! one screen before any camera is pointed at the box.

use godwinmix_core::catalogue::select::{GstRegistry, Request, Selection};
use godwinmix_core::catalogue::{check, Catalogue};
use godwinmix_core::config::Config;
use anyhow::{Context, Result};
use clap::Subcommand;
use std::path::{Path, PathBuf};

#[derive(Subcommand, Debug, Clone)]
pub enum Codec {
    /// Every entry in the merged catalogue, and whether this machine has it.
    List {
        /// Print the catalogue as JSON, the same shape the `codec.list` RPC
        /// method returns.
        #[arg(long)]
        json: bool,
    },
    /// Encode the test pattern through an entry, decode it back, and report.
    ///
    /// This is the check that a driver which loads can also encode. The report
    /// it prints is what goes into the entry's `verified` list.
    Test {
        /// Entry id, as `gmx codec list` prints it.
        entry: String,
        /// How long to encode for.
        #[arg(long, default_value_t = 10.0)]
        seconds: f64,
        #[arg(long, default_value_t = 1280)]
        width: i32,
        #[arg(long, default_value_t = 720)]
        height: i32,
        #[arg(long, default_value_t = 30)]
        fps: i32,
        #[arg(long)]
        json: bool,
    },
    /// Prove an entry on this machine and record that you did.
    ///
    /// Runs the same encode and decode `gmx codec test` runs, then appends a
    /// `verified` record (platform, driver, GStreamer version, who, when) to
    /// your own catalogue and prints the block to paste into a pull request
    /// against codecs.toml. That is how the catalogue reaches hardware the
    /// project does not own: whoever has the card runs this and sends the
    /// record.
    Verify {
        /// Entry id, as `gmx codec list` prints it.
        entry: String,
        /// Your name or handle, for the record. Defaults to $GMX_AUTHOR or
        /// $USER.
        #[arg(long)]
        by: Option<String>,
        /// The graphics driver version. Read off the machine when it can be.
        #[arg(long)]
        driver: Option<String>,
        #[arg(long, default_value_t = 10.0)]
        seconds: f64,
        #[arg(long, default_value_t = 1280)]
        width: i32,
        #[arg(long, default_value_t = 720)]
        height: i32,
        #[arg(long, default_value_t = 30)]
        fps: i32,
    },
    /// Fetch the catalogue from the signed release channel and install it.
    ///
    /// A driver rename or a new GPU generation is a catalogue entry, not a
    /// core release, so the catalogue ships on its own. The update is verified
    /// the same way a plugin is, parsed before it is installed, and laid over
    /// the built in catalogue and under your own `[codecs]` table.
    Update {
        /// Where the catalogue comes from: `owner/repo`, or a URL to the
        /// directory holding codecs.toml and its signature.
        #[arg(long)]
        channel: Option<String>,
        /// A particular release. Defaults to the newest.
        #[arg(long)]
        tag: Option<String>,
        /// Print what would be installed and install nothing.
        #[arg(long)]
        dry_run: bool,
    },
    /// One line per entry, each backed by a one second encode. This is what
    /// `gmx doctor` calls.
    Doctor,
}

pub fn run(cmd: Codec, cfg: Option<&Config>, codecs: Option<&Path>) -> Result<()> {
    let cat = godwinmix_core::catalogue::init(cfg, codecs)?;
    match cmd {
        Codec::List { json } => list(&cat, cfg, json),
        Codec::Test { entry, seconds, width, height, fps, json } => {
            let report = check::test_entry(&cat, &entry, seconds, width, height, fps)
                .with_context(|| format!("testing catalogue entry {entry}"))?;
            if json {
                println!("{}", serde_json::to_string_pretty(&report)?);
            } else {
                print!("{}", report.human());
                println!("\nPaste into the entry in codecs.toml, with the blanks filled in:");
                println!("{}", report.verified_toml());
            }
            if !report.ok {
                anyhow::bail!("{} did not pass: {}", report.entry, report.note);
            }
            Ok(())
        }
        Codec::Verify { entry, by, driver, seconds, width, height, fps } => {
            verify(&cat, &entry, by, driver, seconds, width, height, fps)
        }
        Codec::Update { channel, tag, dry_run } => update(channel, tag, dry_run),
        Codec::Doctor => {
            for line in check::doctor_lines(&cat, &GstRegistry) {
                println!("{line}");
            }
            Ok(())
        }
    }
}

/// `gmx codec verify <entry>`.
#[allow(clippy::too_many_arguments)]
fn verify(
    cat: &Catalogue,
    entry: &str,
    by: Option<String>,
    driver: Option<String>,
    seconds: f64,
    width: i32,
    height: i32,
    fps: i32,
) -> Result<()> {
    use godwinmix_core::catalogue::update;
    let report = check::test_entry(cat, entry, seconds, width, height, fps)
        .with_context(|| format!("testing catalogue entry {entry}"))?;
    print!("{}", report.human());
    if !report.ok {
        anyhow::bail!(
            "{} did not pass, so nothing was recorded: {}. A `verified` record says an entry \
             works on this hardware; recording a failure would say the opposite of what it \
             means. If you think this is the catalogue's fault rather than the machine's, \
             open an issue with the report above.",
            report.entry,
            report.note
        );
    }
    let by = by.unwrap_or_else(update::whoami);
    let driver = driver.unwrap_or_else(update::driver);
    let recorded = update::record_verified(entry, cat, &report, &by, &driver)?;
    println!("\nrecorded on this machine, in {}", recorded.path.display());
    println!(
        "  {} on {}, GStreamer {}, by {} on {}",
        recorded.record.report,
        recorded.record.platform,
        recorded.record.gstreamer,
        recorded.record.by,
        recorded.record.date
    );
    println!("\nSend it: open a pull request against codecs.toml with this block.");
    println!("\n{}", recorded.block);
    println!(
        "docs/how-to/add-a-codec-entry.md says what a good pull request carries besides \
         the block."
    );
    Ok(())
}

/// `gmx codec update`.
fn update(channel: Option<String>, tag: Option<String>, dry_run: bool) -> Result<()> {
    use godwinmix_core::catalogue::update;
    let channel = channel.unwrap_or_else(update::default_channel);
    if dry_run {
        println!("would fetch the catalogue from {channel} and install it at");
        println!("  {}", update::installed_path().display());
        println!("Nothing was fetched.");
        return Ok(());
    }
    println!("fetching the catalogue from {channel}");
    let installed = update::install(&channel, tag.as_deref())?;
    println!(
        "installed {} ({}, {})",
        installed.tag, installed.trust, installed.detail
    );
    println!("  {}", installed.path.display());
    println!(
        "  {} video, {} audio, {} graphics entries",
        installed.video, installed.audio, installed.graphics
    );
    for id in &installed.added {
        println!("  new      {id}");
    }
    for id in &installed.changed {
        println!("  changed  {id}");
    }
    if installed.added.is_empty() && installed.changed.is_empty() {
        println!("  nothing changed against the catalogue this core shipped with");
    }
    println!(
        "\nIt takes effect on the next start. `godwinmix --probe` says what will be chosen \
         and why; delete the file above to go back to what the core shipped with."
    );
    Ok(())
}

fn request(cfg: Option<&Config>, cat: &Catalogue) -> Request {
    match cfg {
        Some(c) => godwinmix_core::catalogue::request_from(c, cat),
        None => Request {
            container: cat.programme_container.clone().or_else(|| Some("flv".into())),
            ..Request::default()
        },
    }
}

fn list(cat: &Catalogue, cfg: Option<&Config>, json: bool) -> Result<()> {
    let listing = godwinmix_core::catalogue::listing(cat, &GstRegistry, &request(cfg, cat));
    if json {
        println!("{}", serde_json::to_string_pretty(&listing)?);
        return Ok(());
    }
    println!("catalogue on {} with GStreamer {}\n", listing.platform, listing.gstreamer);
    println!(
        "{:<26} {:<10} {:<15} {:<5} {:<22} {:<9} license",
        "entry", "kind", "accel", "rank", "elements", "here"
    );
    for e in &listing.entries {
        let elements = [e.encoder.as_deref(), e.decoder.as_deref()]
            .into_iter()
            .flatten()
            .collect::<Vec<_>>()
            .join(" / ");
        let here = if e.present { "yes" } else { "no" };
        let verified = if e.verified.is_empty() { "" } else { " verified" };
        println!(
            "{:<26} {:<10} {:<15} {:<5} {:<22} {:<9} {}{}",
            e.id, e.kind, e.accel, e.rank, elements, here, e.license, verified
        );
    }
    println!("\ncontainers");
    for c in &listing.containers {
        println!(
            "  {:<10} {:<14} video {:<22} audio {}",
            c.name,
            c.muxer,
            c.video.join(","),
            c.audio.join(",")
        );
    }
    Ok(())
}

/// What `godwinmix --probe` prints: every entry that was considered, present
/// or absent, and the rank decision that followed.
pub fn print_probe(cfg: Option<&Config>, codecs: Option<&Path>) -> Result<()> {
    let cat = godwinmix_core::catalogue::init(cfg, codecs)?;
    let req = request(cfg, &cat);
    let sel = cat.select(&req, &GstRegistry)?;
    println!(
        "GodwinMix on {} with GStreamer {}",
        godwinmix_core::catalogue::select::current_platform(),
        godwinmix_core::catalogue::gstreamer_version()
    );
    println!("programme container: {}\n", sel.container.as_deref().unwrap_or("any"));
    let mut role = String::new();
    for c in &sel.considered {
        if c.role != role {
            role.clone_from(&c.role);
            println!("{role}");
        }
        let mark = if c.chosen { "->" } else { "  " };
        let state = if c.present { "installed".to_string() } else { c.note.clone() };
        println!(
            "  {mark} {:<26} accel {:<15} rank {:<5} {:<22} {}",
            c.id, c.accel, c.rank, c.element, state
        );
    }
    println!("\nchosen");
    print_chosen(&sel);
    for problem in cat.validate() {
        println!("warning: {problem}");
    }
    Ok(())
}

fn print_chosen(sel: &Selection) {
    let pinned = |c: &godwinmix_core::catalogue::select::Chosen| {
        format!("{} ({}, entry {}, rank {})", c.element, c.accel, c.id, c.rank)
    };
    println!("  video decoder : {}", pinned(&sel.video_decode));
    println!("  video encoder : {}", pinned(&sel.video_encode));
    println!("    codec       : {}", sel.video_encode.codec);
    println!("    parser      : {}", sel.video_encode.parser.as_deref().unwrap_or("none"));
    println!("  audio decoder : {}", pinned(&sel.audio_decode));
    println!("  audio encoder : {}", pinned(&sel.audio_encode));
    println!(
        "    av offset   : {} ms of priming delay held off the video",
        sel.audio_encode.priming_delay_ms.unwrap_or(0)
    );
    println!(
        "  graphics      : {} ({}, {} + {}, memory {})",
        sel.graphics.id,
        sel.graphics.accel,
        sel.graphics.compositor,
        sel.graphics.convert,
        sel.graphics.memory
    );
    println!("    why         : {}", sel.graphics.why);
    println!(
        "\nEvery entry above came from codecs.toml. Override one with a [codecs] table in your\n\
         config, a --codecs file, or GMX_CODEC_RANK=<entry>=<rank>."
    );
}

/// Where `--codecs` points, if anywhere. Kept here so `lib.rs` carries one
/// line rather than the plumbing.
pub fn extra_path(arg: &Option<PathBuf>) -> Option<&Path> {
    arg.as_deref()
}
