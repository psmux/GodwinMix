# The plugin manifest

`gmx-plugin.toml` sits at the root of every plugin. It says what the plugin
provides, how to start it and what it costs, and it is readable by a person, a
package index and a model without running anything.

The validator in
[`crates/godwinmix-sdk/src/manifest.rs`](../../crates/godwinmix-sdk/src/manifest.rs)
is what accepts or refuses a manifest, and this page matches it. It reports
every problem it finds in one pass, each with the TOML key path that caused it.
The last section lists all of them.

## A complete example

```toml
[plugin]
name = "ndi"                       # the namespace: every id becomes "ndi/<id>"
version = "1.2.0"                  # semver
api = 1                            # the protocol level this plugin was written against
description = "NDI sources and outputs, with mDNS discovery of every NDI sender on the LAN"
license = "MIT"                    # an SPDX id
authors = ["Jane Roe <jane@example.com>"]
repository = "https://github.com/example/gmx-ndi"
platforms = ["linux-x86_64", "linux-aarch64", "macos-aarch64", "windows-x86_64"]
placements = ["sidecar", "node"]   # where this plugin may run
process = "per-instance"           # or "singleton"

[run]                              # exactly one runtime key wins
bin = { "linux-x86_64" = "bin/gmx-ndi", "linux-aarch64" = "bin/gmx-ndi", "macos-aarch64" = "bin/gmx-ndi", "windows-x86_64" = "bin/gmx-ndi.exe" }

[[provides]]
kind = "source"
id = "source"                      # the full id is ndi/source
uri_schemes = ["ndi://"]           # lets a bare URL pick this plugin
rank = 200                         # 0 to 256; higher wins when two plugins claim a scheme
media = { video = "raw", audio = "raw", alpha = false, thumb = true }
transports = ["unixfd", "container"]
capabilities = ["restart-in-place", "latency-report", "health", "keyframe-request"]
latency_ms = 80
settings = "schemas/source.json"   # JSON Schema draft 2020-12; this is the settings UI
skill = "skills/source/SKILL.md"   # what an agent reads before using it

[provides.designer]                # source, filter and graphic only
icon = "ui/icons/ndi.svg"

[[provides]]
kind = "device"
id = "discover"
discovery = { mdns = ["_ndi._tcp"] }

[[provides]]
kind = "panel"
id = "senders"
panel = { kind = "custom-element", entry = "ui/senders.js", slots = ["sidebar"] }

[[tools]]                          # MCP tools this plugin contributes
name = "list_senders"
description = "List every NDI sender visible on the network with its resolution and frame rate. Use before adding an NDI source when the operator names a camera rather than an address. Example: list_senders {} returns [{name:'CAM 1 (Studio)', address:'10.0.0.21:5961', width:1920, height:1080, fps:30}]."
input = "schemas/tools/list_senders.json"
output = "schemas/tools/list_senders.out.json"
annotations = { readOnlyHint = true, destructiveHint = false, idempotentHint = true, openWorldHint = true }
user_invocable = true
model_invocable = true

[hooks]
"take.before" = { mode = "rpc", timeout_ms = 20 }
"source.added" = { mode = "rpc" }
```

Every path in a manifest is relative to the plugin root. An absolute path, or
one containing `..`, is refused.

## `[plugin]`

| Key | Type | Required | Meaning |
|---|---|---|---|
| `name` | string | yes | The namespace. A slug: lower case letters, digits and hyphens, starting with a letter, no trailing hyphen, no double hyphen, at most 64 characters. Every id, tool, event and panel from this plugin is prefixed with it |
| `version` | string | yes | Semver, `MAJOR.MINOR.PATCH`, with an optional prerelease or build suffix |
| `api` | integer | yes | The protocol level this plugin was written against. 1 or more. The plugin loads on a core whose `api_compatible <= api <= api_level` |
| `description` | string | yes | One sentence, at most 1,024 characters. It is what a person and a model both read first in the index |
| `license` | string | yes | An SPDX id such as `MIT` or `Apache-2.0` |
| `authors` | array of strings | no | Free form |
| `repository` | string | no | Where the source lives |
| `platforms` | array of strings | yes | At least one platform triple from the table below |
| `placements` | array of strings | yes | At least one of `in-process`, `sidecar`, `node`, `wasm` |
| `process` | string | no | `per-instance` (the default) or `singleton` |
| `wasi` | array of strings | no | `filesystem`, `network`, or both. Read only at the `wasm` placement, and only half a grant: see below |

