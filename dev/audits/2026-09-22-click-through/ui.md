# UI layer audit: every place the GUI tells a person to leave it

Scope: `ui/` (not `ui/legacy`, not `ui/test`), the desktop shell in `tauri-app/`,
and the text those surfaces render that arrives from elsewhere (preset
manifests, core error messages, core alerts). Written against the worktree at
`/Users/godwin/workspace/GodwinMix/.claude/worktrees/agent-a17c8d7ab539f4523`,
branch `work/composer-live`.

The real UI was run for this. The release binary from the main checkout was
started against a copy of `godwinmix.example.toml.bak` with the `[[sources]]` and
`[[outputs]]` blocks removed, `control.bind` set to `127.0.0.1:18611` and
`control.ui_dir` pointed at this worktree's `ui/`, so every screenshot is this
branch's JavaScript. Headless Chrome drove it over DevTools port 19611. Every
quoted screen string below was read off the running page, not off the source.

Screenshots are in
`/private/tmp/claude-501/-Users-godwin-workspace-GodwinMix/1eed66fb-ad84-4f9b-a731-6a38fb3c92c0/scratchpad/uiaudit/`
and are named in each finding.

No skill text reaches any UI. `skills/godwinmix-operate` and
`skills/godwinmix-develop` are agent facing markdown and nothing under `ui/` or
`tauri-app/` reads them, so there is nothing to report there.

---

## 1. Findings, worst first

### 1. The "Nearly there" dialog after every welcome tile tells the person to edit a TOML file and run a command

Where: `ui/panels/welcome/panel.js` lines 162 to 186, the `showSteps` function.
Line 172 renders each string in `result.plan.steps` as a list item with no
filtering. The strings come from the preset manifests, for example
`presets/church/gmx-plugin.toml` lines 28 to 32.

Click path: open the UI on a mixer with no sources, the welcome tiles appear on
their own, press Church service. Screenshot `03-church-nearly-there.png`.

What it says, exactly as shown:

```
Church service is set up. Three things left.

1. Put your YouTube and Facebook stream keys into the two [[outputs]] blocks of godwinmix.toml.
2. Run `gmx` and open http://localhost:8080 on the volunteer's screen.
3. Press Wide to put a picture on air, then press the red button when the service starts.
```

Classroom is the same shape, screenshot `21-classroom-nearly-there.png`:

```
1. Point the camera source at your webcam, or leave the placeholder and drop a file on the page.
2. Put your school's streaming URL and key into the [[outputs]] block, or record only.
3. Run `gmx`, open http://localhost:8080, and press Camera to start the lesson.
```

Every one of the six presets does this. The other four, read from their
manifests: broadcast says "Put your SRT contribution address and passphrase into
the [[sources]] blocks" and "check `gmx doctor` before the first feed"; default
says "Put your camera's RTMP address into the two [[sources]] blocks"; esports
says "Put the four player feeds and the caster camera into the [[sources]]
blocks"; headless-agent says "point your agent at it with `gmx mcp --url
http://localhost:8080`".

How bad: blocks a beginner, and it is the single worst thing in the product. The
volunteer has clicked one tile, is looking at the finished mixer, and step one
tells them to find and open a configuration file. Step two tells them to start
the program they are already using and open the page they are already on. The
screenshot makes the absurdity plain: while the dialog says to put stream keys
into `[[outputs]]`, the Outputs panel behind it is already showing `youtube` and
`facebook` with the label "Needs a stream key" and a primary button that says
"Add key". The GUI solved this problem and then told the person to go and solve
it again by hand.

Design: the dialog stops printing `steps` verbatim and builds its own list from
things it can act on. `preset.apply` already returns everything needed. The
`plan.plugins` array carries `installed` per plugin, which the dialog already
turns into an install button, and the mixer's own state carries the outputs with
`has_key === false`. So the list becomes rows, each with a button:

* An output with `has_key === false` becomes "youtube needs its stream key" with
  an "Add key" button that calls `editDestination` from
  `ui/panels/outputs/destination.js`, the same form the Outputs panel already
  opens. No new method.
* A missing plugin keeps the existing install row. No new method.
* A source with a placeholder address becomes "Wide has no camera yet" with a
  button that opens the source picker on the Cameras category. No new method.
* Only after those does the dialog show the one genuinely human step, the one
  about pressing a tile when the service starts.

The preset manifests then keep only that last kind of step. That part of the
change belongs to whoever owns `presets/`, but the UI must stop trusting the
field either way, because a third party preset can put anything in it. Size for
the UI half: M. Size for the manifest rewrite: S.

### 2. The OBS import tile prints a terminal command, and the protocol already has the method

