# {{name}}

{{description}}

This template exists to show that the plugin protocol needs no SDK and no
language runtime. It is a POSIX shell script that answers JSON-RPC on stderr and
hands stdout to `gst-launch-1.0`. ffmpeg would work the same way, and so would
anything else that can write a Matroska stream to fd 1.

## What to change

1. **The pipeline in `start_media()` in `run.sh`.** One line. That is your
   picture, and it is the only part of the file you have to touch.
2. **`schemas/source.json`.** Every key you want an operator to set, with a
   description, a default and an example. This is the settings UI, everywhere.
3. **`tests/test_picture.sh`.** It fails on purpose until you write it.
4. **`skills/source/SKILL.md`.** One paragraph an agent reads before using your
   source. Keep the `description` under 1,024 characters.

Then:

```sh
./check          # under 30 seconds, no core needed
gmx plugin add . # once gmx is installed
gmx source add cam "{{name}}/source"
```

## What you need installed

The plugin needs `sh` and `gst-launch-1.0`. On Debian and Ubuntu:

```sh
sudo apt-get install -y gstreamer1.0-tools gstreamer1.0-plugins-good
```

The tests need python3 as well. The plugin does not: python3 drives `run.sh`
from the outside in `tests/replay.py` and reads the manifest in
`tests/check_manifest.py`, and neither runs when the core starts the plugin.

## No Windows

`platforms` in the manifest lists Linux and macOS and stops there. The core
refuses a `[run] shell` entry on Windows unless `[run]` also carries a `bin`
entry for it, and this plugin has no binary to offer. If you need Windows, the
node, python, go and rust templates are the same plugin in a language that has
one.

## Reading JSON with sed

`number()` and `word()` in `run.sh` pull integers and strings out of the core's
lines with `sed`. That is genuinely what a shell plugin does, and for a canvas,
a method name and an instance id it is enough. The moment you need an array, a
nested object or a string with an escape in it, stop: that is the point where
this template has run out, and `templates/python` or `templates/go` is the same
plugin with a real parser.

## Why container mode

This template hands the core raw frames inside a Matroska stream on a pipe. That
works on Linux, macOS and Windows with no shared memory and no sockets, and a
shell script has no other option. It costs one copy of each frame: the pipe
carries about 41 MB/s at 720p30 and 93 MB/s at 1080p30.

`unixfd` is cheaper and is Linux and macOS only. If you need it, write the
plugin in Rust with `godwinmix-sdk` or in Go, and declare
`transports = ["unixfd", "container"]` so the core picks the best one available.

## Nothing else

The rest of `run.sh` is the protocol, and it is the same in every plugin. Read
it once and you have read the whole of what a GodwinMix plugin has to do.
