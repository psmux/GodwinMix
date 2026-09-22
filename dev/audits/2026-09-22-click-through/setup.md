# Setup audit: from "I downloaded it" to "I am streaming"

Auditor: setupaudit. Worktree `/Users/godwin/workspace/GodwinMix/.claude/worktrees/agent-a46e40290aa93da40`.
Control port 18613, DevTools port 19613. Nothing in the worktree was edited except
`tauri-app/binaries/` (build output, gitignored) to get the desktop shell to compile.

## What I actually ran

* `cargo build --release -p godwinmix`, 3m19s, clean.
* `godwinmix` with no config in an empty folder.
* `gmx doctor` in the same empty folder.
* `godwinmix --example-config > godwinmix.toml`, then the core on that file, then the page
  in headless Chrome.
* A second core on a config with the sample sources and outputs commented out, which is
  what the desktop app writes on a first run, then the welcome tiles and the church preset
  through the page.
* `cargo build --release` in `tauri-app/`.

Exact output is quoted in the findings.

---

# 1. The findings, worst first

## F1. The very first command fails, and the fix it names is a shell redirect

Where: `crates/godwinmix/src/lib.rs:623-628`, and `crates/godwinmix-core/src/config.rs:1523`.

I made an empty folder, ran the release binary, and got this and nothing else:

```
Error: could not load godwinmix.toml. Run with --example-config to print a starting point.

Caused by:
    0: reading config godwinmix.toml
    1: No such file or directory (os error 2)
```

Exit code 1. No page, no port, no mixer. The person who downloaded a tarball, read
"run `godwinmix`", and did that, now has to work out that the answer is a shell
redirection they were never shown: `godwinmix --example-config > godwinmix.toml`.

`gmx doctor` in the same folder says it more explicitly, and is still a file edit:

```
warn  config             godwinmix.toml does not exist. Run 'godwinmix --example-config > godwinmix.toml' to start one
```

How a person reaches it: download, unpack, run the binary. There is no earlier step.

How bad: blocks a beginner. It is the first thing that happens.

Design. A missing config is not an error, it is a first run. `Config::load` on a
missing path should fall back to `Config::default()`, log one line saying which file
would be written and where, and start. The first mutating call that needs persistence
(`source.add`, `output.add`, `preset.apply`, anything that writes the runtime store)
writes the file at that point, from the same embedded `EXAMPLE_CONFIG` the desktop
already uses, with the sample sources and outputs commented out exactly as
`tauri-app/src/settings.rs:123` `first_run_config` does today. The commenting logic is
already written and tested; it is in the wrong crate. Move `first_run_config` into
`godwinmix-core::config` and let both the daemon and the shell call it.

No new protocol method. Size: S.

Second half of the same change: print one human line on start, not only the JSON log
line `{"message":"control server listening","bind":"127.0.0.1:18613"}`. The preset
tutorial claims "It prints the address it is listening on"; what it prints is a JSON
record with a `bind` field. A first time user wants `GodwinMix is running. Open
http://127.0.0.1:8080/` on stderr, once. Size: S.

## F2. The welcome dialog, the one good first run path, tells the person to edit TOML and run a command

Where: the web UI, the "Nearly there" modal. Text from `presets/*/gmx-plugin.toml`
`steps`, rendered by `ui/panels/welcome/panel.js:162-186`.

Click path: open the page on a mixer with no sources and no preset, the tiles appear,
click **Church service**. This is exactly what it showed me:

```
Nearly there

Church service is set up. Three things left.

1. Put your YouTube and Facebook stream keys into the two [[outputs]] blocks of godwinmix.toml.
2. Run `gmx` and open http://localhost:8080 on the volunteer's screen.
3. Press Wide to put a picture on air, then press the red button when the service starts.

lyrics did not start: cannot render web pages: the GStreamer `wpesrc` element is not
installed. Install gstreamer1.0-wpe (Debian, Ubuntu) or gst-plugins-bad built with
wpewebkit. It is not available on macOS.

slides did not start: starting input pipeline: starting input pipeline for slides:
Element failed to change its state

3 configuration key(s) were written to godwinmix.toml and take effect on restart
```

Three separate defects in one dialog.

Step 1 tells the person to open a text file. Behind this very dialog, the Outputs panel
already shows two rows reading `youtube  Needs a stream key  [Add key]` and
`facebook  Needs a stream key  [Add key]`, and clicking **Add key** opens a dialog that
says "YouTube Studio, Go live, Stream settings. Copy the stream key, not the stream URL."
with a Server field, a Stream key field and Save. The GUI already does step 1 better
than the instruction does. The instruction is simply stale.

Step 2 tells a person who is looking at `http://127.0.0.1:18613/` in a browser to run
`gmx` and open `http://localhost:8080`. It is wrong on both halves: the mixer is
already running (that is how the dialog got on screen) and the port is whatever this
core bound, not 8080.

