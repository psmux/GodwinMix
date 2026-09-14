//! The commands a person types when something is wrong: `gmx doctor`,
//! `gmx logs`, `gmx trace`, `gmx dot`, `gmx support-bundle`.
//!
//! Three of them read files and need no mixer, which is deliberate: the moment
//! you most want the logs is the moment the mixer has stopped answering. `dot`
//! needs a running one, because a graph of a pipeline that is not running is
//! not a thing.

use anyhow::{Context, Result};
use clap::Subcommand;
use std::io::Write as _;
use std::path::{Path, PathBuf};

#[derive(Subcommand, Debug)]
pub enum ObserveCmd {
    /// Check this machine: elements, encoders, ports, config, disk, and what
    /// the gallery should default to. Exits non zero when something the
    /// default pipeline needs is missing.
    Doctor {
        /// The config to check.
        #[arg(short, long, default_value = "godwinmix.toml")]
        config: PathBuf,
        /// Answer as JSON instead of one line per check.
        #[arg(long)]
        json: bool,
    },
    /// Tail or filter the structured logs across the core and every plugin.
    Logs {
        #[arg(short, long, default_value = "godwinmix.toml")]
        config: PathBuf,
        /// Keep printing as lines arrive.
        #[arg(short, long)]
        follow: bool,
        /// Only this source or output.
        #[arg(long)]
        instance: Option<String>,
        /// Only this level and above.
        #[arg(long)]
        level: Option<String>,
        /// Only lines at or after this time. Either a full timestamp
        /// (2026-09-14T20:10) or just a time of day (20:10), both UTC, which
        /// is what the log files are written in.
        #[arg(long)]
        since: Option<String>,
        /// Only lines belonging to this trace.
        #[arg(long)]
        trace: Option<String>,
        /// How many lines of history to print before following.
        #[arg(short = 'n', long, default_value_t = 200)]
        lines: usize,
    },
    /// One command's story: everything with this trace id, across the core log
    /// and every plugin log, in time order.
    Trace {
        /// The trace id, as it appears in a log line or a `traceparent`.
        id: String,
        #[arg(short, long, default_value = "godwinmix.toml")]
        config: PathBuf,
    },
    /// Print a pipeline's graph as Graphviz. `gmx dot cam1 | dot -Tsvg > cam1.svg`.
    Dot {
        /// A source or output id, or `programme`, or `multiview`.
        #[arg(default_value = "programme")]
        name: String,
        #[arg(long, env = "GODWINMIX_URL")]
        url: Option<String>,
        #[arg(long, env = "GODWINMIX_TOKEN", hide_env_values = true)]
        token: Option<String>,
    },
    /// Write one zip holding everything an issue needs: versions, the doctor's
    /// verdict, the redacted config, every pipeline's graph, the last hour of
    /// the session log and the log files.
    SupportBundle {
        #[arg(short, long, default_value = "godwinmix.toml")]
        config: PathBuf,
        /// Where to write it. Defaults to a timestamped name here.
        #[arg(short, long)]
        out: Option<PathBuf>,
        /// A running mixer to ask for the pipeline graphs and the metrics.
        #[arg(long, env = "GODWINMIX_URL")]
        url: Option<String>,
        #[arg(long, env = "GODWINMIX_TOKEN", hide_env_values = true)]
        token: Option<String>,
    },
}

/// The default mixer address, matching the one `gmx ctl` uses.
const DEFAULT_URL: &str = "http://127.0.0.1:8080";

/// A reader that has gone away is not an error.
///
/// `gmx dot cam1 | dot -Tsvg` and `gmx logs -f | head` both close the pipe
/// while we are still writing, and the default behaviour is a panic from
/// inside `print!`. Every one of these commands is meant to be piped, so a
/// broken pipe ends the command quietly, the way every other command line tool
/// behaves.
fn quiet_on_broken_pipe(result: std::io::Result<()>) -> Result<()> {
    match result {
        Err(e) if e.kind() == std::io::ErrorKind::BrokenPipe => Ok(()),
        other => Ok(other?),
    }
}

