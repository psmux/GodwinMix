# Core and protocol audit: what the mixer tells a person to do outside the GUI

Agent: coreaudit. Worktree `/Users/godwin/workspace/GodwinMix/.claude/worktrees/agent-a292420f9e790a9fa`.
Layer: `crates/godwinmix-protocol`, `crates/godwinmix/src/control` and its `methods/`, and the refusal text in `crates/godwinmix-core/src`.

Everything quoted below was either read out of the source at the line given or pulled out of a running core. I built `target/release/godwinmix`, ran it in tmux against a scratch copy of `godwinmix.example.toml.bak` on `127.0.0.1:18612`, and called it over the WebSocket `/rpc` channel. Answers marked "live" are verbatim from that core.

One thing to know before the list: `POST /rpc` is 405. `/rpc` is a WebSocket upgrade only (`crates/godwinmix/src/control.rs:342`, `.route("/rpc", get(rpc_upgrade))`), and plain HTTP callers go through the generated REST layer at `/api/v1/*`. That is fine, but it is worth writing down because a person trying curl against `/rpc` gets an empty 405 body with no hint.

---

## 1. Findings, worst first

### 1. Every preset's "three steps" tells the person to edit TOML and open a terminal

Where: `presets/*/gmx-plugin.toml`, the `steps` array of each preset's `[provides.preset]` block. It travels to the browser through `preset.apply` and is rendered by `ui/panels/welcome/panel.js:162` (`showSteps`), line 172: `for (const step of steps) list.appendChild(el("li", { text: step }));`

Click path: open the UI on a core nobody has set up, the welcome tiles appear, press "Church service". The apply succeeds and a modal titled "Nearly there" comes up saying "Church service is set up. Three things left." followed by the steps.

Live, from `preset.apply {"name":"church"}` against the running core:

```
"steps": [
  "Put your YouTube and Facebook stream keys into the two [[outputs]] blocks of godwinmix.toml.",
  "Run `gmx` and open http://localhost:8080 on the volunteer's screen.",
  "Press Wide to put a picture on air, then press the red button when the service starts."
]
```

The other five are the same shape:

* default: "Put your camera's RTMP address into the two [[sources]] blocks, or drop a file on the page." / "Put your destination's URL and stream key into the [[outputs]] block." / "Run `gmx`, open http://localhost:8080, and press a tile to put it on air."
* classroom: "Point the camera source at your webcam, or leave the placeholder and drop a file on the page." / "Put your school's streaming URL and key into the [[outputs]] block, or record only." / "Run `gmx`, open http://localhost:8080, and press Camera to start the lesson."
* esports: "Put the four player feeds and the caster camera into the [[sources]] blocks." / "Put your destination's URL and stream key into the [[outputs]] block." / "Run `gmx`, open http://localhost:8080, and press Quad to bring all four up."
* broadcast: "Put your SRT contribution address and passphrase into the [[sources]] blocks." / "Put the SRT destination and the tally controller's address into the config." / "Run `gmx`, open http://localhost:8080, and check `gmx doctor` before the first feed."
* headless-agent: "Put your destination's URL and stream key into the [[outputs]] block." / "Run `gmx`, then point your agent at it with `gmx mcp --url http://localhost:8080`." / "Ask the agent for `get_state`, then `take` a source, and watch `gmx ctl status`."

Two of the three steps in every preset are impossible from the GUI as written, and the middle one ("Run `gmx`, open http://localhost:8080") is being read by somebody who has already done exactly that. The person is standing in the running UI being told to go and start it.

How bad: blocks a beginner. This is the first thing a new user sees and it hands them a text editor.

Design. The steps are a wizard, so make them one. `preset.apply` already reports enough to drive it: the plan carries `outputs` with their placeholder URIs, `sources` with theirs, and `plugins` with `installed` false. Replace the `<ol>` of prose with a checklist where each row is a control:

* An output whose `uri` still holds a marker (`YOUR-STREAM-KEY`, `change-me`, `CHANGE-ME`, `YOUR-KEY`) becomes a row with the destination's name, a "Stream key" field and a Save button. Saving calls the existing `output.set` with the placeholder substituted. No new method, no restart: `output.set` is live and the mixer persists the result to the runtime store (`persist_runtime`, `crates/godwinmix-core/src/mixer.rs:2693`).
* A source whose `uri` holds a marker gets the same treatment through `source.set`.
* A missing plugin already gets a working Install button (see section 3). Keep it.
* The last step, the one that is actually an instruction to press something in this window, stays as prose but points at the thing: "Press Wide, top left, to put a picture on air."

The manifest field then becomes structured rather than prose. Add to the preset manifest schema a per step `{ text, does }` where `does` is one of `fill_output`, `fill_source`, `install_plugin` or `press`, so the UI knows which control to render and the CLI can still print the text. Six manifest files change, plus `crates/godwinmix-core/src/preset/manifest.rs:88` and `ui/panels/welcome/panel.js`.

Size: M.

### 2. There is no method that reads or writes the mixer's own configuration

Live, against the running core:

```
=== config.get {}
{
  "code": -32601,
  "message": "there is no method 'config.get'.  Call core.api for the whole list.",
  "data": { "method": "config.get", "nearest": [], "retryable": false }
}
```

I read all 126 methods out of `protocol.json`. Nothing named `config.*`. The closest things that exist are:

* `core.info`, which reports `canvas` read only.
* `preset.apply`, which rewrites the whole config file.
* `plugin.settings.set`, which rewrites `[plugins.<name>]` in the config file (`crates/godwinmix/src/control.rs:245`, `save_plugin_settings`).
* `source.*` and `output.*`, which persist to the `<config>.runtime.toml` sidecar.

So a GUI today cannot read or set a single one of: `canvas` (width, height, fps, sample_rate, channels), `[program]` bitrates and `encoder` and `audio_ramp_ms`, `[control] bind` and `token` and `[[tokens]]`, `[multiview]`, `[snapshot]`, `[media] dir` and `allow_upload`, `[hardware]` encode/decode/graphics, `[plugins]` allow lists, `[browser]`, `[nodes]`, `[security]`, `[safety]`, `[stall]`, `[codecs]`.

That single gap is what produces findings 4 through 13 below. Every one of them is an error message pointing at a key the protocol has no way to change.

How bad: blocks a beginner, and it is the root cause of most of this report.

Design: see section 2 of this document, which is the full `config.get` / `config.set` design the brief asked for.

