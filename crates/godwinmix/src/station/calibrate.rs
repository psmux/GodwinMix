//! Calibration in a process of its own: how a station asks for one, and the
//! child that takes it. See `godwinmix_core::render::station::apart` for why.

use anyhow::{Context, Result};
use godwinmix_core::config::Config;
use godwinmix_core::render::{Apart, Station};
use std::path::Path;

/// How to start a child that calibrates with the same config and catalogue
/// as this process. None when this program cannot find itself, which leaves
/// the calibration in process as before.
pub fn apart(config: &Path, codecs: Option<&Path>) -> Option<Apart> {
    let exe = std::env::current_exe().ok()?;
    let mut args = vec!["--config".to_string(), config.display().to_string()];
    if let Some(codecs) = codecs {
        args.push("--codecs".into());
        args.push(codecs.display().to_string());
    }
    Some(Apart { exe, args })
}

/// The child: read the config for the catalogue and the encoder pin, measure,
/// keep the result in `dir`, and exit non zero when nothing was kept.
pub fn run(config: &Path, codecs: Option<&Path>, dir: &Path) -> Result<()> {
    let cfg = Config::load(config).ok();
    godwinmix_core::catalogue::init(cfg.as_ref(), codecs).context("reading the codec catalogue")?;
    let pin = cfg.map(|c| c.hardware.encode).unwrap_or_default();
    match Station::calibrate_here(pin, dir) {
        true => Ok(()),
        false => anyhow::bail!("the calibration was not kept in {}; the station measures again on its next start", dir.display()),
    }
}
