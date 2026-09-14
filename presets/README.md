# Presets

A preset is a plugin of kind `preset`. It is a name for a working setup: the
plugins it needs, a configuration, a UI layout, a theme and the scenes. One
command puts it on a machine:

```
gmx preset apply church
```

Six ship with GodwinMix. Each is a directory here.

| Preset | For | Plugins it names |
|---|---|---|
| `default` | what GodwinMix does out of the box: two RTMP cameras, one destination | rtmp |
| `church` | a Sunday service: two cameras, lyrics, slides, two destinations | ndi, browser, rtmp, lowerthird, tally |
| `classroom` | a lesson: camera, screen, a recording and a stream | camera, screen, rtmp, file-record |
| `esports` | a match: four player feeds, a caster, an overlay, replays | ndi, browser, rtmp, screen, replay |
| `headless-agent` | a channel nobody watches: MCP on, no UI | browser, rtmp, director |
| `broadcast` | a contribution feed: SRT in and out, tally, Companion | srt, ndi, tally, companion |

Some of those plugins do not exist yet. That is expected and it is not a
mistake in the preset: `gmx preset apply` installs what it can find and names
what it cannot, so the preset is a working target the plugins are written
towards.

## What is in a preset

```
church/
  gmx-plugin.toml        the manifest: name, version, and a [[provides]] of kind "preset"
  config/
    godwinmix.toml       the mixer's configuration
    layout.json          which panels go in which slot of the web UI
  scenes/
    full.json            one scene document per file
    two-box.json
  README.md              for the person who has four hours and a service on Sunday
```

The manifest's `[provides.preset]` table is the whole contract:

```toml
[[provides]]
kind = "preset"
id = "church"

[provides.preset]
plugins = ["ndi@^1", "browser@^1", "rtmp@^1", "lowerthird@^2", "tally@^1"]
config = "config/godwinmix.toml"
layout = "config/layout.json"
surface = "web"
theme = "calm"
scenes = "scenes"
```

`plugins` are semver ranges. `surface` is `web`, `none`, or the name of a
surface plugin. `theme` is a theme name the surface resolves. The three paths
are relative to the preset directory.

`layout.json` maps a UI slot to the panels in it, top to bottom:

```json
{
  "header": ["header"],
  "main": ["multiview", "scenes"],
  "sidebar": ["sources", "outputs"],
  "footer": ["alerts"]
}
```

The slots are `header`, `main`, `sidebar`, `strip`, `footer` and `modal`. The
panels the first party UI ships are `header`, `multiview`, `sources`,
`outputs`, `media`, `alerts` and `scenes`; a panel plugin adds its own under
its plugin name.

## Making your own

1. Copy the preset closest to what you want. `cp -r presets/church presets/my-church`
2. Change `name`, `version` and `description` in `gmx-plugin.toml`. The name is
   the namespace, so it has to be unique and it has to match the directory.
3. Edit `config/godwinmix.toml`. Put every line somebody has to change near the
   top, and say in a comment what to change it to.
4. Copy the layouts you want out of `layouts/` into `scenes/`, and give each
   one fresh ids. Ids are never reused across documents: `gmx scene layout
   two-box --values a=cam1,b=cam2 --out scenes/two-box.json` writes one with
   new ids already in it.
5. Rewrite `README.md` for the person who will use it. Four hours, on a
   Sunday, with no time to read anything else. What it gives them, what they
   need, three steps, and what to do when each of the usual things goes wrong.
6. `gmx preset apply ./presets/my-church` to try it from the directory before
   you publish it anywhere.

## Publishing one

A preset is a plugin, so it goes on the index the same way any plugin does:
its own repository, a tag, and a listing pull request. See `06-ecosystem.md`
for the index and the quality scale, and `docs/how-to/make-a-preset.md` for the
walk through.