Where: `ui/panels/welcome/panel.js` lines 232 to 262, the `importFromObs`
function.

Click path: welcome tiles, press Import from OBS. Screenshot
`02-obs-import.png`.

What it says, exactly:

```
GodwinMix reads an OBS scene collection and keeps your scenes, their items and
their positions. The importer runs on the machine that has OBS on it.

In a terminal, with the path to the collection:

gmx import obs ~/.config/obs-studio/basic/scenes/Untitled.json \
  --out godwinmix.scenes.json

On Windows the collections are in %APPDATA%\obs-studio\basic\scenes, and on
macOS in ~/Library/Application Support/obs-studio/basic/scenes.
docs/how-to/import-from-obs.md has the rest, including what does not come across.
```

How bad: blocks a beginner. It is one of five tiles on the first screen, it is
the only one that does nothing at all, and it sends the person to a terminal, to
two platform specific filesystem paths, and to a markdown file in a repository.

Design: `scene.import.obs` already exists (operate scope, takes `path`, returns
an `ImportReport` with `scenes`, `items`, `skipped`, `sources`,
`filters_duplicated` and `config_toml`). `media.upload` already exists and
already creates its directory. So the whole flow is buildable now with one small
protocol change:

1. The tile opens a dialog with a file input that accepts `.json`, plus the two
   platform sentences kept as a hint about where OBS writes the file, because
   the person genuinely does have to find it in another program.
2. The page uploads the chosen file with `client.upload`, which answers with a
   `path` on the mixer.
3. The page calls `scene.import.obs` with that path.
4. The report is shown as a summary: so many scenes, so many items, what was
   skipped, which filters were duplicated onto items.

The one gap is that `scene.import.obs` adds the scenes but not the sources they
draw. It hands back `config_toml`, a `[[sources]]` block for a person to paste,
which is the same defect one level down. The new parameter:

`scene.import.obs` gains `add_sources: bool` (optional, default false so nothing
already written changes). When true the handler calls `source.add` for each
source it parsed instead of only reporting them, and the report gains
`sources_added: string[]`. It does not touch the config file, it applies live,
and no restart is involved. The dialog always passes true.

Size: M for the UI, S for the parameter.

### 3. The token dialog sends the person to an environment variable or a config file

Where: `ui/shell/firstrun.js` line 41 for the first ask, `ui/boot.js` line 56 for
the second.

Click path: open the UI on a mixer started with a token. The dialog appears
before anything else. Screenshots `22-token-dialog.png` and
`23-token-refused.png`.

What it says, exactly:

```
This mixer needs a token
The mixer was started with a token, so it will not answer without one. It is
the value of GODWINMIX_TOKEN, or the token line in the config file.
```

and after a wrong one:

```
That token was refused. It is the value of GODWINMIX_TOKEN, or the token line
in the config file.
```

How bad: blocks a beginner on any secured install, and it is literally the first
screen. It is also the hardest of these to fix honestly, because the UI cannot
manufacture a secret it is not trusted with.

Design, in three parts:

* For the desktop shell this is already solved and the dialog should never be
  seen: `tauri-app/src/sidecar.rs` starts the core with a generated token and
  `connect_core` hands the page the URL with the token on it. Nothing to do
  except confirm the path, which this audit could not do because the Tauri app
  was not built.
* For a headless install the core should print a one time pairing URL on stdout
  when it starts with a token, the way a Jupyter server does. The person is
  already looking at that terminal, because they just typed the command that
  started it. That is a core change, not a UI one, but the UI should say
  "Paste the link the mixer printed when it started, or the token from it"
  rather than naming a variable and a file.
* The dialog should offer a second route on the same screen: a QR code or a copy
  button is useless without the secret, but a "the mixer printed a link when it
  started" sentence with the exact shape of that link is actionable in a way
  that "the token line in the config file" is not.

New method: none. Size: S for the wording, M for the core printing a pairing
link.

### 4. Settings cannot change a single setting the mixer actually has

Where: `ui/shell/settings.js`, the whole file. Both tabs.

Click path: the gear at the top right. Screenshots `15-settings-simple.png` and
`16-settings-advanced.png`.

Every control in Settings writes to `localStorage` under the key `gmx.settings`.
Theme, tile pictures, producer mode, meters, faders, the scrubber, the two
confirm toggles, tile width, picture rate, snapshot refresh. Not one of them
reaches the mixer. The Advanced tab adds a read only protocol line, a button
that forgets the saved token, a read only list of panels, a layout reset, and a
read only dump of the keyboard map.

Nothing in Settings touches the recording folder, the media folder, the canvas
size, the frame rate, the programme bitrate, `security.allow_exec_sources`,
`media.allow_upload`, `multiview.enabled`, the control bind address, the token,
or the installed plugins.

