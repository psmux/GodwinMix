//! `gmx-plugin.toml`: the types, the parser and the validator.
//!
//! The validator reports every problem it finds, each with the key path that
//! caused it, so an author or an agent fixes the whole file in one pass instead
//! of one error per run. It is the same check the conformance harness runs
//! (03 section 11, check 7).

use std::collections::BTreeMap;
use std::path::Path;

use serde::{Deserialize, Serialize};
use serde_json::Value;

use super::wire::Transport;

/// A parsed `gmx-plugin.toml`.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Manifest {
    pub plugin: PluginMeta,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub run: Option<Run>,
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub provides: Vec<Provide>,
    #[serde(default, rename = "tools", skip_serializing_if = "Vec::is_empty")]
    pub tools: Vec<Tool>,
    #[serde(default, skip_serializing_if = "BTreeMap::is_empty")]
    pub hooks: BTreeMap<String, Hook>,
    /// Only on a git source; `gmx plugin add` runs it.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub build: Option<Build>,
}

/// The `[plugin]` table.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct PluginMeta {
    /// The namespace. Every id from this plugin is prefixed `<name>/`.
    pub name: String,
    pub version: String,
    /// The protocol level this plugin was written against.
    pub api: u32,
    #[serde(default)]
    pub description: String,
    #[serde(default)]
    pub license: String,
    #[serde(default)]
    pub authors: Vec<String>,
    #[serde(default)]
    pub repository: String,
    #[serde(default)]
    pub platforms: Vec<String>,
    #[serde(default)]
    pub placements: Vec<String>,
    #[serde(default = "default_process")]
    pub process: String,
}

fn default_process() -> String {
    "per-instance".into()
}

/// The `[run]` table. One key wins per platform.
#[derive(Debug, Clone, Default, Serialize, Deserialize)]
pub struct Run {
    /// Platform triple to path.
    #[serde(default, skip_serializing_if = "BTreeMap::is_empty")]
    pub bin: BTreeMap<String, String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub python: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub node: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub shell: Option<String>,
}

impl Run {
    /// How many runtime keys are set. Exactly one is required.
    pub fn keys_set(&self) -> usize {
        usize::from(!self.bin.is_empty())
            + usize::from(self.python.is_some())
            + usize::from(self.node.is_some())
            + usize::from(self.shell.is_some())
    }
}

/// The `[build]` table, used when the plugin came from git.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Build {
    pub command: String,
    pub output: String,
}

/// One `[[provides]]` block.
#[derive(Debug, Clone, Default, Serialize, Deserialize)]
pub struct Provide {
    pub kind: String,
    pub id: String,
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub uri_schemes: Vec<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub rank: Option<u32>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub media: Option<Media>,
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub transports: Vec<Transport>,
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub capabilities: Vec<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub latency_ms: Option<u32>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub settings: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub skill: Option<String>,
    /// `filter` only: which insertion points it may sit at.
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub sides: Vec<String>,
    /// `device` only.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub discovery: Option<Value>,
    /// `panel` only.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub panel: Option<Panel>,
    /// `surface` only.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub surface: Option<Value>,
    /// `preset` only.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub preset: Option<Value>,
    /// `graphic` only.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub graphic: Option<String>,
    /// `collection` only.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub collection: Option<String>,
    /// `encoder` only: entries merged into the codec catalogue.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub codecs: Option<String>,
    /// `[provides.designer]`, rendered by every designer client (11 section 5).
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub designer: Option<Designer>,
}

/// `media = { video = "raw", audio = "raw", alpha = false, thumb = true }`.
#[derive(Debug, Clone, Default, Serialize, Deserialize)]
pub struct Media {
    #[serde(default = "media_none")]
    pub video: String,
    #[serde(default = "media_none")]
    pub audio: String,
    #[serde(default)]
    pub alpha: bool,
    #[serde(default)]
    pub thumb: bool,
}

fn media_none() -> String {
    "none".into()
}

impl Media {
    pub fn has_video(&self) -> bool {
        self.video != "none"
    }
    pub fn has_audio(&self) -> bool {
        self.audio != "none"
    }
}

/// `panel = { kind, entry, slots }`.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Panel {
    pub kind: String,
    pub entry: String,
    #[serde(default)]
    pub slots: Vec<String>,
}

/// `[provides.designer]`: what a designer client needs to place this thing.
#[derive(Debug, Clone, Default, Serialize, Deserialize)]
pub struct Designer {
    /// The icon for the add input gallery.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub icon: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub ui: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub default_frame: Option<Value>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub thumbnail: Option<String>,
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub gizmos: Vec<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub snap: Option<Value>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub editor: Option<String>,
}

