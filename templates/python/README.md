# {{name}}

{{description}}

## What to change

1. **`draw()` in `main.py`.** It returns one I420 frame. That is your picture.
2. **`schemas/source.json`.** Every key you want an operator to set, with a
   description, a default and an example. This is the settings UI, everywhere.
3. **`tests/test_picture.py`.** It fails on purpose until you write it.
4. **`skills/source/SKILL.md`.** One paragraph an agent reads before using your
   source. Keep the `description` under 1,024 characters.

Then:

```sh
./check          # under 30 seconds, no core needed
gmx plugin add . # once gmx is installed
gmx source add cam "{{name}}/source"
```

## Why container mode

This template writes raw frames inside a Matroska stream on a pipe. That works
on Linux, macOS and Windows with no shared memory and no sockets, and it is the
only mode Python can use on every platform today. It costs one copy of each
frame: the pipe carries about 41 MB/s at 720p30 and 93 MB/s at 1080p30.

`unixfd` is cheaper and is Linux and macOS only. If you need it, write the
plugin in Rust with `godwinmix-sdk` or in Go, and declare
`transports = ["unixfd", "container"]` so the core picks the best one available.

## Nothing else

The rest of `main.py` is the protocol, and it is the same in every plugin. It is
in this file rather than in a library so you can read the whole thing in one
sitting and change it if you need to.