The line "3 configuration key(s) were written to godwinmix.toml and take effect on
restart" (`crates/godwinmix/src/control/methods/presets.rs:333-339`) names a file and a
restart with no button to do either. Nothing in the web UI calls `core.shutdown`;
I grepped `ui/` and the only hits are a comment in `palette.js` and a legacy transport
route.

The same shape appears on every preset. All eighteen steps across the six presets are
either a file edit or a shell command:

* church: "Put your YouTube and Facebook stream keys into the two [[outputs]] blocks of godwinmix.toml." / "Run `gmx` and open http://localhost:8080 on the volunteer's screen."
* broadcast: "Put your SRT contribution address and passphrase into the [[sources]] blocks." / "Put the SRT destination and the tally controller's address into the config." / "Run `gmx`, open http://localhost:8080, and check `gmx doctor` before the first feed."
* classroom: "Put your school's streaming URL and key into the [[outputs]] block, or record only." / "Run `gmx`, open http://localhost:8080, and press Camera to start the lesson."
* default: "Put your camera's RTMP address into the two [[sources]] blocks, or drop a file on the page." / "Put your destination's URL and stream key into the [[outputs]] block."
* esports: "Put the four player feeds and the caster camera into the [[sources]] blocks."
* headless-agent: "Put your destination's URL and stream key into the [[outputs]] block." / "Run `gmx`, then point your agent at it with `gmx mcp --url http://localhost:8080`."

How bad: blocks a beginner. It is the one screen written for somebody who has never
used the product, and it hands them a terminal.

Design.

Give a preset step a shape instead of a sentence. In `gmx-plugin.toml`, replace the
string list with a table list:

```toml
[[provides.preset.steps]]
title = "Add your YouTube and Facebook stream keys"
does = "output.set"
targets = ["youtube", "facebook"]

[[provides.preset.steps]]
title = "Put the Wide camera on air"
does = "program.take"
targets = ["cam-wide"]
```

`does` names a method the UI already has a dialog for, `targets` names the ids it
applies to. The welcome dialog then draws a checklist where each row is a button that
opens the dialog that already exists (the Add key dialog for `output.set`, the source
drawer for `source.set`, the take for `program.take`), and ticks itself off when the
thing it names is satisfied, read from the live state: an output whose `has_key` is
true is done, a source that is `live` is done. Keep the strings as a fallback for a
third party preset that has not been updated, but ship none with a command in them.
Size: M.

The restart line needs a `core.restart` method. See F4.