/// One `[[tools]]` block: an MCP tool this plugin contributes.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Tool {
    pub name: String,
    #[serde(default)]
    pub description: String,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub input: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub output: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub annotations: Option<Annotations>,
    #[serde(default = "yes")]
    pub user_invocable: bool,
    #[serde(default = "yes")]
    pub model_invocable: bool,
}

fn yes() -> bool {
    true
}

/// The MCP annotation names, verbatim.
#[derive(Debug, Clone, Default, Serialize, Deserialize)]
#[allow(non_snake_case)]
pub struct Annotations {
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub readOnlyHint: Option<bool>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub destructiveHint: Option<bool>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub idempotentHint: Option<bool>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub openWorldHint: Option<bool>,
}

/// One entry in `[hooks]`.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Hook {
    pub mode: String,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub timeout_ms: Option<u32>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub command: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub url: Option<String>,
}

// ---------------------------------------------------------------------------
// The vocabularies
// ---------------------------------------------------------------------------

/// Every plugin kind the core knows.
pub const KINDS: &[&str] = &[
    "source",
    "output",
    "filter",
    "transition",
    "encoder",
    "service",
    "device",
    "panel",
    "surface",
    "preset",
    "graphic",
    "collection",
];

/// The capability vocabulary of 03 section 4.
pub const CAPABILITIES: &[&str] = &[
    "restart-in-place",
    "latency-report",
    "health",
    "keyframe-request",
    "idle",
    "seek",
    "audio-layers",
    "alpha",
];

/// The platform triples `gmx plugin add` matches against.
pub const PLATFORMS: &[&str] = &[
    "linux-x86_64",
    "linux-aarch64",
    "linux-armv7",
    "macos-aarch64",
    "macos-x86_64",
    "windows-x86_64",
    "windows-aarch64",
];

/// Where a plugin says it can run.
pub const PLACEMENTS: &[&str] = &["in-process", "sidecar", "node"];

/// The hooks of 03 section 8.
pub const HOOKS: &[&str] = &[
    "take.before",
    "take.after",
    "source.added",
    "source.removed",
    "output.state",
    "alert.raised",
    "session.start",
    "session.end",
    "plugin.loaded",
    "plugin.failed",
];

/// What a `media` stream key may say.
pub const MEDIA_MODES: &[&str] = &["raw", "container", "none"];

// ---------------------------------------------------------------------------
// Validation
// ---------------------------------------------------------------------------

/// One thing wrong with the manifest, with the key path that caused it.
#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
pub struct Problem {
    /// A TOML key path, for example `provides[0].transports`.
    pub path: String,
    pub message: String,
}

impl std::fmt::Display for Problem {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        write!(f, "{}: {}", self.path, self.message)
    }
}

fn problem(path: impl Into<String>, message: impl Into<String>) -> Problem {
    Problem {
        path: path.into(),
        message: message.into(),
    }
}

/// What went wrong loading a manifest.
#[derive(Debug)]
pub enum ManifestError {
    Io(std::io::Error),
    /// The TOML did not parse. The message carries the line and column.
    Syntax(String),
    /// The TOML parsed but the manifest is not valid.
    Invalid(Vec<Problem>),
}

impl std::fmt::Display for ManifestError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            ManifestError::Io(e) => write!(f, "could not read gmx-plugin.toml: {e}"),
            ManifestError::Syntax(s) => write!(f, "gmx-plugin.toml does not parse: {s}"),
            ManifestError::Invalid(ps) => {
                writeln!(f, "gmx-plugin.toml has {} problems:", ps.len())?;
                for p in ps {
                    writeln!(f, "  {p}")?;
                }
                Ok(())
            }
        }
    }
}

impl std::error::Error for ManifestError {}

impl Manifest {
    /// Parse without validating. Use [`Manifest::load`] unless you have a
    /// reason to see an invalid manifest.
    pub fn parse(text: &str) -> Result<Manifest, ManifestError> {
        toml::from_str(text).map_err(|e| ManifestError::Syntax(e.to_string()))
    }

    /// Parse and validate a manifest file. `root` (the file's directory) is
    /// used to check that every path the manifest names exists.
    pub fn load(path: impl AsRef<Path>) -> Result<Manifest, ManifestError> {
        let path = path.as_ref();
        let text = std::fs::read_to_string(path).map_err(ManifestError::Io)?;
        let manifest = Manifest::parse(&text)?;
        let root = path.parent().unwrap_or(Path::new("."));
        let problems = manifest.validate(Some(root));
        if problems.is_empty() {
            Ok(manifest)
        } else {
            Err(ManifestError::Invalid(problems))
        }
    }