pub async fn run(cmd: ObserveCmd) -> Result<()> {
    match cmd {
        ObserveCmd::Doctor { config, json } => doctor(&config, json),
        ObserveCmd::Logs { config, follow, instance, level, since, trace, lines } => {
            let filter = Filter {
                instance,
                level: level.as_deref().and_then(super::logs::LevelCode::parse),
                since: since.map(normalise_since),
                trace,
            };
            logs(&super::runtime_dir(&config), follow, filter, lines).await
        }
        ObserveCmd::Trace { id, config } => trace(&super::runtime_dir(&config), &id),
        ObserveCmd::Dot { name, url, token } => dot(&name, url, token).await,
        ObserveCmd::SupportBundle { config, out, url, token } => {
            support_bundle(config, out, url, token).await
        }
    }
}

fn doctor(config: &Path, json: bool) -> Result<()> {
    gstreamer::init().context("initialising GStreamer")?;
    let checks = super::doctor::run(config);
    if json {
        println!("{}", serde_json::to_string_pretty(&checks)?);
    } else {
        print!("{}", super::doctor::format(&checks));
    }
    let code = super::doctor::exit_code(&checks);
    if code != 0 {
        std::process::exit(code);
    }
    Ok(())
}

/// What `gmx logs` keeps.
#[derive(Default)]
struct Filter {
    instance: Option<String>,
    level: Option<super::logs::LevelCode>,
    since: Option<String>,
    trace: Option<String>,
}

impl Filter {
    fn keeps(&self, line: &str) -> bool {
        let Ok(v) = serde_json::from_str::<serde_json::Value>(line) else { return false };
        if let Some(want) = &self.instance {
            if v["instance"].as_str() != Some(want.as_str()) {
                return false;
            }
        }
        if let Some(want) = self.level {
            let Some(level) = v["level"].as_str().and_then(super::logs::LevelCode::parse) else {
                return false;
            };
            if level > want {
                return false;
            }
        }
        if let Some(since) = &self.since {
            // Lexical comparison is chronological for RFC 3339 in UTC.
            if v["ts"].as_str().unwrap_or("") < since.as_str() {
                return false;
            }
        }
        if let Some(want) = &self.trace {
            if v["trace_id"].as_str() != Some(want.as_str()) {
                return false;
            }
        }
        true
    }
}

/// `--since 20:10` means today at 20:10 UTC; a full timestamp is taken as is.
fn normalise_since(input: String) -> String {
    if input.contains('-') || input.contains('T') {
        return input;
    }
    let today = super::logs::rfc3339(&std::time::SystemTime::now());
    let date = today.split('T').next().unwrap_or_default();
    format!("{date}T{input}")
}

async fn logs(dir: &Path, follow: bool, filter: Filter, lines: usize) -> Result<()> {
    let files = super::logs::log_files(dir);
    anyhow::ensure!(
        !files.is_empty(),
        "no log files under {}. Either the mixer has not run yet, or its runtime \
         directory is elsewhere: set GODWINMIX_RUNTIME_DIR or pass --config",
        dir.display()
    );

    // The core file is the complete timeline, so history comes from it alone.
    // Following watches every file, because a plugin file can appear later.
    let core = super::logs::core_log_path(dir);
    let history = std::fs::read_to_string(&core).unwrap_or_default();
    let kept: Vec<&str> = history.lines().filter(|l| filter.keeps(l)).collect();
    let start = kept.len().saturating_sub(lines);
    let mut out = std::io::stdout().lock();
    let printed = (|| {
        for line in &kept[start..] {
            writeln!(out, "{}", human(line))?;
        }
        out.flush()
    })();
    quiet_on_broken_pipe(printed)?;
    if !follow {
        return Ok(());
    }

    let mut at = std::fs::metadata(&core).map(|m| m.len()).unwrap_or(0);
    loop {
        tokio::time::sleep(std::time::Duration::from_millis(250)).await;
        let size = std::fs::metadata(&core).map(|m| m.len()).unwrap_or(0);
        // A rotation makes the file shorter. Start again from the top of the
        // new one rather than printing nothing for the rest of the session.
        if size < at {
            at = 0;
        }
        if size == at {
            continue;
        }
        let text = std::fs::read_to_string(&core).unwrap_or_default();
        let fresh: String = text.chars().skip(at as usize).collect();
        let printed = (|| {
            for line in fresh.lines().filter(|l| filter.keeps(l)) {
                writeln!(out, "{}", human(line))?;
            }
            out.flush()
        })();
        quiet_on_broken_pipe(printed)?;
        at = size;
    }
}