### 3. The OBS import tile prints a terminal command and stops

Where: `ui/panels/welcome/panel.js:232`, `importFromObs()`. The comment above it is honest: "The OBS importer is `gmx import obs`; the page says where to point it."

Click path: welcome tiles, press the OBS tile. A modal titled "Import from OBS" comes up with:

```
"GodwinMix reads an OBS scene collection and keeps your scenes, their items and
 their positions. The importer runs on the machine that has OBS on it."
"In a terminal, with the path to the collection:"
gmx import obs ~/.config/obs-studio/basic/scenes/Untitled.json \
  --out godwinmix.scenes.json
"On Windows the collections are in %APPDATA%\obs-studio\basic\scenes, and on
 macOS in ~/Library/Application Support/obs-studio/basic/scenes.
 docs/how-to/import-from-obs.md has the rest, including what does not come across."
```

The galling part is that the method already exists. `scene.import.obs` is in the table at `crates/godwinmix/src/control/methods/scenes.rs:206`, Operate scope, and works. But it takes a server side path:

```rust
pub struct ImportObsRequest {
    /// The collection JSON exported from OBS (Scene Collection, Export), as a
    /// path on the machine the core is running on.
    pub path: String,
}
```
(`crates/godwinmix/src/control/methods/scenes/requests.rs:64`)

Live refusal when the path is wrong, which is the only thing a browser can produce:

```
=== scene.import.obs {"path":"/nope/scenes.json"}
{
  "code": -32004,
  "message": "could not read /nope/scenes.json: No such file or directory (os error 2). Export the collection from OBS with Scene Collection, Export, and give the path to the file it writes."
}
```

And the answer hands back more TOML to paste. `ImportReport.config_toml` at `crates/godwinmix/src/control/methods/scenes.rs:422`:

```rust
/// The `[[sources]]` block to paste into a config, so the sources the
/// scenes draw can be added in one edit rather than one call each.
pub config_toml: Option<String>,
```

So the successful path also ends in a text editor.

How bad: blocks a beginner who is migrating, which is the single most valuable person to not lose.

Design. Two changes, both small.

First, let the method take the file itself. Widen `ImportObsRequest` to `{ path?: String, content?: String, upload?: String }`, exactly one of the three:

* `path` as today, for the CLI and for an agent on the mixer's machine.
* `content`, the collection JSON inline. An OBS scene collection is tens of kilobytes; `INLINE_LIMIT` in `share.rs` already sets the precedent for where inline stops.
* `upload`, the name of a file that came in through `media.upload`, for a collection too big to inline. This is the least work of the three because `media.upload` already streams a body to disk and `MediaLibrary::resolve` already refuses a path outside the library.

Refuse all three missing with a message that names the browser path: "scene.import.obs needs the collection: `content` with the JSON in it (what the page's file picker sends), or `path` to a file on the mixer's machine."

Second, replace the modal body. A drop zone and a "Choose a collection" `<input type=file>`, `FileReader.readAsText`, then `scene.import.obs { content }`. The page already knows how to take a dropped file; `ui/panels/media/` does it for clips. The `%APPDATA%` and `~/Library` paths stay as a dim hint under the drop zone so the person knows where to look in their own file dialog, which is a legitimate use of prose: it tells them where their own files are, not what to type.

Third, retire `config_toml`. The sources the collection needs are already in `imported.sources`; add them with `source.add` in a loop, in the same call, and report which ones came up. The import report then reads "14 items across 4 scenes, 6 sources added, 2 need the ndi plugin" with an Install button beside the second number. If keeping `config_toml` for the CLI matters, mark it `#[serde(skip_serializing_if)]` behind an explicit `want_config_toml: bool` so it never reaches a browser by accident.

Size: M for the method plus the page, S if only the inline `content` variant lands first.

### 4. The multiview panel, when multiview is off, tells the person to edit the config and restart

Where: `crates/godwinmix/src/control/streams.rs:131`.

Live, `GET /mjpeg/sheet` on a core with `[multiview] enabled = false`:

```
HTTP/1.1 404 Not Found
{"error":"[multiview] enabled = false, so there is no picture to preview. Turn it on and restart the core."}
```

Click path: the multiview panel is in `main` in the church preset's own layout, so this is on the first screen. Every tile in the gallery is a dead box and the only explanation names a TOML key.

How bad: blocks a beginner. The multiview is the main screen of a video mixer.

Design. With `config.set` in place (section 2), the panel renders an empty state with one button: "Turn the multiview on". It calls `config.set { "multiview.enabled": true }`, gets back `{"applied": [], "needs_restart": ["multiview.enabled"], "restart": {...}}`, and shows the restart offer described in section 4 of this document. The refusal text itself changes to name the control rather than the key: "The multiview is switched off. Turn it on in Settings, then restart the mixer." Keep the key in `data` so an agent and a log still have it: `.with("config_key", "multiview.enabled")`.

Size: S for the text and the empty state, once `config.set` exists.

### 5. Snapshots refused with "[snapshot] enabled = false ... Set it to true and restart"

Where: `crates/godwinmix-core/src/snapshot.rs:167`, `Tracker::disabled_reason`.

Live, `snapshot.get {"id":"sheet"}` on a core with `[snapshot] enabled = false`:

```
{
  "code": -32001,
  "message": "snapshots are switched off by [snapshot] enabled = false in the mixer's config. Set it to true and restart to get stills and motion back.",
  "data": { "retryable": true }
}
```

Live, the same over HTTP at `GET /api/snapshot/sheet`: `503` with the same sentence as the body.

The second branch at `snapshot.rs:175`, which I could not provoke on the same run because both switches were off, reads:

```
"stills are cut out of the mosaic, and the mosaic is switched off by
 [multiview] enabled = false in the mixer's config. Set it to true and
 restart, or read /api/agent/state, which works without pictures."
```

Click path: any gallery tile set to the "snapshot" mode, or the scenes panel asking for a still.

How bad: blocks a beginner when it fires, though the default config has both on so most people never see it. The "or read /api/agent/state" tail is written for an agent and is noise in a browser.

Design. Same as finding 4: an empty state with "Turn snapshots on", `config.set { "snapshot.enabled": true }`, restart offer. Split the message by caller the way `crates/godwinmix-core/src/safety.rs:304` already splits by `token.agent`: an agent gets the `/api/agent/state` fallback, a person gets "Switch snapshots on in Settings."

Size: S.

