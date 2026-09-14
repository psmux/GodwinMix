//! `[run]` and `[build]` turned into a command line, an environment and a
//! working directory.
//!
//! One key of `[run]` wins per platform, and the manifest validator has
//! already refused a manifest with more than one set. What is left here is
//! choosing the binary for this machine, finding the interpreter, and writing
//! down the `GMX_*` variables of 03 section 4 so that every plugin, in every
//! language, reads the same names.

use godwinmix_protocol::plugin::manifest::{Build, Manifest, Run};
use std::collections::BTreeMap;
use std::path::{Path, PathBuf};

/// Which of the four `[run]` keys this plugin uses.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Runtime {
    /// A binary per platform. Run exactly as declared, no extra arguments.
    Bin,
    /// `.venv/bin/python <entry>`, the venv made when the plugin was added.
    Python,
    /// `node <entry>`.
    Node,
    /// `sh <entry>` on Unix; refused on Windows unless a `bin` entry exists.
    Shell,
}

impl Runtime {
    pub const fn as_str(self) -> &'static str {
        match self {
            Self::Bin => "bin",
            Self::Python => "python",
            Self::Node => "node",
            Self::Shell => "shell",
        }
    }
}

/// What the core needs to know to start one instance.
#[derive(Debug, Clone)]
pub struct LaunchCtx {
    /// The directory the plugin was installed into.
    pub root: PathBuf,
    /// The provide id within the plugin, `source`.
    pub provide: String,
    /// The instance id, `cam1`. A device or service singleton is named after
    /// its provide.
    pub instance: String,
    /// The core's `api_level`.
    pub api_level: u32,
    /// The per instance token, scoped `plugin:<name>`.
    pub token: String,
    /// WebSocket URL of the core's `/rpc`, for a plugin that wants to call
    /// core methods outside its stdio channel.
    pub rpc: String,
    /// The unixfd or shm address. Empty in container mode, and empty until the
    /// transport has been negotiated.
    pub media: String,
}

/// A command ready to be spawned.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Launch {
    pub argv: Vec<String>,
    pub env: BTreeMap<String, String>,
    pub cwd: PathBuf,
    pub runtime: Runtime,
}

impl Launch {
    /// The command as one line, for a log or an error message.
    pub fn command_line(&self) -> String {
        self.argv.join(" ")
    }
}

/// The platform triple this build runs on, in the manifest's spelling.
///
/// `plugin.platforms` and `run.bin` are both keyed by it, so a plugin that
/// shipped no asset for this machine is refused with a message naming the ones
/// it does have rather than failing at exec time.
pub fn this_platform() -> &'static str {
    match (std::env::consts::OS, std::env::consts::ARCH) {
        ("linux", "x86_64") => "linux-x86_64",
        ("linux", "aarch64") => "linux-aarch64",
        ("linux", "arm") => "linux-armv7",
        ("macos", "aarch64") => "macos-aarch64",
        ("macos", "x86_64") => "macos-x86_64",
        ("windows", "x86_64") => "windows-x86_64",
        ("windows", "aarch64") => "windows-aarch64",
        // Something the table does not carry. Named rather than guessed, so
        // the error says what this machine is.
        _ => "unsupported",
    }
}

/// Where a python plugin's interpreter lives once `gmx plugin add` has made
/// the venv.
pub fn venv_python(root: &Path) -> PathBuf {
    if cfg!(windows) {
        root.join(".venv").join("Scripts").join("python.exe")
    } else {
        root.join(".venv").join("bin").join("python")
    }
}

/// Build the command line for one instance.
///
/// Errors name the key that is wrong and what this machine would accept,
/// because "failed to start plugin" sends an author looking in the wrong file.
pub fn plan(manifest: &Manifest, ctx: &LaunchCtx) -> anyhow::Result<Launch> {
    let run = manifest.run.as_ref().ok_or_else(|| {
        anyhow::anyhow!(
            "{} has no [run] table, so there is nothing to start. A sidecar plugin needs one \
             of bin, python, node or shell.",
            manifest.plugin.name
        )
    })?;
    let (runtime, argv) = argv_for(manifest, run, &ctx.root)?;
    Ok(Launch { argv, env: environment(manifest, ctx), cwd: ctx.root.clone(), runtime })
}