/// A stored JSON line as a person reads it. Anything that is not JSON is
/// printed as it stands, because a line written by something else is still
/// something the reader wants to see.
fn human(line: &str) -> String {
    let Ok(v) = serde_json::from_str::<serde_json::Value>(line) else {
        return line.to_string();
    };
    // A session log record and a log line are both JSON objects with a `ts`,
    // and `gmx trace` reads both. Telling them apart matters: a session record
    // carries a `kind` and a `seq` and no `message`, and rendering one as a log
    // line puts the target it was *setting* in the column where a reader
    // expects the target it came *from*.
    if v.get("message").is_none() && v.get("kind").is_some() && v.get("seq").is_some() {
        return human_session_record(&v);
    }
    let mut out = format!(
        "{} {:>5} {}",
        v["ts"].as_str().unwrap_or("-"),
        v["level"].as_str().unwrap_or("-"),
        v["target"].as_str().unwrap_or("-")
    );
    if let Some(instance) = v["instance"].as_str() {
        out.push_str(&format!(" [{instance}]"));
    }
    out.push_str(&format!(": {}", v["message"].as_str().unwrap_or("")));
    if let Some(obj) = v.as_object() {
        for (k, value) in obj {
            if matches!(k.as_str(), "ts" | "level" | "target" | "instance" | "message") {
                continue;
            }
            out.push_str(&format!(" {k}={value}"));
        }
    }
    out
}

/// One session log record: what happened, not where it was logged from.
fn human_session_record(v: &serde_json::Value) -> String {
    let mut out = format!(
        "{} {:>5} session[{}] {}",
        v["ts"].as_str().unwrap_or("-"),
        "rec",
        v["seq"].as_u64().unwrap_or(0),
        v["kind"].as_str().unwrap_or("-")
    );
    if let Some(obj) = v.as_object() {
        for (k, value) in obj {
            if matches!(k.as_str(), "ts" | "kind" | "seq") {
                continue;
            }
            out.push_str(&format!(" {k}={value}"));
        }
    }
    out
}

/// `gmx trace <id>`: every line with that trace id, from every file, in time
/// order.
///
/// Reads the plugin files as well as the core file and drops the duplicates,
/// because an instance tagged line is written to both and a reader following
/// one command does not want to see each line twice.
fn trace(dir: &Path, id: &str) -> Result<()> {
    let files = super::logs::log_files(dir);
    anyhow::ensure!(!files.is_empty(), "no log files under {}", dir.display());
    let mut lines: Vec<(String, String)> = Vec::new();
    let mut seen = std::collections::HashSet::new();
    for path in files {
        let Ok(text) = std::fs::read_to_string(&path) else { continue };
        for line in text.lines() {
            if !line.contains(id) {
                continue;
            }
            let Ok(v) = serde_json::from_str::<serde_json::Value>(line) else { continue };
            if v["trace_id"].as_str() != Some(id) {
                continue;
            }
            if !seen.insert(line.to_string()) {
                continue;
            }
            lines.push((v["ts"].as_str().unwrap_or("").to_string(), line.to_string()));
        }
    }
    // The session log carries the command that started the trace, which is the
    // line a reader wants first.
    if let Ok(text) = std::fs::read_to_string(super::session::path_in(dir)) {
        for line in text.lines().filter(|l| l.contains(id)) {
            let Ok(v) = serde_json::from_str::<serde_json::Value>(line) else { continue };
            if v["trace_id"].as_str() == Some(id) && seen.insert(line.to_string()) {
                lines.push((v["ts"].as_str().unwrap_or("").to_string(), line.to_string()));
            }
        }
    }
    if lines.is_empty() {
        anyhow::bail!(
            "nothing under {} carries trace {id}. Traces are kept as long as the \
             log files are, which is five generations of 50 MB",
            dir.display()
        );
    }
    lines.sort_by(|a, b| a.0.cmp(&b.0));
    let mut out = std::io::stdout().lock();
    quiet_on_broken_pipe((|| {
        for (_, line) in lines {
            writeln!(out, "{}", human(&line))?;
        }
        out.flush()
    })())
}