### 6. The node bridge refusal names a TOML table and a restart

Where: `crates/godwinmix/src/control/methods/nodes.rs:243`, `runtime()`.

Live:

```
=== node.enrol {"name":"pi"}
{
  "code": -32001,
  "message": "this core has no node bridge, so it has no nodes. Add a [nodes] table to the config with `listen` and restart.",
  "data": { "retryable": true }
}
```

The same sentence again at `crates/godwinmix-core/src/plugin/host/bridged.rs:320`, for a plugin placed on a node: "this core has no node bridge, so nothing can be placed on a node. Add a [nodes] table to the config with `listen` and restart."

And `node.discover` when multicast is missing, `nodes.rs:236`: "A network without multicast finds nothing this way; list the node in the [nodes] table instead".

Note that `node.list` does *not* refuse. Live it answers `{"listening": false, "nodes": []}`, which is the right shape: a GUI can see the bridge is off without provoking an error.

How bad: annoys. Nodes are an advanced feature and the person reaching for them is not a first timer. But it is the same defect.

Design. The nodes panel reads `node.list`, sees `listening: false`, and shows "This mixer is not listening for nodes" with a "Start listening" button. That calls `config.set { "nodes.listen": "0.0.0.0:7443" }` (offer the port, default it) and then the restart. The refusal keeps its first clause and loses the second: "this core has no node bridge, so it has no nodes. Turn it on under Nodes in Settings." with `data.config_key = "nodes.listen"`.

Size: S.

### 7. `node.enrol` hands back a command line for the other machine

Where: `crates/godwinmix/src/control/methods/nodes.rs:175`.

```rust
"command": format!("godwinmix node --core <this core>:<node port> --name {name} --token {token}"),
```

and when the token expires, `nodes.rs:191`:

```
"`{waiting}` did not enrol before the token expired. Mint another with `gmx node token --name {waiting}`"
```

Click path: Nodes panel, "Add a node", read out the command to somebody standing at the Pi.

How bad: annoys, and honestly this one is half legitimate. The other machine is a different machine; something has to be typed there or carried there. But `gmx node token --name X` is not: the GUI just called `node.enrol` and can call it again.

Design. Keep `command` (it is the genuine cross machine handoff) but make the panel show it as a copy button plus a QR code, not as prose to retype, and add a short enrolment code as an alternative so a person at the Pi can type six characters into that machine's own first run screen rather than a 90 character token. The expiry error drops the CLI: "`pi` did not enrol before the token expired. Press Add a node again for a fresh one." The button is already there.

Size: S for the text, M with the QR and the short code.

### 8. Exec sources refused with a `[security]` key to set by hand

Where: `crates/godwinmix-core/src/input.rs:278`, `ExecSpec::from_uri`.

Live:

```
=== source.add {"id":"x1","uri":"exec:cat /dev/null"}
{
  "code": -32001,
  "message": "exec sources are disabled. They run a command line on this machine, so anyone who can reach the control port could run anything. Set security.allow_exec_sources = true only if that port is on a trusted network.",
  "data": { "method": "source.add", "retryable": true }
}
```

Click path: Sources panel, Add, pick the Command kind, fill in a command line, press Add.

How bad: annoys. The explanation is genuinely good, which is what makes the ending bad: it earns the person's trust and then abandons them at a config file.

Design. This one wants a confirmation, not a settings toggle, because the whole point is that the person should think before doing it. The picker's Command kind shows the same paragraph and a checkbox: "I understand this runs a command on the mixer's machine. Allow command sources." Ticking it calls `config.set { "security.allow_exec_sources": true }`. Note this key is read per source add from the mixer's held config (`crates/godwinmix-core/src/mixer.rs:1773`, `self.cfg.security.allow_exec_sources`), so with a live `config.set` that reaches the mixer's `cfg` it applies to the very next add with no restart at all. That makes it one of the best keys to prove the live path with.

The refusal loses its last sentence and gains `data.config_key = "security.allow_exec_sources"`.

Size: S.

### 9. The take guard tells the operator to edit `[safety]` and restart

Where: `crates/godwinmix-core/src/safety.rs:312`.

```
"the shot on air has been up for {held} ms and this core holds a
 shot for {} ms, so there are {left} ms left. Wait {left} ms and
 take again, or set [safety] min_hold_ms lower in the config and
 restart."
```

Click path: press two source tiles in quick succession during a show. A toast comes up with that sentence.

Worth saying what is right here first: the code already branches on `token.agent` and gives an agent a different message, with a comment explaining exactly why ("an agent cannot, and telling it to go and get a better token is the one piece of advice it must not take", `safety.rs:302`). That reasoning is correct and should stay. It is only the human branch's tail that is the defect.

The neighbouring refusals are clean: the rate limit at `safety.rs:331` and both flash guard messages at `safety.rs:355` and `safety.rs:377` say "Wait {left} ms and take again" and name no key.

How bad: annoys. It fires mid show, which is the worst moment to be pointed at a text editor, but the primary advice ("wait 300 ms") is right there and works.

Design. Drop the tail, or replace it with a pointer to the control: "...Wait {left} ms and take again. The hold is set under Safety in Settings." Then `[safety] min_hold_ms` becomes a slider in a Settings pane. `Guard::new` takes the config by value at `crates/godwinmix/src/control.rs:181`, so making this live needs a `Guard::set_limits` behind its existing lock; that is a few lines and no restart.

Size: S.

### 10. Snapshot limits name two `[snapshot]` keys

Where: `crates/godwinmix-core/src/snapshot.rs:79`, `Refusal::message`.

Live:

```
=== snapshot.get {"id":"sheet","width":9000}
{
  "code": -32602,
  "message": "a 9000 pixel wide snapshot is above the 1280 pixel ceiling in [snapshot] max_width. Ask for width=1280 or less, or repeat the request with allow_large=true if you really want the big one."
}
```

and the rate limit, `snapshot.rs:86`: "one snapshot per client per [snapshot] min_interval_secs. Wait {retry_after_secs}s and ask again, read /api/agent/state for motion in the meantime, or repeat the request with force=true."

How bad: cosmetic. Both messages name a working escape (`allow_large`, `force`) in the same breath, so nobody is stuck. The key name is there as context, not as an instruction. A GUI never hits these anyway because it asks for the widths it wants.

Design. Leave the text alone. Move the key into `data` if you are tidying: `.with("config_key", "snapshot.max_width")`. The sentence reads fine without it.

