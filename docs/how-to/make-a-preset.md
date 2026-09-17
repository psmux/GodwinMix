# Make a preset

A preset is a name for a working setup. Somebody runs one command and their
machine is configured the way yours is:

```
gmx preset apply church
```

It is data, not code. You do not write any Rust to make one, and you do not
need the GodwinMix source to publish one.

The fastest way to your first one is not this page at all. Get a machine
working, then:

```sh
gmx preset save my-church
```

which writes the whole directory from what is running, with the stream keys and
the control token taken out. Read on when you want to know what it wrote, or
when you would rather start from one of the six.

[The presets reference](../reference/presets.md) is every manifest key, the
merge rules and the six official presets in a table.

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
theme_css = "theme.css"            # when the theme is your own
scenes = "scenes"
gallery = "icon"                   # live, snapshot, icon or label

# What the person does next, one table each. See below.
[[provides.preset.next]]
do = "stream_key"
output = "stream"
```

`plugins` is a list of `name@range`, semver ranges. `gmx preset apply` installs
each of them, and names any it cannot find rather than failing outright.
Naming a plugin that does not exist yet is allowed: the preset is the target
the plugin gets written towards.

`surface` is `web`, `none` for a headless box, or the name of a surface plugin.
`theme` is a theme name your surface resolves: one of `dark`, `light`,
`high-contrast` and `system`, or your own with `theme_css` beside it. The paths
are relative to this directory.

`gallery` fixes what the input tiles show for your audience: `live` for a
producer, `icon` for a volunteer on a modest machine, `snapshot` for a laptop on
battery, `label` for a headless box. Leave it out and the surface asks the
machine, which is what `gmx doctor` proposes.

### What the person does next

Three to five `[[provides.preset.next]]` tables, in the order they are done.
Each one is typed, so the welcome panel can draw a control for it:

```toml
[[provides.preset.next]]
do = "stream_key"
output = "youtube"
text = "YouTube Studio, then Create, then Go Live, shows the key."

[[provides.preset.next]]
do = "install_plugin"
name = "camera"
text = "The two cameras are test patterns until this is here."

[[provides.preset.next]]
do = "take"
source = "cam-wide"
text = "Press Wide to put a picture on air."
```

| `do` | Its field | What the person gets |
|---|---|---|
| `stream_key` | `output` | A box and a Save on that destination, which goes green when the mixer says the key took. |
| `install_plugin` | `name` | An Install button. |
| `add_source` | `kind` | A button that opens the add picker on that kind. |
| `take` | `source` | The source to put on air first, named. |
| `note` | `text` | A sentence, when there is nothing to press. |

`text` is optional and is the line under the control. On a `note` it is the
whole entry.

Write them as things, not as instructions. "Put your stream key into the
`[[outputs]]` block" is the sentence this schema exists to delete: the person
reading it is looking at a page with a box on it, and sending them to a text
editor instead is the thing the owner calls nonsense. Say `do = "stream_key"`
and let the page put the box up.

[The presets reference](../reference/presets.md) has the whole schema, including
what happens to a `do` a build does not know.

`steps`, a list of three sentences, is what this replaced. A preset that still
carries one applies fine and every line is read as a `note`, so nothing third
party breaks. Do not write a new one.

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
`media`, `alerts`, `scenes`, `welcome`. A panel from a plugin is named
`<plugin>/<panel id>`.

`welcome` is the first five minutes: the three large tiles a person picks a
preset from. The shell puts it up on its own when the core has no sources and
no preset has been applied, so a layout never has to name it. Naming it anyway
pins it to a slot, which is what a kiosk build wants.

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
* **Three steps.** Three, not five, and the same three as your `[[next]]`
  tables. Written for somebody looking at the page: which button, which box,
  which tile. Never "edit this file".
* **When it does not work.** The three or four things that actually go wrong,
  each with the one command that says which it is.

## Test it before you publish it

The one command that checks the whole of it:

```
gmx preset apply ./presets/my-church --dry-run --config /tmp/try.toml
```

It reads the manifest, checks every file it names exists, loads the config under
the same rules the mixer does, parses and validates every scene and resolves its
bindings against your own sources, checks the layout names real slots and
panels, checks the theme is one somebody has, and prints the plan: the plugins
that are missing, the keys it would set, the sources and outputs it would add,
and what is left for the person. It writes nothing.

Then apply it for real into a scratch directory and start the mixer on what it
wrote, which is the only check that matters:

```
gmx preset apply ./presets/my-church --config /tmp/try.toml
gmx --config /tmp/try.toml
```

Ctrl-C once it comes up. `gmx scene validate presets/my-church/scenes/two-box.json`
checks one scene on its own when the plan says something about it.

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

* `gmx preset apply my-church` on a fresh machine merges the config, writes the
  scenes, sets the layout, the theme and the gallery mode, and names any plugin
  it could not find rather than failing.
* `gmx build --preset my-church --name "MyChurchMix" --icon icon.png` assembles
  a distributable directory with your branding and nothing forked, so `gmx
  build` after the next core release produces the next version. See
  [Make a custom build](custom-build.md).

## Starting from a machine instead

`gmx preset save my-church` writes the whole directory from what is running:

```
saved my-church to /home/you/.godwinmix/presets/my-church
  wrote    config/godwinmix.toml
  wrote    theme.css
  wrote    config/layout.json
  wrote    scenes/full-screen.json
  wrote    scenes/two-boxes-side-by-side.json
  wrote    gmx-plugin.toml
  wrote    README.md

taken out
  the control token was replaced with change-me
  a stream key in uri = "rtmp://a.rtmp.youtube.com/live2/..." was replaced
```

It works out the `plugins` list from the kinds your sources and outputs
actually use, carries the theme across, gives every scene fresh ids, and leaves
a README with the four headings for you to fill in. Three things it cannot do
for you, and you have to: write the description somebody browsing the index
will read, write the `[[next]]` tables, and write the "when it does not work"
section from what has actually gone wrong for you.

Read the config it wrote before you publish it. The redaction covers the control
token, the `[[tokens]]` table and the tail of every RTMP and SRT output URL, and
nothing else knows what a secret looks like in your deployment.
