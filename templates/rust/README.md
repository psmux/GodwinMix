# {{name}}

{{description}}

## What to change

1. **`draw()` in `src/main.rs`.** It fills one I420 frame. That is your picture.
2. **`schemas/source.json`.** Every key you want an operator to set, with a
   description, a default and an example. This is the settings UI, everywhere.
3. **`tests/your_picture.rs`.** It fails on purpose until you write it.
4. **`skills/source/SKILL.md`.** One page an agent reads before using your
   source. Keep the `description` under 1,024 characters.

Then:

```sh
./check            # under 30 seconds on a warm target directory
gmx plugin add .   # once gmx is installed
gmx source add cam "{{name}}/source"
```

## What the SDK does for you

`godwinmix-sdk` runs the JSON lines loop, the handshake and the state machine,
and it gives you a pacer, a frame pool and the media writers. Three things it
gets right that are easy to get wrong by hand:

* The frame deadline comes from the frame index, not from adding a sleep each
  time, so one slow draw does not push every later frame back.
* PTS starts at zero on the plugin's own monotonic clock, which is what the core
  retimes from.
* `health` is answered on the reader thread, so it still comes back while a slow
  `start` is running.

## Transports

The template declares `transports = ["container"]`: raw frames in a streamable
Matroska stream on stdout, which works on Linux, macOS and Windows. It costs one
copy of each frame, and the pipe carries about 41 MB/s at 720p30 and 93 MB/s at
1080p30.

Rust can do better. Build with `--features gst` and add `"unixfd"` to
`transports` to get `unixfdsink`, which is zero copy and is Linux and macOS
only. The core picks the best transport both sides declare, so listing both is
safe: a Windows core falls back to the container on its own.

## Nothing else

`src/main.rs` is the whole plugin. The parts that are not `draw()` are the
manifest of what this source is, and they are short enough to read in one
sitting.