The two failed sources need to say what a person does about them rather than repeating
the pipeline's words. `lyrics did not start: cannot render web pages: the GStreamer
wpesrc element is not installed... It is not available on macOS` is, on macOS, a dead
end with a Linux package name in it. The browser sidecar (`browser/`) is the supported
path on macOS and the message never mentions it. That belongs in the source row as a
state with a button, not in the setup dialog as prose.

## F3. Nothing in the GUI can change any setting the mixer actually runs on

Where: `ui/shell/settings.js`. Everything in it is `localStorage` under the key
`gmx.settings`, plus the theme and the layout.

I opened Settings on a live mixer. Simple tab: Theme, Tile pictures, Producer mode,
Show meters on tiles, Show faders on tiles, Show the scrubber on files, Ask before
removing anything, Ask before one source replaces a scene on air, Show the welcome
tiles again. Advanced tab: Tile width, Picture rate, Snapshot refresh, Protocol
(read only), Forget the saved token, a list of panels, Reset the layout, a list of
keyboard chords.

Not one of those reaches the core. The canvas, the bitrate, the media folder, the
multiview and snapshot switches, hardware acceleration, the control address and token,
the plugin directory, the safety rules, the nodes: all of them are settable only by
editing `godwinmix.toml` and restarting. I confirmed by reading the protocol table:
there are 126 methods in `protocol.json` and none of them is a `config.*`. The only
thing that writes the config file is `preset.apply`.

How bad: blocks a beginner for canvas size and bitrate (the two things a person with a
slow uplink is told to change, by the tutorial itself: "Lower `video_bitrate_kbps` in
`[program]` to 3000 and restart"), annoys for the rest.

### The table asked for

GUI today means: can a person change it from the web UI or the desktop shell as
shipped, without a file or a terminal.

| Setting | Config key | GUI today | Restart needed | Where it should live |
|---|---|---|---|---|
| Canvas size | `[canvas] width`, `height` | no | yes, the encoder is started once | Mixer Settings, Picture. Warn that it stops the programme |
| Canvas rate | `[canvas] fps` | no | yes | same row |
| Audio rate, channels | `[canvas] sample_rate`, `channels` | no | yes | Mixer Settings, Sound, behind Advanced |
| Video bitrate | `[program] video_bitrate_kbps` | no | yes today | Mixer Settings, Picture. Should become live: see below |
| Audio bitrate | `[program] audio_bitrate_kbps` | no | yes | same |
| Keyframe interval | `[program] keyframe_interval_secs` | no | yes | Advanced |
| Audio ramp, av offset | `[program] audio_ramp_ms`, `av_offset_ms` | no | yes | Advanced |
| Control address | `[control] bind` | no | yes | Mixer Settings, This mixer. Desktop overrules it anyway |
| Control token | `[control] token` | no (the page can only forget a saved one) | yes | Mixer Settings, This mixer, with a Generate button |
| Multiview on or off | `[multiview] enabled` | no | yes | Mixer Settings, Load |
| Multiview size, rate, quality | `[multiview] width/height/fps/jpeg_quality` | rate only, and only as a client request (`multiviewFps` in page Settings) | no, the mosaic follows the highest request | Mixer Settings, Load, as the ceiling |
| Snapshot on or off | `[snapshot] enabled` | no | yes | Mixer Settings, Load |
| Snapshot widths and intervals | `[snapshot] default_width`, `max_width`, `min_interval_secs`, `idle_secs` | no | yes | Advanced |
| Media folder | `[media] dir` | no | yes | Mixer Settings, Files |
| Uploads allowed, max size, convert threads | `[media] allow_upload`, `max_upload_bytes`, `convert_threads` | no | yes | Advanced. Note these three are not in `godwinmix.example.toml` at all |
| Recording folder | the Record dialog's `directory` | yes, as free text | no | already fine, see F7 |
| Hardware decode, encode, graphics | `[hardware] decode`, `encode`, `graphics` | no | yes | Mixer Settings, Picture, with what `--probe` found |
| Which plugins are installed | the plugins directory | yes, `plugin.add` from the picker and the welcome dialog | no | already good, see part 3 |
| Plugins directory | `[control] plugins_dir` | no | yes | Advanced |
| Plugin budgets | `[plugins.<name>] max_rss_mb` etc | no | yes | the plugin's own drawer |
| WASI grants | `[plugins] allow_wasi` | no | yes | Advanced, one row per plugin that asks |
| Browser sidecar | `[browser] sidecar`, `args`, `env`, `overlay_fps` | no | yes | Mixer Settings, Picture, with a found or not found line |
| Nodes | `[nodes] listen`, `server_names`, `clock` | no, and `node.enrol` refuses without it | yes | Mixer Settings, Machines |
| Exec sources | `[security] allow_exec_sources` | no | yes | Advanced, with the warning it already carries |
| Safety rules | `[safety] min_hold_ms` etc | no | yes | Mixer Settings, Safety |
| Stall and rebuild policy | `[stall] *` | no | yes | Advanced |
| Sources and outputs | `[[sources]]`, `[[outputs]]` | yes | no | already good |
| Layout, theme, gallery | `[ui] *` | yes, per browser | no | already good, but see F8 |

### The design: a Mixer Settings dialog, distinct from the page Settings

Page Settings stays what it is: this browser, this operator, this screen. It keeps the
theme, the tile pictures, the meters, the confirms, the keyboard. Nothing in it needs
the core's permission and nothing in it survives a change of browser, which is correct
for what is in it.

Mixer Settings is a second dialog, admin scope, reached from the gear menu as
**Mixer settings...** and from the palette as `mixer.settings`. It has one banner at the
top that only appears when there is unapplied work: "Three changes are waiting for the
mixer to restart. [Restart now] [Later]". Tabs:

* **Picture**: canvas size and rate, video and audio bitrate, hardware encode and decode
  with what the machine actually has beside each option (`codec.list` already answers
  this), the browser sidecar row.
* **Sound**: sample rate, channels, audio ramp, the A/V offset.
* **Load**: multiview on or off and its ceiling, snapshot on or off and its limits. The
  two switches that decide whether anything runs when nobody is watching.
* **Files**: media folder, recording folder, upload limits. Each with a **Create it**
  button when the folder is not there (F6).
* **This mixer**: bind address, token (show, regenerate, remove), plugins directory,
  the machines list (F5).
* **Safety**: min hold, takes a minute, flash guard, operator silence.
* **Advanced**: everything above that is rarely touched, plus exec sources, stall
  policy, plugin budgets, WASI grants.

New protocol methods, named in the existing style:

`config.get`, read scope. Params `{}`. Returns `{values, schema, source_path,
applies_live}`: the effective config as JSON, a JSON Schema for it with a title and a
one sentence description per key (the comments in `godwinmix.example.toml` are already
written and are the copy for these), the path of the file in force, and a list of which
keys take effect live. The UI renders it with `ui/client/schema-form.js`, which already
draws a form from a schema for plugin settings, so the dialog does not hard code a
field list and a build with more options grows the dialog by itself.

`config.set`, admin scope, mutating, idempotent, takes `idempotency_key` and
`dry_run` like everything else. Params `{values: {…}, apply: "now" | "on_restart"}`
where `values` is a sparse tree: only the keys named move. Returns `{written: [keys],
live: [keys], needs_restart: [keys], path}`. It rewrites the config file in place
through the same merge `preset::apply` uses, keeps a `.bak`, applies what it can live
and reports the rest. The keys that can apply live today without touching the encoder:
`[multiview]` everything, `[snapshot]` everything, `[safety]` everything, `[media]`
`max_depth`/`max_files`/`allow_upload`/`max_upload_bytes`, `[stall]` everything,
`[browser] overlay_fps`, `[security] allow_exec_sources`. Everything else is
`needs_restart`.

`config.reset`, admin scope. Params `{keys: [...]}`. Puts named keys back to the
shipped default. Same return shape.

Size: L for the whole dialog, but it splits cleanly. `config.get` plus `config.set`
plus the Load and Safety tabs (all live, no restart anywhere) is an M that lands on its
own and is the half most likely to be used.

## F4. When a change needs a restart, nothing can restart it

Where: `crates/godwinmix/src/control/methods.rs:203`, `tauri-app/src/ui.rs:57-67`,
`deploy/systemd/godwinmix.service`.

`core.shutdown` exists and is admin scope. There is no `core.restart`. The desktop
shell answers exactly two navigations from the page, `godwinmix://quit` and
`godwinmix://quit-all` (`on_navigation`, `tauri-app/src/ui.rs:61-65`), so a page cannot
ask the shell to restart the mixer. The shell has both halves of the work already
(`sidecar::stop` and `sidecar::ensure`) and exposes neither as a command.

