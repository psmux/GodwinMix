//! `gmx codec list`, `gmx codec test <entry>`, and what `--probe` prints.
//!
//! All three answer the same question from different distances: what will this
//! machine encode with, and why that one. `--probe` is the whole decision on
//! one screen before any camera is pointed at the box.

use super::select::{GstRegistry, Request, Selection};
use super::{check, Catalogue};
use crate::config::Config;
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
    /// One line per entry, each backed by a one second encode. This is what
    /// `gmx doctor` calls.
    Doctor,
}

pub fn run(cmd: Codec, cfg: Option<&Config>, codecs: Option<&Path>) -> Result<()> {
    let cat = super::init(cfg, codecs)?;
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
        Codec::Doctor => {
            for line in check::doctor_lines(&cat, &GstRegistry) {
                println!("{line}");
            }
            Ok(())
        }
    }
}

fn request(cfg: Option<&Config>, cat: &Catalogue) -> Request {
    match cfg {
        Some(c) => super::request_from(c, cat),
        None => Request {
            container: cat.programme_container.clone().or_else(|| Some("flv".into())),
            ..Request::default()
        },
    }
}

fn list(cat: &Catalogue, cfg: Option<&Config>, json: bool) -> Result<()> {
    let listing = super::listing(cat, &GstRegistry, &request(cfg, cat));
    if json {
        println!("{}", serde_json::to_string_pretty(&listing)?);
        return Ok(());
    }
    println!("catalogue on {} with GStreamer {}\n", listing.platform, listing.gstreamer);
    println!(
        "{:<26} {:<10} {:<15} {:<5} {:<22} {:<9} {}",
        "entry", "kind", "accel", "rank", "elements", "here", "license"
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
    let cat = super::init(cfg, codecs)?;
    let req = request(cfg, &cat);
    let sel = cat.select(&req, &GstRegistry)?;
    println!(
        "GodwinMix on {} with GStreamer {}",
        super::select::current_platform(),
        super::gstreamer_version()
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
    let pinned = |c: &super::select::Chosen| {
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
