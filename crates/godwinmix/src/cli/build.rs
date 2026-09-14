//! `gmx build --preset church --name "AcmeMix"`: a custom build, assembled here.
//!
//! 06 section 5 says a custom build is a preset plus branding, produced by one
//! command and kept in sync with upstream. The CI half of that (the template
//! repository that turns this directory into signed installers for three
//! platforms) is Phase 5. What this command does is the local half: it writes
//! the directory those installers are built from, and it writes the README
//! that says how.
//!
//! One rule is enforced rather than documented: a codec entry whose licence is
//! copyleft is not bundled. GPL is what that means here, not LGPL. The
//! GStreamer elements are LGPL and a custom build links them the way every
//! other program does; x264 and the other GPL entries are ones the operator
//! installs on their own machine, which is why the always present fallbacks in
//! `codecs.toml` are openh264 and SVT-AV1.

use anyhow::{Context, Result};
use godwinmix_core::catalogue::Catalogue;
use godwinmix_core::preset::{self, plan::Options};
use std::path::{Path, PathBuf};

/// Licences a custom build will not carry. Prefix matched, so `-only` and
/// `-or-later` are both caught.
const COPYLEFT: &[&str] = &["GPL-2.0", "GPL-3.0", "AGPL-", "SSPL-"];

#[derive(clap::Args, Debug, Clone)]
pub struct BuildArgs {
    /// The preset the build is made of. A name or a directory.
    #[arg(long)]
    pub preset: String,
    /// The product name, as it appears on the window and in the installer.
    #[arg(long)]
    pub name: String,
    /// A square PNG, 512x512 or larger, for the installer and the window.
    #[arg(long)]
    pub icon: Option<PathBuf>,
    /// Where to write the directory [default: build/<name>].
    #[arg(long)]
    pub out: Option<PathBuf>,
    /// A reverse domain identifier for the installer, for example com.acme.mix.
    #[arg(long)]
    pub identifier: Option<String>,
    /// Do not copy the core binary in. The directory is then a recipe CI fills.
    #[arg(long)]
    pub no_binary: bool,
}

/// What was assembled.
#[derive(Debug, serde::Serialize)]
pub struct Built {
    pub name: String,
    pub preset: String,
    pub dir: PathBuf,
    pub wrote: Vec<PathBuf>,
    /// Codec entries left out because their licence is copyleft, and why.
    pub refused: Vec<String>,
}

pub fn run(args: BuildArgs) -> Result<()> {
    let built = assemble(&args)?;
    println!("built {} from the {} preset", built.name, built.preset);
    println!("  {}", built.dir.display());
    for path in &built.wrote {
        println!("  wrote    {}", relative(&built.dir, path));
    }
    if !built.refused.is_empty() {
        println!();
        println!("left out, copyleft licence");
        for note in &built.refused {
            println!("  {note}");
        }
        println!("  A machine that has these installed still uses them; they are not bundled.");
    }
    println!();
    println!("Read {}/README.md: it says what CI does with this.", built.dir.display());
    Ok(())
}

fn relative(root: &Path, path: &Path) -> String {
    path.strip_prefix(root).unwrap_or(path).display().to_string()
}

pub fn assemble(args: &BuildArgs) -> Result<Built> {
    let found = preset::resolve(&args.preset)?;
    let block = found.block()?.clone();
    let slug = slug(&args.name);
    let dir = args.out.clone().unwrap_or_else(|| PathBuf::from("build").join(&slug));
    std::fs::create_dir_all(&dir).with_context(|| format!("making {}", dir.display()))?;

    // A plan that cannot be made is a preset that will not work in the build
    // either, and finding that out now costs nothing.
    let plan = preset::plan::build(&found, &Options::new(dir.join("godwinmix.toml")))
        .with_context(|| format!("the preset {} cannot be built", found.name))?;

    let mut wrote = Vec::new();
    copy_preset(&found, &block, &dir, &mut wrote)?;
    write(dir.join("godwinmix.toml"), &found.read(&block.config)?, &mut wrote)?;
    if let Some(css) = &block.theme_css {
        write(dir.join("theme.css"), &found.read(css)?, &mut wrote)?;
    }

    let (codecs, refused) = permissive_catalogue()?;
    write(dir.join("codecs.toml"), &codecs, &mut wrote)?;

    let identifier = args
        .identifier
        .clone()
        .unwrap_or_else(|| format!("com.example.{}", slug.replace('-', "")));
    write(dir.join("tauri.conf.json"), &tauri_fragment(&args.name, &identifier), &mut wrote)?;

    if let Some(icon) = &args.icon {
        std::fs::create_dir_all(dir.join("icons"))
            .with_context(|| format!("making {}", dir.join("icons").display()))?;
        let target = dir.join("icons").join("icon.png");
        std::fs::copy(icon, &target)
            .with_context(|| format!("copying {} to {}", icon.display(), target.display()))?;
        wrote.push(target);
    }

    if !args.no_binary {
        if let Some(target) = copy_binary(&dir)? {
            wrote.push(target);
        }
    }

    write(dir.join("README.md"), &readme(args, &found.name, &plan, &refused), &mut wrote)?;
    Ok(Built { name: args.name.clone(), preset: found.name, dir, wrote, refused })
}

