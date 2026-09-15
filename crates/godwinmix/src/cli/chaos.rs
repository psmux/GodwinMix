//! `gmx chaos`: break something on purpose and watch what the programme does.
//!
//! The README's resilience table was measured by hand, with somebody typing
//! `kill -9` at the right moment. This is that, as a command, so the freeze
//! frame and the rebuild are reproduced the same way every time and a fix can
//! be shown to work.
//!
//! Two guards, because this is a loaded gun. Every subcommand needs the
//! `admin` scope, and a core that is not a harness core refuses outright
//! unless `--i-am-sure` is given. A mixer that is on air should be very hard
//! to break from a terminal.

use crate::ctl::Api;
use anyhow::{Context, Result};
use clap::Subcommand;
use serde_json::Value;

#[derive(Subcommand, Debug)]
pub enum Chaos {
    /// Kill a plugin's process, the way a segfault would.
    ///
    /// The supervisor should notice within one stall timeout, cover the gap
    /// with the freeze frame, and rebuild. The programme's frame interval must
    /// never exceed 34 ms while it does.
    Kill {
        /// The instance id, as `gmx plugin stats` prints it.
        instance: String,
        #[arg(long)]
        i_am_sure: bool,
    },
    /// Stop a plugin producing for a while, without killing it.
    ///
    /// The harder case: the process is alive and answering `health`, and no
    /// buffers are arriving. It is what a camera being unplugged looks like.
    Stall {
        instance: String,
        /// How long to hold the picture, in seconds.
        #[arg(long, default_value_t = 12)]
        secs: u64,
        #[arg(long)]
        i_am_sure: bool,
    },
}

pub async fn run(base: &str, token: Option<&str>, cmd: Chaos) -> Result<()> {
    let api = Api::new(base, token)?;
    let (instance, i_am_sure) = match &cmd {
        Chaos::Kill { instance, i_am_sure } => (instance.clone(), *i_am_sure),
        Chaos::Stall { instance, i_am_sure, .. } => (instance.clone(), *i_am_sure),
    };
    guard(&api, i_am_sure).await?;
    let found = find_instance(&api, &instance).await?;
    match cmd {
        Chaos::Kill { .. } => {
            println!(
                "killing {instance} (pid {}), plugin {}",
                found.pid.map(|p| p.to_string()).unwrap_or_else(|| "?".into()),
                found.plugin
            );
            kill(found.pid)?;
            println!("killed. Watch: gmx events --type plugin.* and gmx ctl status");
            println!(
                "The supervisor covers the gap with the freeze frame and rebuilds; the \
                 programme's frame interval must never exceed 34 ms."
            );
        }
        Chaos::Stall { secs, .. } => {
            println!(
                "stalling {instance} (pid {}) for {secs} s",
                found.pid.map(|p| p.to_string()).unwrap_or_else(|| "?".into())
            );
            stall(found.pid, secs)?;
            println!(
                "stopped and resumed. The source should have gone to `stalled` and come back \
                 without the programme losing a frame."
            );
        }
    }
    Ok(())
}

/// One instance, as `plugin.stats` reports it.
struct Found {
    plugin: String,
    pid: Option<u32>,
}

async fn find_instance(api: &Api, instance: &str) -> Result<Found> {
    let stats: Value = api.get("plugin.stats", None, &[]).await?;
    let instances = stats["instances"].as_array().cloned().unwrap_or_default();
    let found = instances.iter().find(|i| i["instance"].as_str() == Some(instance));
    let Some(found) = found else {
        let have: Vec<&str> =
            instances.iter().filter_map(|i| i["instance"].as_str()).collect();
        anyhow::bail!(
            "there is no instance called `{instance}`. Running: {}. `gmx plugin stats` lists \
             them.",
            if have.is_empty() { "none".into() } else { have.join(", ") }
        );
    };
    let pid = found["pid"].as_u64().map(|p| p as u32);
    anyhow::ensure!(
        pid.is_some(),
        "`{instance}` has no process behind it, so there is nothing to break. It is a built \
         in kind, or it has already stopped."
    );
    Ok(Found { plugin: found["plugin"].as_str().unwrap_or("?").to_string(), pid })
}

/// Refuse on a live core unless the operator insisted.
async fn guard(api: &Api, i_am_sure: bool) -> Result<()> {
    let info: Value = api.get("core.info", None, &[]).await?;
    let rehearsal = info["rehearsal"].as_bool().unwrap_or(false);
    if rehearsal || i_am_sure {
        return Ok(());
    }
    anyhow::bail!(
        "this core is not in rehearsal, so it may be on air. Run the mixer with --rehearsal, \
         or pass --i-am-sure if you really mean to break a live show."
    )
}

/// A pid the core reported that is safe to aim a signal at.
///
/// Zero is the caller's own process group and one is init, and `libc::kill`
/// takes both without complaint. A core that answers with either is a core
/// with a bug, and the right response to it is an error and not a dead shell.
fn a_real_pid(pid: Option<u32>) -> Result<u32> {
    let pid = pid.context("no pid")?;
    anyhow::ensure!(
        pid > 1,
        "the core reported pid {pid} for this instance, which is not a process this command \
         may signal (0 is our own process group, 1 is init). Read `plugin.list` to see what \
         the core thinks is running."
    );
    Ok(pid)
}

#[cfg(unix)]
fn kill(pid: Option<u32>) -> Result<()> {
    let pid = a_real_pid(pid)?;
    // SIGKILL, not SIGTERM: the point is the case where a plugin has no chance
    // to tidy up, which is what a segfault and an OOM both look like.
    let sent = unsafe { libc::kill(pid as i32, libc::SIGKILL) };
    anyhow::ensure!(sent == 0, "could not kill {pid}: {}", std::io::Error::last_os_error());
    Ok(())
}

#[cfg(unix)]
fn stall(pid: Option<u32>, secs: u64) -> Result<()> {
    let pid = a_real_pid(pid)?;
    // SIGSTOP leaves the process alive and unable to produce, which is a
    // stalled camera and not a crashed plugin. SIGCONT afterwards, always,
    // even if the wait is interrupted: a stopped process nobody resumes is a
    // worse bug than the one being reproduced.
    let stopped = unsafe { libc::kill(pid as i32, libc::SIGSTOP) };
    anyhow::ensure!(stopped == 0, "could not stop {pid}: {}", std::io::Error::last_os_error());
    std::thread::sleep(std::time::Duration::from_secs(secs));
    let resumed = unsafe { libc::kill(pid as i32, libc::SIGCONT) };
    anyhow::ensure!(resumed == 0, "could not resume {pid}: {}", std::io::Error::last_os_error());
    Ok(())
}

#[cfg(not(unix))]
fn kill(pid: Option<u32>) -> Result<()> {
    let pid = a_real_pid(pid)?;
    let status = std::process::Command::new("taskkill")
        .args(["/F", "/PID", &pid.to_string()])
        .status()
        .context("running taskkill")?;
    anyhow::ensure!(status.success(), "taskkill refused to kill {pid}");
    Ok(())
}

#[cfg(not(unix))]
fn stall(_pid: Option<u32>, _secs: u64) -> Result<()> {
    anyhow::bail!(
        "stalling a process needs SIGSTOP, which Windows has no equivalent of that is safe \
         to use here. `gmx chaos kill` works on every platform; the stall case is \
         reproduced on Linux or macOS."
    )
}
