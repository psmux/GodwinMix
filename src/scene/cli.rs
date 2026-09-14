//! `gmx scene ...` and `gmx import obs ...`.
//!
//! Everything here is one command that reads a file and writes a file or a
//! report. None of it needs a running mixer, which is deliberate: an operator
//! deciding whether to move off OBS should be able to try the import before
//! installing anything.

use std::collections::BTreeMap;
use std::path::{Path, PathBuf};

use anyhow::{bail, Context, Result};
use clap::{Subcommand, ValueEnum};

use super::document::{Canvas, Collection};
use super::flat::FlatDocument;
use super::obs_import::{self, Outcome};
use super::{layout, schema, validate};

/// How a report comes out.
#[derive(Debug, Clone, Copy, PartialEq, Eq, ValueEnum)]
pub enum ReportFormat {
    /// Lines for a person.
    Text,
    /// One JSON object, for a script or an agent.
    Json,
}

/// `gmx import <what>`.
#[derive(Subcommand, Debug)]
pub enum Import {
    /// Read an OBS Studio scene collection and write a source list and a scene
    /// document.
    Obs {
        /// The collection JSON exported from OBS (Scene Collection, Export).
        file: PathBuf,
        /// Where to write the source list. Nothing is written without this.
        #[arg(long, value_name = "FILE")]
        out: Option<PathBuf>,
        /// Where to write the scene document. Nothing is written without this.
        #[arg(long, value_name = "FILE")]
        scenes: Option<PathBuf>,
        /// The canvas to import onto. OBS keeps this in its profile, not in
        /// the collection, so it cannot be read from the file.
        #[arg(long, value_name = "WIDTHxHEIGHT")]
        canvas: Option<String>,
        /// How big a source is, when the collection does not say. Repeatable:
        /// --source-size "CAM 1=1920x1080".
        #[arg(long, value_name = "NAME=WIDTHxHEIGHT")]
        source_size: Vec<String>,
        /// text or json.
        #[arg(long, value_enum, default_value_t = ReportFormat::Text)]
        report: ReportFormat,
    },
}

/// `gmx scene <what>`.
#[derive(Subcommand, Debug)]
pub enum Scene {
    /// Report overlaps, off canvas items and safe area breaches in a document.
    Validate {
        file: PathBuf,
        /// text or json.
        #[arg(long, value_enum, default_value_t = ReportFormat::Text)]
        report: ReportFormat,
    },
    /// Resolve a built in layout into a concrete scene and print it.
    Layout {
        /// The layout's name. Omit it with --list to see them all.
        #[arg(required_unless_present = "list")]
        name: Option<String>,
        /// a=cam1,b=cam2,inset=0.4
        #[arg(long, default_value = "")]
        values: String,
        /// The canvas to resolve against.
        #[arg(long, value_name = "WIDTHxHEIGHT", default_value = "1920x1080")]
        canvas: String,
        /// Write to a file instead of stdout.
        #[arg(short, long, value_name = "FILE")]
        out: Option<PathBuf>,
        /// List the built in layouts and what each one takes.
        #[arg(long)]
        list: bool,
    },
    /// Read the flat record store and print the nested document.
    ExportTree {
        file: PathBuf,
        #[arg(short, long, value_name = "FILE")]
        out: Option<PathBuf>,
    },
    /// Read a nested document and print the flat record store.
    ImportTree {
        file: PathBuf,
        #[arg(short, long, value_name = "FILE")]
        out: Option<PathBuf>,
    },
    /// Print the JSON Schema for the scene document.
    Schema {
        /// The schema for the flat record store instead of the document.
        #[arg(long)]
        flat: bool,
    },
}

/// Run `gmx import ...`.
pub fn run_import(cmd: Import) -> Result<()> {
    let Import::Obs { file, out, scenes, canvas, source_size, report } = cmd;
    let text = read(&file)?;
    let options = obs_import::Options {
        canvas: canvas.as_deref().map(parse_canvas).transpose()?,
        source_sizes: parse_sizes(&source_size)?,
    };
    let imported = obs_import::import(&text, &options)
        .with_context(|| format!("importing {}", file.display()))?;
    if let Some(path) = &out {
        write(path, &imported.to_config_toml()?)?;
    }
    if let Some(path) = &scenes {
        write(path, &imported.document.to_json())?;
    }
    match report {
        ReportFormat::Json => println!("{}", serde_json::to_string_pretty(&imported.report)?),
        ReportFormat::Text => print!("{}", render_report(&imported, out.as_deref(), scenes.as_deref())),
    }
    Ok(())
}

