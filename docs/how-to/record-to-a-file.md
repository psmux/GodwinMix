# Record to a file

Open **Outputs**, choose **Record**, select a folder and format, then choose
**Start recording**. The folder is on the mixer, including when the UI is on
another computer. The dialog opens with the default filled in, `Videos/GodwinMix`
in the mixer's home folder, so Start recording works on the first press.

To record somewhere else, press **Choose** beside the folder. That walks the
folders on the mixer: its home folder, its media folder and the folder its
config is in, and nothing outside them. Each folder the mixer cannot write
into is marked read only, and **Use this folder** stays off while you are in
one. To make a new folder, type its name and press **New folder**. A script
does the same with `path.list` and `path.create`, described in the
[HTTP API reference](../reference/http-api.md).

The output row shows the filename, status and amount of
muxed data. Choose **Stop recording** when the programme is finished.

This built in recording path works on Windows, macOS and Linux without a
plugin or restart. It keeps the same quality and audio mix as the stream.
MP4 is fragmented. Matroska is also available. Each start creates a new file.
Allow up to five seconds after stopping for the file to finish.

The public API is in [Recording outputs](../reference/recording.md).

## Optional file recording sidecar

The remaining instructions describe `file-record/output`, the separate plugin
with timed splitting and recording discovery tools. Its FIFO transport is
limited to Linux and macOS. These limits do not apply to `record/output` above.


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
```

Then write the output into the config file and restart the mixer:

```toml
[[outputs]]
id = "archive"
type = "file-record/output"
```

That writes `archive-2026-09-13-1030.mp4` into `~/Videos/GodwinMix`.

`gmx ctl output add` takes an RTMP address rather than a plugin type, because
the core does not register `output` provides yet; see
[the plugin reference](../reference/plugins.md). To stop a recording and close
the file properly, remove the output:

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

```toml
[[outputs]]
id = "archive"
type = "file-record/output"
params = { directory = "/media/archive", pattern = "{date}/sunday-{time}" }
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

```toml
[[outputs]]
id = "archive"
type = "file-record/output"
params = { split_after_minutes = 30 }
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
| `health` says failing and names an element | the container cannot hold what the programme is encoded as. Try `format = "mkv"` |

## Next

* [Use a webcam](use-a-webcam.md).
* [Capture the screen](capture-the-screen.md).
* [The first party plugins](../reference/plugins.md).