This matters most because of what the welcome dialog promises on the screen
before it. `ui/panels/welcome/panel.js` line 105: "Nothing here is permanent:
Settings changes any of it afterwards." That is not true of anything the preset
wrote into the configuration file.

How bad: blocks a beginner in combination with findings 1, 5, 8 and 11, each of
which ends with a person being told to change a setting that Settings does not
have. On its own, annoys.

Design: a third tab called "This mixer", drawn from a schema the core publishes,
so the reference UI is not hand maintaining a list of the core's fields. Two new
methods, named to match `plugin.settings.get` and `plugin.settings.set` which
already exist:

`core.settings.get` (read scope, no params) returns
`{ schema, values, restart_required }` where `schema` is a JSON Schema of the
settable core fields with the same `x-gmx-group`, `x-gmx-unit` and
`format: "secret"` annotations `ui/client/schema-form.js` already renders, and
`restart_required` is the list of field paths that only take effect on the next
start.

`core.settings.set` (admin scope, destructive) takes a partial object of the
same shape and returns `{ applied: string[], pending_restart: string[] }`. It
writes to the runtime overlay file that already exists beside the config (the
audit saw `empty.runtime.toml` being written with `[[sources]]`, `[[outputs]]`
and `[ui]` in it), so the person's own config file and their comments are left
alone. Fields that the mixer can take live are taken live and named in
`applied`; fields that cannot are named in `pending_restart`, and the UI then
offers the restart described in finding 6.

Size: L, and it is the change that unlocks four other findings.

### 5. The Command source kind says "Off unless the config allows it", and adding one hands the person a config line

Where: `ui/client/kinds.js` line 96 for the tile description; the refusal comes
from `crates/godwinmix-core/src/input.rs` lines 276 to 283 and arrives in a toast
through `errorToast`.

Click path: Add sources, the More category in the left rail, press the Command
tile, type anything into "Command line", press Add. Screenshots
`06-cat-more.png`, `07-command-form.png`, `08-command-refused.png`.

The tile says, exactly:

```
Command
A program that writes frames to its output. Off unless the config allows it.
```

The toast after pressing Add says, exactly:

```
Command: exec sources are disabled. They run a command line on this machine, so
anyone who can reach the control port could run anything. Set
security.allow_exec_sources = true only if that port is on a trusted network.
```

How bad: annoys rather than blocks, because a first time user is not reaching
for an exec source. But it is the clearest single example of the pattern: the
GUI offers a tile, lets the person fill in a form, then refuses and hands them a
TOML key. The description even warns them in advance and still lets them waste
the trip.

Design: the picker knows this before the person types anything. `core.info`
already carries a `features` array, and the settings schema from finding 4 would
carry `security.allow_exec_sources`. So:

* The More category draws the Command tile with the switch state on it, not a
  vague sentence. If exec sources are off, the tile reads "Command (switched
  off)" and opening it shows one sentence about why it is off and one button,
  "Allow command sources on this mixer", which calls `core.settings.set` with
  `{ security: { allow_exec_sources: true } }` behind a confirm that repeats the
  security reason in the core's own words.
* If the setting only takes effect on restart, the button offers the restart
  from finding 6.
* If the token does not hold admin scope, the button is replaced by the sentence
  naming what is needed, which the error's `data.needed` already carries.

Size: S once finding 4 exists.

### 6. `needs_restart` notes are printed as dead text, including one that names a command

Where: `ui/panels/welcome/panel.js` lines 177 to 179. The strings come from
`crates/godwinmix/src/control/methods/presets.rs` lines 303 to 338.

Click path: any preset tile. Screenshots `03-church-nearly-there.png` and
`21-classroom-nearly-there.png`.

Live, on Church service:

```
4 configuration key(s) were written to /private/tmp/.../uiaudit/empty.toml and
take effect on restart
```