fn write(path: PathBuf, body: &str, wrote: &mut Vec<PathBuf>) -> Result<()> {
    if let Some(parent) = path.parent() {
        std::fs::create_dir_all(parent).with_context(|| format!("making {}", parent.display()))?;
    }
    std::fs::write(&path, body).with_context(|| format!("writing {}", path.display()))?;
    wrote.push(path);
    Ok(())
}

/// The whole preset, copied under `preset/`, so `gmx preset apply ./preset`
/// works inside the built directory and the build tracks upstream by name.
fn copy_preset(
    found: &preset::Preset,
    block: &preset::PresetBlock,
    dir: &Path,
    wrote: &mut Vec<PathBuf>,
) -> Result<()> {
    let root = dir.join("preset");
    write(root.join("gmx-plugin.toml"), &found.read("gmx-plugin.toml")?, wrote)?;
    write(root.join(&block.config), &found.read(&block.config)?, wrote)?;
    write(root.join(&block.layout), &found.read(&block.layout)?, wrote)?;
    if let Ok(readme) = found.read("README.md") {
        write(root.join("README.md"), &readme, wrote)?;
    }
    if let Some(css) = &block.theme_css {
        write(root.join(css), &found.read(css)?, wrote)?;
    }
    for (name, body) in found.json_files(&block.scenes)? {
        write(root.join(&block.scenes).join(name), &body, wrote)?;
    }
    Ok(())
}

/// The shipped catalogue with every copyleft entry taken out.
fn permissive_catalogue() -> Result<(String, Vec<String>)> {
    let catalogue = godwinmix_core::catalogue::load(None, None)
        .context("reading the codec catalogue")?;
    let mut refused = Vec::new();
    let mut kept = Catalogue::default();
    for entry in &catalogue.video {
        let license = entry.license.clone().unwrap_or_default();
        if copyleft(&license) {
            refused.push(format!("{} ({license})", entry.id()));
        } else {
            kept.video.push(entry.clone());
        }
    }
    for entry in &catalogue.audio {
        let license = entry.license.clone().unwrap_or_default();
        if copyleft(&license) {
            refused.push(format!("{} ({license})", entry.id()));
        } else {
            kept.audio.push(entry.clone());
        }
    }
    kept.graphics = catalogue.graphics.clone();
    kept.container = catalogue.container.clone();
    kept.programme_container = catalogue.programme_container.clone();
    let body = format!(
        "# The codec catalogue for this build, with every copyleft entry taken out\n\
         # by `gmx build`. A machine that has one installed still selects it; this\n\
         # file is what the installer carries.\n\n{}",
        toml::to_string_pretty(&kept).context("writing the catalogue")?
    );
    Ok((body, refused))
}

fn copyleft(license: &str) -> bool {
    COPYLEFT.iter().any(|prefix| license.starts_with(prefix))
}

/// The binary running this command, which is the core the build ships.
fn copy_binary(dir: &Path) -> Result<Option<PathBuf>> {
    let Ok(exe) = std::env::current_exe() else { return Ok(None) };
    let name = if cfg!(windows) { "godwinmix.exe" } else { "godwinmix" };
    let target = dir.join("bin").join(name);
    std::fs::create_dir_all(dir.join("bin"))
        .with_context(|| format!("making {}", dir.join("bin").display()))?;
    std::fs::copy(&exe, &target)
        .with_context(|| format!("copying {} to {}", exe.display(), target.display()))?;
    Ok(Some(target))
}

fn slug(name: &str) -> String {
    let mut out = String::new();
    for c in name.chars() {
        if c.is_ascii_alphanumeric() {
            out.push(c.to_ascii_lowercase());
        } else if !out.ends_with('-') {
            out.push('-');
        }
    }
    let trimmed = out.trim_matches('-').to_string();
    if trimmed.is_empty() { "custom".into() } else { trimmed }
}

/// The parts of `tauri.conf.json` that branding changes, and nothing else.
///
/// A fragment rather than a whole file, because the rest of the desktop app's
/// configuration is upstream's and a build that copied it would stop tracking
/// the next release. CI merges this over `tauri-app/tauri.conf.json`.
fn tauri_fragment(name: &str, identifier: &str) -> String {
    serde_json::to_string_pretty(&serde_json::json!({
        "productName": name,
        "identifier": identifier,
        "app": { "windows": [{ "title": name, "label": "main" }] },
        "bundle": {
            "publisher": name,
            "icon": ["icons/icon.png"],
        },
    }))
    .unwrap_or_default()
        + "\n"
}

