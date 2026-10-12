//! Which GStreamer plugins this process has loaded, for the `other` column.
//!
//! A plugin's shared library and everything it links against stay mapped
//! once it is loaded, so the list says where an idle core's resident memory
//! went without anyone having to read `/proc/<pid>/smaps` on the runner. On
//! Ubuntu loading `libav` alone pulls in FFmpeg and what it links (speech
//! synthesis, SVG, libplacebo, x265), which was most of the gap between an
//! idle core and its budget.

use gstreamer as gst;

/// The names of every loaded plugin, sorted.
pub fn loaded() -> Vec<String> {
    let mut names: Vec<String> = gst::Registry::get()
        .plugins()
        .into_iter()
        .filter(|p| p.is_loaded())
        .map(|p| p.plugin_name().to_string())
        .collect();
    names.sort();
    names
}

/// `12 plugins loaded: app, audiomixer, ...`, for a row.
pub fn loaded_line() -> String {
    let names = loaded();
    format!("{} plugins loaded: {}", names.len(), names.join(", "))
}

/// Anonymous and file backed resident memory in MB, from `/proc/self/status`.
/// Anonymous is frames, queues and heap; file backed is mapped libraries. Only
/// Linux says, so elsewhere this is `None` and the rows leave it out.
pub fn rss_split() -> Option<(f64, f64)> {
    let status = std::fs::read_to_string("/proc/self/status").ok()?;
    let mb = |key: &str| -> Option<f64> {
        let line = status.lines().find(|l| l.starts_with(key))?;
        line.split_whitespace().nth(1)?.parse::<f64>().ok().map(|kb| kb / 1024.0)
    };
    Some((mb("RssAnon:")?, mb("RssFile:")?))
}