On a headless server the unit reads `Restart=on-failure`, so a clean `core.shutdown`
exits zero and systemd leaves it stopped. Today, "restart the mixer" from a browser on
a server means somebody with ssh.

How bad: annoys today, blocks a beginner the moment F3 lands, because every second row
in that dialog ends in a restart.

Design, three pieces.

1. `core.restart`, admin scope, mutating, destructive. Params
   `{when: "now" | "when_idle"}`. Returns `{restarting: true, expect_back_within_ms,
   supervised: bool, url}`. `supervised` is the honest part: the core knows whether
   something will start it again. It is true when the desktop shell started it (the
   shell sets an env var when it spawns the sidecar, say `GMX_SUPERVISOR=desktop`),
   when the systemd unit is `Restart=always`, or when it is PID 1 in a container with a
   restart policy. When `supervised` is false the method still works but the answer
   says so, and the UI must not offer a Restart button that turns the mixer off and
   leaves it off.
   `when: "when_idle"` refuses while any output is `live` and says which, which is the
   right default for a mixer that might be on air.

2. The desktop shell grows a `godwinmix://restart` navigation target next to `quit`,
   which runs `sidecar::stop` then `sidecar::start` and navigates the window back to
   the new port. About twenty lines in `ui.rs` and `main.rs`, reusing what is there.

3. `deploy/systemd/godwinmix.service` changes `Restart=on-failure` to `Restart=always`
   with the burst limit kept, and the header comment says why. Without this the server
   case is a lie.

The UI then shows, in the Mixer Settings banner and at the bottom of the preset steps,
"Restart now" when `supervised`, and "These take effect the next time the mixer starts"
with no button when it is not.

Size: M.

## F5. Adding a second machine is a config edit and a copied command

Where: `crates/godwinmix/src/control/methods/nodes.rs:243-251`, `cli/node.rs:58-76`,
`godwinmix.example.toml:147-175`.

`node.enrol` exists, is admin scope, and mints a token. But on a core with no `[nodes]`
table the whole family refuses with:

```
this core has no node bridge, so it has no nodes. Add a [nodes] table to the config
with `listen` and restart.
```

And when it does work, `gmx node token` prints a command for the person to type on the
other machine:

```
  godwinmix node --core <this core>:8443 --name studio-b --enrol-token <token>
```

There is no nodes panel in the web UI at all. I grepped: `node.list` and `node.enrol`
appear nowhere under `ui/`.

How bad: annoys. Nobody reaches this in their first hour, but it is the clearest case
of a GUI that does not exist rather than a GUI that instructs.

Design. A **Machines** tab in Mixer Settings. Empty state: one sentence and a button,
"Use another computer for cameras or graphics. [Turn on] ". Turn on writes
`[nodes] listen` and `server_names` through `config.set` and offers the restart from F4.
Then **Add a machine**, which asks for a name, calls `node.enrol`, and shows the
enrolment as a QR code and a copyable one line command side by side, because the other
machine may not have the app on it yet. `node.discover` already exists and fills a
"found on this network" list. Once a node has dialled in, `node.list` drives the rows.

No new method beyond `config.set`. Size: M.

## F6. The media folder is never created and the panel shows a raw errno

Where: `crates/godwinmix-core/src/media.rs:147-183`, `crates/godwinmix-core/src/config.rs:840`
(`dir` defaults to the relative path `media`), the Media panel in the UI.

Nobody creates the media directory. Not the core at start, not `media.list`. I opened
the Media tab on a fresh mixer and the panel reads, in full:

```
Rescan
Upload

reading media: No such file or directory (os error 2)

Ad break in
s
Roll ad
End early
Custom path
```

