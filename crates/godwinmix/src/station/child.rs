//! Starting and stopping one show's process.
//!
//! The same binary, told which show it is, where the station's link is, and
//! to bind a loopback port of its own choosing. On Unix it leads a process
//! group of its own, so a Ctrl-C at the terminal reaches the station alone
//! and the station stops each show in turn rather than all of them at once
//! behind its back.

use super::link::SECRET_ENV;
use super::state::Launch;
use std::net::SocketAddr;
use std::path::Path;
use std::process::Stdio;
use std::time::Duration;
use tokio::process::{Child, Command};

/// How long a show gets to stop on its own before it is killed.
pub const STOP_GRACE: Duration = Duration::from_secs(10);

/// Every log line of a show carries this, so one terminal can tell them apart.
const NODE_ENV: &str = "GODWINMIX_NODE";

pub struct Start<'a> {
    pub id: &'a str,
    pub config: &'a Path,
    pub link: SocketAddr,
    pub secret: &'a str,
    /// Its runtime directory, when the station's was set by the environment
    /// and a second show must not share it.
    pub runtime_dir: Option<std::path::PathBuf>,
}

pub fn command(launch: &Launch, start: &Start) -> Command {
    let mut cmd = Command::new(&launch.exe);
    cmd.arg("--config").arg(start.config);
    cmd.args(["--show", start.id, "--station", &start.link.to_string(), "--supervised", "--bind", "127.0.0.1:0"]);
    cmd.args(&launch.common);
    cmd.env(SECRET_ENV, start.secret);
    cmd.env(NODE_ENV, format!("show:{}", start.id));
    if let Some(dir) = &start.runtime_dir {
        cmd.env("GODWINMIX_RUNTIME_DIR", dir);
    }
    if let Some(dir) = &launch.calibration {
        cmd.env(godwinmix_core::render::CALIBRATION_ENV, dir);
    }
    // What the station's port answers HTTPS with, for the show's `core.info`.
    if let Some(json) = crate::tls::info().and_then(|info| serde_json::to_string(&info).ok()) {
        cmd.env(crate::tls::INFO_ENV, json);
    }
    cmd.stdin(Stdio::null()).stdout(Stdio::inherit()).stderr(Stdio::inherit());
    cmd.kill_on_drop(true);
    #[cfg(unix)]
    cmd.process_group(0);
    cmd
}

pub fn spawn(launch: &Launch, start: &Start) -> std::io::Result<Child> {
    command(launch, start).spawn()
}

/// Ask it to stop, give it [`STOP_GRACE`], then insist.
pub async fn stop(child: &mut Child) {
    #[cfg(unix)]
    if let Some(pid) = child.id() {
        // The same signal a Ctrl-C sends: the show fires its session end
        // hook and takes its pipelines down in order.
        unsafe {
            libc::kill(pid as i32, libc::SIGINT);
        }
        if tokio::time::timeout(STOP_GRACE, child.wait()).await.is_ok() {
            return;
        }
        // It leads its group, so this reaches the plugins it started that
        // did not lead groups of their own.
        unsafe {
            libc::kill(-(pid as i32), libc::SIGKILL);
        }
    }
    let _ = child.kill().await;
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn a_show_is_told_who_it_is_where_its_station_is_and_to_pick_its_own_port() {
        let launch = Launch { exe: "/bin/godwinmix".into(), common: vec!["--log-format".into(), "json".into()], calibration: None };
        let start = Start { id: "b", config: Path::new("/data/shows/b/godwinmix.toml"), link: "127.0.0.1:4000".parse().unwrap(), secret: "s", runtime_dir: None };
        let cmd = command(&launch, &start);
        let args: Vec<String> = cmd.as_std().get_args().map(|a| a.to_string_lossy().into_owned()).collect();
        assert_eq!(
            args,
            ["--config", "/data/shows/b/godwinmix.toml", "--show", "b", "--station", "127.0.0.1:4000", "--supervised", "--bind", "127.0.0.1:0", "--log-format", "json"]
        );
        let env: Vec<_> = cmd.as_std().get_envs().collect();
        assert!(env.iter().any(|(k, v)| *k == SECRET_ENV && v.is_some()));
    }
}
