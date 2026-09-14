# {{name}}

{{description}}

## What to change

1. **`draw()` in `main.js`.** It returns one I420 frame as a Buffer. That is your
   picture, and it is the only function in the file you have to touch.
2. **`schemas/source.json`.** Every key you want an operator to set, with a
   description, a default and an example. This is the settings UI, everywhere.
3. **`tests/test_picture.js`.** It fails on purpose until you write it.
4. **`skills/source/SKILL.md`.** One paragraph an agent reads before using your
   source. Keep the `description` under 1,024 characters.

Then:

```sh
./check          # under 30 seconds, no core needed
gmx plugin add . # once gmx is installed
gmx source add cam "{{name}}/source"
```

## No dependencies

There is no `package.json` and no `node_modules`. Everything here is a Node 20
built in: `node:readline` for the control channel, `Buffer` for the frames.
Adding a dependency means shipping it, so weigh it against writing the twenty
lines yourself.

## Why container mode

This template writes raw frames inside a Matroska stream on a pipe. That works
on Linux, macOS and Windows with no shared memory and no sockets. It costs one
copy of each frame: the pipe carries about 41 MB/s at 720p30 and 93 MB/s at
1080p30.

`unixfd` is cheaper and is Linux and macOS only. If you need it, write the
plugin in Rust with `godwinmix-sdk` or in Go, and declare
`transports = ["unixfd", "container"]` so the core picks the best one available.

## The bitwise trap

JavaScript's `<<`, `|` and `&` all convert to 32 bit signed integers first, so
`1 << 35` is 8 and not what you want. A 1080p frame is over 3 million bytes, and
its EBML size needs four bytes with the marker bit in bit 4 of the first. `vint()`
therefore writes the value big endian a byte at a time and sets the marker with
`out[0] |= 1 << (8 - length)`, where the shift is small enough to be safe. If you
rewrite that function, keep a test on a 1080p sized value.

## Nothing else

The rest of `main.js` is the protocol, and it is the same in every plugin. It is
in this file rather than in a library so you can read the whole thing in one
sitting and change it if you need to.