"os error 2" in a panel a volunteer is looking at. The directory is created only by
`POST /api/v1/media/upload` (`crates/godwinmix/src/control/upload.rs:17`), so the state
heals if you happen to drag a file in first.

The plugins directory is created by the desktop shell (`settings.rs:93`) and by
`plugin.add`, so that one is fine. The recording folder is created by the recorder at
start (`plugins/file-record/src/output.rs:101`), also fine.

How bad: annoys, and it reads as a broken product on the first look at that panel.

Design. Create the media directory at start, next to the runtime directory, which is
already created and checked for writability by the doctor. One line in `run`. When it
cannot be created, say so in the panel as "The clip folder /path could not be made:
<reason>. [Choose another folder]", which opens Mixer Settings on the Files tab. Never
print an errno. Size: S.

## F7. The Record dialog asks a person to type a path

Where: the Outputs panel, **Record** button.

The dialog reads:

```
Record the programme

The file is saved on the mixer, at the same quality as the stream. Each start creates a new file.

Folder on the mixer
[                                               ]   placeholder: Videos/GodwinMix in the mixer's home folder
File format
MP4 (fragmented) / Matroska

Cancel   Start recording
```

The field is empty with a placeholder. There is no browse, no recent list, and the
default is described in words rather than filled in.

How bad: annoys. It works, and the default is sensible, but typing a path is the
desktop version of editing a file.

Design. Fill the field with the resolved default rather than hinting at it, so Start
recording works on the first click. Beside it a **Change...** button. In the desktop
shell that opens a native folder picker through a new Tauri command and writes the
chosen path back (the shell and the mixer are the same machine, so this is honest).
In a browser against a remote mixer, Change opens a small remote folder browser backed
by a new read only method `media.browse` (read scope, params `{path}`, returns
`{path, parent, dirs: [...], writable: bool}`), which is the same thing the media
folder chooser in F3 needs. Size: M.

## F8. `preset.apply` throws away the commented config file

Where: `crates/godwinmix-core/src/preset/apply.rs` and `save.rs`.

Applying the church preset from the welcome tiles turned my 506 line commented
`godwinmix.toml` into 111 lines of bare values. The header it writes is honest about it:

```
# Merged by `gmx preset apply church`. The file as it was before is in godwinmix.toml.bak.
# Comments from your own file are in that copy: this one is rewritten from
# the values, which is the price of merging two configurations.
```

The `.bak` is kept, so nothing is lost. But the comments in that file are the entire
documentation for every option the GUI cannot set, and the one first run path that a
beginner is supposed to take is also the one that deletes them. The menu item in the
desktop app is literally **Open config folder**, and the tutorial says "The canvas size,
the bitrate and the reconnect behaviour are all there, with a comment on every line."
After one click on a welcome tile, they are not.

This has already happened on the owner's own machine. In
`~/Library/Application Support/mix.godwin.desktop/`, `godwinmix.toml.bak` is 215 lines
and starts "# GodwinMix desktop: the mixer's config file. # Yours to edit." The live
`godwinmix.toml` beside it is 99 lines and starts "# Merged by `gmx preset apply
church`." One click on a welcome tile, and the file the menu item **Open config folder**
exists to show has no comments left in it.

How bad: annoys today. Once F3 lands and the GUI can set those things, it stops
mattering, which is the real answer.

Design. Short term, `toml_edit` instead of serialise from values: it preserves comments
and formatting and changes only the keys that moved. That also makes `config.set` in F3
safe to run repeatedly, which matters more. Size: M, and it is the same work as
`config.set`, so do them together.

## F9. The documented CLI first run starts a mixer full of errors and never shows the welcome tiles

Where: `godwinmix.example.toml:315-371` (two `[[sources]]` and one `[[outputs]]` are
live in the example), against `ui/panels/welcome/panel.js:76-80` (the tiles show only
when `sources === 0`).

I did exactly what `gmx doctor` says: `godwinmix --example-config > godwinmix.toml`,
then ran it. The log immediately filled with:

```
"message":"pipeline error","pipeline":"output-primary",...,"error":"Connection refused: Could not connect to 127.0.0.1: Connection refused"
"message":"pipeline error","pipeline":"input-cam1",...,"error":"Connection refused: Could not connect to 127.0.0.1: Connection refused"
"message":"scheduling output reconnect","output":"primary","delay":"100ms"
```

and kept reconnecting. The page showed two dead cameras and one destination reading
"Reconnecting, attempt 28". The welcome tiles never appeared, because two sources
exist, so the CLI user never sees the good path at all.

The desktop shell does not have this problem: `first_run_config` comments the samples
out. The difference is that the shell has the fix and the daemon does not.

How bad: blocks a beginner, in the sense that their first impression of the product is
a page full of failures they did not ask for.

Design. Same change as F1: `--example-config` and the auto written first run config both
go through `first_run_config`, so the samples ship commented. Then the tiles appear and
the person is on the path the product was designed around. Size: S, and it is already
written in the wrong crate.

## F10. The OBS tile is a terminal command, and `scene.import.obs` exists

Where: `ui/panels/welcome/panel.js:232-262`, `importFromObs()`.

One of the five welcome tiles is "Import from OBS: Bring a scene collection across from
OBS Studio and keep your scenes." Clicking it opens a dialog whose body is:

```
GodwinMix reads an OBS scene collection and keeps your scenes, their items and their
positions. The importer runs on the machine that has OBS on it.