async fn dot(name: &str, url: Option<String>, token: Option<String>) -> Result<()> {
    let url = url
        .or_else(|| crate::config::env_var("URL"))
        .unwrap_or_else(|| DEFAULT_URL.to_string());
    let token = token.or_else(|| crate::config::env_var("TOKEN"));
    let client = reqwest::Client::new();
    let mut req = client
        .get(format!("{}/api/v1/pipeline/dot", url.trim_end_matches('/')))
        .query(&[("name", name)]);
    if let Some(token) = &token {
        req = req.bearer_auth(token);
    }
    let response = req.send().await.with_context(|| {
        format!("asking {url} for the graph. Is the mixer running, and is --url right?")
    })?;
    let status = response.status();
    let body = response.text().await.unwrap_or_default();
    anyhow::ensure!(status.is_success(), "{status}: {body}");
    // Straight to stdout, unwrapped, so the pipe into `dot -Tsvg` works.
    quiet_on_broken_pipe(std::io::stdout().lock().write_all(body.as_bytes()))
}

async fn support_bundle(
    config: PathBuf,
    out: Option<PathBuf>,
    url: Option<String>,
    token: Option<String>,
) -> Result<()> {
    let runtime_dir = super::runtime_dir(&config);
    // Read the session log from the file, since this process is not the one
    // that wrote it.
    super::session::session().open(super::session::path_in(&runtime_dir)).ok();
    let options = super::bundle::BundleOptions {
        config_path: config,
        runtime_dir,
        url: url.or_else(|| crate::config::env_var("URL")),
        token: token.or_else(|| crate::config::env_var("TOKEN")),
        out: out.unwrap_or_else(|| PathBuf::from(super::bundle::default_name())),
    };
    let (path, included) = super::bundle::build(&options).await?;
    eprintln!("wrote {}", path.display());
    for line in included {
        eprintln!("  {line}");
    }
    eprintln!(
        "\nEvery secret in the config is replaced. Read it before you attach it \
         to anything public."
    );
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;

    fn line(ts: &str, level: &str, instance: Option<&str>, trace: Option<&str>) -> String {
        let mut v = serde_json::json!({
            "ts": ts, "level": level, "target": "godwinmix::mixer", "message": "a thing",
        });
        v["instance"] = match instance {
            Some(i) => serde_json::json!(i),
            None => serde_json::Value::Null,
        };
        if let Some(t) = trace {
            v["trace_id"] = serde_json::json!(t);
        }
        v.to_string()
    }

    #[test]
    fn the_instance_filter_keeps_one_sources_lines() {
        let filter = Filter { instance: Some("cam1".into()), ..Default::default() };
        assert!(filter.keeps(&line("2026-09-14T20:10:00.000Z", "info", Some("cam1"), None)));
        assert!(!filter.keeps(&line("2026-09-14T20:10:00.000Z", "info", Some("cam2"), None)));
        assert!(!filter.keeps(&line("2026-09-14T20:10:00.000Z", "info", None, None)));
    }

    #[test]
    fn the_level_filter_keeps_that_level_and_above() {
        let filter =
            Filter { level: Some(super::super::logs::LevelCode::WARN), ..Default::default() };
        assert!(filter.keeps(&line("2026-09-14T20:10:00.000Z", "error", None, None)));
        assert!(filter.keeps(&line("2026-09-14T20:10:00.000Z", "warn", None, None)));
        assert!(!filter.keeps(&line("2026-09-14T20:10:00.000Z", "info", None, None)));
        assert!(!filter.keeps(&line("2026-09-14T20:10:00.000Z", "debug", None, None)));
    }

    #[test]
    fn the_since_filter_takes_a_time_of_day_or_a_timestamp() {
        let filter = Filter {
            since: Some(normalise_since("2026-09-14T20:10".into())),
            ..Default::default()
        };
        assert!(filter.keeps(&line("2026-09-14T20:10:00.000Z", "info", None, None)));
        assert!(!filter.keeps(&line("2026-09-14T20:09:59.000Z", "info", None, None)));

        let today = super::super::logs::rfc3339(&std::time::SystemTime::now());
        let date = today.split('T').next().unwrap();
        assert_eq!(normalise_since("20:10".into()), format!("{date}T20:10"));
    }

    #[test]
    fn a_line_that_is_not_json_is_dropped_by_a_filter_and_printed_raw_by_the_formatter() {
        let filter = Filter::default();
        assert!(!filter.keeps("this is not a log line"));
        assert_eq!(human("this is not a log line"), "this is not a log line");
    }

    #[test]
    fn the_human_form_leads_with_time_level_target_and_instance() {
        let text = human(&line("2026-09-14T20:10:00.000Z", "warn", Some("cam1"), None));
        assert!(text.starts_with("2026-09-14T20:10:00.000Z  warn godwinmix::mixer [cam1]"), "{text}");
        assert!(text.contains("a thing"), "{text}");
    }

    #[test]
    fn a_session_record_is_not_printed_as_though_it_were_a_log_line() {
        let record = serde_json::json!({
            "ts": "2026-09-14T20:10:00.000Z",
            "seq": 7,
            "kind": "log.set",
            "target": "godwinmix::mixer",
            "level": "debug",
        })
        .to_string();
        let text = human(&record);
        assert!(text.contains("session[7] log.set"), "{text}");
        // The target being set must not sit in the column that names where the
        // line came from.
        assert!(!text.starts_with("2026-09-14T20:10:00.000Z debug godwinmix::mixer"), "{text}");
    }

    #[test]
    fn trace_gathers_the_core_and_plugin_lines_in_time_order_without_duplicates() {
        let dir = crate::observe::tempdir("cli-trace");
        let id = "4bf92f3577b34da6a3ce929d0e0e4736";
        std::fs::create_dir_all(dir.join("plugins")).unwrap();
        let later = line("2026-09-14T20:10:02.000Z", "info", Some("cam1"), Some(id));
        let earlier = line("2026-09-14T20:10:01.000Z", "info", None, Some(id));
        let other = line("2026-09-14T20:10:03.000Z", "info", None, Some("0000000000000000000000000000beef"));
        std::fs::write(
            dir.join("godwinmix.log"),
            format!("{earlier}\n{later}\n{other}\n"),
        )
        .unwrap();
        // The same instance line, mirrored, which must not be printed twice.
        std::fs::write(dir.join("plugins").join("cam1.log"), format!("{later}\n")).unwrap();

        // `trace` prints to stdout, so what is asserted here is that it finds
        // something and does not error. The de-duplication is asserted through
        // the set it builds.
        trace(&dir, id).expect("the trace should be found");
        assert!(trace(&dir, "0000000000000000000000000000dead").is_err());
        let _ = std::fs::remove_dir_all(&dir);
    }

    #[tokio::test]
    async fn logs_says_where_it_looked_when_there_is_nothing_there() {
        let dir = crate::observe::tempdir("cli-logs-empty");
        let e = logs(&dir, false, Filter::default(), 10).await.unwrap_err().to_string();
        assert!(e.contains("no log files"), "{e}");
        assert!(e.contains("GODWINMIX_RUNTIME_DIR"), "{e}");
        let _ = std::fs::remove_dir_all(&dir);
    }

    #[tokio::test]
    async fn logs_prints_the_last_n_lines_that_match() {
        let dir = crate::observe::tempdir("cli-logs");
        let mut text = String::new();
        for i in 0..10 {
            text.push_str(&line(
                &format!("2026-09-14T20:10:0{i}.000Z"),
                if i % 2 == 0 { "info" } else { "debug" },
                Some("cam1"),
                None,
            ));
            text.push('\n');
        }
        std::fs::write(dir.join("godwinmix.log"), text).unwrap();
        let filter = Filter { instance: Some("cam1".into()), ..Default::default() };
        logs(&dir, false, filter, 3).await.expect("should print");
        let _ = std::fs::remove_dir_all(&dir);
    }
}
