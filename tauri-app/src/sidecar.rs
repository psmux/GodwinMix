//! The mixer on this computer.
//!
//! The daemon is bundled beside the app as an `externalBin` sidecar, one
//! binary per target triple, and this module is the whole of its life: pick a
//! free port, hand it the config and the token, start it, pipe what it says
//! into a log file, and stop it again when the app goes away.
//!
//! Nothing here is compiled in but the names of things. The port is taken
//! from the operating system at every start and the token comes from
//! `settings`, so two copies of this app, or this app beside a mixer someone
//! started by hand, do not fight over 8080.

use std::collections::HashMap;
use std::fs::{self, OpenOptions};
use std::io::Write;
use std::net::TcpListener;
use std::path::{Path, PathBuf};
use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::Arc;
use std::time::{Duration, Instant};

use tauri::{AppHandle, Manager};
use tauri_plugin_shell::process::{CommandChild, CommandEvent};
use tauri_plugin_shell::ShellExt;

use crate::core_link::{self, Target};

/// How long the daemon gets to answer before the app gives up on it. It has a
/// config to read and a GStreamer registry to scan, which on a cold Raspberry
/// Pi is the slow part.
const START_TIMEOUT: Duration = Duration::from_secs(25);
/// How long a stopping daemon gets to close its outputs before it is killed.
/// It closes them in milliseconds when it is well; this is the budget for one
/// that is not, and it is spent while the app is on its way out.
const STOP_GRACE: Duration = Duration::from_secs(3);
/// Past this, the log is rolled over. One previous file is kept.
const LOG_MAX_BYTES: u64 = 4 * 1024 * 1024;

/// The mixer on this computer, as this app knows it: one it started, or one
/// it started in an earlier run and has found again.
pub struct Local {
    pub target: Target,
    /// The port it answers on, kept so a restart can use the same one and
    /// the page already open on it reconnects by itself.
    pub port: u16,
    pub log: PathBuf,
    /// `None` for a mixer adopted from an earlier run of the app. It can
    /// still be asked to stop over the API; it cannot be killed, because this
    /// process is not its parent.
    child: Option<CommandChild>,
    stopped: Arc<AtomicBool>,
}

impl Local {
    pub fn is_running(&self) -> bool {
        !self.stopped.load(Ordering::Relaxed)
    }

    /// The daemon's process id, for `--headless-check` and for anyone who has
    /// to look at the process list on a machine that is misbehaving.
    pub fn pid(&self) -> Option<u32> {
        self.child.as_ref().map(|c| c.pid())
    }
}

/// The mixer on this computer: the one left over from an earlier run of this
/// app if there is one, else a new one. This is the way in; `start` and
/// `adopt_existing` are the two halves of it.
pub async fn ensure(app: &AppHandle) -> Result<Local, String> {
    match adopt_existing(app).await {
        Some(found) => Ok(found),
        None => start(app).await,
    }
}

/// Start the mixer on this computer and wait until it answers.
pub async fn start(app: &AppHandle) -> Result<Local, String> {
    let port = free_port().map_err(|e| format!("no free port to give the mixer: {e}"))?;
    start_on(app, port).await
}

/// The same, on a port chosen by the caller: a restart gives the new mixer
/// the old one's port, so the page open on it reconnects without being sent
/// anywhere.
pub async fn start_on(app: &AppHandle, port: u16) -> Result<Local, String> {
    let config = crate::settings::config_path(app).map_err(|e| format!("could not find the config folder: {e}"))?;
    let token = crate::settings::local_token(app).map_err(|e| format!("could not make a token: {e}"))?;
    let log = log_file(app)?;

    let target = Target::new(format!("http://127.0.0.1:{port}"), token.clone());
    let mut env = environment(app);
    env.insert("GODWINMIX_TOKEN".into(), token);
    // The daemon colours its output for a terminal. This one goes to a file
    // that a person opens in a text editor, where the escape codes are just
    // noise around every word.
    env.insert("NO_COLOR".into(), "1".into());

    let command = app
        .shell()
        .sidecar("godwinmix")
        .map_err(|e| format!("this build has no mixer in it: {e}"))?
        .args([
            "--config".as_ref(),
            config.as_os_str(),
            "--bind".as_ref(),
            format!("127.0.0.1:{port}").as_ref(),
            // This app starts the mixer again when it exits asking for a
            // restart (see `record`), so core.restart may exit.
            "--supervised".as_ref(),
        ])
        .envs(env);

    let (rx, child) = command.spawn().map_err(|e| format!("the mixer would not start: {e}"))?;
    let stopped = Arc::new(AtomicBool::new(false));
    record(app.clone(), rx, log.clone(), stopped.clone());
    let local = Local { target, port, log, child: Some(child), stopped };

    match wait_until_answering(app, &local).await {
        Ok(()) => {
            crate::settings::remember_local_port(app, port);
            Ok(local)
        }
        Err(why) => {
            if let Some(child) = local.child {
                let _ = child.kill();
            }
            Err(why)
        }
    }
}