`per-instance` means one process per instance, which is the simpler main loop
and what the templates use. `singleton` means one process serves every instance,
and every call arrives tagged with its `instance`. A `device` or `service`
provide is a singleton instance named after the provide either way.

## `[run]`

How the core starts the process. Exactly one of the four runtime keys may be
set; two is an error, and so is none.

| Key | Type | What the core runs | What `gmx plugin add` does |
|---|---|---|---|
| `bin` | table of platform triple to path | the binary, argv exactly as declared, no extra arguments | verifies the signature, marks it executable |
| `python` | path | `.venv/bin/python <entry>` | finds `python3 >= 3.10` on PATH or `GMX_PYTHON`, creates `.venv` under the plugin directory with `uv venv` if `uv` exists and `python -m venv` otherwise, installs `pyproject.toml` or `requirements.txt` |
| `node` | path | `node <entry>` | finds `node >= 20`, runs `npm ci --omit=dev` |
| `shell` | path | `sh <entry>` on Unix | marks it executable |

And one key that is not a runtime, because it starts no process:

| Key | Type | What the core does | What `gmx plugin add` does |
|---|---|---|---|
| `wasm` | path, ending `.wasm` | loads the component into the WebAssembly host in this process | copies it with the rest of the directory |

`wasm` does not count towards the one runtime rule. A plugin may ship a binary
for `sidecar` and a component for `wasm` in the same manifest, and the operator
chooses between them with `place`.

The right hand column is what `gmx plugin add` does when it installs the plugin.
Signature checking is the one part of it that is not built: a plugin installed
from a local path is trusted because you gave the path, and installing from an
index with a signature arrives with the index. See
[install a plugin](../how-to/install-a-plugin.md) and
[the lifecycle](plugin-lifecycle.md).

`[run]` is required when `placements` names `sidecar` or `node`. A plugin that
only ever runs `in-process` needs none. `placements` naming `wasm` makes
`[run] wasm` required, and a plugin whose only placement is `wasm` needs no
runtime key at all.

Two rules on `bin`: each key must be a known platform triple, and each must also
appear in `plugin.platforms`, because nothing will ever pick a binary for a
platform the plugin does not claim.

The Windows rule on `shell`: a `shell` entry is refused when `plugin.platforms`
contains a Windows triple and `[run]` has no `bin` entry. Either drop Windows
from `platforms` or ship the binary. There is no `sh` to run.

## `[build]`

Only used when the plugin came from git. Both keys are required if the table is
present.

| Key | Type | Meaning |
|---|---|---|
| `command` | string | What to run, for example `cargo build --release` |
| `output` | string | The path that must exist afterwards |

The result is then treated as `bin`.

## `[[provides]]`

One block per thing the plugin registers. A plugin with no `[[provides]]` block
registers nothing and is refused.

| Key | Type | Meaning |
|---|---|---|
| `kind` | string | One of the twelve kinds below |
| `id` | string | A slug, unique within this plugin. The full id is `<plugin name>/<id>` |
| `uri_schemes` | array of strings | Schemes this provide claims, each ending `://`, so a bare URL can pick it |
| `rank` | integer | 0 to 256, GStreamer style. Higher wins when two plugins claim a scheme |
| `media` | table | What this provide carries. See below |
| `transports` | array of strings | `unixfd`, `shm`, `container`, in preference order |
| `capabilities` | array of strings | From the vocabulary below |
| `latency_ms` | integer | What this provide adds, in milliseconds |
| `settings` | path | A JSON Schema file, draft 2020-12, ending `.json` |
| `skill` | path | A `SKILL.md` in the Agent Skills format |
| `sides` | array of strings | `filter` only: `source`, `programme`, or both |
| `discovery` | table | `device` only, for example `{ mdns = ["_ndi._tcp"] }` |
| `panel` | table | `panel` only: `{ kind, entry, slots }` |
| `surface` | table | `surface` only: `{ run, api }` |
| `preset` | table | `preset` only: `{ plugins, config, layout, surface }` |
| `graphic` | path | `graphic` only: an OGraf manifest |
| `collection` | path | `collection` only: a `collection.json` |
| `codecs` | path | `encoder` only: entries merged into the codec catalogue |
| `designer` | table | `[provides.designer]`, see below |