    /// Every problem with this manifest, in file order.
    ///
    /// Pass `root` to check that the files the manifest points at exist; pass
    /// `None` to check only what the text itself says.
    pub fn validate(&self, root: Option<&Path>) -> Vec<Problem> {
        let mut out = Vec::new();
        self.validate_plugin(&mut out);
        self.validate_run(&mut out, root);
        self.validate_provides(&mut out, root);
        self.validate_tools(&mut out, root);
        self.validate_hooks(&mut out);
        out
    }

    fn validate_plugin(&self, out: &mut Vec<Problem>) {
        let p = &self.plugin;
        if !is_slug(&p.name) {
            out.push(problem(
                "plugin.name",
                format!(
                    "'{}' is not a slug. Use lower case letters, digits and hyphens, \
                     starting with a letter, because the name is the namespace of every id.",
                    p.name
                ),
            ));
        }
        if !is_semver(&p.version) {
            out.push(problem(
                "plugin.version",
                format!("'{}' is not semver. Write it as MAJOR.MINOR.PATCH.", p.version),
            ));
        }
        if p.api == 0 {
            out.push(problem("plugin.api", "api must be 1 or more."));
        }
        if p.description.trim().is_empty() {
            out.push(problem(
                "plugin.description",
                "write one sentence saying what this plugin does. It is what a person and \
                 a model both read first in the index.",
            ));
        } else if p.description.len() > 1024 {
            out.push(problem(
                "plugin.description",
                format!(
                    "{} characters; keep it under 1024, the limit the index and the skill \
                     frontmatter both use.",
                    p.description.len()
                ),
            ));
        }
        if p.license.trim().is_empty() {
            out.push(problem(
                "plugin.license",
                "name a licence, an SPDX id such as MIT or Apache-2.0.",
            ));
        }
        if p.platforms.is_empty() {
            out.push(problem(
                "plugin.platforms",
                format!("name at least one platform. Known: {}.", PLATFORMS.join(", ")),
            ));
        }
        for (i, plat) in p.platforms.iter().enumerate() {
            if !PLATFORMS.contains(&plat.as_str()) {
                out.push(problem(
                    format!("plugin.platforms[{i}]"),
                    format!("'{plat}' is not a platform triple. Known: {}.", PLATFORMS.join(", ")),
                ));
            }
        }
        if p.placements.is_empty() {
            out.push(problem(
                "plugin.placements",
                format!("name at least one placement. Known: {}.", PLACEMENTS.join(", ")),
            ));
        }
        for (i, pl) in p.placements.iter().enumerate() {
            if !PLACEMENTS.contains(&pl.as_str()) {
                out.push(problem(
                    format!("plugin.placements[{i}]"),
                    format!("'{pl}' is not a placement. Known: {}.", PLACEMENTS.join(", ")),
                ));
            }
        }
        if p.process != "per-instance" && p.process != "singleton" {
            out.push(problem(
                "plugin.process",
                format!(
                    "'{}' is neither 'per-instance' nor 'singleton'. Use per-instance unless \
                     one process must serve every instance.",
                    p.process
                ),
            ));
        }
    }

    fn validate_run(&self, out: &mut Vec<Problem>, root: Option<&Path>) {
        let needs_process = self
            .plugin
            .placements
            .iter()
            .any(|p| p == "sidecar" || p == "node");
        match &self.run {
            None => {
                if needs_process {
                    out.push(problem(
                        "run",
                        "placements name 'sidecar' or 'node', so [run] must say how to start \
                         the process: one of bin, python, node or shell.",
                    ));
                }
            }
            Some(run) => {
                match run.keys_set() {
                    0 => out.push(problem(
                        "run",
                        "[run] is empty. Set exactly one of bin, python, node or shell.",
                    )),
                    1 => {}
                    n => out.push(problem(
                        "run",
                        format!("{n} runtime keys are set; exactly one wins, so set one."),
                    )),
                }
                for (plat, path) in &run.bin {
                    if !PLATFORMS.contains(&plat.as_str()) {
                        out.push(problem(
                            format!("run.bin.{plat}"),
                            format!("'{plat}' is not a platform triple. Known: {}.", PLATFORMS.join(", ")),
                        ));
                    }
                    if !self.plugin.platforms.contains(plat) {
                        out.push(problem(
                            format!("run.bin.{plat}"),
                            "this platform is not in plugin.platforms, so nothing will ever \
                             pick this binary.",
                        ));
                    }
                    check_relative(out, &format!("run.bin.{plat}"), path, None);
                }
                for (key, value) in [
                    ("run.python", &run.python),
                    ("run.node", &run.node),
                    ("run.shell", &run.shell),
                ] {
                    if let Some(v) = value {
                        check_relative(out, key, v, root);
                    }
                }
                if run.shell.is_some()
                    && self.plugin.platforms.iter().any(|p| p.starts_with("windows"))
                    && run.bin.is_empty()
                {
                    out.push(problem(
                        "run.shell",
                        "a shell entry is refused on Windows unless [run] also has a bin entry \
                         for it. Drop windows from plugin.platforms, or add the binary.",
                    ));
                }
            }
        }
    }