Size: S, and low priority.

### 11. A failed upload tells the person to choose a writable `[media].dir`

Where: `crates/godwinmix/src/control/upload.rs:17`.

```rust
tokio::fs::create_dir_all(dir).await.map_err(|e| RpcError::internal(format!(
    "creating media directory {}: {e}. Choose a writable directory in [media].dir and retry.",
    dir.display()
)))?;
```

Click path: drag a clip onto the window when the configured media directory is somewhere the core cannot write, for instance a packaged install whose config still says `dir = "media"` relative to a read only working directory.

How bad: blocks a beginner when it fires, and it fires exactly on the "just drop a file on it" path the preset steps advertise.

Design. `config.set { "media.dir": "..." }` plus a folder picker. In the desktop shell that is a real native dialog through Tauri; in a browser there is no folder picker, so offer the two places the core can always write, `<runtime dir>/media` and the user's home, as two buttons. `MediaLibrary` holds `cfg` at construction (`crates/godwinmix-core/src/media.rs:106`), so this is a restart key unless the library is put behind a lock. Given how rarely it changes, restart is acceptable, and the restart offer covers it.

Size: S for the message and the two buttons, M with a native folder picker.

### 12. An unreadable media directory shows a raw errno and hides the only hint

Where: `crates/godwinmix-core/src/media.rs:154` produces the error; `ui/panels/media/panel.js:125` renders it:

```js
el("p.dim.sm", { text: this.error || `Nothing in ${this.dir || "the media directory"}. Drop a file on the window to upload one.` })
```

Live, on a core whose media directory does not exist:

```
=== media.list {}
{ "dir": "media", "error": "reading media: No such file or directory (os error 2)", "items": [] }
```

So the panel shows "reading media: No such file or directory (os error 2)" and, because `this.error` wins the `||`, the "Drop a file on the window to upload one" hint disappears. The one case where the person most needs to be told they can just drop a file is the one case where they are not told.

This is also a false alarm: `upload::store` calls `create_dir_all` (`crates/godwinmix/src/control/upload.rs:17`), so dropping a file would have created the directory and worked.

How bad: annoys, bordering on blocks. An empty library that says "os error 2" reads as broken software.

Design. Two small changes and no new method. In the core, when the failure is `NotFound`, say so plainly: `"the media folder {dir} does not exist yet. Dropping a file on the page creates it."` In the panel, change the `||` so the drop hint is always shown and the error sits above it as a dim line.

Size: S.

### 13. `plugin.add` names a `gmx` command for something that has no method at all

Where: the source parser in `crates/godwinmix-core/src/plugin/`, surfaced live:

```
=== plugin.add {"source":"nosuchplugin"}
{
  "code": -32004,
  "message": "`nosuchplugin` is not a source. Write one of:\n  owner/repo                a GitHub release\n ... \nA bare name with no slash is looked up in your marketplaces; add one with `gmx marketplace add owner/repo`.",
  "data": { "retryable": false, "source": "nosuchplugin" }
}
```

`gmx marketplace` exists as a CLI subcommand. There is no `marketplace.*` method in the 126. So this is worse than the others: the GUI is told to run a command whose protocol equivalent was never written.

How bad: annoys. Most people install from the marketplace that ships.

Design. Add three methods in the existing style:

* `marketplace.list` (Read) returns `{ marketplaces: [{ name, url, plugins, added_at }] }`.
* `marketplace.add` (Admin) takes `{ source }`, the same string the CLI takes, and answers with the added row. Writes to wherever the CLI writes it.
* `marketplace.remove` (Admin) takes `{ id }`.

Then the plugin browser gets an "Add a marketplace" row, and the error's last sentence becomes "A bare name with no slash is looked up in your marketplaces. Add one under Plugins, Marketplaces."

Size: M.

### 14. Plugin loader errors name `gmx plugin add` and `gmx plugin enable`

Where: `crates/godwinmix-core/src/plugin/loader.rs:752` and `:759`:

```
"no plugin called `{name}` is installed. Installed: {}. Add one with `gmx plugin add <path>`."
"the plugin `{name}` is installed but disabled. Turn it on with `gmx plugin enable {name}`."
```

and `loader.rs:1132`: "`gmx plugin add {spec}` installs it."

Click path: add a source whose type comes from a plugin that is missing or switched off, for instance after importing an OBS collection that used NDI.

How bad: annoys. `plugin.add` and `plugin.enable` both exist as methods, so the GUI can do this; the text just does not say so.

Design. No new method. Change the text to name the action rather than the command and put the machine readable part in `data`:

```
"no plugin called `ndi` is installed. Installed: camera, browser, rtmp."
  data: { "install": "ndi", "method": "plugin.add" }
"the plugin `ndi` is installed but switched off."
  data: { "enable": "ndi", "method": "plugin.enable" }
```

The UI already has the right widget for the first one: `installRow` in `ui/panels/welcome/panel.js:197` is a working Install button. Reuse it wherever `data.install` appears, which makes this a generic error to button mapping rather than one more special case.

Size: S.

### 15. A web page source on a machine with no sidecar tells the person to install a Debian package

Where: `crates/godwinmix-core/src/input.rs` (the `wpesrc` fallback path), surfaced live on macOS:

```
=== source.add {"id":"web1","uri":"web+https://example.com"}
{
  "code": -32001,
  "message": "cannot render web pages: the GStreamer `wpesrc` element is not installed. Install gstreamer1.0-wpe (Debian, Ubuntu) or gst-plugins-bad built with wpewebkit. It is not available on macOS."
}
```

Click path: Sources, Add, Web page, type a URL, Add. The church preset's `lyrics` source is exactly this.

Two things wrong. First, it is a package manager instruction in a GUI. Second, and worse, it is the wrong advice on this machine: the message ends "It is not available on macOS" and offers nothing else, but the intended macOS answer is the browser sidecar, `godwinmix-browser`, which `find_browser_sidecar` (`crates/godwinmix-core/src/input.rs:334`) looks for next to the binary, in an `.app` bundle, and on `PATH`. The message never mentions it. A macOS user reads "not available on macOS" and concludes web sources do not work, when in fact they need one file next to the binary.

How bad: blocks a beginner on macOS, and macOS is a first class platform by AGENTS.md rule 5. This is the finding I would fix first after the preset steps.

