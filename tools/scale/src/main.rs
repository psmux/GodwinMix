//! gmx-scale: the tools a headend benchmark is run with.
//!
//! ```text
//! gmx-scale feeds   send N looped MPEG-TS files to N groups or ports, paced by PCR
//! gmx-scale check   receive N UDP streams and count packets, CC errors, PCR and GOPs
//! gmx-scale add     add the shows a feeds list names, with show.add_many or show.add
//! gmx-scale sample  CPU and memory of a station and everything under it, and show.stats, each second
//! gmx-scale report  put the run's numbers in one markdown table
//! gmx-scale call    one method on the station, timed
//! ```
//!
//! `gmx-scale <command> --help` says what each takes. dev/bench/scale.sh runs
//! them in order; docs/how-to/benchmark-at-scale.md says how to read the result.

mod args;
mod check;
mod feeds;
mod net;
mod procs;
mod station;
mod ts;

use std::process::ExitCode;

const USAGE: &str = "usage: gmx-scale <feeds|check|add|sample|report|call> [options]. \
gmx-scale <command> --help lists the options of one.";

fn main() -> ExitCode {
    let mut it = std::env::args().skip(1);
    let Some(cmd) = it.next() else {
        eprintln!("{USAGE}");
        return ExitCode::from(2);
    };
    let rest: Vec<String> = it.collect();
    let run = match cmd.as_str() {
        "feeds" => feeds::main,
        "check" => check::main,
        "add" => station::add::main,
        "call" => station::call::main,
        "sample" => station::sample::main,
        "report" => station::report::main,
        "-h" | "--help" | "help" => {
            println!("{USAGE}");
            return ExitCode::SUCCESS;
        }
        other => {
            eprintln!("there is no command {other}. {USAGE}");
            return ExitCode::from(2);
        }
    };
    match args::Args::parse(rest).and_then(run) {
        Ok(()) => ExitCode::SUCCESS,
        Err(e) => {
            eprintln!("gmx-scale {cmd}: {e}");
            ExitCode::FAILURE
        }
    }
}