On Classroom the same line said 12 keys. The other strings this field can carry,
read from the source: "<id> waits for the <plugin> plugin: `gmx plugin add
<plugin>`" and "<id> is in the config file; this core did not bring it up now,
so it starts on the next `gmx` restart".

How bad: blocks a beginner. A long absolute path in a dialog is a wall, and the
first of those two unseen strings prints the exact terminal command that the
same dialog has an install button for six lines higher up. Nobody knows what to
do with any of it.

Design: three changes in the same dialog.

* The plugin line is dropped entirely. `installRow` already handles that case
  with a button and does it properly; the note is a second, worse copy of the
  same information.
* The configuration line becomes a row with a button: "Some settings take effect
  when the mixer restarts" and a "Restart the mixer" button.
* The restart itself. `core.shutdown` exists (admin scope, destructive, no
  params) but shutting down is not restarting, and a GUI that stops the mixer
  and cannot start it again is worse than one that says nothing. So a new method:

`core.restart` (admin scope, destructive, no params) returns
`{ restarting: true, expect_back_in_secs: number }`. The core replies, then
re-executes itself with the same argv, the same config path and the same bind,
so the page can poll `/api/status` and reconnect on its own. The UI shows
"Restarting" over the page and comes back by itself, which is the behaviour the
disconnect banner in `ui/shell/shell.js` already half implements.

Where a core genuinely cannot re-exec, `core.restart` answers `-32001` with
`data.reason`, and the two hosts each have an answer. The desktop shell already
starts and stops the core (`tauri-app/src/sidecar.rs` `start` and `stop`,
`tauri-app/src/core_link.rs` `shutdown`), so a `restart_core` Tauri command is a
few lines and the shell can honour the restart even on an old core. The headless
install cannot: `deploy/systemd/godwinmix.service` line 37 is
`Restart=on-failure`, and a clean shutdown is not a failure, so the unit would
leave the mixer down. That line should become `Restart=always`, which is a one
word change in the deploy layer and the thing that makes a GUI restart honest on
a server.

Size: M for the UI, M for `core.restart`, S for the unit file.

### 7. The recording dialog asks for a folder path and offers no way to pick one

Where: `ui/panels/outputs/recording.js` lines 9 and 15.

Click path: Outputs, Record. Screenshots `11-record.png` and
`12-record-bad-folder.png`.

The field is a bare text input labelled "Folder on the mixer" with the
placeholder "Videos/GodwinMix in the mixer's home folder". There is no browse,
no list of anything, no indication of what is already there, and no way to know
whether a path exists before pressing the button. Typing `/does/not/exist/anywhere`
and pressing Start recording gives, in the dialog:

```
attaching output recording-mucwm5vn: building the record/output half of output
recording-mucwm5vn: cannot create /does/not/exist/anywhere; choose a writable
recording folder: Read-only file system (os error 30)
```

How bad: annoys. The failure is at least caught in the dialog rather than a
toast, and the message names the next step. But "choose a writable recording
folder" is advice a GUI should be acting on, not giving, and a volunteer on a
mixer in another room has no way to know what paths exist on it.

Design: the field becomes a select of folders the mixer proposes, with a
"Somewhere else" option that reveals the text box for the rare case. One new
method, in the style of `device.discover` and `media.list`:

`path.list` (read scope) takes `{ purpose: "recording" | "media", under?: string }`
and returns `{ dir, writable, free_bytes, entries: [{ name, path, writable }] }`.
With no `under` it answers with the places worth proposing: the configured
recording directory, the media directory, the mixer's home Videos folder, and any
mounted volume with free space. With `under` it lists that directory's
subfolders, so the select can drill down. It never lists files and never leaves
a small allowlist of roots, so it is not a filesystem browser bolted onto a
control port.

The dialog also gains a "New folder" button beside the select, which sends the
chosen parent and a name to a `path.create` of the same shape, because "there is
no folder for this yet" is the common case on a fresh mixer and telling the
person to make one elsewhere is the defect this audit is about.

Size: M.

### 8. The Media panel with no folder tells the person to create a directory the mixer would have created anyway

Where: `ui/panels/media/panel.js` lines 120 to 128 render `this.error`, which is
the `error` field of the `media.list` answer.

Click path: Media tab in the footer, on a mixer whose media directory does not
exist. Screenshot `13-media-no-folder.png`.

What it says, exactly:

```
The media folder /private/tmp/claude-501/-Users-godwin-workspace-GodwinMix/1eed66fb-ad84-4f9b-a731-6a38fb3c92c0/scratchpad/uiaudit/media does not exist yet. Create it and put files in it, or upload one here, then press Rescan.
```

How bad: annoys, and the instruction is wrong. `crates/godwinmix/src/control/upload.rs`
line 17 is `tokio::fs::create_dir_all(dir)`, so pressing Upload creates the
folder. The panel is telling the person to do by hand a thing the button beside
it does automatically, and it leads with that instruction rather than with the
button.

Also in the same area, `crates/godwinmix/src/control.rs` line 855 refuses an
upload on a mixer with `allow_upload = false` with "Set `allow_upload = true`
under [media] and restart, or put the file in the media directory yourself."
`ui/panels/media/panel.js` line 241 puts that string straight into the panel's
hint line, and `ui/shell/source-files.js` line 63 puts it into a toast. Neither
offers anything.

Design: the empty state becomes "Nothing here yet" with the Upload button, which
is already three inches away, made primary. No sentence about creating
directories at all. The `allow_upload` refusal becomes a row with an "Allow
uploads on this mixer" button via `core.settings.set` from finding 4, guarded by
the same admin scope check as finding 5.

Size: S for the empty state, S more once finding 4 exists.

### 9. Error toasts throw away everything the protocol gives them to act on

Where: `ui/shell/toast.js` line 54 is the whole of it:

```js
toast({ kind: "error", text: lead + (err.message || err.title), ms: 11000 });
```

`ui/client/errors.js` computes four things that nothing in the UI ever reads.
`title` (a heading per error code), `nextStep` (the sentence after the last full
stop), `retryable`, and `retryAfterMs`. A grep across `ui/` for `nextStep`,
`retryable`, `retryAfterMs`, `CODES.` and `err.data` finds no use outside
`errors.js` itself. `CODES.CONFIRM_REQUIRED` (-32020) is defined and never
handled, so a destructive call that is refused once pending a confirm token
becomes an error toast with no confirm button.

`crates/godwinmix-protocol/src/error.rs` shows what is being discarded. Every
error carries a `data` object. `not_found` puts `id`, `kind` and the array of
`valid` ids in it. `scope` puts `method`, `needed` and `held` in it. Others carry
`retry_after_ms` and `confirm_token`. AGENTS.md rule 4 says every error message
carries "a `data` object a caller can act on". The reference client is the one
caller that never acts on it.

How bad: annoys, but it is the multiplier under half of this report. Every
finding above ends with a toast that could have carried a button and does not.

Design: `errorToast` grows an optional action, and a small table maps the codes
that have an obvious one:

* `NO_SCOPE` (-32002): the toast reads `err.title` as its heading and says which
  scope is needed from `data.needed`, with a button that opens the token dialog.
* `CONFIRM_REQUIRED` (-32020): a "Do it anyway" button that repeats the call with
  `confirm: data.confirm_token`. The token is valid thirty seconds, so the toast
  uses that as its `ms`.
* `NOT_FOUND` (-32004) with `data.valid` non empty: the toast lists the ids that
  would have worked, which the message already does, and where the caller passed
  a select the UI refreshes it.
* Anything with `retryable` true: a "Try again" button, disabled for
  `retry_after_ms` and then enabled, taking a retry function the caller passes.
* Everything else keeps today's behaviour, message verbatim, which is right.

The signature becomes `errorToast(err, what, { retry })`. Every existing call
site keeps working unchanged. Size: M.

### 10. Two places in Settings ask the person to reload the page

Where: `ui/shell/settings.js` line 181, "Token forgotten. Reload the page to
enter a new one." And line 204, "Layout reset. Reload the page to see it."

Click path: the gear, Advanced, Forget the saved token or Reset the layout.
Screenshot `16-settings-advanced.png`.

How bad: cosmetic, but it is the same reflex in miniature and it is free to fix.

Design: forgetting the token closes Settings and calls `askForToken` from
`ui/shell/firstrun.js` at once, which is the thing the reload is for. Resetting
the layout calls `mountShell` again, or, if that is more than the shell can do
today, calls `location.reload()` itself. Either way the person presses one
button, not two. Size: S.

### 11. Multiview switched off leaves a dead monitor and no switch

Where: `ui/panels/multiview/panel.js` line 35, shown when
`state.multiview.enabled` is false (line 143).

What it says, exactly: "Multiview is switched off, so there is no picture here.
The programme is still going out."

How bad: annoys. The sentence is honest and does not send the person anywhere,
which is better than most of this list. But the mixer's main window is black and
there is nothing to press. The only way to get the picture back is
`[multiview] enabled = true` in the config and a restart.

Design: the note gains a "Turn the picture on" button using `core.settings.set`
from finding 4 with `{ multiview: { enabled: true } }`. Multiview is already
demand driven, so the change can apply live. If the core answers that it needs a
restart, the button hands over to the restart from finding 6. Size: S once
finding 4 exists.

### 12. The plugin install button offers to install a plugin that is installed, and pressing it does nothing visible

Where: `ui/client/kinds.js` line 487, `hasPlugin` returns false for a plugin with
`enabled === false`, so the picker draws it as missing. `ui/shell/picker.js`
lines 384 to 418, `installBlock`, only ever calls `plugin.add`.

Click path, reproduced live: disable a plugin over the protocol
(`POST /api/v1/plugins/camera/disable`), then Add sources, Cameras. Screenshots
`17-camera-plugin-missing.png` and `18-install-camera-result.png`.

The panel says:

```
Cameras need the camera plugin. It installs into this mixer while it runs, and
nothing goes off air.
[Install camera support]
```

Pressing it calls `plugin.search` and `plugin.add`, both of which answer ok in
under a millisecond because the plugin is already on disk, then `plugin.list`
comes back with `enabled: false` again, `hasPlugin` is still false, and the panel
redraws the identical offer. A warning toast fires and is gone in seven seconds.
Pressing it again does the same thing forever.

How bad: annoys, and it is a dead end with no exit inside the GUI.

Design: `plugin.enable`, `plugin.disable`, `plugin.update` and `plugin.remove`
all exist in the protocol and the UI calls none of them.

* `hasPlugin` splits into three states: absent, present but disabled, present
  with a `problem`.
* The install block reads the state and offers the matching button. Absent gives
  today's "Install camera support" and `plugin.add`. Disabled gives "Turn camera
  support back on" and `plugin.enable`. A problem gives the problem string the
  listing already carries and a "Try it again" button on `plugin.reload`.
* Settings gains a plugins section listing what is installed with enable,
  disable, update and remove, since all four methods exist and nothing in the UI
  reaches them.

Size: S for the picker, M for the Settings section.

### 13. The desktop shell sends the person to a folder, to a website, and to a markdown file

Where: `tauri-app/src/ui.rs`.

Three things, in the app menu:

Line 76, a menu item called "Open config folder". It opens Finder on the
directory holding the configuration file. That is the desktop app's only answer
to every core setting, and it is a text editor job with a nicer door on it.

Line 212, after Check for updates: "GodwinMix {version} is out. This copy is
{version}. Download it from the releases page." There is no link in the dialog
and no in app update. The person is expected to find the releases page.

Line 221, when the updater cannot be reached: "{error}\n\nAutomatic updates are
not switched on in this build. See docs/how-to/desktop-app.md." That names a file
in a source repository to somebody running a signed desktop application.

Also `tauri-app/src/sidecar.rs` line 185, when the core will not start: "The
mixer started and stopped again. What it said is in {path}." followed by the last
twenty log lines. The log tail in the dialog is the right instinct. The file path
beside it is the reflex.

How bad: annoys. None of these is on the path a first time user takes, but "Open
config folder" is the desktop app admitting it has no settings.

Design: "Open config folder" stays as a last resort and is joined by a real
Settings entry once finding 4 exists, so the folder is not the only route. The
update dialog gains an "Open the releases page" button using the opener plugin
that `reveal` already uses two functions above it, rather than telling the person
to find it. The "not switched on in this build" sentence drops the file name and
says what is true in one sentence: this build does not update itself. The sidecar
failure dialog gains a "Show the log" button that opens the file, and drops the
path from the sentence. Size: S for all four.

### 14. The source drawer tells the person to delete and recreate a source

Where: `ui/panels/sources/panel.js` lines 534 to 536.

What it says: "The address is fixed once a source exists. To point it somewhere
else, remove this source and add it again."

How bad: annoys. The statement is true. `SetSourceRequest` in `protocol.json`
lists `id`, `name`, `color`, `latency_ms`, `params`, `place` and `transport`, and
`additionalProperties` is false with a schema comment saying the first party
drawer sent `uri` for months and told the operator it was saved. So the protocol
is right to refuse it.

But the GUI is being asked to explain a two step manual dance that it could
perform. A person following the advice loses the source's scene placements, its
name, its colour and its position in the tray.

Design: a "Change the address" button in the drawer, which opens the kind's own
form with the current values, then, on save, does the remove and re-add in one
go, keeping the name, colour and every scene item that pointed at the old id. The
scene side needs `scene.item.bind`, which exists. Where the core can do the swap
atomically it should, and that would be a new method,
`source.replace` (operate scope), taking the old `id` and the full add request,
returning the new source, rebinding every scene item and keeping the id where it
can. Live, no config rewrite, no restart. Size: M for the UI alone, L with
`source.replace`.

### 15. The picker asks for a path typed by hand for a file already on the mixer

Where: `ui/shell/picker.js` lines 292 to 302.

Click path: Add sources, Video and images. Screenshot
`06-cat-video-and-images.png`. The row reads:

```
A file already on the mixer
A path on the machine the mixer runs on
[Enter path]
```

How bad: cosmetic on its own. Browse files sits directly above it and uploads
from the browser, which covers the common case, and this row is the escape hatch
for a large file already sitting on the mixer. But it is the same shape as
finding 7 and `path.list` would serve both.

Design: the row opens the folder picker from finding 7 rather than a free text
box, with a "type a path instead" fallback. Size: S once `path.list` exists.

### 16. The Screens category tells the person to grant an operating system permission and says nothing about how

Where: `ui/client/kinds.js` line 377.

What it says: "Nothing to capture was offered. Rescan after granting this machine's
screen recording permission."

How bad: annoys. Granting a screen recording permission genuinely is outside this
GUI, so a sentence is not wrong in principle. What is wrong is that it is the
only thing offered.

Design: on macOS the pane opens from a URL,
`x-apple.systempreferences:com.apple.preference.security?Privacy_ScreenCapture`,
and the desktop shell already carries the opener plugin. So the empty state gains
an "Open screen recording settings" button on macOS, and on Windows and Linux
keeps a sentence naming the right place rather than a general one. The same
applies to the Cameras and Microphones empty states, which today say "Check it is
plugged in" and "Check the machine can hear it". Size: S.

### 17. A toast names a protocol method to a volunteer

Where: `ui/panels/sources/panel.js` line 559.

What it says: "Folders arrive with source.group. To build a scene, drag these
onto Scenes."

Click path: drag one source tile onto another source tile.

How bad: cosmetic. Nobody is blocked. But `source.group` is a wire method name in
a message aimed at a person, and it reads as a developer talking to themselves.

Design: "Grouping sources into folders is not ready yet. To build a scene, drag
these onto Scenes instead." Size: S.

### 18. Core alerts and core refusals reach the person verbatim with nothing to press

Where: `ui/panels/alerts/panel.js` line 66 renders `a.message` straight, and
`ui/shell/toast.js` does the same for every refusal.

This is deliberate, and `ui/shell/toast.js` says so in its header comment: the
core's messages name the state and the next step, so the UI does not reword them.
That is the right rule. The problem is that a number of those next steps are
outside the GUI, and the UI has no mechanism to notice. Seen live in this audit:

```
lyrics did not start: cannot render web pages: the GStreamer `wpesrc` element is
not installed. Install gstreamer1.0-wpe (Debian, Ubuntu) or gst-plugins-bad built
with wpewebkit. It is not available on macOS.
```

That is in the "Nearly there" dialog after Church service, screenshot
`03-church-nearly-there.png`. A volunteer is being told to install a Debian
package.

How bad: annoys to blocks depending on the message. The wording belongs to the
core and to whoever audits that layer. What belongs here is the mechanism.

Design: finding 9's action table is that mechanism. Once an error's `data` can
carry `action: { kind, params }`, the core can say "this needs a plugin" or "this
needs a setting" in a machine readable way and the UI can put the right button
under it without parsing prose. The UI half is finding 9. Size: covered above.

### 19. Applying a preset from the GUI rewrites the person's configuration file and strips their comments, silently

Where: `ui/panels/welcome/panel.js` line 145 calls `preset.apply` with nothing
but a name. The dialog before it says, line 105: "Nothing here is permanent:
Settings changes any of it afterwards."

Confirmed live. After pressing Classroom, the first three lines of the config
file are:

```
# Merged by `gmx preset apply classroom`. The file as it was before is in empty.toml.bak.
# Comments from your own file are in that copy: this one is rewritten from
# the values, which is the price of merging two configurations.
```

Every comment in the stripped example config was gone, and the key order was
resorted. The dialog that caused it never mentioned that a file would be
rewritten, and `preset.apply`'s own MCP description says "it rewrites the
operator's configuration file, so it is not something to do during a show",
which the human facing tile does not repeat.

How bad: annoys a beginner (they have no comments yet) and would badly upset
anyone who has configured a mixer by hand and then presses a welcome tile to see
what it does.

Design: `preset.apply` already takes `dry_run` and already returns the whole plan
with every configuration key it would change. The welcome tile should call it
with `dry_run: true` first and show a short confirm: what will be added, what
will be changed, and one line saying the configuration file will be rewritten
with a backup at the named path. That is one extra call and one confirm for a
choice that reconfigures the machine. The "Nothing here is permanent" sentence
should go, because it is not true. Size: S.

---

## 2. What is already right and is the pattern to copy

The Outputs panel is the best thing in the UI and the template for all of the
above. `ui/panels/outputs/panel.js` lines 24 to 49 turn `has_key === false` into
the words "Needs a stream key" ahead of the connection state, and line 167 turns
that same fact into a primary button labelled "Add key". The file's own header
comment names the defect it was built against: "it is the one that used to end
with somebody being told to open a TOML file on the machine". Confirmed live in
screenshot `04-after-church.png`.

`ui/panels/outputs/destination.js` is the form behind that button and it is
exemplary. Five platform tiles with the exact words a person needs ("YouTube
Studio, Go live, Stream settings. Copy the stream key, not the stream URL"), a
server address filled in, a key field that is `format: "secret"` so it is never
read back, a "Replace key" button that must be pressed deliberately before a live
key can be typed over, and a key that is trimmed because pasting one out of a
dashboard picks up a newline. Screenshots `09-add-destination.png` and
`10-youtube-form.png`. Its header comment states the rule outright: "a GUI user
never edits a TOML file".

The plugin install block in `ui/shell/picker.js` lines 377 to 418 is the second
model. A category whose plugin is missing still appears, with one sentence and
one button that calls `plugin.add` over the same protocol the CLI uses, and the
listing is read again afterwards so the line reports what happened rather than
what was hoped for. The comment says why: "telling an operator to open a terminal
is telling them the answer is somewhere else". Screenshot
`17-camera-plugin-missing.png`.

The source picker's shape is right. Hardware first, real devices listed one per
row from `device.discover`, address boxes underneath as the fallback, and every
category drawn whether or not its plugin is installed. On this machine every
category listed something real without a word typed: "MacBook Pro Camera", "The
whole screen", "MacBook Pro Microphone", four named test patterns. Screenshots
`06-cat-cameras.png` and the rest of the `06-cat-*` set.

The welcome tiles themselves are right, up to the moment the follow up dialog
opens. Five pictures, one press, `preset.apply` over the public protocol, the
result applied live where it can be. `ui/panels/welcome/defaults.js` is careful
in the way the rest should be: the core proposes the theme, the gallery mode and
the layout, and a browser that already has a preference of its own keeps it.

`ui/panels/welcome/panel.js` lines 188 to 196 already contain the fix for
finding 1 applied to one case. The comment reads: "This used to print `gmx plugin
add camera` and stop there, which asks somebody who has just picked Church
service to go and find a terminal." Somebody did exactly the right thing to the
plugin line and left the preset's own three steps alone six lines above it.

The runtime overlay file is the mechanism finding 4 needs and it already works.
Sources, outputs and the `[ui]` block added from the UI go into
`<config>.runtime.toml` beside the config, with a header saying they take
precedence and that deleting the file goes back to the config. Core settings can
follow the same route without touching the person's own file.

The command palette is an honest escape hatch. `ui/shell/commands.js` lines 60 to
92 list every method `core.api` publishes as a command with a form built from its
schema, so a method with no button is still reachable and a plugin's new command
works the day it ships. `ui/shell/palette.js` lines 155 to 165 put a confirm in
front of anything the protocol marks destructive, because `core.shutdown` takes
no parameters and was one click from the palette. That is the right instinct in
both directions.

The tauri connect page is clean. Two radio buttons, "This computer" with the
subtitle "Start the mixer here. Nothing else to fill in.", and "Another machine"
with an address and a token. Everything that touches a socket, a process or a
file happens in Rust, and the page holds no token and knows no default address.

---

## 3. What could not be determined

The desktop shell was not built or run. `cargo build` for `tauri-app/` needs the
bundled GStreamer runtime under `tauri-app/gstreamer/` and the bundled plugins
under `tauri-app/plugins/`, both of which are gitignored and absent in this
worktree. So everything said about `tauri-app/` in finding 13 and in the token
part of finding 3 comes from reading `src/ui.rs`, `src/sidecar.rs`,
`src/core_link.rs` and `shell/connect.js`, not from pressing the menu items. In
particular, whether the desktop shell's generated token really does reach the
page without the token dialog ever appearing is unverified. It looks right in
`core_link.rs` lines 260 to 280, which build a URL with the token on it, but
somebody should press it.

The preset install row with a genuinely missing plugin was not seen. All three
first party device plugins are installed on this machine, so `installRow` in
`ui/panels/welcome/panel.js` never drew. The disabled plugin test in finding 12
exercised the same code in `picker.js` and found the dead end there, so the row
is probably fine when the plugin really is absent, but the "Installed, and
nothing restarted" and "Installed, but the mixer has not picked it up yet"
branches were not observed.

The `wasm` tier UI was not examined. It is behind a binary feature that is off by
default and the running core did not have it.

One thing that happened once and did not reproduce: on the load immediately after
applying the Church preset, the welcome tiles appeared again even though
`core.info` carried `ui.preset: "church"` and the mixer had two sources. A clean
reload a minute later behaved correctly, screenshot `05-reload-after-preset.png`,
with no dialog. It may be a race between `core.info` answering and
`client.state.sources` filling in, in `decide()` at
`ui/panels/welcome/panel.js` lines 76 to 88, or it may be an artifact of how the
audit reattached the debugger. It is not a finding I can stand behind and it is
worth ten minutes from somebody who can reproduce it.

Whether `security.allow_exec_sources`, `media.allow_upload` and
`multiview.enabled` can be changed live or only on restart was not tested,
because there is no way to change them at all short of editing the file and
restarting. Finding 4's `core.settings.get` needs a `restart_required` list and
somebody has to work out, field by field, which side each one falls on.