### Per kind

| Kind | Required | Optional |
|---|---|---|
| `source` | `media`, `transports`, `settings` | `uri_schemes`, `rank`, `capabilities`, `latency_ms`, `skill`, `designer` |
| `output` | `media`, `settings` | `uri_schemes`, `rank`, `capabilities`, `skill` |
| `filter` | `media`, `settings`, `latency_ms` | `sides`, `skill`, `designer` |
| `transition` | `settings` | `skill` |
| `encoder` | `codecs` | |
| `service` | `settings` | `skill`, `[[tools]]` |
| `device` | `discovery` | `settings` |
| `panel` | `panel` | `settings` |
| `surface` | `surface` | |
| `preset` | `preset` | |
| `graphic` | `graphic` | `designer` |
| `collection` | `collection` | |

`latency_ms` on a filter may be 0, but it must be there: the aligner absorbs
what you declare, so a missing number is worse than a zero.

A `surface` is the one kind with no `[run]` table. It is a whole UI, not
something the supervisor places in the pipeline, so it names its own command
and the protocol level it speaks:

```toml
[[provides]]
kind = "surface"
id = "tui"
surface = { run = "gmx-tui", api = 1 }
```

`run` is looked for inside the plugin directory, then beside the `gmx` binary,
then on `PATH`; `gmx ui <name>` is what starts it. The whole contract is in
[surfaces.md](surfaces.md).

`skill` is accepted on any provide, whatever the kind. The table above names
what each kind is checked for.

## `media`

```toml
media = { video = "raw", audio = "none", alpha = false, thumb = true }
```

| Key | Type | Default | Meaning |
|---|---|---|---|
| `video` | string | `none` | `raw`, `container` or `none` |
| `audio` | string | `none` | `raw`, `container` or `none` |
| `alpha` | boolean | `false` | The provide sends AYUV rather than I420 and is composited over the programme layer |
| `thumb` | boolean | `false` | The provide can feed a thumbnail branch, built only while the multiview is running |

Both streams `none` is refused: the provide would carry nothing. `alpha = true`
without `alpha` in `capabilities` is refused, because the compositor reads the
capability, not the flag.

`raw` means frames at canvas caps. `container` means a stream `decodebin` can
open, which the core demuxes and decodes.

## `transports`

| Value | What it is | Where |
|---|---|---|
| `unixfd` | `unixfdsink` and `unixfdsrc`, memfd or DMABUF backed, zero copy, one sink can feed several readers | Linux, macOS |
| `shm` | `shmsink` and `shmsrc`, one copy per frame | Linux, macOS |
| `container` | a container on stdout, read by the core | everywhere, including Windows |

Listed in your order of preference; the core picks the first it can serve and
names it in the handshake. `container` works everywhere and is the safe first
choice. A source must declare at least one.

## Capabilities

| String | What the supervisor does when it is declared |
|---|---|
| `restart-in-place` | on a stall, NULL the pipeline and restart the same process, rather than rebuilding from nothing |
| `latency-report` | trust the plugin's `latency_ms` when answering the latency query, rather than assuming 0 and letting the aligner absorb it |
| `health` | call `health`, and combine the plugin's own view with buffer observation, so `degraded` raises an alert before a stall |
| `keyframe-request` | forward `keyframe` calls, rather than answering a remote output's request from the core's own encoder GOP |
| `idle` | set the source to PAUSED when it is in no scene, not on preview and not subscribed to, after `[sources] idle_after_secs`, and re preroll on demand behind a freeze frame |
| `seek` | `seek` and `position` are legal, and the source appears with a scrubber |
| `audio-layers` | `audio.set` accepts `layers`, page and media levels |
| `alpha` | the source emits AYUV and is composited over the programme layer, not under it |

`seek` is only meaningful on a `source` and is refused anywhere else.

## `settings`