/// Run `gmx scene ...`.
pub fn run_scene(cmd: Scene) -> Result<()> {
    match cmd {
        Scene::Validate { file, report } => validate_file(&file, report),
        Scene::Layout { name, values, canvas, out, list } => {
            if list {
                return list_layouts();
            }
            let name = name.expect("clap requires a name without --list");
            let doc = layout::builtin(&name)?;
            let values = layout::parse_values(&values)?;
            let canvas = parse_canvas(&canvas)?;
            let scene = layout::apply(&doc, &values, canvas)?;
            let mut resolved = Collection::new(doc.name.clone(), canvas);
            resolved.scenes.push(scene);
            emit(out.as_deref(), &resolved.to_json())
        }
        Scene::ExportTree { file, out } => {
            let flat = FlatDocument::from_json(&read(&file)?)
                .with_context(|| format!("reading the record store in {}", file.display()))?;
            emit(out.as_deref(), &flat.to_tree()?.to_json())
        }
        Scene::ImportTree { file, out } => {
            let doc = Collection::from_json(&read(&file)?)
                .with_context(|| format!("reading the scene document in {}", file.display()))?;
            emit(out.as_deref(), &doc.to_flat().to_json())
        }
        Scene::Schema { flat } => {
            print!("{}", if flat { schema::generate_flat() } else { schema::generate() });
            Ok(())
        }
    }
}

/// Validate one document and print what was found.
fn validate_file(file: &Path, report: ReportFormat) -> Result<()> {
    let doc = Collection::from_json(&read(file)?)
        .with_context(|| format!("reading the scene document in {}", file.display()))?;
    let findings = validate::collection(&doc);
    match report {
        ReportFormat::Json => println!("{}", serde_json::to_string_pretty(&findings)?),
        ReportFormat::Text => match validate::summary(&findings) {
            None => println!(
                "{}: {} scene(s), nothing to report.",
                doc.name,
                doc.scenes.len()
            ),
            Some(summary) => {
                println!("{}: {summary}.", doc.name);
                for f in &findings {
                    println!("  {:?} {}: {}", f.severity, f.code, f.message);
                }
            }
        },
    }
    if validate::has_errors(&findings) {
        bail!("the document has errors. Fix the ones marked Error above; the rest are advice.");
    }
    Ok(())
}

/// Print the built in layouts and what each one takes.
fn list_layouts() -> Result<()> {
    for name in layout::NAMES {
        let doc = layout::builtin(name)?;
        let properties = doc.params.get("properties").and_then(serde_json::Value::as_object);
        let params: Vec<String> = properties
            .into_iter()
            .flatten()
            .map(|(key, schema)| {
                let kind = schema
                    .get("x-gmx-kind")
                    .and_then(serde_json::Value::as_str)
                    .or_else(|| schema.get("type").and_then(serde_json::Value::as_str))
                    .unwrap_or("string");
                match schema.get("default") {
                    Some(d) if !d.as_str().is_some_and(str::is_empty) => {
                        format!("{key} ({kind}, default {d})")
                    }
                    _ => format!("{key} ({kind})"),
                }
            })
            .collect();
        println!("{name:<16} {}", doc.scenes[0].name);
        println!("{:<16} {}", "", params.join(", "));
    }
    Ok(())
}

