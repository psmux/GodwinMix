//! Stopping a plugin's processes, and waiting for them to go, before its
//! folder is removed or replaced.
//!
//! A plugin runs with its own folder as its working directory, and on Windows
//! neither the folder nor a file a process holds open in it can be deleted
//! while that process lives. Stopping an instance returns at once on purpose
//! (`Sidecar::shutdown` asks politely on a thread of its own and kills after a
//! grace), so `plugin.remove` deleted the folder while a Python plugin was
//! still answering its `shutdown`, and Windows said "Access is denied". A
//! reinstall of the same version hit the same wall, and a new version was
//! installed beside a singleton of the old one that nothing stopped.
//!
//! So the processes are waited for, by the pids this host started and
//! recorded, never by name: a process the host did not start is not its to
//! wait on. The wait is on the caller's blocking thread, never the mixer's.

use super::Supervisor;
use crate::plugin::loader;
use anyhow::Result;
use std::path::Path;
use std::time::{Duration, Instant};

/// How long the processes get to leave: the `stop` call, the shutdown grace,
/// and the kill after it, with a second to spare.
pub const RETIRE_WAIT: Duration = Duration::from_secs(godwinmix_host::SHUTDOWN_GRACE_SECS + 5);

impl Supervisor {
    /// Stop every process this supervisor runs for `plugin` and wait until
    /// each has exited. With `whole`, a process it does not run (a source or
    /// an output the operator added, which only the operator removes) is a
    /// refusal, made before anything is stopped.
    pub fn retire(&self, plugin: &str, reason: &str, whole: bool) -> Result<()> {
        let running = processes(plugin);
        let held: Vec<String> = running.iter().filter(|(i, _)| !self.runs(i)).map(|(i, _)| i.clone()).collect();
        if whole && !held.is_empty() {
            anyhow::bail!(
                "`{plugin}` is in use by {}, which run from its folder. Remove {} first (source.remove \
                 or output.remove), then try again; nothing was stopped.",
                held.join(", "),
                if held.len() == 1 { "it" } else { "them" }
            );
        }
        self.stop_plugin(plugin, reason);
        let deadline = Instant::now() + RETIRE_WAIT;
        let left: Vec<String> = running
            .into_iter()
            .filter(|(i, _)| !held.contains(i))
            .filter(|(_, pid)| !exited(*pid, deadline))
            .map(|(i, pid)| format!("{i} (pid {pid})"))
            .collect();
        match left.is_empty() {
            true => Ok(()),
            false => anyhow::bail!(
                "`{plugin}` was told to stop and {} {} still running after {} seconds. Stop {} by \
                 hand, then try again.",
                left.join(", "),
                if left.len() == 1 { "is" } else { "are" },
                RETIRE_WAIT.as_secs(),
                if left.len() == 1 { "it" } else { "them" }
            ),
        }
    }

    /// Whether `instance` is a singleton or a component this supervisor runs.
    fn runs(&self, instance: &str) -> bool {
        let inner = self.inner.lock();
        inner.instances.contains_key(instance) || inner.components.contains_key(instance)
    }
}

/// The processes the host started for `plugin`, by instance.
pub fn processes(plugin: &str) -> Vec<(String, u32)> {
    loader::stats().into_iter().filter(|s| s.plugin == plugin).filter_map(|s| Some((s.instance, s.pid?))).collect()
}

/// Remove a plugin folder. On Windows a handle closes a moment after its
/// process has gone, and a virus scanner may be reading a file just written,
/// so a refusal is tried again for two seconds before it is believed.
pub fn remove_folder(path: &Path) -> std::io::Result<()> {
    let deadline = Instant::now() + Duration::from_secs(2);
    loop {
        match std::fs::remove_dir_all(path) {
            Err(e) if e.kind() == std::io::ErrorKind::PermissionDenied && Instant::now() < deadline => {
                std::thread::sleep(Duration::from_millis(100));
            }
            done => return done,
        }
    }
}

/// Whether `pid` has exited by `deadline`.
#[cfg(windows)]
pub fn exited(pid: u32, deadline: Instant) -> bool {
    type Handle = *mut std::ffi::c_void;
    const SYNCHRONIZE: u32 = 0x0010_0000;
    const WAIT_OBJECT_0: u32 = 0;
    #[link(name = "kernel32")]
    extern "system" {
        fn OpenProcess(access: u32, inherit: i32, pid: u32) -> Handle;
        fn WaitForSingleObject(handle: Handle, ms: u32) -> u32;
        fn CloseHandle(handle: Handle) -> i32;
    }
    // SAFETY: a handle opened here, waited on and closed here. A null handle
    // is a process that is not there any more.
    let handle = unsafe { OpenProcess(SYNCHRONIZE, 0, pid) };
    if handle.is_null() {
        return true;
    }
    let ms = deadline.saturating_duration_since(Instant::now()).as_millis().min(u32::MAX as u128) as u32;
    let waited = unsafe { WaitForSingleObject(handle, ms) };
    unsafe { CloseHandle(handle) };
    waited == WAIT_OBJECT_0
}

/// Whether `pid` has exited by `deadline`. The undertaker reaps what it
/// kills, so a zombie does not outlast it.
#[cfg(unix)]
pub fn exited(pid: u32, deadline: Instant) -> bool {
    loop {
        // SAFETY: signal 0 checks only that the process exists.
        if unsafe { libc::kill(pid as i32, 0) } != 0 {
            return true;
        }
        if Instant::now() >= deadline {
            return false;
        }
        std::thread::sleep(Duration::from_millis(50));
    }
}
