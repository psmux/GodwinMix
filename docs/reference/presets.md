# Presets

A preset is a plugin of kind `preset`: a name for a working setup, with the
plugins it needs, a configuration, a UI layout, a theme and the scenes. One
command puts the whole of it on a machine.

```sh
gmx preset apply church
```

This page is the format and the rules. [Make a preset](../how-to/make-a-preset.md)
is the walk through, and [Your first stream with a preset](../tutorials/first-stream-with-a-preset.md)
is the timed path from install to on air.

## The six official presets

| Preset | For | Gallery | Theme | Plugins it names |
|---|---|---|---|---|
| `default` | what GodwinMix does out of the box: two RTMP cameras, one destination | `live` | `dark` | rtmp |
| `church` | a Sunday service: two cameras, lyrics, slides, YouTube and Facebook | `icon` | `calm` | camera, browser, rtmp |
| `classroom` | a lesson: a camera, the screen, slides, a recording and a stream | `snapshot` | `daylight` | camera, screen, rtmp |
| `esports` | a match: four player feeds, a caster, an overlay, replays | `live` | `neon` | ndi, browser, rtmp, screen, replay |
| `headless-agent` | a channel nobody watches: no multiview, snapshots on, MCP documented | `label` | `dark` | browser, rtmp, director |
| `broadcast` | a contribution feed: SRT in and out, TSL tally, Companion | `live` | `broadcast` | srt, ndi, tally, companion |

They ship inside the binary, so `gmx preset apply church` works on a machine
that downloaded one file. A directory of the same name in a search path wins
over the built in one, so editing `presets/church/` needs no rebuild.

Some of the plugins named do not exist yet. That is not a mistake in the
preset: `gmx preset apply` reports what is missing and applies the rest, so a
preset is a working target the plugins are written towards. `church` and
`classroom` use built in kinds for everything they can, which is why they put a
picture on air today with the camera plugin still missing.

## Where a preset is found

`gmx preset apply <name>` looks, in order:

1. `./presets/<name>` in the working directory;
2. `presets/` beside the binary, which is what a release archive unpacks to;
3. `~/.godwinmix/presets/<name>`;
4. the six compiled into the binary.

`gmx preset apply ./my-church` loads a directory outright, which is how you try
one before publishing it. A directory is a preset when it has a
`gmx-plugin.toml` in it.

## The manifest

```toml
[plugin]
name = "church"                # the namespace, and the directory name
version = "1.0.0"
api = 1
description = "Sunday service: two cameras, lyrics from a browser page, slides."
license = "Apache-2.0"

[[provides]]
kind = "preset"
id = "church"                  # the same as the plugin name, for a preset

[provides.preset]
plugins = ["camera@^1", "browser@^1", "rtmp@^1"]
config = "config/godwinmix.toml"
layout = "config/layout.json"
surface = "web"
theme = "calm"
theme_css = "theme.css"
scenes = "scenes"
gallery = "icon"
steps = [
  "Put your two stream keys into the [[outputs]] blocks of godwinmix.toml.",
  "Run `gmx` and open http://localhost:8080.",
  "Press Wide to put a picture on air.",
]
```

| Key | Required | What it is |
|---|---|---|
| `plugins` | yes | Plugins by name and semver range, `name@range`. A bare name means `*`. Reported when missing, never fatal. |
| `config` | yes | A configuration file, relative to the preset root. It must load under `Config::load` rules on the build applying it. |
| `layout` | yes | A JSON file mapping a UI slot to the panels in it, top to bottom. |
| `surface` | yes | `web`, `none`, or the name of a surface plugin. |
| `theme` | yes | A theme id the surface resolves. One of `dark`, `light`, `high-contrast`, `system`, or your own with `theme_css`. |
| `scenes` | yes | A directory of scene documents, one file each. |
| `theme_css` | no | A stylesheet inside the preset, served at `/presets/<name>/theme.css`. Required when `theme` is not a built in one. |
| `gallery` | no | What the tiles start as: `live`, `snapshot`, `icon` or `label` (05 section 3b). Absent means the surface asks the machine, which is what `gmx doctor` proposes. |
| `steps` | no | The three things the person does next. The welcome panel shows these in order, and the README repeats them. |