In a terminal, with the path to the collection:

gmx import obs ~/.config/obs-studio/basic/scenes/Untitled.json \
  --out godwinmix.scenes.json

On Windows the collections are in %APPDATA%\obs-studio\basic\scenes, and on macOS in
~/Library/Application Support/obs-studio/basic/scenes. docs/how-to/import-from-obs.md
has the rest, including what does not come across.
```

A file path, a shell command, a Windows environment variable and a docs path, in a
dialog, on the first screen. And `scene.import.obs` is a real protocol method
(`POST /api/v1/scenes/import/obs`, operate scope) that takes `{path}`.

The honest part is that `path` is a path on the core's machine, and a browser file
picker hands back a file from the operator's machine. That is a real problem and the
dialog is a workaround for it rather than a lazy shortcut. But the product already
solved that exact problem for clips: `media.upload` streams the bytes to the core.

How bad: blocks a beginner who came from OBS, which is most of the audience.

Design. Two parts.

1. `scene.import.obs` grows a second form: the same REST path accepts a JSON body
   instead of a `path`, `{collection: <the parsed OBS json>}`. A collection file is a few
   hundred kilobytes, well inside the call budget. The welcome dialog then has a real
   file input: "Choose your OBS scene collection", the page reads the file with
   `FileReader`, posts it, and shows the `ImportReport` that the method already returns,
   including what did not come across. Where the assets it references are not on the
   mixer, the report lists them and each row gets an upload button that goes through
   `media.upload`.
2. In the desktop shell, where OBS and the mixer are the same machine, the dialog also
   offers "Find it for me", which looks in the three standard locations and lists the
   collections found, so the common case is one click with no file picker at all.

Size: M.

## F11. Every plugin's install instructions in the docs are two shell commands, though the GUI installs them

Where: `docs/how-to/use-a-webcam.md:15-32`, `docs/how-to/record-to-a-file.md`,
`docs/how-to/install-a-plugin.md`, and the `needs_restart` line in
`presets.rs:322-325`: ``"{} waits for the {plugin} plugin: `gmx plugin add {plugin}`"``.

The webcam page says:

```
From a checkout or a bare `godwinmix`:

    ./plugins/camera/build
    gmx plugin add ./plugins/camera
