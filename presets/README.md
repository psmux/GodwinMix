# Presets

A preset is a plugin of kind `preset`. It is a name for a working setup: the
plugins it needs, a configuration, a UI layout, a theme and the scenes. One
command puts it on a machine:

```
gmx preset apply church
```

Six ship with GodwinMix. Each is a directory here.

| Preset | For | Gallery | Plugins it names |
|---|---|---|---|
| `default` | what GodwinMix does out of the box: two RTMP cameras, one destination | live | rtmp |
| `church` | a Sunday service: two cameras, lyrics, slides, two destinations | icon | camera, browser, rtmp |
| `classroom` | a lesson: camera, screen, slides, a recording and a stream | snapshot | camera, screen, rtmp |
| `esports` | a match: four player feeds, a caster, an overlay, replays | live | ndi, browser, rtmp, screen, replay |
| `headless-agent` | a channel nobody watches: MCP documented, no UI | label | browser, rtmp, director |
| `broadcast` | a contribution feed: SRT in and out, tally, Companion | live | srt, ndi, tally, companion |

Some of those plugins do not exist yet. That is expected and it is not a
mistake in the preset: `gmx preset apply` applies what it can and names what it
cannot, so the preset is a working target the plugins are written towards.
`church` and `classroom` use built in kinds for everything they can, which is
why they put a picture on air today with the camera plugin still missing.

The six ship inside the binary, so `gmx preset apply church` works on a machine
that downloaded one file. A directory of the same name here wins over the built
in one, so editing these needs no rebuild.

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
  theme.css              optional: the preset's own theme, served by the core
  README.md              for the person who has four hours and a service on Sunday
```

The manifest's `[provides.preset]` table is the whole contract:

```toml
[[provides]]
kind = "preset"
id = "church"

[provides.preset]
plugins = ["camera@^1", "browser@^1", "rtmp@^1"]
config = "config/godwinmix.toml"
layout = "config/layout.json"
surface = "web"
theme = "calm"
theme_css = "theme.css"
scenes = "scenes"
gallery = "icon"

[[provides.preset.next]]
do = "stream_key"
output = "youtube"
text = "YouTube Studio, then Create, then Go Live, shows the key."

[[provides.preset.next]]
do = "install_plugin"
name = "camera"

[[provides.preset.next]]
do = "take"
source = "cam-wide"
```

`plugins` are semver ranges. `surface` is `web`, `none`, or the name of a
surface plugin. `theme` is a theme name the surface resolves: one of `dark`,
`light`, `high-contrast` and `system`, or your own with `theme_css` beside it.
`gallery` fixes what the input tiles show (`live`, `snapshot`, `icon`,
`label`); leave it out and the surface asks the machine. The `[[next]]` tables
are what the person does next, typed rather than written out, so the welcome
panel puts a box, a Save or an Install button on the screen instead of telling
somebody to go and edit a file. Three to five of them. `do` is one of
`stream_key`, `install_plugin`, `add_source`, `take` and `note`. The paths are
relative to the preset directory.

`docs/reference/presets.md` is every key and the merge rules.

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
   need, three steps written as things to press rather than files to edit,
   and what to do when each of the usual things goes wrong.
   Under 300 words, which the tests check.
6. `gmx preset apply ./presets/my-church --dry-run` checks the whole of it and
   writes nothing: every file it names, the config under the mixer's own
   loading rules, every scene parsed, validated and resolved, the layout, the
   theme, and the plugins that are missing.

Or skip all of that: get a machine working and run `gmx preset save my-church`,
which writes the directory from what is running with the stream keys and the
control token taken out. `docs/how-to/make-a-preset.md` has both paths.

## Publishing one

A preset is a plugin, so it goes on the index the same way any plugin does:
its own repository, a tag, and a listing pull request. See `06-ecosystem.md`
for the index and the quality scale, and `docs/how-to/make-a-preset.md` for the
walk through.