### The layout file

```json
{
  "header": ["header"],
  "main": ["scenes", "multiview"],
  "sidebar": ["sources"],
  "footer": ["alerts", "outputs"]
}
```

Slots: `header`, `main`, `sidebar`, `strip`, `footer`, `modal`. Panels the first
party UI ships: `header`, `multiview`, `sources`, `outputs`, `media`, `alerts`,
`scenes`, `welcome`. A plugin's panel is named `<plugin>/<panel id>`. The
reference UI registers its own under `core/<name>` and resolves the short names
above, so a preset written against this table works on it.

## What applying one does

`gmx preset apply <name>` writes three files beside the config and nothing else.

| File | What happens to it |
|---|---|
| `godwinmix.toml` | Copied whole with its comments when there was none. Merged key by key when there was, and the original is kept as `godwinmix.toml.bak`. |
| `godwinmix.scenes.json` | The preset's scenes, resolved against its sources, appended by name. A scene already there under the same name is replaced. |
| `godwinmix.runtime.toml` | A `[ui]` section: `preset`, `theme`, `gallery` and `layout`. |

The merge rules:

* A key the operator has not written is set from the preset.
* A key the operator has written wins. `--force` takes the preset's instead.
* `[[sources]]` and `[[outputs]]` are appended by `id`. An id already there is
  left alone, so applying the same preset twice changes nothing the second time.
  `--keep-sources` appends neither.
* `[codecs]` is a catalogue rather than a setting: a preset that carries one
  replaces it whole.

Applying it exits non zero only on a real error: a file the preset names that is
not there, a config this build cannot load, a scene that does not validate, a
layout naming a panel that does not exist, or a theme nobody has. A plugin that
is not installed is reported and the rest is applied.

## The commands

```sh
gmx preset list                          # every preset this machine can apply
gmx preset show church                   # what it is, and what applying it would do
gmx preset show church --readme          # the page written for the person using it
gmx preset diff church                   # only what would change in this config
gmx preset apply church --dry-run        # the whole plan, writing nothing
gmx preset apply church                  # do it
gmx preset apply ./my-church --force     # from a directory, preset values winning
gmx preset save my-church                # turn this machine back into a preset
```

Every one takes `--config` to name a config other than `godwinmix.toml`, and
`--json` to print the same object the RPC methods return.

## Over the protocol

| Method | Scope | What it does |
|---|---|---|
| `preset.list` | read | Every preset this core can apply. |
| `preset.apply {name, dry_run, force, keep_sources}` | admin | The plan, and the three files. Marked destructive: it rewrites the operator's config. |
| `preset.save {name, out}` | admin | A preset directory from this core's working setup. |

A running core picks up what it can without a restart: the sources and outputs
the preset brought whose plugin is installed are added through the same commands
`source.add` and `output.add` use, and the answer's `live` array says which.
`needs_restart` says what did not, in plain words. The layout, theme and gallery
mode go out to every client as `event/ui.changed`, and `core.info` answers with
them from then on.

Nothing here restarts the encoder and nothing here touches the programme.

## `gmx preset save`

Turns a working machine into a preset directory: the config with its secrets
taken out, the layout, one file per scene with fresh ids, a manifest naming the
plugins the config actually uses, and a README with the four headings to fill
in.

Taken out before anything is written: the `[control] token`, the whole
`[[tokens]]` table, and the tail of every `rtmp://`, `rtmps://` and `srt://`
output URL, which becomes `YOUR-STREAM-KEY`. The command prints what it took.
Read the result before you publish it anyway.

Ids are never reused across documents, because two presets applied to the same
core would collide. Every id in a saved scene is a fresh one, references
included.

## Custom builds

`gmx build --preset church --name "AcmeMix"` assembles a preset plus branding
into a directory CI turns into installers. See
[Make a custom build](../how-to/custom-build.md).
