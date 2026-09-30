//! Run every scenario, one after another, and print the table.

use std::collections::HashMap;
use std::process::Child;

use godwinmix_framebus::monotonic_ns;

use crate::table::{self, Row};
use crate::{clip, procs, Args};

#[derive(Clone, Copy, PartialEq, Eq)]
pub enum Mech {
    Bus,
    Unixfd,
    Decode,
}

pub struct Scenario {
    pub mech: Mech,
    pub readers: usize,
    pub stalled: bool,
}

pub const SEC: u64 = 1_000_000_000;

pub fn run(args: &Args) {
    let secs = args.num("seconds", 10);
    let decoder = args.get("decoder", "avdec_h264");
    let format = args.get("format", "NV12");
    let counts: Vec<usize> = args
        .get("readers", "1,4,8")
        .split(',')
        .filter_map(|n| n.parse().ok())
        .collect();
    let clip = clip::make(secs + 12);
    let mut plan = vec![Scenario {
        mech: Mech::Bus,
        readers: 0,
        stalled: false,
    }];
    for mech in [Mech::Bus, Mech::Decode, Mech::Unixfd] {
        for &n in &counts {
            plan.push(Scenario {
                mech,
                readers: n,
                stalled: false,
            });
            if mech != Mech::Decode {
                plan.push(Scenario {
                    mech,
                    readers: n,
                    stalled: true,
                });
            }
        }
    }
    let repeat = args.num("repeat", 1).max(1);
    let mut rows = vec![];
    for s in &plan {
        let label = table::label(s);
        // Other work on the machine moves single runs by a few percent of a
        // core; with --repeat the run with the median total CPU is kept.
        let mut runs: Vec<Row> = (0..repeat)
            .map(|i| {
                eprintln!("running: {label} ({} of {repeat})", i + 1);
                Row::from(
                    s,
                    label.clone(),
                    one(s, &clip.display().to_string(), &decoder, &format, secs),
                )
            })
            .collect();
        runs.sort_by(|a, b| a.total().total_cmp(&b.total()));
        rows.push(runs.swap_remove(runs.len() / 2));
    }
    let text = table::render(&rows, secs, &decoder, &format, repeat);
    println!("{text}");
    let out = args.get("out", "");
    if !out.is_empty() {
        std::fs::write(&out, &text).unwrap();
    }
}

/// Run one scenario and return every process's `BENCH` line as a map.
fn one(
    s: &Scenario,
    clip: &str,
    decoder: &str,
    format: &str,
    secs: u64,
) -> Vec<HashMap<String, String>> {
    let tag = format!("{}-{}", std::process::id(), monotonic_ns() % 100_000);
    let dir = format!("/tmp/gmxfb-bench-{tag}");
    let socket = format!("/tmp/gmxfb-bench-{tag}.sock");
    let t0 = monotonic_ns() + 5 * SEC;
    let t1 = t0 + secs * SEC;
    let common = |role: &str| -> Vec<String> {
        let mech = if s.mech == Mech::Unixfd {
            "unixfd"
        } else {
            "bus"
        };
        [
            role,
            "--mechanism",
            mech,
            "--clip",
            clip,
            "--decoder",
            decoder,
            "--format",
            format,
            "--dir",
            &dir,
            "--socket",
            &socket,
            "--t0",
            &t0.to_string(),
            "--t1",
            &t1.to_string(),
            "--until",
            &(t1 + 2 * SEC).to_string(),
        ]
        .iter()
        .map(|a| a.to_string())
        .collect()
    };
    let mut kids: Vec<Child> = vec![];
    if s.mech != Mech::Decode {
        kids.push(procs::spawn(common("owner")));
        std::thread::sleep(std::time::Duration::from_millis(500));
    }
    for i in 0..s.readers {
        let role = if s.mech == Mech::Decode {
            "decode"
        } else {
            "reader"
        };
        let mut a = common(role);
        if s.stalled && i == 0 {
            a.extend(["--stall-ms".to_string(), "1000".to_string()]);
        }
        kids.push(procs::spawn(a));
    }
    procs::kill_after(&kids, t1 + 10 * SEC);
    let results = kids.into_iter().filter_map(procs::collect).collect();
    let _ = std::fs::remove_dir_all(&dir);
    let _ = std::fs::remove_file(&socket);
    results
}
