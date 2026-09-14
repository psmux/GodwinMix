# AGENTS.md

For a coding agent extending this plugin. Read this before changing anything.

## What this is

A GodwinMix source plugin in Python. One process, started by the core. Control
is JSON-RPC 2.0 on stdin and stderr, one object per line. Media is a streamable
Matroska stream on stdout carrying raw I420 frames at the canvas caps.

Nothing outside the Python standard library is imported, and the check script
runs on a bare machine. Keep it that way unless the plugin genuinely needs a
library, and if it does, add it to `requirements.txt` so `gmx plugin add`
installs it into the plugin's own venv.

## Build and test

```sh
./check            # lint, the manifest, the offline transcript, your test. Under 30 s.
./check --quick    # the same without the test you have not written yet
```

There is nothing to build. `gmx plugin test .` runs the core's conformance
harness on top of this once `gmx` is installed; it takes about 60 seconds and
adds the checks this script cannot do without a core (real caps, frame counts,
a kill mid stream, the footprint).

## Where things are

| Path | What it is |
|---|---|
| `main.py` | the whole plugin. `draw()` is the part you change |
| `gmx-plugin.toml` | the manifest: what this plugin provides and how the core starts it |
| `schemas/source.json` | the settings, JSON Schema draft 2020-12. Every surface renders this |
| `skills/source/SKILL.md` | what an agent operating the mixer needs to know about this source |
| `tests/transcript.jsonl` | a recorded conversation with the core, replayed offline |
| `tests/replay.py` | the replayer. Do not edit it to make a test pass |
| `tests/check_manifest.py` | a rough manifest check for machines with no `gmx` |
| `tests/test_picture.py` | the failing test. Replace it |
| `check` | everything above, in order |

## The rules that matter

1. **stdout is media.** A `print()` anywhere in this process corrupts the video
   stream. Log with the `log()` helper, which writes to stderr.
2. **`draw()` must return exactly `width * height * 3 // 2` bytes** for the
   canvas the core sent. A wrong length stops the media loop and logs both
   numbers.
3. **`draw()` runs on the media thread.** No network calls, no file reads, no
   locks held for long. Do that work elsewhere and hand the result over.
4. **`health` must answer while `start` is still running.** The template answers
   it from the main loop without touching the media thread; keep that shape.
5. **`configure` gets the full validated object, not a diff.** Store it; the
   next frame reads it.
6. **Every error names the next step.** Look at the `-32601` message in
   `dispatch()` for the shape: what is wrong, then what to do about it.

## Changing it

* A different picture: edit `draw()`. Nothing else.
* New settings: add them to `schemas/source.json` with a `description`, a
  `default` and at least one example, then read them from the `params` argument
  of `draw()`. Do not add a settings UI; the schema is the UI.
* Audio as well as video: set `audio = "raw"` in the manifest's `media` table,
  add an audio track to the Matroska header, and write 10 ms buffers of
  interleaved F32LE at 48 kHz. `examples/zero-dep-source.py` in the GodwinMix
  repository shows the header; the Rust SDK does it for you if you would rather
  switch languages.
* A tool an agent can call: add a `[[tools]]` block to the manifest with an
  input schema and a description carrying one example call, then handle
  `tool.call` in `dispatch()`.

## What not to do

* Do not edit `tests/replay.py` or `tests/transcript.jsonl` to make a failing
  check pass. If the plugin's behaviour changed on purpose, re-record the
  transcript and say so in the commit message.
* Do not remove the `-32601` branch. A plugin that ignores an unknown method
  leaves the caller waiting.
* Do not write to stdout. See rule 1.
