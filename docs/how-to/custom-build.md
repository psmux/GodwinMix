# Make a custom build

A custom build is a preset plus your branding, under your own name, tracking
upstream. One command assembles it:

```sh
gmx build --preset church --name "AcmeMix" --icon acme.png
```

Nothing is forked. `gmx build` again after a GodwinMix release produces the next
version of your product from the same command.

## What you get

A directory, `build/acmemix` by default:

```
build/acmemix/
  bin/godwinmix           the core binary this build ships
  preset/                 the preset, whole. `gmx preset apply ./preset` applies it
    gmx-plugin.toml
    config/godwinmix.toml
    config/layout.json
    scenes/*.json
    theme.css
    README.md
  godwinmix.toml          the configuration the installer drops beside the binary
  codecs.toml             the codec catalogue, permissive entries only
  theme.css               the theme, when the preset ships one
  tauri.conf.json         the branding, merged over the upstream desktop config
  icons/icon.png          the icon, when one was given
  README.md               what CI does with all of this
```

## The flags

| Flag | What it does |
|---|---|
| `--preset <name\|path>` | The preset the build is made of. A name, or a directory. Required. |
| `--name "<Brand>"` | The product name, on the window and in the installer. Required. |
| `--icon x.png` | A square PNG, 512x512 or larger. |
| `--out dir` | Where to write it. Default `build/<slug of the name>`. |
| `--identifier com.acme.mix` | The reverse domain identifier the installers use. Default `com.example.<slug>`. |
| `--no-binary` | Do not copy the core in. The directory is then a recipe CI fills. |

## Licences: what is not bundled

A codec entry whose licence is copyleft is left out of `codecs.toml`, and the
command says which:

```
left out, copyleft licence
  h264-software-x264 (GPL-2.0-or-later)
  h265-software (GPL-2.0-or-later)
  aac-software-decode (GPL-2.0-or-later)
  A machine that has these installed still uses them; they are not bundled.
```

GPL is what copyleft means here, not LGPL. The GStreamer elements are LGPL and a
build links them the way every other program does. x264 and x265 are GPL, which
is exactly why `codecs.toml` keeps openh264 (BSD) and SVT-AV1 (BSD) as the
always present fallbacks: a closed custom build still encodes.

This is not legal advice and it is not the whole of your obligations. It is one
rule, enforced, so that the easy mistake is not available.

## Turning it into installers

The local half is done. The CI half is the GitHub template repository
`godwinmix-build-template`: fork it, drop this directory in at `build/`, and its
workflow

1. checks out the GodwinMix release the build was made from, by tag;
2. merges your `tauri.conf.json` over `tauri-app/tauri.conf.json`;
3. runs `cargo tauri build` on a macOS, a Windows and a Linux runner;
4. signs each artefact with the secrets in your fork, and publishes them.

The template repository is Phase 5 and not published yet. Until it is, the four
steps above are what to do by hand, and `tauri-app/` in this repository is the
desktop app the third one builds.

## Before you ship it

* `godwinmix.toml` still has the preset's placeholders in it. Check that your
  own stream key did not end up in the installer.
* The core is Apache 2.0 and the `LICENSE` file goes with it. So does the
  licence of every plugin you bundle.
* `gmx doctor` on each platform you ship to, on a machine that has never had
  GodwinMix on it. It checks elements, encoders, ports and the disk and exits
  non zero when something the default pipeline needs is missing.
* Hand the result to somebody who has not seen it and watch them install it.

## Keeping up with upstream

Your branding is `tauri.conf.json` plus an icon, and your product is a preset.
Neither is a fork, so a new GodwinMix release means running `gmx build` again
with the new binary and pushing the result. If you changed the preset in the
meantime, `gmx preset save` on the machine you changed it on writes the new one
(see [the presets reference](../reference/presets.md)).

## See also

* [Presets, the reference](../reference/presets.md)
* [Make a preset](make-a-preset.md)
* [Add a theme](add-a-theme.md)
* [The desktop app](desktop-app.md)