Design. Rewrite the message to lead with the sidecar, since that is the supported path on two of the three platforms: "cannot render web pages: no browser sidecar is installed and the GStreamer `wpesrc` element is not here either. GodwinMix ships `godwinmix-browser`; install it, or set `browser.sidecar` to where it is." Then give the GUI a way to act: the Web page kind in the source picker checks `core.info` features for a `browser` entry (add one alongside `whep` and `local-preview` at `crates/godwinmix/src/control.rs:283`) and, when it is absent, shows an "Install the browser sidecar" button that runs `plugin.add` against the sidecar's release. Where the sidecar is present but at a path the core did not guess, `config.set { "browser.sidecar": "/path" }` with a file picker.

Size: S for the message and the `core.info` feature, M with the install button.

### 16. `core.doctor` prints a shell redirect and a brew command

Live, `core.doctor` on the running core (which was started with `--config .../gmx.toml`):

```
"detail": "godwinmix.toml does not exist. Run 'godwinmix --example-config > godwinmix.toml' to start one"
"detail": "wpesrc is missing, so web page sources without a browser sidecar will not work. Install gst-plugins-bad, built with WPE (brew install gstreamer)"
```

The first is also wrong: the core is running, from a config file, at a path it knows (`preset.apply` reported it correctly in the same session). The check hardcodes `godwinmix.toml` in the working directory and ignores `--config`.

Click path: none today. I grepped `ui/` and `core.doctor` is not called anywhere; the only hits for "doctor" are comments in `ui/shell/settings.js:70` and `ui/panels/welcome/defaults.js:81`. So this is latent rather than live, but `core.doctor` has a REST binding (`GET /api/v1/core/doctor`) and is an obvious thing to surface.

How bad: cosmetic today, would be "annoys" the moment a diagnostics panel exists.

Design. Fix the config check to report the file actually in force rather than guessing. Give each doctor row an optional `fix` object (`{ method, params, label }`) so a future panel renders a button next to the verdict, and leave `detail` as the human sentence with the shell commands taken out. Then build the panel: `core.doctor` plus `core.startup_report` in one "Diagnostics" pane, which is also where `support-bundle` belongs.

Size: S for the `--config` bug, M for the panel.

### 17. Two alerts tell the operator to restart the mixer

Where: `crates/godwinmix-core/src/mixer.rs:199` (`Wedged`) and `:4890` (the command handler panic).

```
"; the command loop is held by {command} since {held_ms} ms. The programme
 is still on air, and this request was not cancelled: it runs when the loop
 comes back. ... Ask again, and restart the core if it does not clear."

"the mixer failed while handling {label} and the command did not complete.
 The programme is still on air. Check whatever you just changed; restart
 the mixer when you can."
```

The second goes out as `Event::Alert { severity: Error, .. }` (`mixer.rs:3471`), which the Alerts panel in the footer renders (`ui/panels/alerts/panel.js:54`).

Click path: nothing a person does on purpose. It appears in the footer when something has gone wrong.

How bad: cosmetic to annoys. "Restart the mixer when you can" is honest advice and the right advice; the defect is only that the GUI offers no way to do it.

Design. Do not change the text. Add the button. An alert carries an optional `action` (`{ label, method, params }`), and an alert whose action is `core.shutdown` renders as "Restart the mixer" in the footer, going through the restart flow in section 4. That is the same generic error to button mapping as finding 14, applied to alerts.

Size: S once the restart flow exists.

### 18. `preset.apply` reports a restart for keys it did not change

Where: `still_pending` at `crates/godwinmix/src/control/methods/presets.rs:333`:

```rust
if !plan.config.is_empty() {
    out.push(format!(
        "{} configuration key(s) were written to {} and take effect on restart",
        plan.config.len(),
        plan.config_path.display()
    ));
}
```

It counts `plan.config.len()`, but a `ConfigChange` can carry `Action::Keep`, which means the operator's own value won and nothing was written. Live, applying the church preset to a core that already had all three values:

```
"needs_restart": [
  "3 configuration key(s) were written to /private/tmp/.../gmx.toml and take effect on restart"
],
"plan": { "config": [
  { "action": "keep", "from": "127.0.0.1:18612", "key": "control.bind",              "to": "0.0.0.0:8080" },
  { "action": "keep", "from": "180",             "key": "program.audio_ramp_ms",     "to": "250" },
  { "action": "keep", "from": "6000",            "key": "program.video_bitrate_kbps","to": "4500" }
]}
```

All three were kept. Nothing needed a restart. The welcome dialog still printed the line, in the dim paragraph at `ui/panels/welcome/panel.js:177`.

How bad: annoys, and it erodes trust in every other restart notice. A person who restarts and sees nothing change learns to ignore the message.

Design. Filter: `plan.config.iter().filter(|c| c.action != Action::Keep).count()`, and skip the line when that is zero. Then name the keys rather than counting them, because a GUI wants to say which: "`program.video_bitrate_kbps` and 2 others take effect on restart" with the list in `data`.

Size: S.

### 19. `preset.apply` throws away the operator's config comments

Where: `crates/godwinmix-core/src/preset/apply.rs:107`, the header it writes:

```
# Merged by `gmx preset apply {}`. The file as it was before is in {}.
# Comments from your own file are in that copy: this one is rewritten from
# the values, which is the price of merging two configurations.
```

Live, this is exactly what happened. I started the core from `godwinmix.example.toml.bak`, 22471 bytes of commented configuration. After one `preset.apply`, `gmx.toml` was 2132 bytes with every comment gone. The `.bak` is kept, which is decent, but the person's working file is now undocumented.

The same flaw is in `save_plugin_settings` (`crates/godwinmix/src/control.rs:265`), which does `std::fs::write(path, toml::to_string_pretty(&document)?)`. Every `plugin.settings.set` from the GUI destroys the comments in the operator's config file.

How bad: annoys. Nothing breaks, but it is a quiet act of vandalism on a file the person wrote, and it will be the loudest complaint the first time somebody notices.

Design. Use `toml_edit` instead of `toml` for every write back path: `preset::apply`, `save_plugin_settings` and the new `config.set`. `toml_edit::DocumentMut` preserves comments, key order and whitespace and changes only the spans you touch. It is one more dependency in `godwinmix-core` and `godwinmix`, and AGENTS.md rule 3 says to argue for each one; the argument is that not having it means the reference implementation cannot write a config file without destroying it, which makes `config.set` unshippable.

Size: M. It is the prerequisite for finding 2 being a good change rather than a destructive one.

### 20. The preset's sources and outputs did not come up, and nothing said why

