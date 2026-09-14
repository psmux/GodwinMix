---
name: godwinmix-develop
description: Write a GodwinMix plugin. Use when the task is to build a source, output, filter, service, device, panel or preset for GodwinMix, to extend an existing plugin, to fix a plugin that fails the conformance harness, or to publish one to the index. Covers gmx-plugin.toml, the five language templates, the stdio protocol written from a standard library with no SDK, the media contract, the conformance harness and what each check means, and the listing path. Do not use it to run a show; that is godwinmix-operate.
---

# Writing a GodwinMix plugin

A plugin is a process. It talks JSON-RPC 2.0 over stdin and stderr, one object
per line, and it puts media on stdout or on a socket. That is the whole
contract. `examples/zero-dep-source.py` in the repository is a complete source
plugin in under 200 lines of Python importing nothing outside the standard
library, and it is there to prove the protocol never needs an SDK.

## Start from a template

```sh
gmx plugin new --kind source --lang python my-cam   # not built yet: copy templates/python
cd my-cam && ./check
```

Five templates: `rust` (on `godwinmix-sdk`), `python`, `node`, `go`, `shell`.
Each is a working plugin drawing colour bars, with a manifest, a settings
schema, a `SKILL.md`, an `AGENTS.md`, a recorded transcript replayed offline, a
test that fails until you write it, and a `check` script that finishes in under
30 seconds. `templates/README.md` lists the placeholders.

Python and Node are container mode by default because that is what works on
every platform. Rust and Go can do `unixfd`, which is zero copy and is Linux and
macOS only.

Read the template's own `AGENTS.md` before changing it. The one function you
change is named there.

## The manifest

`gmx-plugin.toml` at the plugin root, readable by a person, a package index and
a model without running anything.

```toml
[plugin]
name = "my-cam"          # the namespace: every id becomes "my-cam/<provide id>"
version = "0.1.0"        # semver, required
api = 1                  # the protocol level this plugin was written against
description = "..."      # one sentence; it is what everything reads first
license = "MIT"
platforms = ["linux-x86_64", "macos-aarch64", "windows-x86_64"]
placements = ["sidecar", "node"]
process = "per-instance"

[run]                    # exactly one of bin, python, node, shell
python = "main.py"

[[provides]]
kind = "source"          # source, output, filter, transition, encoder, service,
id = "source"            #   device, panel, surface, preset, graphic, collection
media = { video = "raw", audio = "none", alpha = false, thumb = true }
transports = ["container"]
capabilities = ["restart-in-place", "health"]
settings = "schemas/source.json"   # JSON Schema draft 2020-12. It is the only UI you get
skill = "skills/source/SKILL.md"
```

Full key list in `docs/reference/plugin-manifest.md`. The validator reports every
problem at once with the key path that caused it, so fix the whole file in one
pass: `cargo run --bin check-manifest` in the Rust template, or
`gmx plugin test .`.

Two things authors get wrong. `settings` is the settings UI everywhere, so give
every field a `description`, a `default` and at least one example; `"format":
"secret"` stores a value encrypted and never reads it back, and `"x-gmx-unit"`
labels `db`, `ms` or `kbit`. And a capability you declare is a promise the
supervisor keeps: `restart-in-place` changes how a stall is recovered, `seek`
makes `seek` and `position` legal and gives the source a scrubber, `idle` lets
the supervisor pause the source when nothing is looking at it.

## The protocol, from a standard library

The plugin speaks first.

```
plugin -> core   {"jsonrpc":"2.0","id":0,"method":"initialize","params":{
                   "plugin":"my-cam","version":"0.1.0","api":1,
                   "transports":["container"],"provides":[...]}}
core -> plugin   {"jsonrpc":"2.0","id":0,"result":{
                   "canvas":{"width":1920,"height":1080,"fps":30},
                   "transport":"container","media":"","instance":"cam1",
                   "provide":"source","params":{...}}}
plugin -> core   {"jsonrpc":"2.0","method":"initialized"}
```

Then the core calls `configure`, `start`, `health`, `stop`, `shutdown` and,
where declared, `seek`, `position`, `keyframe`, `audio.set`, `render`,
`discover` and `tool.call`. Rules that bite:

* One JSON object per line, UTF-8, at most 4 MiB. A longer line is `-32011` and
  the channel closes.
* **stdout is media.** A `print` corrupts the stream. Log with a `log`
  notification on stderr; a plain non JSON line on stderr also reaches the log,
  so a Python traceback lands somewhere useful.
* Several requests may be in flight, and ids are per direction. `health` must be
  answered while a slow `start` or `configure` is still running, so never answer
  it from the thread doing the work.
* A method you do not implement is `-32601`, never silence.
* Every error names the current state and the next step, with a `data` object a
  caller can act on. That does more for an unattended caller than any amount of
  description text.

`docs/reference/plugin-protocol.md` has every method with its params and results.

## The media contract

```
video   I420, BT.709, canvas width x height, canvas fps, one frame per buffer
        AYUV when the provide declares alpha = true
audio   F32LE interleaved, 48 kHz, 2 channels, 10 ms per buffer
time    PTS in nanoseconds on your own monotonic clock, starting near 0
```

Three transports. `container` is a streamable Matroska stream on stdout and
works everywhere; it costs one copy per frame and the pipe carries about
41 MB/s at 720p30 and 93 MB/s at 1080p30. `unixfd` is zero copy, Linux and macOS
only. `shm` is one copy, Linux and macOS. Declare the ones you can do and the
core picks.

Pace from the frame index, not by adding a sleep each time, or one slow frame
pushes every later frame back. An I420 frame is a Y plane of `width * height`
bytes, then U and V planes of `ceil(width/2) * ceil(height/2)` each.

## The harness

`gmx plugin test .` runs eight checks in about 60 seconds, and the same checks
run in the index's CI:

1. it starts and sends `initialize` within 5 seconds at a supported `api`
2. `start` produces the canvas caps at the media end within 10 seconds
3. at least 90 percent of the expected frames in 3 seconds, PTS monotonic
4. `configure` with every example in the settings schema applies or asks for a
   restart, and never crashes
5. `stop` then `shutdown` exits within 8 seconds, leaving no child processes, no
   open descriptors and no temp directories
6. killed mid stream, the core shows a freeze frame and restarts it
7. the manifest, every tool schema and the `SKILL.md` frontmatter validate
8. the footprint: the plugin's own CPU and memory, and what the core paid for
   the transport you chose

`gmx plugin test --offline` replays a recorded transcript against the binary with
no core, no sockets and no clock, which is what a plugin's CI runs on any runner
in seconds. The transcript format is in every template at `tests/transcript.jsonl`.

## Contribute to agents

Ship a `SKILL.md` per provided kind, in the Agent Skills format: YAML
frontmatter with `name` and a `description` under 1,024 characters that says
what it does **and when to use it**, then a body under 5,000 tokens. The
description is loaded into every context always, so it pays rent; the body only
loads when the skill is used.

`[[tools]]` in the manifest become MCP tools named `gmx_<plugin>_<tool>`. Every
description carries one example call, because selection accuracy depends on it.
Declare `readOnlyHint`, `destructiveHint` and `idempotentHint` honestly; the
server enforces them independently.

## Publish

Tag a version, add the `godwinmix-plugin` topic, open a PR against the index's
`index.json`. A bot runs the harness. Passing gets a bronze badge; failing means
the plugin can only be listed as custom and unreviewed. `api_level` 1 is frozen
for breaking changes, so a plugin at api 1 keeps working.