/// A mixer this app started in an earlier run and never stopped, because the
/// shell was killed or crashed.
///
/// The daemon outliving the window is the right way round: a broadcast does
/// not end because someone force quit a window. What must not happen is a
/// second mixer starting beside the first and taking the same camera and the
/// same encoder, so the port of the last one is written down and tried first.
/// It has to answer, and answer with this machine's token, or it is not ours
/// and a new one is started.
pub async fn adopt_existing(app: &AppHandle) -> Option<Local> {
    let port = crate::settings::last_local_port(app)?;
    let token = crate::settings::local_token(app).ok()?;
    let target = Target::new(format!("http://127.0.0.1:{port}"), token);
    let http = app.state::<crate::Shell>().http.clone();
    core_link::info(&http, &target, "this computer").await.ok()?;
    eprintln!("[desktop] found the mixer from an earlier run on {}", target.base);
    Some(Local { target, port, log: log_file(app).ok()?, child: None, stopped: Arc::new(AtomicBool::new(false)) })
}

/// Stop the mixer this app started: ask over the API first, so it closes its
/// outputs and its recordings properly, and kill it only if it will not go.
pub async fn stop(app: &AppHandle, local: Local) {
    let http = app.state::<crate::Shell>().http.clone();
    if local.is_running() {
        if let Err(e) = core_link::shutdown(&http, &local.target).await {
            eprintln!("[desktop] {e}");
        }
    }
    if let Some(child) = local.child {
        let deadline = Instant::now() + STOP_GRACE;
        while Instant::now() < deadline && !local.stopped.load(Ordering::Relaxed) {
            tokio::time::sleep(Duration::from_millis(100)).await;
        }
        if !local.stopped.load(Ordering::Relaxed) {
            eprintln!("[desktop] the mixer did not stop when asked; killing it");
        }
        // Unconditionally: a kill on a process that has already gone is a
        // no-op, and the app must not leave while its own daemon still holds
        // the encoder and the port.
        let _ = child.kill();
    }
    crate::settings::forget_local_port(app);
}

/// The last `lines` of a log, on their own lines, or nothing if it cannot be
/// read.
fn log_tail(path: &std::path::Path, lines: usize) -> String {
    let Ok(text) = std::fs::read_to_string(path) else { return String::new() };
    let kept: Vec<&str> = text.lines().rev().take(lines).collect::<Vec<_>>().into_iter().rev().collect();
    if kept.is_empty() {
        return String::new();
    }
    format!("\nThe last lines of it:\n{}", kept.join("\n"))
}

/// Poll until the daemon answers, it dies, or the clock runs out.
async fn wait_until_answering(app: &AppHandle, local: &Local) -> Result<(), String> {
    let http = app.state::<crate::Shell>().http.clone();
    let deadline = Instant::now() + START_TIMEOUT;
    let mut last = String::new();
    while Instant::now() < deadline {
        if !local.is_running() {
            // The last lines of the log go into the message: a headless check
            // on a CI runner has nowhere else to show them, and the person
            // reading a dialog wants the reason before the file name.
            return Err(format!(
                "The mixer started and stopped again. What it said is in {}.{}",
                local.log.display(),
                log_tail(&local.log, 20)
            ));
        }
        match core_link::info(&http, &local.target, "the mixer on this computer").await {
            Ok(_) => return Ok(()),
            Err(why) => last = why,
        }
        tokio::time::sleep(Duration::from_millis(250)).await;
    }
    Err(format!(
        "The mixer did not answer within {} seconds. {last} What it said is in {}.",
        START_TIMEOUT.as_secs(),
        local.log.display()
    ))
}

/// A port nobody is using, from the operating system. There is a gap between
/// letting go of it here and the daemon binding it; on a desktop machine that
/// gap is microseconds and the alternative is a compiled in number, which is
/// the thing this replaces.
fn free_port() -> std::io::Result<u16> {
    let listener = TcpListener::bind(("127.0.0.1", 0))?;
    let port = listener.local_addr()?.port();
    drop(listener);
    Ok(port)
}