```

The record page's "Three steps" are:

```
./plugins/file-record/build
gmx plugin add ./plugins/file-record
```

then "write the output into the config file and restart the mixer" with a TOML block.

Meanwhile the source picker has an install button that does the right thing over
`plugin.add` (`ui/shell/picker.js:377-417`) and the welcome dialog has the same
(`panel.js:197-230`). The code is ahead of the documentation, and the one place the
code still prints a command is `still_pending` in `presets.rs`, which the welcome
dialog renders verbatim.

How bad: annoys. The GUI path exists; the words point away from it.

Design. Delete the command from `still_pending` and return structured data instead:
`{id, waiting_on: {plugin: "camera"}}`, so the UI draws the install button it already
has rather than a sentence. Rewrite the how-to pages to lead with the GUI path and keep
the commands in a "from a terminal" section underneath. Size: S.

## F12. The desktop shell does not build without a documented manual copy

Where: `tauri-app/tauri.conf.json:46` and `tauri-app/binaries/` (empty but for a
`.gitignore`).

`cargo build --release` in `tauri-app/` fails:

```
resource path `binaries/godwinmix-aarch64-apple-darwin` doesn't exist
```

`docs/how-to/desktop-app.md:31-38` documents the copy, so this is expected rather than
broken, and after doing the copy it builds. But there is no `dev/` script that does
step 1 the way `dev/bundle-gstreamer.sh` and `dev/bundle-plugins.sh` do steps 2 and 3,
which is an odd gap.

Also worth saying plainly: `docs/how-to/install-on-macos.md` opens with "No public
installers are published yet. Use the source build instructions in CONTRIBUTING.md for
now." So today, "I downloaded it" means "I cloned it and installed a Rust toolchain and
GStreamer". Everything above about first run is about the future state; right now the
first run is a compiler.

How bad: annoys a developer, cosmetic for the audience this audit is about.

Design. A `dev/bundle-mixer.sh` that does the build and the triple named copy, called
from `dev/desktop.sh` the way the other two already are. Size: S.

## F13. Three media options exist in the code and in no config example

Where: `crates/godwinmix-core/src/config.rs:824-838` against `godwinmix.example.toml:222-227`.

`[media]` in the example documents `dir`, `max_depth`, `max_files` and
`probe_timeout_secs`. The struct also has `allow_upload`, `max_upload_bytes` and
`convert_threads`. A person who wants to stop uploads on a public server has to read
the Rust to find out the key exists, or hit the refusal:

```
uploads are disabled on this server. Set `allow_upload = true` under [media] and
restart, or put the file in the media directory yourself.
```

which is itself a "edit a file, or put a file in a folder" message.

How bad: cosmetic until somebody needs it.

Design. `config.get`'s schema (F3) is generated from the struct, so this class of drift
cannot happen again, which is the real argument for generating the dialog from a schema
rather than hand writing fields. Until then, add the three keys to the example with
their comments. Size: S.

## F14. The token prompt sends the person to a config file

Where: `ui/shell/firstrun.js:41`.

When a mixer answers 401, the page asks:

```
This mixer needs a token
The mixer was started with a token, so it will not answer without one. It is the value
of GODWINMIX_TOKEN, or the token line in the config file.
```

For a browser pointed at somebody else's server that is unavoidable and correct: the
page genuinely cannot know the secret. It is listed here for completeness, not as a
defect to fix. The desktop shell already solves its own case properly, by generating the
token and seeding it into the page (`tauri-app/src/ui.rs:21-32`), and the person is
never asked.

How bad: cosmetic.

Design. No change to the prompt. The one improvement worth making is in Mixer Settings
(F3): a mixer that has no token should offer "Protect this mixer with a token" with a
Generate button, so the person who puts a laptop on a conference network is offered the
safe thing rather than reading about it in a comment.

---

# 2. Designing the desktop first launch

The desktop shell already does most of this well. What follows is what it does today,
then the gaps.

## What it does today, and it is the right shape

* Where the config lives, per platform, from `app_data_dir()`:
  macOS `~/Library/Application Support/mix.godwin.desktop/godwinmix.toml`,
  Windows `%APPDATA%\mix.godwin.desktop\godwinmix.toml`,
  Linux `~/.local/share/mix.godwin.desktop/godwinmix.toml`.
  Beside it: `core-token` (0600), `connection.json`, `local-core.port`,
  `gstreamer-registry.bin` and `plugins/`.
* What is written, with nobody asked anything: the example config with a preamble
  explaining the two lines the app overrules, every sample source and output commented
  out (`settings.rs:123` `first_run_config`), a 64 hex character token from the OS, and
  a free port taken from the OS at every start.
* The connect page: "This computer" is preselected with "Start the mixer here. Nothing
  else to fill in." One button. The token is seeded into the page's localStorage by an
  initialization script and stripped from the URL, so the operator is never asked for a
  secret the app made itself.
* A mixer left running by a crashed shell is adopted rather than fought with
  (`adopt_existing`), by port and token.
* The device plugins (camera, screen, microphone) are carried inside the app and staged
  into the data directory, so a person who has never opened a terminal has a camera to
  pick.
* When the core will not start, the error carries the last twenty lines of the log:
  "The mixer started and stopped again. What it said is in <path>.\nThe last lines of
  it:\n…". That is the right instinct.

## The gaps, and the design

**G1. A person with no file at all reaches a working mixer by clicking. Almost.**
They do, today, as far as an empty desk. What they cannot then do is change anything
about the mixer without the menu item **Open config folder**, which is the defect the
owner described. F3 fixes it. The desktop shell should hide **Open config folder**
behind a Help submenu or drop it entirely once Mixer Settings exists; a menu item whose
purpose is "go and edit a file" is an admission.

**G2. Nothing restarts the core from inside the app.** F4. `godwinmix://restart`
alongside `godwinmix://quit`, backed by `sidecar::stop` then `sidecar::start`, and a
navigate to the new port. The shell should also set `GMX_SUPERVISOR=desktop` in the
environment it spawns the sidecar with, so `core.restart` can answer `supervised: true`
and the web UI can offer the button.

**G3. The failure dialog is a log path.** When `wait_until_answering` fails the person
gets a message box with a file path and twenty lines of JSON log. The three failures
that actually happen are knowable: the port was taken, the config does not parse, or
GStreamer is missing an element. All three are what `core.doctor` reports. The shell
should run the mixer with `--info` or run `gmx doctor` and show the doctor table in the
dialog with a fix button per row, rather than the raw tail. Size: M.

**G4. Nothing asks what the person is streaming until the page loads.** The welcome
tiles come from the core's UI, which is correct by rule 2, and they work. Nothing to
change. But the shell should not preselect Connect and skip past them on a second
launch if the first launch never got a preset applied; today `saved.remembered` is true
after any successful connect, which is right, and `core.info.ui.preset` still drives the
tiles, which is also right. This one is already correct.