    fn validate_provides(&self, out: &mut Vec<Problem>, root: Option<&Path>) {
        if self.provides.is_empty() {
            out.push(problem(
                "provides",
                "a plugin with no [[provides]] block registers nothing. Add one.",
            ));
        }
        let mut seen: Vec<&str> = Vec::new();
        for (i, p) in self.provides.iter().enumerate() {
            let at = format!("provides[{i}]");
            if !KINDS.contains(&p.kind.as_str()) {
                out.push(problem(
                    format!("{at}.kind"),
                    format!("'{}' is not a kind. Known: {}.", p.kind, KINDS.join(", ")),
                ));
            }
            if !is_slug(&p.id) {
                out.push(problem(
                    format!("{at}.id"),
                    format!(
                        "'{}' is not a slug. The full id becomes '{}/{}', and ids are slugs, \
                         never UUIDs.",
                        p.id, self.plugin.name, p.id
                    ),
                ));
            }
            if seen.contains(&p.id.as_str()) {
                out.push(problem(
                    format!("{at}.id"),
                    format!("'{}' is used by an earlier provide. Ids are unique per plugin.", p.id),
                ));
            }
            seen.push(&p.id);
            if let Some(rank) = p.rank {
                if rank > 256 {
                    out.push(problem(
                        format!("{at}.rank"),
                        format!("{rank} is over 256. Rank runs 0 to 256, GStreamer style."),
                    ));
                }
            }
            for (j, scheme) in p.uri_schemes.iter().enumerate() {
                if !scheme.ends_with("://") {
                    out.push(problem(
                        format!("{at}.uri_schemes[{j}]"),
                        format!("'{scheme}' should end in '://', for example 'ndi://'."),
                    ));
                }
            }
            for (j, cap) in p.capabilities.iter().enumerate() {
                if !CAPABILITIES.contains(&cap.as_str()) {
                    out.push(problem(
                        format!("{at}.capabilities[{j}]"),
                        format!(
                            "'{cap}' is not a capability the supervisor knows. Known: {}.",
                            CAPABILITIES.join(", ")
                        ),
                    ));
                }
            }
            if let Some(media) = &p.media {
                for (key, mode) in [("video", &media.video), ("audio", &media.audio)] {
                    if !MEDIA_MODES.contains(&mode.as_str()) {
                        out.push(problem(
                            format!("{at}.media.{key}"),
                            format!("'{mode}' is not one of {}.", MEDIA_MODES.join(", ")),
                        ));
                    }
                }
                if !media.has_video() && !media.has_audio() {
                    out.push(problem(
                        format!("{at}.media"),
                        "both video and audio are 'none', so this provide carries nothing.",
                    ));
                }
                if media.alpha && !p.capabilities.iter().any(|c| c == "alpha") {
                    out.push(problem(
                        format!("{at}.media.alpha"),
                        "alpha is true but 'alpha' is not in capabilities, so the compositor \
                         will not put this source over the programme layer.",
                    ));
                }
            }
            if p.capabilities.iter().any(|c| c == "seek") && p.kind != "source" {
                out.push(problem(
                    format!("{at}.capabilities"),
                    "'seek' is only meaningful on a source.",
                ));
            }
            if let Some(settings) = &p.settings {
                if !settings.ends_with(".json") {
                    out.push(problem(
                        format!("{at}.settings"),
                        "settings must point at a JSON Schema file, draft 2020-12, ending .json.",
                    ));
                }
                check_relative(out, &format!("{at}.settings"), settings, root);
            }
            if let Some(skill) = &p.skill {
                if !skill.ends_with(".md") {
                    out.push(problem(
                        format!("{at}.skill"),
                        "skill must point at a SKILL.md in the Agent Skills format.",
                    ));
                }
                check_relative(out, &format!("{at}.skill"), skill, root);
                if let (Some(root), true) = (root, skill.ends_with(".md")) {
                    let full = root.join(skill);
                    if full.exists() {
                        match std::fs::read_to_string(&full) {
                            Ok(text) => {
                                for p in super::skill::validate(&text) {
                                    out.push(problem(format!("{at}.skill"), p.message));
                                }
                            }
                            Err(e) => out.push(problem(
                                format!("{at}.skill"),
                                format!("could not read '{skill}': {e}"),
                            )),
                        }
                    }
                }
            }
            self.validate_kind_keys(out, &at, p, root);
        }
    }

