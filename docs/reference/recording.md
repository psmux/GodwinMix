# Recording outputs

`record/output` is a built in output on Windows, macOS and Linux. It uses the
same public output contract and shared encoded programme as RTMP and SRT.
No recording plugin installation or mixer restart is needed.

Start with `output.add` over JSON RPC:

```json
{"id":"archive","uri":"record://programme","type":"record/output","params":{"format":"mp4"}}
```

`params.format` accepts `mp4` (default) or `mkv`. `params.directory` is a folder
on the machine running the mixer. The default is `Videos/GodwinMix` under that
process's home directory. The folder is created when recording starts. Every
start and reconnect reserves a new filename. Existing recordings are kept.

`output.list` and `output.get` include `type`, `recording_path`,
`bytes_muxed` and `recording_secs`. The byte count measures muxed data
delivered to the file sink, not a guarantee that the operating system has
flushed it to physical storage. `recording_secs` counts whole seconds since
the output was added, and a reconnect that starts a new file does not reset
it. The header's REC button shows it.

Stop with `output.remove` and the recording's id. The output is detached
immediately. File finalisation runs on a worker so a slow disk cannot hold the
mixer's command loop. Allow up to five seconds for finalisation before moving
the file. MP4 uses one second fragments to preserve completed fragments if the
process ends before finalisation completes. Normal mixer shutdown waits up to
six seconds for outstanding recording finalisers.

The page stops a recording through the same `output.remove`, from the
Outputs panel's row, from the header's REC button and from
**Outputs > Stop recording**. The last two ask first.

Recording has the programme's resolution, codec, bitrate and audio mix.
Independent recording quality, separate audio tracks and replay buffers are
not provided by this output. A full disk can fail the recorder; other outputs
remain independent. The file recording sidecar remains available for timed
file splitting and its additional tools on supported platforms.

## Restarts

A recording added with `output.add` is not written to the runtime store
(`godwinmix.runtime.toml` beside the config), so it does not start again when
the mixer restarts. Other outputs added at runtime are saved there and come
back. A `record/output` or `file-record/output` in the runtime store, which
0.2.2 wrote, is skipped at start and logged.

A recorder in the config file's `[[outputs]]` starts every time the mixer
does, also once the runtime store has its own output list. That is the way
to record on start. Stopping it with `output.remove` lasts until the next
start.