Observed live, applying the church preset to a core that already had `cam1`, `cam2` and `primary` in its runtime store:

```
"live": [],
"needs_restart": [ "3 configuration key(s) ..." ]
```

and afterwards `source.list` still shows only `cam1` and `cam2`, while `gmx.toml` now lists `cam-wide`, `cam-pulpit`, `lyrics` and `slides`. The runtime store `gmx.runtime.toml` takes precedence over the config file's lists (its own header says so), and `reload` at `presets.rs:247` loads `Config::load(path_in_force(...))`, so it read the runtime store's list, found the preset's sources absent from it, and the second apply then reported them as `already_there: true` because the plan diffs against the file.

I want to be honest about the limits of this observation: I called `preset.apply` for real twice in that session, so the second call's `already_there: true` is partly the first call's doing. What I am confident of is the outcome a person sees, which is that they pressed "Church service", got "Church service is set up", and the four sources the preset promised are not in the tray.

How bad: blocks a beginner, if it reproduces from a clean start. It is the difference between the welcome flow working and the welcome flow lying.

Design. Not mine to design fully; it belongs to whoever owns `preset::apply`. What this layer should do is stop the silence: `still_pending` already has a branch for "this core did not bring it up now, so it starts on the next `gmx` restart", and that branch is not firing here because the plan says `already_there`. The plan's `already_there` should mean "already running on this core", checked against `mixer.status()`, not "already in the file".

Size: M, and it wants its own investigation from a clean runtime directory first.

---

## 2. The design the brief asked for: `config.get` and `config.set`

### What already sets the precedent

Three things in the tree already write the operator's configuration over the protocol, so this is not a new kind of thing:

* `preset.apply` rewrites the whole config file (`crates/godwinmix-core/src/preset/apply.rs`).
* `plugin.settings.set` rewrites `[plugins.<name>]` in place (`crates/godwinmix/src/control.rs:245`).
* `source.add`, `source.set`, `output.add` and `output.set` persist to `<config>.runtime.toml` through `persist_runtime` (`crates/godwinmix-core/src/mixer.rs:2693`).

The third is the interesting one. There is already a sidecar file, already authoritative over the config file for the things it carries, already carrying `[ui]` as well as sources and outputs. `config.set` should write scalars into the config file proper (so they stay where a person expects to read them) and reuse the sidecar's precedence rule only where it already applies.

### `config.get`

```
config.get   Read       GET /api/v1/config
params: { "keys": ["multiview.enabled", "program.video_bitrate_kbps"] }   // optional; omit for all
result: {
  "path": "/etc/godwinmix/godwinmix.toml",
  "runtime_store": "/etc/godwinmix/godwinmix.runtime.toml",
  "keys": [
    { "key": "multiview.enabled", "value": true, "source": "file",
      "default": true, "applies": "restart", "type": "boolean",
      "title": "Show the multiview mosaic",
      "help": "The grid of every source. Off saves a core on a small machine." },
    { "key": "control.token", "value": null, "source": "default",
      "applies": "restart", "type": "secret", "set": true }
  ]
}
```

Notes on the shape, each with a reason:

* Dotted keys, not nested JSON. `preset.plan` already speaks dotted keys (`ConfigChange.key` is documented as "A dotted path, for example `program.video_bitrate_kbps`", `crates/godwinmix-core/src/preset/plan.rs:51`), so a client that understands a plan understands this.
* `source` is one of `file`, `runtime`, `default`, `cli`. A person editing a value that `--bind` is currently overriding needs to be told so, or the GUI lies to them.
* `applies` is `live`, `restart` or `next_source`, on every key. This is the field that lets a GUI build the whole Settings pane without a hardcoded table of which toggles need a restart.
* `type`, `title` and `help` come from the same place the schema does. Derive them with `schemars` off the `Config` struct, which already derives it for the method table, and take `title` and `help` from the doc comments. Then a Settings pane is a generic schema form over `config.get`, which is what `ui/kits/schema/` already builds for plugin settings. That is the difference between M and L for this work.
* A `secret` type never returns its value. `plugin.settings.set` already does exactly this with `secret_fields_of` and the sentinel for "unchanged" (`crates/godwinmix/src/control/methods/plugins.rs:977`). Follow it: `[control] token` and every `[[tokens]] token` come back as `{ "set": true, "value": null }`.
* Not everything is settable. `[[sources]]`, `[[outputs]]` and `[[filters]]` belong to `source.*` and `output.*`; `[codecs]` is a catalogue; `[plugins.<name>]` belongs to `plugin.settings.set`. `config.get` may report them read only with `"settable": false`, and `config.set` refuses them by name pointing at the method that does own them.

### `config.set`

```
config.set   Admin, destructive, mutating, idempotent
POST /api/v1/config/set
params: {
  "values": { "multiview.enabled": true, "program.video_bitrate_kbps": 4500 },
  "dry_run": false
}
result: {
  "applied":       ["multiview.enabled"],        // in force now
  "needs_restart": ["program.video_bitrate_kbps"],
  "written":       "/etc/godwinmix/godwinmix.toml",
  "restart": {
    "possible": true,
    "how": "desktop",
    "label": "Restart the mixer",
    "warning": "The programme goes off air for about four seconds."
  }
}
```

Behaviour:

* Admin scope, `destructive()`, so it takes a confirm token where the token policy demands one and it accepts `dry_run`. A dry run answers with the same `applied` and `needs_restart` split and writes nothing, which is how a GUI shows "this needs a restart" before the person commits.
* Validation first, then write, then apply. Build the candidate `Config` by merging the values into the current document and running it through `Config`'s own deserialisation and its validation (`crates/godwinmix-core/src/config.rs:146` onwards already produces good messages like "`[plugins] {key}` is a {}. A plugin's settings are a table..."). A value that fails is `-32602` naming the key and the expected type, and nothing at all is written. All or nothing, because a half written config that does not parse is a mixer that will not start.
* Write with `toml_edit`, preserving comments (finding 19). A key that is not in the file yet is appended to its section, creating the section if it is absent.
* Then apply the live ones. This needs one new mixer command, `Command::Reconfigure(Box<Config>, Option<Ack>)`, which swaps `self.cfg` and touches only what is safe to touch on the command loop. It must not rebuild the encoder or the pipeline, because AGENTS.md's first rule is that the programme output never stops.

Which keys are live and which are not, from reading where each is held:

Live, because they are read per use from the mixer's held config or sit behind a lock already:

* `security.allow_exec_sources`, read at every source add (`crates/godwinmix-core/src/mixer.rs:1773`).
* `browser.*`, read per browser source launch through `find_browser_sidecar` (`crates/godwinmix-core/src/input.rs:334`).
* `safety.*`, once `Guard` grows a `set_limits`; it already holds its state behind a lock (`crates/godwinmix-core/src/safety.rs`).
* `stall.*`, per source.
* `ui.*`, which `preset.apply` already applies live by emitting `event/ui.changed`.

These three are honestly "applies to the next source you add", not "applies now", which is why `applies` needs a third value, `next_source`, rather than being a boolean. Telling somebody a change is live when their existing sources keep the old behaviour is its own small lie.

Restart, because the value is copied into a structure at startup:

* `canvas.*`. The whole pipeline is built around it.
* `program.video_bitrate_kbps`, `program.audio_bitrate_kbps`, `program.encoder`, `program.audio_ramp_ms`. Read once at encoder build (`crates/godwinmix-core/src/mixer.rs:1129`).
* `control.bind`, `control.token`, `[[tokens]]`. The server is already listening.
* `multiview.*`. `MultiviewHandle` holds its config.
* `snapshot.*`. `Tracker` holds `cfg` (`crates/godwinmix-core/src/snapshot.rs:111`).
* `media.dir`, `media.allow_upload`. `MediaLibrary` holds `cfg` (`crates/godwinmix-core/src/media.rs:106`).
* `hardware.*`. Element selection happens at build.
* `nodes.*`. The bridge binds at startup.
* `plugins.allow_wasi` and the other `[plugins]` switches. The host reads them at load.

`[control] token` deserves a special note. Changing it invalidates the calling client's own credentials at the next start, so `config.set` on it must answer with the new token in the result exactly once and the GUI must store it before offering the restart, or the person restarts into a mixer they can no longer reach. Refuse the change outright when the caller cannot be told, for instance a REST call with no session to hold it.

Size: M for the method with a hand written key table, L to do it properly with the schema derived from `Config` and a generic Settings pane over `ui/kits/schema/`. I would do the L. The hand written table will drift from the struct within two releases, and the schema form is the thing that makes every future config key free.

---

## 3. What restarts the mixer, and how the GUI offers it

`core.shutdown` exists (`crates/godwinmix/src/control/methods.rs:203`), is Admin and destructive, and does one thing: `call.app.quit.notify_one()`. Nothing in the tree starts the process again. So the answer differs by how the mixer was started, and `config.set` has to say which case it is in. That is what the `restart` object in the result is for.

**Desktop shell.** `tauri-app/src/sidecar.rs:66` has `ensure` (adopt an existing core or start one) and `start`, and `main.rs:125` and `:168` call `stop`. The shell already owns the whole lifecycle. But `main.rs:71` exposes only two commands to the page:

```rust
.invoke_handler(tauri::generate_handler![commands::saved_connection, commands::connect_core])
```

There is no restart command. Two ways to get one, and the second is nearly free:

* Add `#[tauri::command] pub async fn restart_core(app: AppHandle) -> Result<CoreInfo, String>` that calls `sidecar::stop` then `sidecar::start` and waits for the core to answer. Clean, and it is the one to write.
* Or rely on what is already there: `connect_core` with `Mode::Local` goes through `local_target` to `sidecar::ensure`, which starts a core when none is there. So the page could call `core.shutdown`, wait for the socket to drop, then call `connect_core`. This works today but is a side effect rather than an intention, and it races with `adopt_existing` if the old process has not finished exiting.

`restart.how` is `"desktop"`, `possible` is true.

**Headless behind systemd.** `deploy/systemd/godwinmix.service:37` has `Restart=on-failure` with `RestartSec=5`. `core.shutdown` exits cleanly, so systemd will *not* bring it back. As shipped, the restart button would take the mixer off air permanently. Either change the unit to `Restart=always`, or have the shutdown path exit non zero when the shutdown was asked for as a restart. I prefer an explicit flag: `core.shutdown { "restart": true }` sets an exit code the service file treats as restartable, and the method's answer says `{"stopping": true, "will_return": true}` only when it is confident. Until the unit changes, `restart.possible` is false for this case and the GUI says "Stop the mixer, then start it again on the server" rather than offering a button that strands them.

**Docker.** `deploy/docker/docker-compose.yml` lines 35 and 71 have `restart: unless-stopped`, which does restart on a clean exit. `restart.how` is `"supervised"`, `possible` true.

**`gmx` in a terminal.** Nothing restarts it. `gmx --help` lists 21 subcommands and none is a restart; the closest is `gmx ctl`, which can call `core.shutdown` and then has nothing to call. `restart.possible` is false, and the GUI says so: "This mixer was started from a terminal, so it has to be started again there. The change is saved and takes effect next time."

How the core knows which case it is in: it cannot, reliably, so tell it. A `--supervised` flag on the binary, set by the service file, the compose file and the Tauri sidecar launch, and absent when a person runs `gmx` by hand. Default false, because promising a restart that does not come is worse than not offering one.

What the person sees. After a `config.set` that returns a non empty `needs_restart`, a bar at the top of the window: "Two settings are waiting for a restart: the multiview and the programme bitrate." with a "Restart now" button and a "Later" link. Pressing Restart shows the warning from the result ("The programme goes off air for about four seconds."), and on confirm the page calls `core.shutdown { restart: true }`, shows a reconnecting state, and the client's existing reconnect logic brings it back. Where `restart.possible` is false the bar has no button and says what to do on the machine instead, which is the one case where prose is the honest answer.

Size: S for the Tauri command, S for the systemd unit, M for the `--supervised` flag and the bar.

---

## 4. The full gap list: what a GUI cannot do today because no method exists

Read out of the 126 methods in `protocol.json`.