**G5. Updates.** "Check for updates..." answers either "Automatic updates are not
switched on in this build. See docs/how-to/desktop-app.md." or "Download it from the
releases page." Both send the person out of the app. Honest today, since nothing is
signed. Once releases exist this becomes a Download and Install button. Cosmetic for
now, but it should not ship as a docs path in a dialog.

---

# 3. What is already done well, and should be the pattern

1. **The welcome tiles apply a preset over the public protocol.** `preset.apply`, admin
   scope, the same call `gmx preset apply` makes. The comment at the top of
   `presets.rs` says why, and it is the right reason. The tiles themselves are good
   copy: "Pick the one closest to what you are doing."

2. **The plugin install button.** `ui/shell/picker.js:377` and
   `ui/panels/welcome/panel.js:197`. The comment in `panel.js` is the whole standard in
   one sentence: "This used to print `gmx plugin add camera` and stop there, which asks
   somebody who has just picked Church service to go and find a terminal." Both call
   `plugin.add`, both re-read the listing afterwards and say what actually happened
   rather than what was hoped for, both say "Installed, and nothing restarted."

3. **The Add key dialog.** An output with a placeholder key shows `Needs a stream key`
   and an **Add key** button, and the dialog says "YouTube Studio, Go live, Stream
   settings. Copy the stream key, not the stream URL." It knows the platform, it tells
   the person where to click in somebody else's product, and it never shows the key
   again. This is the best dialog in the product and it is the model for every row in
   Mixer Settings.

4. **The desktop first run writing its own config and its own token.** Nobody is asked
   for a port, a token or a file. The preamble it writes explains what the app overrules
   and why. `first_run_config` commenting the sample sources out so the app opens on an
   empty desk rather than a wall of reconnects is exactly the right instinct, and F1 and
   F9 are just asking for it to be applied to the daemon too.

5. **Adopting a mixer left running by a crashed shell**, by remembered port and token,
   rather than starting a second one to fight for the encoder. The comment explains it:
   "The daemon outliving the window is the right way round: a broadcast does not end
   because someone force quit a window."

6. **`gmx doctor` names a package per platform.** `install_hint` says "Install
   gst-plugins-base and gst-plugins-good (brew install gstreamer)" on macOS and "from
   the GStreamer MSI, the complete profile" on Windows. It is already in the protocol as
   `core.doctor`. It is not in the GUI anywhere, which is a missed opportunity rather
   than a defect: a **Check this machine** button in Mixer Settings that renders the
   doctor table would cost almost nothing.

7. **`preset.apply` keeps a `.bak` and says so in the file it writes.** The header
   explains what was lost and where to find it.

8. **The separation of "waiting on a plugin", "was refused", and "will start next time"**
   in `still_pending`. The comment is right that they read differently to an operator.
   The only problem is that all three are then rendered as prose.

---

# 4. What I could not determine

* **The desktop app running.** It compiles: after staging the sidecar by hand (F12),
  `cargo build --release` in `tauri-app/` finished in 1m08s and produced
  `tauri-app/target/release/godwinmix-desktop`. I did not launch it. The owner's own
  `~/Library/Application Support/mix.godwin.desktop/` already holds a live mixer's state
  (a config, a token, a remembered port, installed plugins, scenes), and starting the
  shell would have adopted or restarted that mixer and written to that directory. What I
  did instead was read that directory, which is the strongest confirmation available
  that `settings.rs` does what it says: `godwinmix.toml`, `godwinmix.toml.bak`,
  `core-token`, `connection.json`, `local-core.port`, `gstreamer-registry.bin` and
  `plugins/` are all there, with exactly the names and the preamble the code writes.
  Everything else I say about the connect page and the menus is read from
  `tauri-app/src/*.rs` and `tauri-app/shell/*`, not seen on screen. This was well inside
  the fifteen minutes allowed.
* **Windows and Linux first launch.** The paths and the bundled GStreamer logic are
  read from the code and from `docs/how-to/install-on-*.md`. Only macOS was exercised.
* **Whether `[canvas]` and `[program]` really cannot change live.** The example config
  states it as a contract ("It cannot change while a broadcast is running, because the
  output encoder is started once and never restarted") and I took it at its word. If
  the encoder can be rebuilt while the compositor keeps drawing, `video_bitrate_kbps` at
  least could become live, which would remove the most commonly needed restart. Worth a
  look by whoever owns the encoder.
* **`godwinmix.example.toml.bak`.** The brief named it as the commented original. No
  such file is in the tree. `godwinmix.example.toml` at the root is the commented
  original and is byte identical to what `godwinmix --example-config` prints; I diffed
  them. `godwinmix.toml.bak` is what `preset.apply` writes beside a config it rewrites,
  which is probably what the brief meant.
* **The browser sidecar.** `[browser] sidecar` is a path in the config and nothing finds
  or installs one. `browser/` is a whole subproject with its own build. Whether it is
  installable the way a plugin is, and so whether it could get an install button, I did
  not work out. The consequence is visible: on macOS a web source fails with a Linux
  package name and the message ends "It is not available on macOS."
