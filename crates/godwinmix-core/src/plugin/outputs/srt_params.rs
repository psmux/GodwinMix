//! What `srt/output` accepts in its params, checked before anything is built.

use crate::config::Params;
use anyhow::Result;

pub fn validate(params: &Params) -> Result<()> {
    for (key, value) in params {
        match key.as_str() {
            "uri" => {
                let s = value.as_str().unwrap_or_default();
                anyhow::ensure!(
                    s.to_lowercase().starts_with("srt://"),
                    "srt/output params.uri must be an srt:// address, not `{s}`"
                );
            }
            "latency_ms" => {
                let n = value.as_integer().unwrap_or(-1);
                anyhow::ensure!(
                    (0..=10_000).contains(&n),
                    "srt/output params.latency_ms must be 0 to 10000, not `{value}`"
                );
            }
            _ => {}
        }
    }
    Ok(())
}