fn readme(
    args: &BuildArgs,
    preset_name: &str,
    plan: &preset::Plan,
    refused: &[String],
) -> String {
    let missing = plan
        .missing()
        .iter()
        .map(|p| format!("* `{}` is not in this build. `gmx plugin add {}` on the machine, or add it to the build's plugin list.\n", p.spec, p.name))
        .collect::<String>();
    let refused_list = refused
        .iter()
        .map(|r| format!("* {r}\n"))
        .collect::<String>();
    format!(
        "# {name}\n\n\
         A custom build of GodwinMix: the core, the `{preset_name}` preset, and this\n\
         branding. `gmx build --preset {preset_name} --name {name:?}` made it, and running\n\
         that again after a GodwinMix release makes the next version with nothing forked.\n\n\
         ## What is in here\n\n\
         | Path | What it is |\n\
         |---|---|\n\
         | `bin/` | the core binary this build ships |\n\
         | `preset/` | the preset, whole. `gmx preset apply ./preset` applies it |\n\
         | `godwinmix.toml` | the configuration the installer drops next to the binary |\n\
         | `codecs.toml` | the codec catalogue, permissive entries only |\n\
         | `theme.css` | the theme, when the preset ships one |\n\
         | `tauri.conf.json` | the branding, merged over the upstream desktop config |\n\
         | `icons/icon.png` | the icon, when one was given |\n\n\
         ## Turning this into installers\n\n\
         The GitHub template repository `godwinmix-build-template` does it in CI. Fork it,\n\
         drop this directory in at `build/`, and its workflow:\n\n\
         1. checks out the GodwinMix release the build was made from, by tag;\n\
         2. merges `tauri.conf.json` over `tauri-app/tauri.conf.json`;\n\
         3. runs `cargo tauri build` on a macOS, a Windows and a Linux runner;\n\
         4. signs each artefact with the secrets in the fork, and publishes them.\n\n\
         Nothing here needs a fork of GodwinMix itself. The next release is the same four\n\
         steps against the next tag.\n\n\
         ## What is not here\n\n\
         {missing}\
         The codec entries below were left out because their licence is copyleft. A\n\
         machine that has one installed still selects it at runtime; a closed build does\n\
         not carry it.\n\n\
         {refused_list}\n\
         ## What to check before you ship it\n\n\
         * `godwinmix.toml` still has the preset's placeholders in it. The installer\n\
           should not carry your stream key.\n\
         * The licence of the core is Apache 2.0 and the LICENSE file goes with it.\n\
         * `gmx doctor` on each platform you ship to, before you publish.\n",
        name = args.name,
    )
}

#[cfg(test)]
mod tests {
    use super::*;

    fn args(out: PathBuf) -> BuildArgs {
        BuildArgs {
            preset: "church".into(),
            name: "AcmeMix".into(),
            icon: None,
            out: Some(out),
            identifier: None,
            no_binary: true,
        }
    }

    #[test]
    fn a_build_carries_the_preset_the_config_and_the_branding() {
        let _ = gstreamer::init();
        let dir = std::env::temp_dir().join(format!("gmx-build-{}", std::process::id()));
        std::fs::remove_dir_all(&dir).ok();
        let built = assemble(&args(dir.clone())).unwrap();

        for wanted in [
            "godwinmix.toml",
            "codecs.toml",
            "tauri.conf.json",
            "README.md",
            "theme.css",
            "preset/gmx-plugin.toml",
            "preset/config/godwinmix.toml",
        ] {
            assert!(dir.join(wanted).exists(), "{wanted} is missing");
        }
        let conf: serde_json::Value =
            serde_json::from_str(&std::fs::read_to_string(dir.join("tauri.conf.json")).unwrap())
                .unwrap();
        assert_eq!(conf["productName"], "AcmeMix");
        assert_eq!(conf["identifier"], "com.example.acmemix");

        // The copied preset is one the loader reads back.
        let copied = preset::load(&dir.join("preset")).unwrap();
        assert_eq!(copied.name, "church");
        assert!(!built.refused.is_empty(), "codecs.toml has copyleft entries in it");
        std::fs::remove_dir_all(&dir).ok();
    }

    #[test]
    fn no_copyleft_codec_is_bundled_and_the_permissive_fallbacks_still_are() {
        let (body, refused) = permissive_catalogue().unwrap();
        assert!(!body.contains("GPL-2.0"), "a GPL entry is in the bundled catalogue");
        assert!(body.contains("openh264"), "the permissive H.264 fallback was dropped");
        assert!(body.contains("LGPL-2.1"), "LGPL is not copyleft for this purpose");
        assert!(refused.iter().any(|r| r.contains("x264")), "{refused:?}");
    }

    #[test]
    fn the_licence_test_catches_every_spelling_of_gpl() {
        assert!(copyleft("GPL-2.0-or-later"));
        assert!(copyleft("GPL-3.0-only"));
        assert!(copyleft("AGPL-3.0-or-later"));
        assert!(!copyleft("LGPL-2.1-or-later"));
        assert!(!copyleft("BSD-2-Clause"));
        assert!(!copyleft("Apache-2.0"));
    }

    #[test]
    fn a_product_name_becomes_a_directory_name() {
        assert_eq!(slug("AcmeMix"), "acmemix");
        assert_eq!(slug("St Mary's Mixer"), "st-mary-s-mixer");
        assert_eq!(slug("!!!"), "custom");
    }
}