/// Everything the daemon's stdout and stderr go to, appended, with one
/// previous file kept.
fn log_file(app: &AppHandle) -> Result<PathBuf, String> {
    let dir = crate::settings::log_dir(app).map_err(|e| format!("no log directory: {e}"))?;
    let path = dir.join("mixer.log");
    if fs::metadata(&path).map(|m| m.len()).unwrap_or(0) > LOG_MAX_BYTES {
        let _ = fs::rename(&path, dir.join("mixer.log.1"));
    }
    Ok(path)
}

/// Pipe the daemon's output into the log file, and notice when it exits.
/// An exit with `RESTART_EXIT_CODE` is the mixer asking to be started again
/// (`core.restart`), and it is.
///
/// A task rather than the main thread: the daemon writes a line per source
/// event and the shell must never be the reason a write blocks.
fn record(app: AppHandle, mut rx: tauri::async_runtime::Receiver<CommandEvent>, path: PathBuf, stopped: Arc<AtomicBool>) {
    tauri::async_runtime::spawn(async move {
        let mut file = OpenOptions::new().create(true).append(true).open(&path).ok();
        let mut put = |bytes: &[u8]| {
            if let Some(f) = file.as_mut() {
                let _ = f.write_all(bytes);
                let _ = f.flush();
            }
        };
        put(format!("\n--- mixer started {} ---\n", stamp()).as_bytes());
        while let Some(event) = rx.recv().await {
            match event {
                CommandEvent::Stdout(line) | CommandEvent::Stderr(line) => {
                    put(&line);
                    // The plugin hands over whole lines, and the daemon's own
                    // newline is sometimes still on the end of one.
                    if !line.ends_with(b"\n") {
                        put(b"\n");
                    }
                }
                CommandEvent::Error(why) => put(format!("[shell] {why}\n").as_bytes()),
                CommandEvent::Terminated(end) => {
                    put(format!("--- mixer exited with {:?} ---\n", end.code).as_bytes());
                    stopped.store(true, Ordering::Relaxed);
                    if end.code == Some(crate::restart::RESTART_EXIT_CODE) {
                        crate::restart::after_exit(app.clone());
                    }
                    break;
                }
                _ => {}
            }
        }
        stopped.store(true, Ordering::Relaxed);
    });
}

/// Seconds since the epoch. A desktop log wants a date, but a date needs a
/// calendar crate and this shell is not carrying one for a header line.
fn stamp() -> String {
    std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .map(|d| format!("at unix time {}", d.as_secs()))
        .unwrap_or_else(|_| "at an unknown time".into())
}

/// The environment the daemon is started in.
///
/// If the app carries device plugins, they are put in the application data
/// directory and the daemon is pointed at that with `GODWINMIX_PLUGINS_DIR`,
/// which is what gives a person who has never opened a terminal a camera, a
/// screen and a microphone to choose from. `plugin.add` from the window then
/// installs into the same directory, so the three the app brought and anything
/// added afterwards sit together. A build that carries none sets nothing and
/// the daemon reads the plugins directory it would have read anyway.
///
/// If a `gstreamer/` directory has been put in the app's resources, the
/// daemon is pointed at it and at nothing else: that is how Windows gets a
/// media stack without the operator installing a 527 MB runtime by hand, and
/// it is how any platform can carry a trimmed one. With no such directory
/// this returns almost nothing and the daemon uses the GStreamer on the
/// machine, which is what happens on macOS and Linux today.
pub fn environment(app: &AppHandle) -> HashMap<String, String> {
    let mut env = HashMap::new();
    // The registry cache goes beside the config, not next to the plugins: an
    // installed app's resources are read only on all three platforms and a
    // GStreamer that cannot write its registry rescans every plugin at every
    // start.
    if let Ok(dir) = crate::settings::data_dir(app) {
        env.insert("GST_REGISTRY".into(), dir.join("gstreamer-registry.bin").display().to_string());
    }
    if let Some(dir) = crate::plugins::ensure(app) {
        env.insert("GODWINMIX_PLUGINS_DIR".into(), dir.display().to_string());
    }
    let Some(root) = bundled_gstreamer(app) else { return env };

    let plugins = first_that_exists(&root, &["lib/gstreamer-1.0", "lib64/gstreamer-1.0", "plugins"])
        .unwrap_or_else(|| root.clone());
    let libs = first_that_exists(&root, &["lib", "lib64", "bin"]).unwrap_or_else(|| root.clone());
    let bin = first_that_exists(&root, &["bin", "libexec"]).unwrap_or_else(|| root.clone());
    let scanner = first_that_exists(&root, &["libexec/gstreamer-1.0", "lib/gstreamer-1.0"]);

    env.insert("GST_PLUGIN_PATH".into(), plugins.display().to_string());
    // Without this the daemon would also load whatever GStreamer is installed
    // on the machine, and a mixed 1.26 and 1.28 plugin set is a crash nobody
    // can read the backtrace of.
    env.insert("GST_PLUGIN_SYSTEM_PATH".into(), plugins.display().to_string());
    if let Some(scanner) = scanner {
        let name = if cfg!(windows) { "gst-plugin-scanner.exe" } else { "gst-plugin-scanner" };
        let exe = scanner.join(name);
        if exe.exists() {
            env.insert("GST_PLUGIN_SCANNER".into(), exe.display().to_string());
        }
    }
    env.insert("PATH".into(), prepend("PATH", &bin));
    // Windows finds its DLLs on PATH; the other two need their own variable.
    if cfg!(target_os = "macos") {
        env.insert("DYLD_LIBRARY_PATH".into(), prepend("DYLD_LIBRARY_PATH", &libs));
    } else if cfg!(target_os = "linux") {
        env.insert("LD_LIBRARY_PATH".into(), prepend("LD_LIBRARY_PATH", &libs));
    }
    env
}

