# Make a preset

A preset is a name for a working setup. Somebody runs one command and their
machine is configured the way yours is:

```
gmx preset apply church
```

`gmx preset apply` and `gmx build` are not in this release yet; the presets and
everything below are, so a preset you write now is ready for the command that
installs it.

It is data, not code. You do not write any Rust to make one, and you do not
need the GodwinMix source to publish one.

## Copy the closest one

Six presets ship in `presets/`. Start from whichever is nearest to what you
want, rather than from nothing.

```
cp -r presets/church presets/my-church
```

| Preset | Start here when |
|---|---|
| `default` | cameras arrive over RTMP and one destination goes out |
| `church` | two cameras, a lyrics page, more than one destination |
| `classroom` | one camera and a screen, recorded and streamed |
| `esports` | feeds from other machines over NDI, a caster, an overlay |
| `headless-agent` | nobody is at the machine and an agent drives it |
| `broadcast` | SRT in and out, tally, a hardware surface |

## What each file does

```
my-church/
  gmx-plugin.toml        the manifest
  config/
    godwinmix.toml       the mixer's configuration
    layout.json          which panels go in which slot of the web UI
  scenes/
    full.json            one scene document per file
    two-box.json
  README.md              for the person who will actually use it
```

### `gmx-plugin.toml`

The manifest. A preset is a plugin of kind `preset`, so it has the same
`[plugin]` block as any other plugin and one `[[provides]]`.

```toml
[plugin]
name = "my-church"                 # the namespace, and the directory's name
version = "1.0.0"                  # semver, required
api = 1                            # the protocol level this was written against
description = "..."                # what somebody browsing the index reads
license = "Apache-2.0"

[[provides]]
kind = "preset"
id = "my-church"

[provides.preset]
plugins = ["ndi@^1", "browser@^1", "rtmp@^1"]
config = "config/godwinmix.toml"
layout = "config/layout.json"
surface = "web"
theme = "calm"
scenes = "scenes"
```

`plugins` is a list of `name@range`, semver ranges. `gmx preset apply` installs
each of them, and names any it cannot find rather than failing outright.
Naming a plugin that does not exist yet is allowed: the preset is the target
the plugin gets written towards.

`surface` is `web`, `none` for a headless box, or the name of a surface plugin.
`theme` is a theme name your surface resolves. The three paths are relative to
this directory.

### `config/godwinmix.toml`

The mixer's configuration, exactly as it would be on disk. Two rules make it a
good one.

Put everything somebody must change near the top, with a comment saying what
to change it to. A volunteer scrolls once.

Give every source and output a plugin qualified `type` and a `params` table,
alongside the `uri`:

```toml
[[sources]]
id = "cam-wide"                    # a slug: this is what an operator types
name = "Wide"                      # what they read
type = "ndi/source"                # <plugin>/<provide id>
uri = "ndi://CAM 1 (Wide)"         # still read, and still resolves by scheme
params = { name = "CAM 1 (Wide)" }
```

`type` says which plugin plays it, so nothing has to guess from the address.
`uri` is still read and still resolves by scheme and rank, which is why a
configuration written today keeps working.

### `config/layout.json`

Which panels go in which slot of the web UI, in order, top to bottom.

```json
{
  "header": ["header"],
  "main": ["multiview", "scenes"],
  "sidebar": ["sources", "outputs"],
  "footer": ["alerts"]
}
```

Slots: `header`, `main`, `sidebar`, `strip`, `footer`, `modal`.
Panels the first party UI ships: `header`, `multiview`, `sources`, `outputs`,
`media`, `alerts`, `scenes`. A panel from a plugin is named
`<plugin>/<panel id>`.

Leave out a slot you do not want to fill.

### `scenes/`

One scene document per file. The eleven built in layouts are in `layouts/` and
are the place to start. Do not copy the file: write it out with fresh ids, so
two presets on one machine cannot collide.

```
gmx scene layout two-box --values a=cam-wide,b=cam-pulpit \
    --out presets/my-church/scenes/two-box.json
```

`gmx scene layout --list` prints every layout and what each one takes.

### `README.md`

The person reading it has four hours and a service on Sunday. They will not
read anything else. Four headings, in this order, and nothing between them:

* **What it gives you.** What is on air when it works. Two or three sentences.
* **What you need.** Hardware, addresses, accounts. Be specific: "two NDI
  cameras on the same network" and "your YouTube stream key", not "a camera".
* **Three steps.** Three, not five. The first is `gmx preset apply`, the second
  is what to edit, the third is what to run and what to press.
* **When it does not work.** The three or four things that actually go wrong,
  each with the one command that says which it is.

## Test it before you publish it

Check each piece on its own:

```
gmx scene validate presets/my-church/scenes/two-box.json
gmx --config presets/my-church/config/godwinmix.toml
```

The first reports anything wrong with the scene. The second starts the mixer on
your config, which is the only real check that the config is right; stop it with
Ctrl-C once it comes up.

The repository's own tests check every preset in `presets/`: that the manifest
parses and names its files, that the config loads, that every source has a
plugin qualified type and a params table, that the layout names real slots and
panels, that every scene parses and validates, that no two presets share a node
id, and that the README has all four headings. `cargo test presets` runs them,
and a preset you add to that directory is checked the same way.

Then do the thing the tests cannot do: hand it to somebody who has not seen it,
watch them follow the README, and fix whatever they had to ask you about.

## Publish it

A preset is a plugin, so it is distributed like one: its own repository, a
tagged release, and a listing pull request against the index. `06-ecosystem.md`
has the index, the signing and the quality scale.

Two commands make a preset into a product:

* `gmx preset apply my-church` on a fresh machine: installs the plugins, merges
  the config, sets the layout and the surface.
* `gmx build --preset my-church --name "MyChurchMix" --icon icon.png`: a
  distributable bundle with your branding, installers for three platforms, and
  nothing forked, so `gmx build` after the next core release produces the next
  version.
