# file-record

Record the programme to a file. The same thing that goes out to the stream,
kept on the disk.

It is a remux, not a second encode: the file is exactly what the encoder
already made, so recording costs about as much CPU as copying a file and adds
nothing to what the stream costs. That also means it cannot record at a
different size or bitrate from the stream.

## What you need

* Linux or macOS. Not Windows, and the manifest does not claim it: an output
  plugin receives the programme on a FIFO, Windows has none, and the core
  refuses a sidecar output there rather than half working. Recording on Windows
  waits on a named pipe in the core; the gap is written down in
  [the plugin lifecycle](../../docs/reference/plugin-lifecycle.md).
* GStreamer 1.24 or later with the good plugins, for `mp4mux` and
  `matroskamux`.
* Somewhere to write. An hour of 1080p at 6 Mbit/s is about 2.7 GB.

## Three steps

```sh
./plugins/file-record/build
gmx plugin add ./plugins/file-record
gmx ctl output add archive file-record/output --set pattern=sunday-{date}
```

The file appears in `~/Videos/GodwinMix` unless you say otherwise. To stop and
close the file properly:

```sh
gmx ctl output remove archive
```

## Settings

| Setting | Default | What it does |
|---|---|---|
| `directory` | `~/Videos/GodwinMix` | where the files go. Made if it is not there |
| `pattern` | `{instance}-{datetime}` | the name without the extension |
| `format` | `mp4` | `mp4` (fragmented) or `mkv` |
| `split_after_minutes` | 0 | a new file every so many minutes. 0 is one file |
| `min_free_gb` | 1 | report degraded under this much free space |
| `label` | empty | a name for the operator |

In `pattern`, `{date}` is `2026-09-13`, `{time}` is `1030`, `{datetime}` is
both, and `{instance}` is the output's id. A pattern may contain a slash, so
`{date}/service-{time}` puts each day in its own folder. A split recording gets
a five digit number on the end.

Only `label` and `min_free_gb` change while recording. The rest answer
`restart_required` with a reason, because changing the folder or the format
would have to cut the file in half.

## Why MP4 is written in fragments

An ordinary MP4 keeps its index in memory and writes it when the file is
closed. If the machine loses power, or somebody pulls the plug on the rack at
the end of the service, that index is never written and no player will open the
file. The recording you most wanted is the one you lose.

Fragmented MP4 writes an index every second. A crash costs you the last second
and the rest plays. That is why it is the default, and why `mkv` is offered as
the alternative rather than the other way round.

Stopping the output properly (`gmx ctl output remove`) finishes the file
through the muxer either way, which is always better than a crash.

## Splitting

`split_after_minutes` starts a new file on the clock. Splits land on the next
keyframe, so a 30 minute split is 30 minutes plus a second or two. Files are
numbered: `service-2026-09-13-00000.mp4`, `-00001`, and so on.

Useful for a long stream you want to upload in pieces, and for a filesystem
that does not like very large files.

## Disk space

`health` goes degraded when the disk has less room than `min_free_gb`, and the
message says how much is left, which folder, and how much this recording has
written. It does not stop recording. An operator told at 1 GB can delete
something; a recorder that stopped itself in the middle of a service cannot be
argued with.

```sh
gmx plugin stats file-record
```

## When it is not recording

| What you see | What to do |
|---|---|
| the output will not add, and the machine is Windows | it does not ship for Windows. See What you need |
| `could not make the recording folder` | pick a folder you can write to |
| the file is there but 0 bytes | nothing is going out. A recorder records the programme, and the programme is empty until a source is on air |
| `health` says failing and names an element | the format cannot hold what the programme is encoded as. Try `format=mkv` |
| the file will not play after a power cut | with `mp4` it should. A Matroska recording that was never finished may need a repair tool |

## See also

* [Record to a file](../../docs/how-to/record-to-a-file.md).
* [The first party plugins](../../docs/reference/plugins.md).
