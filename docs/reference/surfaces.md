# Surfaces

A `surface` is a whole user interface for GodwinMix: the reference web UI, a
terminal UI, a Tkinter app, a Stream Deck plugin, an agent. This page is the
contract, which is small, and `gmx ui`, which is how one gets started.

## What a surface is, and is not

It is a program that talks the public protocol. It is not placed in the
pipeline, it carries no media, the supervisor does not restart it, and it has
no private channel to the core. Everything it can do, a script you write
yourself can do, which is the rule that keeps the protocol honest.

That makes it the least complicated plugin kind. The manifest says two things:
the command to start, and the protocol level it speaks.

```toml
[plugin]
name = "tui"
version = "0.2.0"
api = 1
description = "A mixer panel in a terminal: sources, outputs, meters, tally and take with number keys"
license = "Apache-2.0"
platforms = ["linux-x86_64", "linux-aarch64", "macos-aarch64", "macos-x86_64", "windows-x86_64"]
placements = ["in-process"]

[[provides]]
kind = "surface"
id = "tui"
surface = { run = "gmx-tui", api = 1 }
```

| Key | What it is |
|---|---|
| `surface.run` | the command to start. A string, required |
| `surface.api` | the protocol level this surface speaks. An integer of 1 or more, required |

There is no `[run]` table and no `media`, `transports` or `settings`. A surface
that declares `placements = ["sidecar"]` would need a `[run]`, and would be
saying it is a sidecar the supervisor should start and restart, which a UI is
not. `in-process` is the nearest true thing to say.

The manifest validator refuses a surface provide with no `surface` table, one
whose `run` is missing or empty, and one whose `api` is absent or below 1, and
each refusal names the key.

## `gmx ui`

```sh
gmx ui list          # every surface this machine can start
gmx ui tui           # start the terminal UI against the running mixer
gmx ui tui -- --multiview --fps 8    # everything after -- goes to the surface
```

`gmx ui` with no name lists, and so does `gmx ui list`. That means a surface may
not be called `list`, which is the price of the shorter line.

```
  NAME  API   COMMAND
  tui   1     /usr/local/bin/gmx-tui
              A mixer panel in a terminal: sources, outputs, meters, tally and take with number keys

Start one with `gmx ui <name>`.
```

### What it sets

A surface is started with its stdin, stdout and stderr inherited, because it
owns the terminal or the window, and with these in its environment:

| Variable | What it is |
|---|---|
| `GODWINMIX_URL` | the mixer's control address, from `--url`, `GODWINMIX_URL`, or the default |
| `GODWINMIX_TOKEN` | the token, from `--token` or `GODWINMIX_TOKEN`. **Removed** when there is none, so a token for another mixer is never handed to this one |
| `GMX_PLUGIN` | the plugin name |
| `GMX_PROVIDE` | the provide id |
| `GMX_PLUGIN_ROOT` | the plugin's directory |
| `GMX_API_LEVEL` | the core's api level |

The first two are the ones `gmx ctl`, `gmx mcp` and `gmx-tui` already read, so
a surface that reads them needs no arguments at all. The point is that an
operator who is already talking to a mixer should not have to tell their UI
where it is.

`gmx ui` carries the surface's own exit code out, so a script can act on it.

### Where the command is looked for

In this order:

1. `<plugin directory>/<run>` and `<plugin directory>/bin/<run>`, which is how a
   third party surface installed with `gmx plugin add` is found;
2. beside the `gmx` binary, which is how the first party terminal UI is found,
   because it ships next to `gmx` rather than being installed;
3. on `PATH`, which is how a surface installed by a package manager is found.

`.exe` is added on Windows. A command that is nowhere is listed by `gmx ui list`
with every place it looked, rather than hidden:

```
  panel  1     not found (looked in: /opt/gmx/plugins/panel/0.1.0/gmx-panel, ... , gmx-panel on PATH)
```

### Where the manifests are looked for

The plugins directory first, so an operator's installed surface wins, then the
directory `gmx` itself is in, then the checkout when you are running from one.
Manifests are found at `<root>/gmx-plugin.toml`, `<root>/<name>/gmx-plugin.toml`
and `<root>/<name>/<version>/gmx-plugin.toml`, which is the layout the loader
installs into plus the two shallower ones a checkout has.

One entry per plugin name: the nearest wins.

## The protocol level

`surface.api` is the level the surface was written against. `gmx ui` refuses to
start a surface whose level is above the core's and says so:

```
'panel' speaks protocol level 2 and this core is level 1. Upgrade GodwinMix, or
install a build of the surface for this level.
```

A surface at a level at or below the core's starts. The core advertises
`api_level` and `api_compatible` in `core.info`, Neovim style, and a surface
that wants to behave differently on an older core reads them there.

## A preset's choice

A preset's `[provides.preset]` block has a `surface` key: `web`, `none`, or the
name of a surface plugin.

```toml
[provides.preset]
plugins = []
config = "godwinmix.toml"
layout = "layout.json"
surface = "web"
theme = "dark"
scenes = "scenes.json"
```

`gmx preset apply` writes it into the `[ui]` section of the runtime store,
beside the layout, the theme and the gallery mode. `gmx ui list` reads it back
and marks the one the preset chose:

```
  NAME  API   COMMAND
* tui   1     /usr/local/bin/gmx-tui

* is the surface the applied preset chose. Start it with `gmx ui tui`.
```

`web` means the UI the core serves itself, so there is nothing to start.
`none` is a headless install. A name the machine does not have is said plainly
rather than silently ignored.

It is a note to whoever starts a UI, not an instruction: `gmx preset apply`
never starts a process.

## Writing one

The contract a surface speaks is in `docs/how-to/write-a-ui.md` and in
`protocol.md`. In short: one WebSocket to `/rpc`, `core.subscribe`, a snapshot
then deltas, and render at `event/flush`. Ask for the expensive streams by name
in `ext` and the core does that work only while you are asking.

There is a client for it in TypeScript, Python and Rust, each generated from
the same `protocol.json`, and each gives you the same things: state, events,
video, settings forms for plugins it has never heard of, and tools it can offer
the operator.

The surfaces that exist today:

| Surface | Where |
|---|---|
| the reference web UI | served by the core, `ui/` |
| the terminal UI | `crates/godwinmix-tui`, the first `surface` plugin |
| a Bitfocus Companion module | `integrations/companion` |
| an Elgato Stream Deck plugin | `integrations/streamdeck` |
| a Tkinter example in 150 lines | `examples/` |
| an agent | `docs/agents.md` |

The Companion module and the Stream Deck plugin are not `surface` plugins,
because neither host lets `gmx ui` start it: the Stream Deck app and Companion
start their own plugins. They are surfaces in every other sense and they use
the same contract.

## When something is wrong

| What you see | What it is |
|---|---|
| `there is no surface called 'x'. Installed: ...` | the name is not one of the installed plugin names. `gmx ui list` shows them |
| `declares surface.run = "..." and that command is not there` | the surface is installed but not built, or its binary is not where the manifest says. The message lists every place it looked |
| `No surfaces are installed.` | nothing on this machine declares `kind = "surface"` |
| `a surface provide must declare surface = { run, api }` | the manifest has `kind = "surface"` and no `surface` table |
| the surface starts and immediately exits | it is saying why on its own stderr, which `gmx ui` does not swallow. Its exit code is carried out too |