* Read or write any mixer setting. No `config.*`. Covers canvas, `[program]`, `[control]`, `[multiview]`, `[snapshot]`, `[media]`, `[hardware]`, `[plugins]`, `[browser]`, `[nodes]`, `[security]`, `[safety]`, `[stall]`. See section 2. L.
* Restart the mixer. `core.shutdown` stops it; nothing starts it. See section 3. M.
* Import an OBS collection from the browser. `scene.import.obs` exists but takes a server side path. See finding 3. M.
* Manage marketplaces. No `marketplace.*`. See finding 13. M.
* Create or manage tokens. `[[tokens]]` is config only, and `core.info` reports the calling token but there is no `token.list`, `token.add` or `token.revoke`. A person who wants to give a volunteer an operate only credential has no path but the config file. M, and it needs the secret handling from section 2.
* See the machine's health. `core.doctor` and `core.startup_report` exist and are never called from `ui/`. No method needed, only a panel. M.
* Write a support bundle. `gmx support-bundle` is CLI only; no `core.support_bundle` method. It is the single most useful thing a confused person can be asked for, and today asking for it means asking them to open a terminal. S to add the method (the code exists), M with the download path.
* Save a preset to somewhere the person can find it. `preset.save` exists but writes to a server side directory and answers with a path. There is no download. S.
* Choose a folder. Several settings are paths (`media.dir`, `browser.sidecar`, plugin roots) and a browser cannot pick one. Needs either the desktop shell's native dialog or a core side `fs.browse` (Admin, read only, rooted at a few safe places). M, and worth thinking about before it is written, because a path browser on the control port is an attack surface.

---

## 5. What is already done well and should be the pattern

**The install button in the welcome flow.** `ui/panels/welcome/panel.js:197`, `installRow`. The comment on it is the best statement of this audit's whole thesis, written before the audit:

```
This used to print `gmx plugin add camera` and stop there, which asks
somebody who has just picked Church service to go and find a terminal.
`plugin.add` is the same call that command makes, it works while the mixer
runs, and the listing is read again afterwards so the line says what
actually happened rather than what was hoped for.
```

Three things it gets right and every fix in this report should copy: the button calls the same method the CLI calls, it works live, and it re-reads the state afterwards so the label says "Installed, and nothing restarted" rather than assuming success. Note it even handles the case where the install worked but the core has not picked it up: "Installed, but the mixer has not picked it up yet."

**The welcome tiles apply a preset over the public protocol.** `panel.js:145` is one `client.call("preset.apply", { name })`. No private endpoint, no shelling out. This is AGENTS.md rule 2 being honoured where it would have been easiest to cheat.

**`preset.apply` reloads what it can.** `reload` at `presets.rs:243` adds the preset's sources and outputs through the same `Command::AddSource` and `Command::AddOutput` that `source.add` uses, and the doc comment says "Nothing in here can fail the call: what would not start is reported, not thrown." Three outcomes kept separate, with the reasoning written down at `presets.rs:299`: waiting on a plugin, refused when tried, and never tried and so waiting for a restart. "Only the last of those is a restart." That distinction is what a good GUI needs and it is already there.

**The safety guard branches on who is asking.** `crates/godwinmix-core/src/safety.rs:302`. An agent gets advice it can act on; a person gets advice a person can act on. The comment explains why: telling an agent to get a better token "is the one piece of advice it must not take". Every refusal in this report that names a TOML key should branch the same way.

**`node.list` answers instead of refusing.** Live it returns `{"listening": false, "nodes": []}` rather than an error. A GUI can render "not listening" with a button without provoking a failure first. Compare `node.enrol`, which refuses. The listing shape is the better one and more methods should use it.

**Errors carry a `data` object a caller can act on.** `plugin.add` puts `source` in `data`; `snapshot` refusals carry `retryable`; `RpcError::not_found` takes the list of ids that do exist. The convention is established, which is why "put the config key in `data`" is a cheap ask throughout this report rather than a new idea.

**The schema kit has a fallback chain, written down.** `ui/kits/schema/index.js` opens with it: a plugin's own web component, else the UI schema rendered natively, else the data schema with default widgets, else a raw JSON box, "so a plugin written against a newer core is still editable on an older client, and a plugin that ships nothing but a schema is still editable at all." That is the machinery a Settings pane over `config.get` needs and it already exists. It is also why I would spend the extra effort on the schema derived version in section 2: the renderer is written, only the schema is missing.

**Method dispatch is a table, not a match arm.** `crates/godwinmix-protocol/src/method.rs`. One `MethodDef` feeds `/rpc`, `/api/v1`, `protocol.json` and the MCP tool list, and `rest_transform` is tested against the written rule. Adding `config.get` and `config.set` costs one registration each and every surface follows. This is why the L estimate in section 2 is about the schema form and not about the plumbing.

**`-32601` names the nearest methods.** `Registry::nearest` at `method.rs:218`. My `config.get` probe came back with `"nearest": []` and "Call core.api for the whole list", which is exactly right: it did not guess, and it said where the list is.

---

## 6. What I could not determine

* Whether finding 20 (the preset's sources not coming up) reproduces from a clean runtime directory. I applied the church preset twice in one session, which muddies the second result. It needs one clean run: fresh directory, no `.runtime.toml`, apply once, read `source.list`.
* The exact text of the second snapshot refusal branch (`snapshot.rs:175`, the "mosaic is switched off" one). I could not provoke it because turning `[snapshot] enabled` on and `[multiview]` off would have needed another restart cycle, and the first branch wins when both are off. It is covered by a test at `snapshot.rs:880` so the string is certainly reachable; I quoted the source rather than a live capture and marked it as such.
* Whether `config.set` on `program.video_bitrate_kbps` could be made live by rebuilding only the encoder. I read that the value is used once at `mixer.rs:1129` but did not trace whether the encoder can be swapped without a visible break in the programme. Given rule 1, I assumed not and put it in the restart list. Somebody who knows the encoder should check, because live bitrate is worth having.
* How `[control] token` interacts with the desktop shell's own token (`tauri-app/src/sidecar.rs:76`, `settings::local_token`). The shell mints one and passes it as `GODWINMIX_TOKEN`. Whether `config.set` on the token would fight with that, I did not work out.
* Whether any of the seven plugin source backends in `godwinmix-host` produce their own user facing refusals that name a config file. I read the core and the control server as briefed and did not sweep `crates/godwinmix-host/src` exhaustively.

---

## Housekeeping

I ran one tmux session, `gmx-coreaudit-core`, twice: once on the stock example config and once with `[multiview] enabled = false` and `[snapshot] enabled = false` to capture those refusals. Both are killed. I used no browser and did not touch DevTools port 19612. Scratch files (the config copy, the probe script, the captured output) are under `/private/tmp/claude-501/-Users-godwin-workspace-GodwinMix/1eed66fb-ad84-4f9b-a731-6a38fb3c92c0/scratchpad/coreaudit/`. I edited no file in the worktree and ran no git command that changes anything.