    fn validate_kind_keys(&self, out: &mut Vec<Problem>, at: &str, p: &Provide, root: Option<&Path>) {
        let require_media = |out: &mut Vec<Problem>| {
            if p.media.is_none() {
                out.push(problem(
                    format!("{at}.media"),
                    format!("a {} provide must declare media, for example \
                             media = {{ video = \"raw\", audio = \"none\" }}.", p.kind),
                ));
            }
        };
        let require_settings = |out: &mut Vec<Problem>| {
            if p.settings.is_none() {
                out.push(problem(
                    format!("{at}.settings"),
                    format!(
                        "a {} provide must declare a settings schema. It is the only settings \
                         UI it gets, and every surface renders it.",
                        p.kind
                    ),
                ));
            }
        };
        match p.kind.as_str() {
            "source" => {
                require_media(out);
                require_settings(out);
                if p.transports.is_empty() {
                    out.push(problem(
                        format!("{at}.transports"),
                        "a source must declare transports. 'container' works everywhere and \
                         is the safe first choice.",
                    ));
                }
            }
            "output" => {
                require_media(out);
                require_settings(out);
            }
            "filter" => {
                require_media(out);
                require_settings(out);
                if p.latency_ms.is_none() {
                    out.push(problem(
                        format!("{at}.latency_ms"),
                        "a filter must declare its latency so the aligner can absorb it. \
                         Write 0 if it adds none.",
                    ));
                }
                for (j, side) in p.sides.iter().enumerate() {
                    if side != "source" && side != "programme" {
                        out.push(problem(
                            format!("{at}.sides[{j}]"),
                            format!("'{side}' is neither 'source' nor 'programme'."),
                        ));
                    }
                }
            }
            "transition" | "service" => require_settings(out),
            "encoder" => {
                if let Some(codecs) = &p.codecs {
                    check_relative(out, &format!("{at}.codecs"), codecs, root);
                } else {
                    out.push(problem(
                        format!("{at}.codecs"),
                        "an encoder provide must point at a codecs.toml whose entries merge \
                         into the catalogue.",
                    ));
                }
            }
            "device" => {
                if p.discovery.is_none() {
                    out.push(problem(
                        format!("{at}.discovery"),
                        "a device provide must say how it discovers, for example \
                         discovery = { mdns = [\"_ndi._tcp\"] }.",
                    ));
                }
            }
            "panel" => match &p.panel {
                None => out.push(problem(
                    format!("{at}.panel"),
                    "a panel provide must declare panel = { kind, entry, slots }.",
                )),
                Some(panel) => {
                    if panel.kind != "custom-element" && panel.kind != "iframe" {
                        out.push(problem(
                            format!("{at}.panel.kind"),
                            format!(
                                "'{}' is neither 'custom-element' nor 'iframe'.",
                                panel.kind
                            ),
                        ));
                    }
                    check_relative(out, &format!("{at}.panel.entry"), &panel.entry, root);
                    if panel.slots.is_empty() {
                        out.push(problem(
                            format!("{at}.panel.slots"),
                            "name at least one slot, or the panel has nowhere to appear.",
                        ));
                    }
                }
            },
            "surface" => {
                if p.surface.is_none() {
                    out.push(problem(
                        format!("{at}.surface"),
                        "a surface provide must declare surface = { run, api }.",
                    ));
                }
            }
            "preset" => {
                if p.preset.is_none() {
                    out.push(problem(
                        format!("{at}.preset"),
                        "a preset provide must declare preset = { plugins, config, layout, surface }.",
                    ));
                }
            }
            "graphic" => match &p.graphic {
                None => out.push(problem(
                    format!("{at}.graphic"),
                    "a graphic provide must point at an OGraf manifest.",
                )),
                Some(g) => check_relative(out, &format!("{at}.graphic"), g, root),
            },
            "collection" => match &p.collection {
                None => out.push(problem(
                    format!("{at}.collection"),
                    "a collection provide must point at a collection.json.",
                )),
                Some(c) => check_relative(out, &format!("{at}.collection"), c, root),
            },
            _ => {}
        }
        if p.designer.is_some() && !matches!(p.kind.as_str(), "source" | "filter" | "graphic") {
            out.push(problem(
                format!("{at}.designer"),
                "only a source, filter or graphic provide may carry a [provides.designer] block.",
            ));
        }
        if let Some(d) = &p.designer {
            for (key, value) in [
                ("icon", &d.icon),
                ("ui", &d.ui),
                ("thumbnail", &d.thumbnail),
                ("editor", &d.editor),
            ] {
                if let Some(v) = value {
                    check_relative(out, &format!("{at}.designer.{key}"), v, root);
                }
            }
        }
    }

