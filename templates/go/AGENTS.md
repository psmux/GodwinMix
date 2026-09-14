# AGENTS.md

For a coding agent extending this plugin. Read this before changing anything.

## What this is

A GodwinMix source plugin in Go. One process, started by the core. Control is
JSON-RPC 2.0 on stdin and stderr, one object per line. Media is a streamable
Matroska stream on stdout carrying raw I420 frames at the canvas caps.

Nothing outside the Go standard library is imported, `go.mod` names no
requirements, and the check script runs on a bare machine with a toolchain and
nothing else. Keep it that way unless the plugin genuinely needs a library.

This template has not been run on the machine that wrote it, which had no Go
installed. If something here does not compile, that is why, and fixing it is the
first job. `./templates/test-templates.sh go` from the GodwinMix repository root
is the fastest way to find out.

## Build and test

```sh
./check            # gofmt, vet, build, the manifest, the transcript, your test. Under 30 s.
./check --quick    # the same without the test you have not written yet
go build -o bin/{{name}} .
```

`gmx plugin test .` runs the core's conformance harness on top of this once
`gmx` is installed; it takes about 60 seconds and adds the checks this script
cannot do without a core (real caps, frame counts, a kill mid stream, the
footprint).

## Where things are

| Path | What it is |
|---|---|
| `main.go` | the whole plugin. `draw()` is the part you change |
| `go.mod` | the module. No requirements, and it should stay that way |
| `gmx-plugin.toml` | the manifest: what this plugin provides, how it is built and how the core starts it |
| `schemas/source.json` | the settings, JSON Schema draft 2020-12. Every surface renders this |
| `skills/source/SKILL.md` | what an agent operating the mixer needs to know about this source |
| `tests/transcript.jsonl` | a recorded conversation with the core, replayed offline |
| `tests/replay` | the replayer. Do not edit it to make a test pass |
| `tests/check_manifest` | a rough manifest check for machines with no `gmx` |
| `picture_test.go` | the failing test. Replace it. It is here, not under `tests/`, because only package main can reach `draw()` |
| `check` | everything above, in order |

## The rules that matter

1. **stdout is media.** An `fmt.Println` anywhere in this process corrupts the
   video stream. Everything the plugin says goes through `send()`, which writes
   to `ctrl`, which is stderr.
2. **`draw()` must return exactly `width*height + 2*cw*ch` bytes** for the canvas
   the core sent, where `cw` and `ch` are the chroma plane dimensions. A wrong
   length stops the media goroutine and logs both numbers.
3. **`draw()` runs on the media goroutine.** No network calls, no file reads, no
   lock held for long. Do that work elsewhere and hand the result over through
   `configure`, which is what `currentSettings()` reads.
4. **`health` is answered on the reader goroutine.** It takes `mu` for the two
   counters and returns; it never waits on the media goroutine. Keep that shape,
   because a `health` that blocks behind a stuck frame is worse than no `health`
   at all.
5. **`mu` guards exactly three things**: `settings`, `frames` and `failed`.
   Everything else in the file belongs to the reader goroutine alone, and
   `stopMedia()` waits for the media goroutine before the reader touches any of
   it.
6. **`configure` gets the full validated object, not a diff.** Store it; the next
   frame reads it.
7. **The frame deadline comes from the frame index.** `produce()` computes when
   frame N is due from `index * step`, not by sleeping a frame time each round,
   so one slow draw does not push every later frame back.
8. **Every error names the next step.** Look at the `-32601` message in
   `dispatch()` for the shape: what is wrong, then what to do about it.

## Changing it

* A different picture: edit `draw()`. Nothing else.
* New settings: add them to `schemas/source.json` with a `description`, a
  `default` and at least one example, then read them from the `params` argument
  of `draw()`. They arrive as `map[string]any`, so numbers are `float64`. Do not
  add a settings UI; the schema is the UI.
* Audio as well as video: set `audio = "raw"` in the manifest's `media` table,
  add a second TrackEntry to the Matroska header, and write 10 ms buffers of
  interleaved F32LE at 48 kHz on track 2. `examples/zero-dep-source.py` in the
  GodwinMix repository shows the header.
* The `unixfd` transport: Go can pass a file descriptor over a unix socket with
  `SCM_RIGHTS`, which saves the copy per frame on Linux and macOS. Declare
  `transports = ["unixfd", "container"]` and keep the container path working, or
  the plugin stops running on Windows.
* A tool an agent can call: add a `[[tools]]` block to the manifest with an
  input schema and a description carrying one example call, then handle
  `tool.call` in `dispatch()`.

## What not to do

* Do not edit `tests/replay` or `tests/transcript.jsonl` to make a failing check
  pass. If the plugin's behaviour changed on purpose, re-record the transcript
  and say so in the commit message.
* Do not add a dependency to `go.mod` for something the standard library does.
* Do not remove the `-32601` branch. A plugin that ignores an unknown method
  leaves the caller waiting.
* Do not write to stdout. See rule 1.
