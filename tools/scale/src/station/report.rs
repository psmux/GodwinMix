//! `gmx-scale report`: the run's JSON files turned into one markdown page.

use super::tables;
use crate::args::Args;
use serde_json::Value;
use std::fmt::Write as _;
use std::process::Command;

const HELP: &str = "
gmx-scale report --dir RUN [--out FILE]

Reads what a run left in RUN (run.json, feeds.json, feeds-run.json, add.json, sample.json,
check.json, and check-in.json when the feeds were checked straight from the
generator) and writes one markdown page: the machine, the version, the
numbers the wave 4 contract asks for, CPU and memory by role, and the worst
streams. Missing files are said to be missing, never guessed.

  --dir RUN        the run's folder
  --out FILE       where to write the page (stdout when absent)
";

pub fn main(a: Args) -> Result<(), String> {
    if a.help(HELP) {
        return Ok(());
    }
    let dir = a.need("dir")?;
    let read = |name: &str| -> Value {
        std::fs::read_to_string(format!("{dir}/{name}")).ok().and_then(|t| serde_json::from_str(&t).ok()).unwrap_or(Value::Null)
    };
    let run = read("run.json");
    let feeds = [read("feeds.json"), read("feeds-run.json")];
    let page = render(&run, &feeds, &read("add.json"), &read("sample.json"), &read("check.json"), &read("check-in.json"));
    match a.str("out") {
        Some(path) => std::fs::write(path, page).map_err(|e| format!("could not write {path}: {e}"))?,
        None => print!("{page}"),
    }
    Ok(())
}

fn render(run: &Value, feeds: &[Value; 2], add: &Value, sample: &Value, check: &Value, check_in: &Value) -> String {
    let mut s = String::new();
    let _ = writeln!(s, "# Scale run: {}\n", run["title"].as_str().unwrap_or("headend"));
    let _ = writeln!(s, "| | |\n|---|---|");
    let _ = writeln!(s, "| Machine | {} |", machine());
    let _ = writeln!(s, "| Version | {} |", run["version"].as_str().unwrap_or("not recorded"));
    let _ = writeln!(s, "| Commit | {} |", run["commit"].as_str().unwrap_or("not recorded"));
    let _ = writeln!(s, "| Date | {} |", run["date"].as_str().unwrap_or("not recorded"));
    let _ = writeln!(s, "| Mode | {} |", run["mode"].as_str().unwrap_or("not recorded"));
    let _ = writeln!(s, "| Load average before the run | {} |", run["load"].as_str().unwrap_or("not recorded"));
    let _ = writeln!(s, "| Feeds | {} of {}, {} s measured |", run["feeds"], run["clips"].as_str().unwrap_or("?"), run["seconds"]);
    if let Some(n) = run["orphans"].as_u64().filter(|_| !sample.is_null()) {
        let _ = writeln!(s, "| Processes still running 15 s after SIGTERM to the station | {n} |");
    }
    let _ = writeln!(s, "| Command | `{}` |\n", run["command"].as_str().unwrap_or("not recorded"));
    if let Some(note) = run["note"].as_str().filter(|n| !n.is_empty()) {
        let _ = writeln!(s, "{note}\n");
    }
    let _ = writeln!(s, "## The numbers\n");
    s.push_str(&tables::headline(feeds, add, sample, check, check_in));
    if !sample.is_null() {
        let _ = writeln!(s, "\n## CPU and memory by role\n");
        let _ = writeln!(s, "Percent of one core, sampled once a second. `total` is the station and everything under it.\n");
        s.push_str(&tables::roles(&sample["roles"]));
    }
    let streams = if check.is_null() { &check_in["streams"] } else { &check["streams"] };
    if !streams.is_null() {
        let _ = writeln!(s, "\n## The worst streams received\n");
        s.push_str(&tables::worst(streams, 10));
    }
    if let Some(refused) = add["refused"].as_array().filter(|r| !r.is_empty()) {
        let _ = writeln!(s, "\n## Refused when adding\n");
        for r in refused.iter().take(10) {
            let _ = writeln!(s, "* {}: {}", r["name"].as_str().unwrap_or("?"), r["why"].as_str().unwrap_or(""));
        }
    }
    s
}

/// The CPU model, cores and memory, from the system's own tools.
fn machine() -> String {
    let run = |cmd: &str, args: &[&str]| Command::new(cmd).args(args).output().ok().map(|o| String::from_utf8_lossy(&o.stdout).trim().to_string()).unwrap_or_default();
    if cfg!(target_os = "macos") {
        let mem: f64 = run("sysctl", &["-n", "hw.memsize"]).parse().unwrap_or(0.0);
        let os = run("sw_vers", &["-productVersion"]);
        format!("{}, {} cores, {:.0} GiB, macOS {os}", run("sysctl", &["-n", "machdep.cpu.brand_string"]), run("sysctl", &["-n", "hw.ncpu"]), mem / 1073741824.0)
    } else {
        let cpu = std::fs::read_to_string("/proc/cpuinfo").unwrap_or_default();
        let model = cpu.lines().find(|l| l.starts_with("model name")).and_then(|l| l.split(':').nth(1)).unwrap_or("unknown CPU").trim().to_string();
        format!("{model}, {} cores, {}", run("nproc", &[]), run("uname", &["-sr"]))
    }
}
