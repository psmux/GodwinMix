# Plugin templates

Five starting points for a GodwinMix plugin, one per language. Each is a whole
working source plugin that draws colour bars at the canvas caps, with a
manifest, a settings schema, a `SKILL.md`, an `AGENTS.md`, a recorded transcript
replayed offline, a test that fails until you write it, CI, and a `check` script
that finishes in under thirty seconds.

| Template | Runtime | Transport | Notes |
|---|---|---|---|
| [rust](rust/) | `godwinmix-sdk` | container, and `unixfd` with `--features gst` | the cheapest picture, and the only template that can do zero copy |
| [python](python/) | python3, standard library only | container | container by default because that is what works on every platform |
| [node](node/) | node 20 or newer, no npm packages | container | same reason |
| [go](go/) | go 1.21, standard library only | container | compiled, one binary per platform |
| [shell](shell/) | `sh` and `gst-launch-1.0` | container | proof that the protocol needs no SDK and no language runtime |

`gmx plugin new --kind source --lang <language> <name>` will copy one of these
and fill in the placeholders. That command does not exist yet, so today you copy
the directory yourself and run `sed` over it, which is what
`test-templates.sh` does.

## The placeholder set

Every template uses these and nothing else. A template that invents a new one
breaks `gmx plugin new`, so add it here first.

| Placeholder | What it becomes | Example |
|---|---|---|
| `{{name}}` | the plugin name, a slug. It is the namespace of every id the plugin registers | `my-cam` |
| `{{name_snake}}` | the same name with underscores, for languages whose identifiers cannot carry a hyphen | `my_cam` |
| `{{description}}` | one sentence saying what the plugin does. It is what a person and a model both read first | `A source that reads an RTSP camera.` |
| `{{author}}` | the author line for the manifest and the package file | `A Person` |
| `{{license}}` | an SPDX licence id | `MIT` |
| `{{year}}` | the current year, for a licence header | `2026` |
| `{{sdk}}` | the Rust template only: the `godwinmix-sdk` dependency, a version once the crate is published and a path to the copy beside the `gmx` binary until then | `{ path = "/usr/lib/gmx/sdk" }` |

Placeholders appear in file contents only, never in file or directory names.

## Testing the templates

```sh
./templates/test-templates.sh            # every template this machine can run
./templates/test-templates.sh python go  # only these
```

For each template it copies the directory to a temporary place, fills in the
placeholders, runs the template's own `./check --quick`, then feeds the plugin a
handshake and a `start`, waits a second, sends `shutdown`, and reads the media
back: an EBML header, a cluster, at least one whole frame at the canvas caps,
and a first luma row that is a picture rather than one flat value.

A template whose toolchain is missing is skipped with a line saying so rather
than failed, because Go is not installed everywhere and neither is cargo.

## What every template agrees on

* Control is JSON-RPC 2.0, one object per line, UTF-8, on **stdin** from the
  core and **stderr** to the core. The plugin speaks first, with `initialize`.
* **stdout is media and only media.** A `print` in any of these languages
  corrupts the video stream, and every template says so in a comment.
* The picture is raw I420 at the canvas caps, in a streamable Matroska stream:
  an EBML header, a Segment of unknown size, Info, Tracks, then SimpleBlocks in
  Clusters. That is enough for `decodebin` to open it, and no more than that is
  written.
* PTS is in nanoseconds on the plugin's own monotonic clock, starting near zero.
  The core retimes onto programme running time.
* The frame deadline is computed from the frame index, so one slow frame does
  not push every later frame back.
* `health` is answered without waiting on the media loop, because the core asks
  it while a slow `start` is still running.
* A method the plugin does not implement is answered `-32601` with a message
  naming what it does implement.

The details are in [docs/reference/plugin-protocol.md](../docs/reference/plugin-protocol.md)
and [docs/reference/plugin-manifest.md](../docs/reference/plugin-manifest.md).

## wasm

`gmx plugin new <name> --lang wasm` writes a tier W plugin: a WebAssembly
component that runs inside the core, sandboxed, with no media. Its `check`
script builds for `wasm32-wasip2` and copies the component to `plugin.wasm`,
which is the file `[run] wasm` names. `--kind` defaults to `service` there,
because a component carries no media and a `source` at that placement is
refused with -32005.