/// `resources/gstreamer` inside the installed app, when there is really one
/// there.
///
/// `dev/bundle-gstreamer.sh` writes one tree per platform, so the per platform
/// directory is looked at first and the flat one after it, which is what a
/// tree assembled by hand tends to look like. A directory with no plugins in
/// it is not a runtime: the repository keeps `tauri-app/gstreamer/` with only
/// a `.gitignore` in it so a build without a bundled runtime still works, and
/// pointing GStreamer at that empty directory would be worse than pointing it
/// at nothing, because it would hide the one on the machine as well.
pub fn bundled_gstreamer(app: &AppHandle) -> Option<PathBuf> {
    let root = app.path().resource_dir().ok()?.join("gstreamer");
    let os = if cfg!(windows) {
        "windows"
    } else if cfg!(target_os = "macos") {
        "macos"
    } else {
        "linux"
    };
    [root.join(os), root].into_iter().find(|dir| has_plugins(dir))
}

/// A directory is a GStreamer runtime when it has a plugin directory with at
/// least one plugin in it.
fn has_plugins(root: &Path) -> bool {
    let Some(dir) = first_that_exists(root, &["lib/gstreamer-1.0", "lib64/gstreamer-1.0", "plugins"])
    else {
        return false;
    };
    fs::read_dir(dir)
        .map(|mut entries| {
            entries.any(|e| {
                e.map(|e| {
                    let name = e.file_name();
                    let name = name.to_string_lossy();
                    name.starts_with("libgst") || name.ends_with(".dll")
                })
                .unwrap_or(false)
            })
        })
        .unwrap_or(false)
}

fn first_that_exists(root: &Path, candidates: &[&str]) -> Option<PathBuf> {
    candidates.iter().map(|c| root.join(c)).find(|p| p.is_dir())
}

/// Ask the bundled GStreamer where an element comes from.
///
/// This is the only honest way to prove a bundled runtime is the one being
/// used. A tree of the right size that does not load, or a tree that loads
/// while the plugins actually answering are a GStreamer someone installed on
/// the machine, both look fine until the app reaches a computer that has no
/// GStreamer on it. `gst-inspect-1.0` prints the file each element was found
/// in, and that path either starts inside the bundle or it does not.
///
/// `Ok(path)` is the plugin file. `Err` says what went wrong in the words the
/// person reading the check needs.
pub fn inspect_element(app: &AppHandle, element: &str) -> Result<PathBuf, String> {
    let root = bundled_gstreamer(app).ok_or("no GStreamer is bundled in this app")?;
    let name = if cfg!(windows) { "gst-inspect-1.0.exe" } else { "gst-inspect-1.0" };
    let exe = first_that_exists(&root, &["bin"])
        .map(|bin| bin.join(name))
        .filter(|p| p.is_file())
        .ok_or_else(|| format!("no {name} in the bundled runtime at {}", root.display()))?;

    let mut command = std::process::Command::new(&exe);
    command.arg(element);
    // Exactly the environment the daemon is started in, so this proves what
    // the daemon will see and not what a shell happens to have.
    for (key, value) in environment(app) {
        command.env(key, value);
    }
    let out = command
        .output()
        .map_err(|e| format!("could not run {}: {e}", exe.display()))?;
    if !out.status.success() {
        return Err(format!("{element} is not in the bundled runtime"));
    }
    let text = String::from_utf8_lossy(&out.stdout);
    let file = text
        .lines()
        .find_map(|line| line.trim().strip_prefix("Filename"))
        .map(|rest| PathBuf::from(rest.trim_start_matches(':').trim()))
        .ok_or_else(|| format!("gst-inspect said nothing about where {element} lives"))?;
    if !file.starts_with(&root) {
        return Err(format!(
            "{element} came from {}, which is outside the bundle at {}",
            file.display(),
            root.display()
        ));
    }
    Ok(file)
}

