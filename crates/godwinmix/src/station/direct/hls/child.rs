//! Starting the HLS packager process and talking to it.
//!
//! The same binary with `--hls-packager`, a secret of its own in the
//! environment, stdin held by the station (it exits when that closes) and
//! stdout read for the one line that says where it listens. On Unix it
//! leads a process group of its own, so a Ctrl-C at the terminal reaches
//! the station alone and the station stops it.

use crate::station::packager::wire::{self, Report, Want};
use crate::station::state::Launch;
use anyhow::{bail, Context, Result};
use std::net::SocketAddr;
use std::process::Stdio;
use std::time::Duration;
use tokio::io::{AsyncBufReadExt, BufReader};
use tokio::process::{Child, ChildStdin, Command};

/// How long a packager gets to say where it listens. GStreamer's first
/// start on a machine scans its plugins, which can take a while.
const HELLO: Duration = Duration::from_secs(60);
/// How long one request to it may take before it counts as hung.
const ASK: Duration = Duration::from_secs(5);

pub struct Proc {
    pub child: Child,
    pub addr: SocketAddr,
    pub secret: String,
    /// Held so the packager's stdin stays open while the station runs.
    _stdin: Option<ChildStdin>,
}

pub fn command(launch: &Launch, secret: &str) -> Command {
    let mut cmd = Command::new(&launch.exe);
    cmd.arg(wire::FLAG).args(&launch.common);
    cmd.env(wire::SECRET_ENV, secret).env("GODWINMIX_NODE", "hls-packager");
    cmd.stdin(Stdio::piped()).stdout(Stdio::piped()).stderr(Stdio::inherit());
    cmd.kill_on_drop(true);
    #[cfg(unix)]
    cmd.process_group(0);
    cmd
}

/// Start one and wait for it to say where it listens.
pub async fn start(launch: &Launch) -> Result<Proc> {
    let secret = godwinmix_core::secrets::random_key(32)?;
    let mut child = command(launch, &secret).spawn().context("starting this program as the HLS packager")?;
    let stdout = child.stdout.take().context("the packager has no stdout")?;
    let stdin = child.stdin.take();
    let mut lines = BufReader::new(stdout).lines();
    let hello = async {
        while let Some(line) = lines.next_line().await? {
            if let Some(addr) = line.strip_prefix(wire::LISTENING) {
                return addr.trim().parse::<SocketAddr>().with_context(|| format!("it said it listens on {addr:?}"));
            }
        }
        bail!("it ended before it said where it listens")
    };
    let addr = tokio::time::timeout(HELLO, hello).await.with_context(|| format!("it said nothing for {} s", HELLO.as_secs()))??;
    // Nothing else is written there, but a full pipe must never stop it.
    tokio::spawn(async move { while let Ok(Some(_)) = lines.next_line().await {} });
    Ok(Proc { child, addr, secret, _stdin: stdin })
}

impl Proc {
    fn url(&self) -> String {
        format!("http://{}{}", self.addr, wire::OUTPUTS)
    }

    /// Hand it every output it should run.
    pub async fn put(&self, http: &reqwest::Client, wants: &[Want]) -> Result<()> {
        http.put(self.url()).bearer_auth(&self.secret).json(wants).timeout(ASK).send().await?.error_for_status()?;
        Ok(())
    }

    /// What every output is doing.
    pub async fn reports(&self, http: &reqwest::Client) -> Result<Vec<Report>> {
        let answer = http.get(self.url()).bearer_auth(&self.secret).timeout(ASK).send().await?.error_for_status()?;
        Ok(answer.json().await?)
    }

    pub async fn stop(&mut self) {
        let _ = self.child.start_kill();
        let _ = self.child.wait().await;
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn a_packager_is_told_what_it_is_and_given_its_secret() {
        let launch = Launch { exe: "/bin/godwinmix".into(), common: vec!["--log-format".into(), "json".into()], calibration: None };
        let cmd = command(&launch, "s");
        let args: Vec<String> = cmd.as_std().get_args().map(|a| a.to_string_lossy().into_owned()).collect();
        assert_eq!(args, ["--hls-packager", "--log-format", "json"]);
        assert!(cmd.as_std().get_envs().any(|(k, v)| k == wire::SECRET_ENV && v.is_some()));
    }
}