A path to a JSON Schema file, draft 2020-12, ending `.json`. It is the only
settings UI a plugin gets to define, and every surface renders the same schema:
web, TUI and agent. Conditional visibility uses `if` and `then`.

Give every property a `description`, a `default` and at least one entry in
`examples`. The conformance harness feeds each example to `configure`.

Two GodwinMix extensions:

| Extension | Meaning |
|---|---|
| `"format": "secret"` | stored encrypted and never returned by `plugin.settings.get` |
| `"x-gmx-unit"` | `"db"`, `"ms"` or `"kbit"`; the surface renders the unit |

```json
"gain_db": {
  "type": "number",
  "title": "Gain",
  "description": "Audio gain. 0 is unity.",
  "default": 0,
  "minimum": -60,
  "maximum": 12,
  "x-gmx-unit": "db",
  "examples": [0, -6, 3.5]
}
```

## `skill`

A path to a `SKILL.md` in the Agent Skills format, ending `.md`. The validator
parses it and reports its problems under the provide's `skill` key.

| Rule | Why |
|---|---|
| YAML frontmatter between two `---` lines | it is how the format is read at all |
| `name` present, and a slug | it names the skill |
| `description` present, at most 1,024 characters | it is loaded into every agent context always, so it pays rent |
| the description says when to use the skill | the phrase `use when`, `use it when` or `use this when` must appear; that sentence is what decides whether a model loads it |
| a non empty body, under about 5,000 tokens | the body is loaded only when the skill is used. Tokens are estimated at four characters each |

## `[provides.designer]`

Only on a `source`, `filter` or `graphic` provide. Every designer client renders
it.

| Key | Type | Meaning |
|---|---|---|
| `icon` | path | The icon for the add input gallery |
| `ui` | path | A UI schema for the editing panel |
| `default_frame` | table | Where a new instance lands on the canvas |
| `thumbnail` | path | A still for the gallery tile |
| `gizmos` | array of strings | Which on canvas handles this provide supports |
| `snap` | table | Snap geometry |
| `editor` | path | A custom editor, when the UI schema is not enough |

`icon`, `ui`, `thumbnail` and `editor` are paths and must exist.

## `[[tools]]`

An MCP tool this plugin contributes. The core exposes it as
`gmx_<plugin>_<tool>`.

| Key | Type | Required | Meaning |
|---|---|---|---|
| `name` | string | yes | Lower case letters, digits and underscores, starting with a letter, at most 64 characters. Unique within the plugin |
| `description` | string | yes | What it does, when to use it, and one example call. The word `example` must appear |
| `input` | path | yes | A JSON Schema for the arguments, even if the object is empty |
| `output` | path | no | A JSON Schema for the result |
| `annotations` | table | no | The MCP annotations, below |
| `user_invocable` | boolean | no | Default true. False means the tool never appears in a menu |
| `model_invocable` | boolean | no | Default true. False means a model may not call it |

Both invocable flags false is refused: nothing could call the tool.

| Annotation | Meaning |
|---|---|
| `readOnlyHint` | the tool changes nothing |
| `destructiveHint` | the tool destroys something; subject to the confirmation policy |
| `idempotentHint` | calling it twice is the same as calling it once |
| `openWorldHint` | it touches something outside the mixer, such as the network |

The names are MCP's, verbatim, including the camel case. `readOnlyHint` and
`destructiveHint` cannot both be true.

## `[hooks]`

```toml
[hooks]
"take.before" = { mode = "rpc", timeout_ms = 20 }
"alert.raised" = { mode = "http", url = "https://example.com/paging" }
```

| Key | Type | Meaning |
|---|---|---|
| `mode` | string | `rpc` (the plugin gets the JSON-RPC call), `command` (a shell command reads JSON on stdin, exit 2 blocks) or `http` (a POST) |
| `timeout_ms` | integer | `take.before` only, at most 100 |
| `command` | string | Required when `mode = "command"` |
| `url` | string | Required when `mode = "http"` |