fn argv_for(manifest: &Manifest, run: &Run, root: &Path) -> anyhow::Result<(Runtime, Vec<String>)> {
    let here = this_platform();
    if !run.bin.is_empty() {
        let path = run.bin.get(here).ok_or_else(|| {
            let have: Vec<&str> = run.bin.keys().map(String::as_str).collect();
            anyhow::anyhow!(
                "{} has no binary for {here}. It ships: {}. Install it from a git source with \
                 a [build] section, or ask the author for this platform.",
                manifest.plugin.name,
                have.join(", ")
            )
        })?;
        let full = root.join(path);
        anyhow::ensure!(
            full.exists(),
            "{} declares run.bin.{here} = \"{path}\" but {} is not there. Reinstall the plugin.",
            manifest.plugin.name,
            full.display()
        );
        return Ok((Runtime::Bin, vec![full.to_string_lossy().into_owned()]));
    }
    if let Some(entry) = &run.python {
        let python = venv_python(root);
        let interpreter = if python.exists() {
            python.to_string_lossy().into_owned()
        } else {
            // No venv: a plugin added by path in a checkout that manages its
            // own environment. Say which interpreter is being used rather than
            // silently picking a different one from the one `add` prepared.
            std::env::var("GMX_PYTHON").unwrap_or_else(|_| "python3".into())
        };
        return Ok((Runtime::Python, vec![interpreter, path_arg(root, entry)]));
    }
    if let Some(entry) = &run.node {
        let node = std::env::var("GMX_NODE").unwrap_or_else(|_| "node".into());
        return Ok((Runtime::Node, vec![node, path_arg(root, entry)]));
    }
    if let Some(entry) = &run.shell {
        anyhow::ensure!(
            cfg!(unix),
            "{} is a shell plugin and this is Windows, where `sh` is not a given. The manifest \
             needs a run.bin entry for windows-x86_64, or the plugin needs another runtime.",
            manifest.plugin.name
        );
        return Ok((Runtime::Shell, vec!["sh".into(), path_arg(root, entry)]));
    }
    anyhow::bail!(
        "{}'s [run] table sets none of bin, python, node or shell.",
        manifest.plugin.name
    )
}

fn path_arg(root: &Path, entry: &str) -> String {
    root.join(entry).to_string_lossy().into_owned()
}

/// The environment of 03 section 4. Everything else is inherited from the
/// core, as `exec:` does today.
pub fn environment(manifest: &Manifest, ctx: &LaunchCtx) -> BTreeMap<String, String> {
    let mut env = BTreeMap::new();
    env.insert("GMX_PLUGIN".into(), manifest.plugin.name.clone());
    env.insert("GMX_PROVIDE".into(), ctx.provide.clone());
    env.insert("GMX_INSTANCE".into(), ctx.instance.clone());
    env.insert("GMX_API_LEVEL".into(), ctx.api_level.to_string());
    env.insert("GMX_PLUGIN_ROOT".into(), ctx.root.to_string_lossy().into_owned());
    env.insert("GMX_TOKEN".into(), ctx.token.clone());
    env.insert("GMX_RPC".into(), ctx.rpc.clone());
    // Empty in container mode, and empty until the handshake has settled the
    // transport. A plugin reads it after `initialize`, not before.
    env.insert("GMX_MEDIA".into(), ctx.media.clone());
    env
}

/// The preparation `gmx plugin add` does for each runtime, as a list of
/// commands to run in the plugin root.
///
/// Returned rather than run so that the caller decides about a task id, a
/// progress line and a timeout, and so that this is testable with no network.
pub fn preparation(manifest: &Manifest, root: &Path) -> Vec<Vec<String>> {
    let Some(run) = manifest.run.as_ref() else { return Vec::new() };
    let mut steps = Vec::new();
    if run.python.is_some() {
        let python = std::env::var("GMX_PYTHON").unwrap_or_else(|_| "python3".into());
        if which("uv").is_some() {
            steps.push(vec!["uv".into(), "venv".into(), ".venv".into()]);
        } else {
            steps.push(vec![python, "-m".into(), "venv".into(), ".venv".into()]);
        }
        let pip = venv_python(root).to_string_lossy().into_owned();
        if root.join("pyproject.toml").exists() {
            steps.push(vec![pip, "-m".into(), "pip".into(), "install".into(), ".".into()]);
        } else if root.join("requirements.txt").exists() {
            steps.push(vec![
                pip,
                "-m".into(),
                "pip".into(),
                "install".into(),
                "-r".into(),
                "requirements.txt".into(),
            ]);
        }
    }
    if run.node.is_some() && root.join("package-lock.json").exists() {
        steps.push(vec!["npm".into(), "ci".into(), "--omit=dev".into()]);
    }
    steps
}