    fn validate_tools(&self, out: &mut Vec<Problem>, root: Option<&Path>) {
        let mut seen: Vec<&str> = Vec::new();
        for (i, t) in self.tools.iter().enumerate() {
            let at = format!("tools[{i}]");
            if !is_tool_name(&t.name) {
                out.push(problem(
                    format!("{at}.name"),
                    format!(
                        "'{}' is not a tool name. Use lower case letters, digits and \
                         underscores; it is exposed as gmx_{}_{}.",
                        t.name, self.plugin.name, t.name
                    ),
                ));
            }
            if seen.contains(&t.name.as_str()) {
                out.push(problem(
                    format!("{at}.name"),
                    format!("'{}' is declared twice.", t.name),
                ));
            }
            seen.push(&t.name);
            if t.description.trim().is_empty() {
                out.push(problem(
                    format!("{at}.description"),
                    "a tool with no description is not chosen correctly by any model. Say what \
                     it does, when to use it, and give one example call.",
                ));
            } else if !t.description.contains("Example") && !t.description.contains("example") {
                out.push(problem(
                    format!("{at}.description"),
                    "add one example call to the description. Every tool description in this \
                     project carries one, because selection accuracy depends on it.",
                ));
            }
            match &t.input {
                None => out.push(problem(
                    format!("{at}.input"),
                    "a tool must point at an input JSON Schema, even if the object is empty.",
                )),
                Some(p) => check_relative(out, &format!("{at}.input"), p, root),
            }
            if let Some(p) = &t.output {
                check_relative(out, &format!("{at}.output"), p, root);
            }
            if let Some(a) = &t.annotations {
                if a.readOnlyHint == Some(true) && a.destructiveHint == Some(true) {
                    out.push(problem(
                        format!("{at}.annotations"),
                        "readOnlyHint and destructiveHint cannot both be true.",
                    ));
                }
            }
            if !t.user_invocable && !t.model_invocable {
                out.push(problem(
                    at.to_string(),
                    "neither a person nor a model may call this tool, so nothing can.",
                ));
            }
        }
    }

    fn validate_hooks(&self, out: &mut Vec<Problem>) {
        for (name, hook) in &self.hooks {
            let at = format!("hooks.\"{name}\"");
            if !HOOKS.contains(&name.as_str()) {
                out.push(problem(
                    at.clone(),
                    format!("'{name}' is not a hook. Known: {}.", HOOKS.join(", ")),
                ));
            }
            match hook.mode.as_str() {
                "rpc" => {}
                "command" => {
                    if hook.command.is_none() {
                        out.push(problem(
                            format!("{at}.command"),
                            "mode 'command' needs a command to run.",
                        ));
                    }
                }
                "http" => {
                    if hook.url.is_none() {
                        out.push(problem(format!("{at}.url"), "mode 'http' needs a url to POST to."));
                    }
                }
                other => out.push(problem(
                    format!("{at}.mode"),
                    format!("'{other}' is not a mode. Known: rpc, command, http."),
                )),
            }
            if let Some(ms) = hook.timeout_ms {
                if name == "take.before" && ms > 100 {
                    out.push(problem(
                        format!("{at}.timeout_ms"),
                        format!(
                            "{ms} ms is over the 100 ms maximum for take.before. At 30 fps that \
                             is three frames of delay on a take decision."
                        ),
                    ));
                }
                if name != "take.before" {
                    out.push(problem(
                        format!("{at}.timeout_ms"),
                        format!("only take.before can delay a decision; '{name}' cannot."),
                    ));
                }
            }
        }
    }

    /// The `[[provides]]` blocks as JSON, for the `initialize` request.
    pub fn provides_json(&self) -> Vec<Value> {
        self.provides
            .iter()
            .filter_map(|p| serde_json::to_value(p).ok())
            .collect()
    }

    /// The provide with this id, if there is one.
    pub fn provide(&self, id: &str) -> Option<&Provide> {
        self.provides.iter().find(|p| p.id == id)
    }
}