| Hook | Fired | Can delay the decision |
|---|---|---|
| `take.before` | before `program.take` is applied | yes, within `timeout_ms`, default 20 |
| `take.after` | after the take landed | no |
| `source.added` | on registry change | no |
| `source.removed` | on registry change | no |
| `output.state` | on an output state change | no |
| `alert.raised` | on any alert | no |
| `session.start` | daemon lifecycle | no |
| `session.end` | daemon lifecycle | no |
| `plugin.loaded` | plugin lifecycle | no |
| `plugin.failed` | plugin lifecycle | no |

Only `take.before` may carry `timeout_ms`, and 100 ms is the ceiling: at 30 fps
that is already three frames of delay on a take decision. A hook that times out
is skipped, the take proceeds, and `event/hook.blocked` says so.

## Platform triples

```
linux-x86_64     linux-aarch64     linux-armv7
macos-aarch64    macos-x86_64
windows-x86_64   windows-aarch64
```

Anything else is refused, in `plugin.platforms` and in the keys of `run.bin`.

## Placements

| Value | Meaning |
|---|---|
| `in-process` | compiled into the core, tier 0 or 1 |
| `sidecar` | a separate process on the same machine, tier 2 |
| `node` | a process on another machine |
| `wasm` | a WebAssembly component inside the core, tier W. `service`, `transition` and `panel` logic only |

Naming `sidecar` or `node` makes `[run]` required. Naming `wasm` makes
`[run] wasm` required.

A plugin that declares `wasm` alongside another placement stays a process until
an operator writes `place = "wasm"` under `[plugins.<name>]`. A plugin whose
only placement is `wasm` is loaded as a component without being asked.

Media never crosses a component boundary. A `source`, `output`, `filter` or
`encoder` asked to run at the `wasm` placement is refused with `-32005`, and
`data.placements` names the three that do carry media. See
[why WASM is not on the frame path](../explanation/why-wasm-is-not-on-the-frame-path.md).

### `wasi`, and why it takes two

`wasi = ["filesystem"]` in the manifest is the plugin saying what it needs.
`[plugins] allow_wasi = ["<name>"]` in the operator's config is the machine
saying yes. Without both, the component gets a store with no preopens and no
sockets, and `capabilities.filesystem` in its handshake reads false.

That is deliberate. One half alone would make "it asked for the filesystem" a
formality rather than information, and the operator is the one who knows
whether this machine should give it.

## What the validator checks

Every problem is reported with the key path that caused it, and every problem is
reported in one pass. Ordered as the validator walks the file.

### `[plugin]`

| Key path | Refused when |
|---|---|
| `plugin.name` | it is not a slug |
| `plugin.version` | it is not semver |
| `plugin.api` | it is 0 |
| `plugin.description` | it is empty, or over 1,024 characters |
| `plugin.license` | it is empty |
| `plugin.platforms` | the list is empty |
| `plugin.platforms[i]` | that entry is not a known triple |
| `plugin.placements` | the list is empty |
| `plugin.placements[i]` | that entry is not `in-process`, `sidecar`, `node` or `wasm` |
| `plugin.process` | it is neither `per-instance` nor `singleton` |
| `plugin.wasi[i]` | that entry is not `filesystem` or `network` |
| `plugin.wasi` | it is not empty and `placements` does not name `wasm` |
| `provides[i].kind` | `wasm` is the only placement and that provide carries media |

### `[run]`

| Key path | Refused when |
|---|---|
| `run` | the table is missing and `placements` names `sidecar` or `node` |
| `run` | the table is missing and `placements` names `wasm` |
| `run` | the table is there and names neither a runtime nor `wasm` |
| `run` | two or more of `bin`, `python`, `node`, `shell` are set |
| `run.bin.<triple>` | the key is not a known platform triple |
| `run.bin.<triple>` | the triple is not in `plugin.platforms` |
| `run.bin.<triple>` | the path is empty, absolute, or contains `..` |
| `run.python`, `run.node`, `run.shell` | the path is empty, absolute, contains `..`, or does not exist |
| `run.shell` | `plugin.platforms` names Windows and `[run]` has no `bin` entry |
| `run.wasm` | `placements` names `wasm` and it is not set |
| `run.wasm` | it is set and `placements` does not name `wasm` |
| `run.wasm` | the path does not end in `.wasm` |
| `run.wasm` | the path is empty, absolute, contains `..`, or does not exist |

### `[[provides]]`