/// The `[build]` step for a plugin that came from git, if it has one.
pub fn build_step(build: &Build) -> Vec<String> {
    // The command is a line in the manifest, not an argv, because that is how
    // an author writes `cargo build --release`.
    build.command.split_whitespace().map(str::to_string).collect()
}

/// Where a binary is, without a crate for it.
pub fn which(name: &str) -> Option<PathBuf> {
    let exe = if cfg!(windows) { format!("{name}.exe") } else { name.to_string() };
    std::env::var_os("PATH").and_then(|paths| {
        std::env::split_paths(&paths).map(|dir| dir.join(&exe)).find(|p| p.is_file())
    })
}

#[cfg(test)]
mod tests {
    use super::*;

    fn manifest(run: &str) -> Manifest {
        let text = format!(
            r#"
[plugin]
name = "clock"
version = "0.1.0"
api = 1
description = "A clock"
license = "MIT"
platforms = ["linux-x86_64", "linux-aarch64", "macos-aarch64", "macos-x86_64", "windows-x86_64"]
placements = ["sidecar"]

{run}

[[provides]]
kind = "source"
id = "source"
media = {{ video = "raw", audio = "none" }}
transports = ["container"]
settings = "settings.json"
"#
        );
        Manifest::parse(&text).expect("the test manifest parses")
    }

    fn ctx(root: &Path) -> LaunchCtx {
        LaunchCtx {
            root: root.to_path_buf(),
            provide: "source".into(),
            instance: "cam1".into(),
            api_level: 1,
            token: "tok-abc".into(),
            rpc: "ws://127.0.0.1:8080/rpc".into(),
            media: String::new(),
        }
    }

    #[test]
    fn the_environment_carries_every_documented_variable() {
        let m = manifest("[run]\npython = \"main.py\"");
        let env = environment(&m, &ctx(Path::new("/plugins/clock/0.1.0")));
        for key in [
            "GMX_PLUGIN",
            "GMX_PROVIDE",
            "GMX_INSTANCE",
            "GMX_API_LEVEL",
            "GMX_PLUGIN_ROOT",
            "GMX_TOKEN",
            "GMX_RPC",
            "GMX_MEDIA",
        ] {
            assert!(env.contains_key(key), "{key} is missing from the plugin environment");
        }
        assert_eq!(env["GMX_PLUGIN"], "clock");
        assert_eq!(env["GMX_INSTANCE"], "cam1");
        assert_eq!(env["GMX_MEDIA"], "");
    }

    #[test]
    fn a_python_plugin_runs_the_entry_under_an_interpreter() {
        let m = manifest("[run]\npython = \"main.py\"");
        let plan = plan(&m, &ctx(Path::new("/plugins/clock/0.1.0"))).expect("a python plan");
        assert_eq!(plan.runtime, Runtime::Python);
        assert_eq!(plan.argv.len(), 2);
        assert!(plan.argv[1].ends_with("main.py"), "{:?}", plan.argv);
    }

    #[test]
    fn a_binary_for_another_platform_names_what_it_does_ship() {
        let m = manifest(r#"[run]
bin = { "linux-armv7" = "bin/clock" }"#);
        let err = plan(&m, &ctx(Path::new("/plugins/clock/0.1.0")))
            .expect_err("this machine is not armv7");
        let text = format!("{err}");
        assert!(text.contains("linux-armv7"), "{text}");
        assert!(text.contains(this_platform()), "{text}");
    }

    #[test]
    fn this_platform_is_one_the_manifest_vocabulary_knows() {
        let known = godwinmix_protocol::plugin::manifest::PLATFORMS;
        assert!(
            known.contains(&this_platform()) || this_platform() == "unsupported",
            "{} is neither a known triple nor the honest fallback",
            this_platform()
        );
    }

    #[cfg(unix)]
    #[test]
    fn a_shell_plugin_runs_under_sh() {
        let m = manifest("[run]\nshell = \"run.sh\"");
        let plan = plan(&m, &ctx(Path::new("/plugins/clock/0.1.0"))).expect("a shell plan");
        assert_eq!(plan.runtime, Runtime::Shell);
        assert_eq!(plan.argv[0], "sh");
    }

    #[test]
    fn a_python_plugin_makes_a_venv_before_it_runs() {
        let m = manifest("[run]\npython = \"main.py\"");
        let steps = preparation(&m, Path::new("/plugins/clock/0.1.0"));
        assert!(!steps.is_empty(), "a python plugin needs a venv");
        assert!(steps[0].iter().any(|a| a == "venv" || a == ".venv"), "{steps:?}");
    }
}