fn check_relative(out: &mut Vec<Problem>, at: &str, path: &str, root: Option<&Path>) {
    if path.trim().is_empty() {
        out.push(problem(at, "the path is empty."));
        return;
    }
    let p = Path::new(path);
    if p.is_absolute() {
        out.push(problem(
            at,
            format!("'{path}' is absolute. Paths in a manifest are relative to the plugin root."),
        ));
        return;
    }
    if path.contains("..") {
        out.push(problem(
            at,
            format!("'{path}' escapes the plugin root with '..'."),
        ));
        return;
    }
    if let Some(root) = root {
        if !root.join(p).exists() {
            out.push(problem(at, format!("'{path}' does not exist.")));
        }
    }
}

/// A slug: lower case letters, digits and hyphens, starting with a letter.
pub fn is_slug(s: &str) -> bool {
    !s.is_empty()
        && s.len() <= 64
        && s.starts_with(|c: char| c.is_ascii_lowercase())
        && s.chars()
            .all(|c| c.is_ascii_lowercase() || c.is_ascii_digit() || c == '-')
        && !s.ends_with('-')
        && !s.contains("--")
}

fn is_tool_name(s: &str) -> bool {
    !s.is_empty()
        && s.len() <= 64
        && s.starts_with(|c: char| c.is_ascii_lowercase())
        && s.chars()
            .all(|c| c.is_ascii_lowercase() || c.is_ascii_digit() || c == '_')
}

/// MAJOR.MINOR.PATCH with an optional prerelease and build.
pub fn is_semver(s: &str) -> bool {
    let core = s.split(['-', '+']).next().unwrap_or("");
    let parts: Vec<&str> = core.split('.').collect();
    parts.len() == 3
        && parts
            .iter()
            .all(|p| !p.is_empty() && p.chars().all(|c| c.is_ascii_digit()))
}

#[cfg(test)]
mod tests {
    use super::*;

    const GOOD: &str = r#"
[plugin]
name = "clock"
version = "0.1.0"
api = 1
description = "A source that draws the current time, large, on a solid background."
license = "MIT"
platforms = ["linux-x86_64", "macos-aarch64", "windows-x86_64"]
placements = ["sidecar", "node"]
process = "per-instance"

[run]
python = "main.py"

[[provides]]
kind = "source"
id = "source"
media = { video = "raw", audio = "none", alpha = false, thumb = true }
transports = ["container"]
capabilities = ["restart-in-place", "health"]
latency_ms = 0
settings = "settings.json"
skill = "SKILL.md"
"#;

    fn paths(problems: &[Problem]) -> Vec<&str> {
        problems.iter().map(|p| p.path.as_str()).collect()
    }

    #[test]
    fn the_worked_example_from_the_plan_is_valid() {
        let m = Manifest::parse(GOOD).unwrap();
        let problems = m.validate(None);
        assert!(problems.is_empty(), "{problems:#?}");
        assert_eq!(m.plugin.name, "clock");
        assert_eq!(m.provides[0].transports, vec![Transport::Container]);
        assert!(m.provide("source").is_some());
    }

    #[test]
    fn every_problem_is_reported_in_one_pass() {
        let text = r#"
[plugin]
name = "Clock_Plugin"
version = "1.0"
api = 0
description = ""
platforms = ["amiga"]
placements = ["everywhere"]
process = "sometimes"

[[provides]]
kind = "widget"
id = "MAIN"
rank = 900
"#;
        let m = Manifest::parse(text).unwrap();
        let got = m.validate(None);
        let got = paths(&got);
        for expected in [
            "plugin.name",
            "plugin.version",
            "plugin.api",
            "plugin.description",
            "plugin.license",
            "plugin.platforms[0]",
            "plugin.placements[0]",
            "plugin.process",
            "provides[0].kind",
            "provides[0].id",
            "provides[0].rank",
        ] {
            assert!(got.contains(&expected), "missing {expected} in {got:?}");
        }
    }

    #[test]
    fn a_source_must_name_media_settings_and_transports() {
        let text = r#"
[plugin]
name = "x"
version = "1.0.0"
api = 1
description = "d"
license = "MIT"
platforms = ["linux-x86_64"]
placements = ["sidecar"]
[run]
python = "main.py"
[[provides]]
kind = "source"
id = "source"
"#;
        let m = Manifest::parse(text).unwrap();
        let got = paths(&m.validate(None)).join(",");
        assert!(got.contains("provides[0].media"), "{got}");
        assert!(got.contains("provides[0].settings"), "{got}");
        assert!(got.contains("provides[0].transports"), "{got}");
    }

