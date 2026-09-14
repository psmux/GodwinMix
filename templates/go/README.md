# {{name}}

{{description}}

## What to change

1. **`draw()` in `main.go`.** It returns one I420 frame as a `[]byte`. That is
   your picture, and it is the only function in the file you have to touch.
2. **`schemas/source.json`.** Every key you want an operator to set, with a
   description, a default and an example. This is the settings UI, everywhere.
3. **`picture_test.go`.** It fails on purpose until you write it.
4. **`skills/source/SKILL.md`.** One paragraph an agent reads before using your
   source. Keep the `description` under 1,024 characters.

Then:

```sh
./check          # under 30 seconds, no core needed
gmx plugin add . # once gmx is installed
gmx source add cam "{{name}}/source"
```

## Not run on the machine that wrote it

This template was written but never executed: the machine it was written on had
no Go toolchain installed, so nothing here has been compiled, vetted or run. The
first thing to do with it is run the harness from the GodwinMix repository root:

```sh
./templates/test-templates.sh go
```

That fills the placeholders into a temp directory, runs `./check --quick`,
builds the plugin, feeds it a handshake and a start, and reads the first
Matroska cluster back. If something does not compile, it will be small and it
will be in `main.go`, `tests/replay/main.go` or `tests/check_manifest/main.go`.

## No dependencies

`go.mod` names no requirements and there is no vendor directory. Everything here
is the standard library: `encoding/json` for the control channel,
`encoding/binary` and `bufio` for the media, `sync` and `time` for the frame
loop. Adding a dependency means shipping it, so weigh it against writing the
twenty lines yourself.

## Where the tests live

`picture_test.go` sits beside `main.go` rather than under `tests/`, because a Go
test can only reach `draw()` from inside package main. `tests/` holds the
transcript and the two helper programs, `tests/replay` and
`tests/check_manifest`, which are their own packages and do not need `draw()`.

## Why container mode

This template writes raw frames inside a Matroska stream on a pipe. That works
on Linux, macOS and Windows with no shared memory and no sockets. It costs one
copy of each frame: the pipe carries about 41 MB/s at 720p30 and 93 MB/s at
1080p30.

`unixfd` is cheaper and is Linux and macOS only. Go can speak it, over a unix
socket with `SCM_RIGHTS`, and when you need the copy back you declare
`transports = ["unixfd", "container"]` so the core picks the best one available.
Start with container: it is the one that works everywhere, and at 720p the copy
does not show up in a profile.

## Nothing else

The rest of `main.go` is the protocol, and it is the same in every plugin. It is
in this file rather than in a library so you can read the whole thing in one
sitting and change it if you need to.