fn prepend(var: &str, dir: &Path) -> String {
    let sep = if cfg!(windows) { ';' } else { ':' };
    match std::env::var(var) {
        Ok(rest) if !rest.is_empty() => format!("{}{sep}{rest}", dir.display()),
        _ => dir.display().to_string(),
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn a_free_port_is_free() {
        let port = free_port().expect("a desktop machine has a spare port");
        assert!(port > 1024, "the operating system hands out an unprivileged port");
        // Nothing is holding it, so it can be taken straight away. This is
        // the property the daemon depends on.
        TcpListener::bind(("127.0.0.1", port)).expect("the port was really let go of");
    }

    #[test]
    fn two_ports_in_a_row_are_not_the_same() {
        let a = TcpListener::bind(("127.0.0.1", 0)).unwrap();
        let b = free_port().unwrap();
        assert_ne!(a.local_addr().unwrap().port(), b);
    }

    #[test]
    fn prepending_keeps_what_was_there() {
        std::env::set_var("GMX_TEST_PATH", "/usr/bin:/bin");
        let out = prepend("GMX_TEST_PATH", Path::new("/opt/gst/bin"));
        assert!(out.starts_with("/opt/gst/bin"));
        assert!(out.ends_with("/usr/bin:/bin"));
        std::env::remove_var("GMX_TEST_PATH");
        assert_eq!(prepend("GMX_TEST_PATH", Path::new("/opt/gst/bin")), "/opt/gst/bin");
    }

    #[test]
    fn an_empty_directory_is_not_a_runtime() {
        // The repository keeps tauri-app/gstreamer/ with nothing in it but a
        // .gitignore, and the bundler copies that into every build. Treating
        // it as a runtime would set GST_PLUGIN_SYSTEM_PATH to an empty
        // directory, which hides the GStreamer on the machine as well and
        // leaves the mixer with no elements at all.
        let root = std::env::temp_dir().join("gmx-desktop-test-empty");
        let _ = fs::remove_dir_all(&root);
        fs::create_dir_all(root.join("lib/gstreamer-1.0")).unwrap();
        assert!(!has_plugins(&root), "a plugin directory with no plugins in it");
        fs::write(root.join("lib/gstreamer-1.0/libgstcoreelements.dylib"), b"x").unwrap();
        assert!(has_plugins(&root), "one plugin is enough to count");
        let _ = fs::remove_dir_all(&root);
    }

    #[test]
    fn a_windows_runtime_counts_by_its_dll_names() {
        let root = std::env::temp_dir().join("gmx-desktop-test-windows");
        let _ = fs::remove_dir_all(&root);
        fs::create_dir_all(root.join("lib/gstreamer-1.0")).unwrap();
        fs::write(root.join("lib/gstreamer-1.0/gstcoreelements.dll"), b"x").unwrap();
        assert!(has_plugins(&root), "the Windows files have no lib prefix");
        let _ = fs::remove_dir_all(&root);
    }

    #[test]
    fn the_plugin_directory_is_found_where_gstreamer_puts_it() {
        let root = std::env::temp_dir().join("gmx-desktop-test-gst");
        let plugins = root.join("lib/gstreamer-1.0");
        fs::create_dir_all(&plugins).unwrap();
        assert_eq!(
            first_that_exists(&root, &["lib/gstreamer-1.0", "plugins"]).unwrap(),
            plugins
        );
        assert_eq!(first_that_exists(&root, &["nothing/here"]), None);
        let _ = fs::remove_dir_all(&root);
    }
}