    #[test]
    fn a_sidecar_with_no_run_block_is_caught() {
        let text = r#"
[plugin]
name = "x"
version = "1.0.0"
api = 1
description = "d"
license = "MIT"
platforms = ["linux-x86_64"]
placements = ["sidecar"]
[[provides]]
kind = "service"
id = "svc"
settings = "s.json"
"#;
        let m = Manifest::parse(text).unwrap();
        assert!(paths(&m.validate(None)).contains(&"run"));
    }

    #[test]
    fn two_runtime_keys_are_one_too_many() {
        let mut m = Manifest::parse(GOOD).unwrap();
        m.run.as_mut().unwrap().node = Some("main.js".into());
        assert!(paths(&m.validate(None)).contains(&"run"));
    }

    #[test]
    fn duplicate_provide_ids_are_caught() {
        let mut m = Manifest::parse(GOOD).unwrap();
        let mut second = m.provides[0].clone();
        second.kind = "output".into();
        m.provides.push(second);
        let got = m.validate(None);
        assert!(got.iter().any(|p| p.path == "provides[1].id"), "{got:#?}");
    }

    #[test]
    fn an_unknown_capability_names_the_vocabulary() {
        let mut m = Manifest::parse(GOOD).unwrap();
        m.provides[0].capabilities.push("teleport".into());
        let got = m.validate(None);
        let p = got
            .iter()
            .find(|p| p.path == "provides[0].capabilities[2]")
            .expect("expected a capability problem");
        assert!(p.message.contains("restart-in-place"), "{}", p.message);
    }

    #[test]
    fn alpha_without_the_capability_is_caught() {
        let mut m = Manifest::parse(GOOD).unwrap();
        m.provides[0].media.as_mut().unwrap().alpha = true;
        let got = paths(&m.validate(None)).join(",");
        assert!(got.contains("provides[0].media.alpha"), "{got}");
    }

    #[test]
    fn a_shell_runtime_on_windows_needs_a_binary_too() {
        let mut m = Manifest::parse(GOOD).unwrap();
        m.run = Some(Run {
            shell: Some("run.sh".into()),
            ..Default::default()
        });
        let got = paths(&m.validate(None)).join(",");
        assert!(got.contains("run.shell"), "{got}");
    }

    #[test]
    fn a_tool_without_an_example_is_caught() {
        let text = format!(
            "{GOOD}\n[[tools]]\nname = \"list_senders\"\ndescription = \"Lists senders.\"\ninput = \"schemas/in.json\"\n"
        );
        let m = Manifest::parse(&text).unwrap();
        let got = m.validate(None);
        assert!(
            got.iter().any(|p| p.path == "tools[0].description"),
            "{got:#?}"
        );
    }

    #[test]
    fn a_hook_timeout_over_a_hundred_is_caught() {
        let text = format!("{GOOD}\n[hooks]\n\"take.before\" = {{ mode = \"rpc\", timeout_ms = 500 }}\n");
        let m = Manifest::parse(&text).unwrap();
        let got = m.validate(None);
        assert!(
            got.iter().any(|p| p.path.contains("timeout_ms")),
            "{got:#?}"
        );
    }

    #[test]
    fn an_unknown_hook_is_caught() {
        let text = format!("{GOOD}\n[hooks]\n\"take.sideways\" = {{ mode = \"rpc\" }}\n");
        let m = Manifest::parse(&text).unwrap();
        assert!(!m.validate(None).is_empty());
    }

    #[test]
    fn absolute_and_escaping_paths_are_refused() {
        let mut out = Vec::new();
        check_relative(&mut out, "a", "/etc/passwd", None);
        check_relative(&mut out, "b", "../../secrets.json", None);
        assert_eq!(out.len(), 2, "{out:#?}");
    }

    #[test]
    fn slugs_and_semver() {
        assert!(is_slug("ndi"));
        assert!(is_slug("gmx-browser"));
        assert!(!is_slug("NDI"));
        assert!(!is_slug("1st"));
        assert!(!is_slug("trailing-"));
        assert!(!is_slug("double--hyphen"));
        assert!(is_semver("1.2.3"));
        assert!(is_semver("0.1.0-rc.1"));
        assert!(!is_semver("1.2"));
        assert!(!is_semver("v1.2.3"));
    }

    #[test]
    fn provides_json_round_trips_for_the_handshake() {
        let m = Manifest::parse(GOOD).unwrap();
        let json = m.provides_json();
        assert_eq!(json.len(), 1);
        assert_eq!(json[0]["kind"], "source");
        assert_eq!(json[0]["transports"][0], "container");
    }
}
