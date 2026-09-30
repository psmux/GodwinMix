//! The frame bus benchmark. `dev/framebus-bench.sh` runs the whole matrix;
//! each role below is one process in it.
//!
//!   framebus-bench matrix [--seconds 10] [--readers 1,4,8] [--decoder avdec_h264]
//!   framebus-bench owner  --mechanism bus|unixfd --clip F --t0 NS --t1 NS ...
//!   framebus-bench reader --mechanism bus|unixfd --t0 NS --t1 NS [--stall-ms 1000]
//!   framebus-bench decode --clip F --t0 NS --t1 NS
//!
//! Every process measures its own CPU over the same window, [t0, t1] on the
//! system monotonic clock, and prints one `BENCH key=value ...` line.

use std::collections::HashMap;

mod clip;
mod matrix;
mod measure;
mod owner;
mod reader;
mod table;

/// `--key value` pairs.
pub struct Args(HashMap<String, String>);

impl Args {
    fn parse(list: &[String]) -> Args {
        let mut m = HashMap::new();
        let mut it = list.iter();
        while let Some(k) = it.next() {
            if let Some(k) = k.strip_prefix("--") {
                m.insert(k.to_string(), it.next().cloned().unwrap_or_default());
            }
        }
        Args(m)
    }

    pub fn get(&self, k: &str, default: &str) -> String {
        self.0.get(k).cloned().unwrap_or_else(|| default.to_string())
    }

    pub fn num(&self, k: &str, default: u64) -> u64 {
        self.0.get(k).and_then(|v| v.parse().ok()).unwrap_or(default)
    }
}

fn main() {
    let argv: Vec<String> = std::env::args().collect();
    let role = argv.get(1).cloned().unwrap_or_default();
    let args = Args::parse(&argv[2.min(argv.len())..]);
    gstreamer::init().expect("GStreamer did not initialise");
    godwinmix_framebus::gst::register().expect("could not register the frame bus elements");
    match role.as_str() {
        "matrix" => matrix::run(&args),
        "owner" => owner::run(&args),
        "reader" => reader::run(&args),
        "decode" => reader::decode(&args),
        _ => {
            eprintln!("usage: framebus-bench matrix [--seconds 10] [--readers 1,4,8] [--decoder avdec_h264] [--out FILE]");
            std::process::exit(2);
        }
    }
}
