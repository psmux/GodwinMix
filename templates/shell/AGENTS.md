# AGENTS.md

For a coding agent extending this plugin. Read this before changing anything.

## What this is

A GodwinMix source plugin in POSIX sh. One process, started by the core. Control
is JSON-RPC 2.0 on stdin and stderr, one object per line, written by hand with
`printf`. Media is a streamable Matroska stream on stdout, produced by a
`gst-launch-1.0` pipeline started as a background job and writing to fd 1.

The point of the template is that the protocol needs no SDK. Keep it that way:
`sh`, `sed`, `tr`, `cut` and `gst-launch-1.0`, and nothing else.

## Build and test

```sh
./check            # sh -n, the manifest, the offline transcript, your test. Under 30 s.
./check --quick    # the same without the test you have not written yet
```

There is nothing to build. python3 runs the tests and is not needed to run the
plugin. `gmx plugin test .` runs the core's conformance harness on top of this
once `gmx` is installed; it takes about 60 seconds and adds the checks this
script cannot do without a core (real caps, frame counts, a kill mid stream, the
footprint).

## Where things are

| Path | What it is |
|---|---|
| `run.sh` | the whole plugin. The pipeline in `start_media()` is the part you change |
| `gmx-plugin.toml` | the manifest: what this plugin provides and how the core starts it |
| `schemas/source.json` | the settings, JSON Schema draft 2020-12. Every surface renders this |
| `skills/source/SKILL.md` | what an agent operating the mixer needs to know about this source |
| `tests/transcript.jsonl` | a recorded conversation with the core, replayed offline |
| `tests/replay.py` | the replayer, in Python because sh cannot hold a deadline. Do not edit it to make a test pass |
| `tests/check_manifest.py` | a rough manifest check for machines with no `gmx` |
| `tests/test_picture.sh` | the failing test. Replace it |
| `check` | everything above, in order |

## The rules that matter

1. **stdout is media.** An `echo` without `>&2` anywhere in this file corrupts
   the video stream. Everything the plugin says goes through `send()`, which
   writes to stderr. Every background job gets `</dev/null` so it cannot eat the
   control channel either.
2. **The JSON parsing is `sed` on `"key":value`, and that is deliberate.** It
   handles three integers, a method name and an instance id. If you need an
   array, nesting or an escaped string, the honest move is to port the plugin to
   `templates/python` or `templates/go` rather than to write a JSON parser in
   `sed`.
3. **Nothing from outside goes into a JSON string unquoted.** `clean()` strips
   everything that would need escaping and cuts to 200 characters. Use it on
   every value that came from the core.
4. **`health` must answer while the pipeline is running.** It does, because the
   pipeline is a background job and the read loop never waits on it. `kill -0`
   on the saved pid is the whole check.
5. **`configure` gets the full validated object, not a diff.** A shell plugin
   applies a setting by killing `gst-launch-1.0` and starting it again, which is
   `stop_media` then `start_media` in that branch.
6. **The pipeline is killed on `stop`, on `shutdown`, and on INT and TERM.**
   A stray `gst-launch-1.0` holding the media pipe open is the worst failure
   this plugin has.
7. **Every error names the next step.** Look at the `-32601` message in
   `dispatch()` for the shape: what is wrong, then what to do about it.

## Changing it

* A different picture: edit the one `gst-launch-1.0` line in `start_media()`.
  `matroskamux streamable=true` writes the same elements the python, node, go
  and rust templates write by hand, so anything ending in
  `! matroskamux streamable=true ! fdsink fd=1` will work. ffmpeg with
  `-f matroska pipe:1` does the same job.
* New settings: add them to `schemas/source.json` with a `description`, a
  `default` and at least one example, then read them in the `configure` branch
  and restart the pipeline. Do not add a settings UI; the schema is the UI.
* Audio as well as video: set `audio = "raw"` in the manifest's `media` table
  and add an audio branch to the pipeline, `audiotestsrc ! audio/x-raw,
  format=F32LE,rate=48000 ! matroskamux`. The mux takes both pads.
* Windows: it cannot be done from here. `[run] shell` is refused on Windows
  without a `bin` entry, which is why `platforms` stops at Linux and macOS.

## What not to do

* Do not edit `tests/replay.py` or `tests/transcript.jsonl` to make a failing
  check pass. If the plugin's behaviour changed on purpose, re-record the
  transcript and say so in the commit message.
* Do not add a dependency on python3, jq, bash or perl to `run.sh`. The template
  is worth nothing if it needs a runtime.
* Do not remove the `-32601` branch. A plugin that ignores an unknown method
  leaves the caller waiting.
* Do not write to stdout. See rule 1.
