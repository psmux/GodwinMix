# Record to a file

Keep a copy of the service on the disk, at the same quality that went out, for
almost no extra CPU.

## What this costs

Nothing worth measuring. The programme is already encoded by the time an output
sees it, so recording is a remux: the file gets exactly the bytes the encoder
made. That is also the limit. You cannot record at a different size or bitrate
from the stream, because there is no second encode to change.

An hour of 1080p at 6 Mbit/s is about 2.7 GB.

## Linux and macOS only

An output plugin receives the programme on a FIFO, and Windows has none. The
core refuses a sidecar output on Windows with a message saying so, and this
plugin does not claim the platform. Recording on Windows waits on a named pipe
in the core; the gap is written down in
[the plugin lifecycle](../reference/plugin-lifecycle.md).

## Three steps

```sh
./plugins/file-record/build
gmx plugin add ./plugins/file-record
gmx ctl output add archive file-record/output
```

That writes `archive-2026-09-13-1030.mp4` into `~/Videos/GodwinMix`.

To stop and close the file properly:

```sh
gmx ctl output remove archive
```

Stopping this way sends an end of stream through the muxer, which is what
writes the index. Killing the mixer does not, which is the next section.

## Why MP4 is written in fragments

An ordinary MP4 keeps its index in memory and writes it when the file is
closed. A machine that loses power, or a rack somebody switched off at the end
of the service, never writes that index, and no player will open what is left.
The recording you most wanted to keep is the one you lose.

Fragmented MP4 writes an index every second. A crash costs the last second and
the rest plays. That is the default here, and `mkv` is the alternative rather
than the other way round.

## Name the files something useful

```sh
gmx ctl output add archive file-record/output \
  --set directory=/media/archive \
  --set pattern="{date}/sunday-{time}"
```

| Token | Becomes |
|---|---|
| `{date}` | `2026-09-13` |
| `{time}` | `1030` |
| `{datetime}` | `2026-09-13-1030` |
| `{instance}` | the output's id |

A pattern may contain a slash, so `{date}/sunday-{time}` puts each day in its
own folder. The folders are made for you.

## Split by the clock

```sh
gmx ctl output add archive file-record/output --set split_after_minutes=30
```

A new file every half hour: `archive-2026-09-13-1030-00000.mp4`, `-00001`, and
so on. Splits land on the next keyframe, so half an hour is half an hour plus a
second or two. Useful for uploading in pieces, and for a filesystem that does
not like very large files.

## Watch the disk

`min_free_gb` decides when the recording reports itself as degraded. It is 1 GB
by default, and it does not stop recording:

```
archive  degraded  740 MB left on the disk holding /media/archive, and the
                   recording has written 3.1 GB. Free some room or stop this
                   output; recording carries on either way.
```

That is deliberate. An operator told at 1 GB can delete something or swap a
disk. A recorder that stopped itself in the middle of a service could not be
argued with.

## Check it is working

The honest answer is the file, not the status:

```sh
ls -la ~/Videos/GodwinMix
```

Twice, a few seconds apart. If the size is going up, it is recording. An agent
asks the same question with `list_recordings`, which also answers where the
folder is and how much room is left.

## When there is no file

| What you see | What to do |
|---|---|
| the output will not add, on Windows | see Linux and macOS only, above |
| `could not make the recording folder` | pick a folder you can write to |
| the file exists but stays at 0 bytes | nothing is going out. A recorder records the programme, and the programme is empty until a source is on air |
| `health` says failing and names an element | the container cannot hold what the programme is encoded as. Try `--set format=mkv` |

## Next

* [Use a webcam](use-a-webcam.md).
* [Capture the screen](capture-the-screen.md).
* [The first party plugins](../reference/plugins.md).