/// The import report as lines a person reads.
fn render_report(
    imported: &obs_import::Import,
    config: Option<&Path>,
    scenes: Option<&Path>,
) -> String {
    let r = &imported.report;
    let mut out = format!(
        "Imported {:?}: {} scene(s), {} item(s), canvas {}x{}.\n",
        r.collection, r.scenes, r.items, r.canvas.width, r.canvas.height
    );
    out.push_str("\nSources\n");
    for source in &r.sources {
        let used = match source.placements {
            0 => "in no scene".to_string(),
            1 => "in 1 item".to_string(),
            n => format!("in {n} items"),
        };
        let line = match &source.outcome {
            Outcome::Imported { r#type, id } => format!("imported as {type} {id:?}, {used}"),
            Outcome::NeedsPlugin { r#type, plugin, id } => {
                format!("imported as {type} {id:?}, {used}: needs the {plugin} plugin")
            }
            Outcome::Skipped { reason, placeholder } => match placeholder {
                Some(p) => format!("skipped, because {reason}; left {p}"),
                None => format!("skipped, because {reason}"),
            },
        };
        out.push_str(&format!("  {:<24} {:<22} {line}\n", source.obs_name, source.obs_type));
    }
    if !r.filters_duplicated.is_empty() {
        out.push_str("\nFilters copied onto each placement\n");
        for f in &r.filters_duplicated {
            out.push_str(&format!(
                "  {:?} ({}) on {:?} -> {} item(s): {}\n",
                f.filter,
                f.obs_type,
                f.source,
                f.placements.len(),
                f.placements.join(", ")
            ));
        }
        out.push_str(
            "  A filter belongs to the placement here, not to the source, so these copies are\n  independent from now on: changing one does not change the others.\n",
        );
    }
    if !r.notes.is_empty() {
        out.push_str("\nWorth knowing\n");
        for note in &r.notes {
            out.push_str(&format!("  {note}\n"));
        }
    }
    out.push('\n');
    match (config, scenes) {
        (None, None) => out.push_str(
            "Nothing was written. Add --out godwinmix.toml and --scenes scenes.json to keep it.\n",
        ),
        _ => {
            for path in [config, scenes].into_iter().flatten() {
                out.push_str(&format!("Wrote {}\n", path.display()));
            }
        }
    }
    out
}

/// `NAME=WIDTHxHEIGHT` pairs off the command line.
fn parse_sizes(given: &[String]) -> Result<BTreeMap<String, (f64, f64)>> {
    let mut out = BTreeMap::new();
    for pair in given {
        let (name, size) = pair.split_once('=').with_context(|| {
            format!("{pair:?} is not a source size. Write it as --source-size \"NAME=WIDTHxHEIGHT\".")
        })?;
        let canvas = parse_canvas(size)?;
        out.insert(name.to_string(), (canvas.width as f64, canvas.height as f64));
    }
    Ok(out)
}

fn parse_canvas(text: &str) -> Result<Canvas> {
    Canvas::parse(text).map_err(|e| anyhow::anyhow!(e))
}

fn read(path: &Path) -> Result<String> {
    std::fs::read_to_string(path)
        .with_context(|| format!("could not read {}. Check the path.", path.display()))
}

fn write(path: &Path, text: &str) -> Result<()> {
    if let Some(parent) = path.parent().filter(|p| !p.as_os_str().is_empty()) {
        std::fs::create_dir_all(parent)
            .with_context(|| format!("could not make the directory {}", parent.display()))?;
    }
    std::fs::write(path, text).with_context(|| format!("could not write {}", path.display()))
}

/// Print, or write to a file when the operator asked for one.
fn emit(out: Option<&Path>, text: &str) -> Result<()> {
    match out {
        Some(path) => {
            write(path, text)?;
            println!("Wrote {}", path.display());
            Ok(())
        }
        None => {
            print!("{text}");
            Ok(())
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn source_size_hints_are_read_off_the_command_line() {
        let sizes = parse_sizes(&["CAM 1 (Studio)=1920x1080".into(), "phone=1080x1920".into()])
            .unwrap();
        assert_eq!(sizes["CAM 1 (Studio)"], (1920.0, 1080.0));
        assert_eq!(sizes["phone"], (1080.0, 1920.0));
        let err = parse_sizes(&["nonsense".into()]).unwrap_err().to_string();
        assert!(err.contains("NAME=WIDTHxHEIGHT"), "{err}");
    }

    #[test]
    fn every_layout_is_listed_with_its_parameters() {
        // The listing walks every built in layout, so this catches a layout
        // whose params block is missing before an operator does.
        list_layouts().unwrap();
    }
}
