# The config methods

Four methods read and change the mixer's own settings, the scalar keys of
`godwinmix.toml`. Keys are dotted, `program.video_bitrate_kbps`, the same
spelling a preset plan uses for `ConfigChange.key`.

| Method | REST | Scope | Destructive | What it does |
|---|---|---|---|---|
| `config.get {keys?}` | `GET /api/v1/config` | admin | no | Every key, or the ones named, with its value, default and `applies` |
| `config.schema` | `GET /api/v1/config/schema` | read | no | One JSON Schema describing every key, for a settings form |
| `config.set {values, dry_run?}` | `POST /api/v1/config/set` | admin | yes | Write values into the file; apply the live ones |
| `config.reset {keys, dry_run?}` | `POST /api/v1/config/reset` | admin | yes | Take keys out of the file so their defaults apply |

`config.get` is admin because the answer carries the control address and the
paths on this machine. `config.schema` carries no values.

## `applies`

Every key has one of three answers to "when does a change take effect". They
come from reading where the running core holds each value, not from a guess.

| `applies` | Meaning | Keys |
|---|---|---|
| `live` | In force when `config.set` answers | `program.audio_ramp_ms`, `security.allow_exec_sources`, every `safety.*`, every `stall.*` |
| `next_source` | Used by every source added or rebuilt from now on | every `browser.*` |
| `restart` | Written to the file, used from the next start | everything else |

The restart keys are copied into something built once at start: the encoder
(`canvas.*`, `program.*` apart from the ramp), the mosaic (`multiview.*`), the
snapshot tracker (`snapshot.*`), the media library (`media.*`), the listening
socket, the web UI directory and the token table (`control.*`), the codec
choice (`hardware.*`), the node bridge (`nodes.*`) and the plugin host
(`plugins.allow_unsigned`, `plugins.allow_wasi`). `config.schema` carries the
same answer for each key as `x-gmx-applies`.

## `config.get`

```json
{
  "path": "/srv/show/godwinmix.toml",
  "needs_restart": [],
  "keys": [
    { "key": "canvas.width", "value": 1920, "default": 1920, "source": "file",
      "applies": "restart", "secret": false, "pending": false },
    { "key": "control.token", "value": null, "default": null, "source": "file",
      "applies": "restart", "secret": true, "set": true, "pending": false }
  ]
}
```

| Field | What it is |
|---|---|
| `value` | What the file says, or the default when the file does not set it. Always null for a secret |
| `source` | `file` when the key is written in the file, `default` when not |
| `set` | Secrets only: whether one is set. The value is never sent |
| `overridden_by` | Present when something outside the file wins: `--bind` for `control.bind`, `GODWINMIX_TOKEN` for `control.token` |
| `pending` | The file differs from what the running core uses and only a restart closes the gap |
| `needs_restart` | Every pending key, whichever keys were asked for |

## `config.set`

```json
{ "values": { "safety.min_hold_ms": 2000, "program.video_bitrate_kbps": 3000 } }
```

```json
{
  "dry_run": false,
  "path": "/srv/show/godwinmix.toml",
  "changed": [
    { "key": "program.video_bitrate_kbps", "applies": "restart" },
    { "key": "safety.min_hold_ms", "applies": "live" }
  ],
  "unchanged": [],
  "applied": ["safety.min_hold_ms"],
  "next_source": [],
  "needs_restart": ["program.video_bitrate_kbps"]
}
```

* Every value is checked against its key first: the type the config struct
  has, the range, the choices. Then the whole file is loaded as it would be
  after the change, which catches what one value cannot show alone (an odd
  canvas width, a snapshot limit below its default). Nothing is written
  unless all of it passes.
* The file is edited in place. Comments, blank lines and key order stay. A key
  the file lacks is added at the end of its section, and a missing section is
  added at the end of the file.
* `null` for a value puts the key back to its default, as `config.reset` does.
* A secret (`control.token`) sent as `"__secret__"` is left alone and listed in
  `unchanged`; that is what a form sends back when nobody retyped it. An empty
  string removes it.
* `needs_restart` lists every pending key, from this call or an earlier one.
  With `dry_run` it lists this call's restart keys instead.
* `changed[].note` says when something outside the file wins for that key.

### Refusals

Each one carries `data.key` and whatever would have worked.

| Case | Code | `data` |
|---|---|---|
| Out of range | -32602 | `minimum`, `maximum` |
| Not one of the choices | -32602 | `choices` |
| Wrong type | -32602 | `expected` (`boolean`, `integer`, `string`, `array`, `object`) and the range when there is one |
| The file would not load with it | -32602 | `reason`, `keys` |
| A key another method owns (`sources.*`, `outputs.*`, `filters.*`, `plugins.<name>`, `ui.*`) | -32602 | `owner`: the method to use |
| No such key | -32004 | `valid`: every key |
| The core has no config file | -32001 | `path` |

## `config.schema`

A flat JSON Schema: one property per dotted key, shaped for `ui/kits/schema`.
Type and description are derived from the config structs, the default is what
an empty file loads as, and the title, range, unit and `applies` come from the
key table in `crates/godwinmix-core/src/config/keys.rs`. A test holds that
table and the structs to the same set of keys.

```json
"program.video_bitrate_kbps": {
  "type": "integer", "format": "uint32", "title": "Video bitrate",
  "description": "Video bitrate in kbit/s for the outgoing program stream.",
  "default": 6000, "minimum": 100, "maximum": 100000,
  "x-gmx-unit": "kbit/s", "x-gmx-group": "program", "x-gmx-applies": "restart"
}
```

A secret has `"format": "secret"` and no default.

## Not covered

`[[sources]]`, `[[outputs]]`, `[[filters]]`, `[plugins.<name>]`, `[[tokens]]`,
`[[hooks]]`, `[codecs]`, `[marketplaces]` and `[ui]` are not settable here.
The first four have their own methods; the rest are still edited in the file.