| Key path | Refused when |
|---|---|
| `provides` | there are no `[[provides]]` blocks |
| `provides[i].kind` | it is not one of the twelve kinds |
| `provides[i].id` | it is not a slug, or an earlier provide already used it |
| `provides[i].rank` | it is over 256 |
| `provides[i].uri_schemes[j]` | it does not end `://` |
| `provides[i].capabilities[j]` | it is not in the capability vocabulary |
| `provides[i].capabilities` | `seek` is declared on a kind other than `source` |
| `provides[i].media.video`, `.audio` | the value is not `raw`, `container` or `none` |
| `provides[i].media` | both video and audio are `none` |
| `provides[i].media.alpha` | it is true and `alpha` is not in `capabilities` |
| `provides[i].media` | the kind is `source`, `output` or `filter` and there is no `media` table |
| `provides[i].settings` | it does not end `.json`, or the file is missing, absolute or escapes the root |
| `provides[i].settings` | the kind is `source`, `output`, `filter`, `transition` or `service` and there is no schema |
| `provides[i].transports` | the kind is `source` and the list is empty |
| `provides[i].latency_ms` | the kind is `filter` and it is missing |
| `provides[i].sides[j]` | it is neither `source` nor `programme` |
| `provides[i].skill` | it does not end `.md`, the file is missing, or its frontmatter fails the `SKILL.md` rules |
| `provides[i].codecs` | the kind is `encoder` and it is missing, or the file is missing |
| `provides[i].discovery` | the kind is `device` and it is missing |
| `provides[i].panel` | the kind is `panel` and it is missing |
| `provides[i].panel.kind` | it is neither `custom-element` nor `iframe` |
| `provides[i].panel.entry` | the file is missing, absolute or escapes the root |
| `provides[i].panel.slots` | the list is empty |
| `provides[i].surface` | the kind is `surface` and it is missing |
| `provides[i].surface.run` | it is missing, or not a non-empty string |
| `provides[i].surface.api` | it is missing, or not an integer of 1 or more |
| `provides[i].preset` | the kind is `preset` and it is missing |
| `provides[i].graphic` | the kind is `graphic` and it is missing, or the file is missing |
| `provides[i].collection` | the kind is `collection` and it is missing, or the file is missing |
| `provides[i].designer` | the kind is not `source`, `filter` or `graphic` |
| `provides[i].designer.icon`, `.ui`, `.thumbnail`, `.editor` | the file is missing, absolute or escapes the root |
| `provides[i].kind` | `wasm` is the only placement and the kind carries media |

### `[[tools]]`

| Key path | Refused when |
|---|---|
| `tools[i].name` | it is not lower case letters, digits and underscores, or it is declared twice |
| `tools[i].description` | it is empty, or it contains no example |
| `tools[i].input` | it is missing, or the file is missing, absolute or escapes the root |
| `tools[i].output` | the file is missing, absolute or escapes the root |
| `tools[i].annotations` | `readOnlyHint` and `destructiveHint` are both true |
| `tools[i]` | neither `user_invocable` nor `model_invocable` is true |

### `[hooks]`

| Key path | Refused when |
|---|---|
| `hooks."<name>"` | the name is not a known hook |
| `hooks."<name>".mode` | it is not `rpc`, `command` or `http` |
| `hooks."<name>".command` | `mode = "command"` and there is no command |
| `hooks."<name>".url` | `mode = "http"` and there is no url |
| `hooks."<name>".timeout_ms` | the hook is `take.before` and the value is over 100 |
| `hooks."<name>".timeout_ms` | the hook is anything other than `take.before` |

### Paths

Every path key is checked the same way: not empty, not absolute, no `..`
anywhere in it, and it must exist under the plugin root. `Manifest::load` checks
existence; `Manifest::validate(None)` checks only what the text says, which is
what a linter with no plugin directory can do.

## See also

* [The plugin protocol](plugin-protocol.md), for what happens after the manifest
  is read.
* [Write a source plugin in Rust](../how-to/write-a-source-plugin.md).
* [Your first plugin](../tutorials/your-first-plugin.md).
* [Plugins as WebAssembly components](wasm.md), for `[run] wasm`, `wasi` and
  what the `wasm` placement grants.
