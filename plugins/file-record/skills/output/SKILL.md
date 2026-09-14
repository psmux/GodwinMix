---
name: file-record-output
description: Record the GodwinMix programme to a file, and diagnose a recording that is not working. Use this when the operator asks to record a service, wants to know where the recording went or whether it is still running, needs the file split by the clock, or is worried about disk space.
---

# Recording to a file

One instance is one recording. It writes what the encoder already made, so it
costs almost no CPU and the file is exactly the quality of the stream.

## Start one

```
output.add {id: "archive", type: "file-record/output", params: {pattern: "sunday-{date}", split_after_minutes: 60}}
```

With no params at all it writes `<id>-<date>-<time>.mp4` into a GodwinMix
folder inside the operator's Videos folder.

| Param | What it does |
|---|---|
| `directory` | where the files go. Made if it is not there. Empty is Videos/GodwinMix |
| `pattern` | the name without the extension. `{date}`, `{time}`, `{datetime}`, `{instance}` |
| `format` | `mp4` (fragmented, survives a crash) or `mkv` |
| `split_after_minutes` | a new file every so many minutes. 0 is one file |
| `min_free_gb` | report degraded under this much free space. Does not stop recording |
| `label` | a name for the operator |

## Where did it go

Call `gmx_file_record_list_recordings` with no arguments:

```json
{"directory": "/home/av/Videos/GodwinMix",
 "free_bytes": 412000000000,
 "recordings": [{"name": "sunday-2026-09-13.mp4", "bytes": 2411724800,
                 "modified": "2026-09-13T11:42:06Z", "recording": true}]}
```

`recording: true` marks the file being written now. To answer "is it actually
working", call it twice a few seconds apart and check `bytes` went up. That is
a better answer than `health`, because it is the file on the disk.

## Settings while it runs

Only `label` and `min_free_gb` change while recording. Everything else answers
`restart_required` with a reason, because changing the folder or the format
would have to cut the file. Stop the output and start it again.

```
output.remove {id: "archive"}     # finishes the file cleanly
output.add {...}                  # starts a new one
```

Stopping finishes the file properly: an end of stream goes through the muxer so
the index is written. Killing the mixer does not, which is why MP4 is written
in fragments.

## Disk space

`health` goes `degraded` when free space drops under `min_free_gb`, with the
figure and the folder in the message. It does not stop recording, on purpose:
an operator who is told at 1 GB can delete something or swap a disk, and a
recorder that stopped itself in the middle of a service could not be argued
with. If you are asked to act on it, say how much is left and what is in the
folder; do not remove anything without being asked.

## When it is not recording

| What you see | What to do |
|---|---|
| the output will not add at all, on Windows | sidecar outputs need a FIFO and Windows has none. This plugin does not ship for Windows and the core refuses one there |
| `could not make the recording folder` | the folder is not writable. Pick another in `directory` |
| `health` says failing and names an element | the format cannot hold what the programme is encoded as. Try `mkv` |
| the file exists but is 0 bytes | the programme is not running. Check a source is on air; a recorder records what is going out, and nothing is |
| the file will not play after a power cut | it should, if `format` was `mp4`. A Matroska recording that was never finished may need a repair tool |

## What it does not do

It does not re-encode, so it cannot record at a different size or bitrate from
the stream. It does not record a single source; it records the programme. It
does not delete old files.
