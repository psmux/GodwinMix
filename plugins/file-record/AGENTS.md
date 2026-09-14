# AGENTS.md

For a coding agent changing this plugin.

## What this is

A GodwinMix **output** plugin in Rust. The media travels the opposite way from
a source's: the core writes the encoded programme into a FIFO and this reads
it. Control is JSON-RPC 2.0 on stdin and stderr as usual.

## Build and test

```sh
./check                                   # everything that needs no core
./build                                   # stage bin/gmx-file-record
cargo test -p gmx-file-record --test records   # a real recording through a real FIFO
gmx plugin test ./plugins/file-record
```

`gmx plugin test` takes an output as far as the handshake and stops: the core
registers only `source` provides today, so everything past that is checked by
`tests/records.rs`, which makes a FIFO, plays the core's part, writes ten
seconds of encoded programme in and plays the file back.

## Where things are

| Path | What it is |
|---|---|
| `src/settings.rs` | the schema as a struct, and where the files go by default |
| `src/naming.rs` | the pattern, and the printf escaping that keeps a path from being a format string |
| `src/pipeline.rs` | fdsrc, parsebin, splitmuxsink, and the fragmented MP4 properties |
| `src/output.rs` | the `output` provide |
| `src/tools.rs` | `list_recordings` |
| `tests/records.rs` | the real recording test |
| `../capture-common/` | the FIFO open, the free space reading, the pipeline and bus watch |

## The rules that matter

1. **Open the FIFO at `initialize`, never at `start`.** Opening a FIFO for
   reading waits for a writer and opening it for writing waits for a reader.
   The core opens its end before it calls `start`, so a plugin that waited for
   `start` would deadlock with the core. `capture_common::fifo` opens with
   `O_NONBLOCK` and clears the flag, which is the whole trick.
2. **`stop` must drain.** An end of stream through the muxer is what finishes
   the file. A pipeline taken straight to NULL leaves an MP4 with no index.
3. **Never re-encode.** The programme arrives encoded. `parsebin` and a muxer,
   and nothing between them. A decoder in here would cost more CPU than the
   whole rest of the mixer.
4. **A path is not a format string.** `splitmuxsink` prints the location with
   the fragment number, so every `%` from an operator's pattern is doubled in
   `naming.rs`. There is a test for it; do not remove it.
5. **Never stop recording on your own.** Low disk space is `degraded` and a
   message. A recorder that stopped itself during a service is the worst
   failure this plugin could have.
6. **Say `restart_required` rather than lying.** Changing the folder or the
   format mid recording would cut the file. Saying no is correct.

## Changing it

* Another container: add it to `muxer()` and to the `format` enum in the
  schema, and say in the README what it survives that the others do not.
* Windows: this needs a named pipe in the core first, not a change here. See
  `docs/reference/plugin-lifecycle.md`. When it lands, add the Windows triple
  to `platforms` and `run.bin`, and teach `capture_common::fifo` the other end.
* Deleting old recordings: do not, unless somebody asks for it and it is opt in
  with a confirmation. A tool that removes files is `destructiveHint` and the
  core has a confirmation policy for exactly this.

## What not to do

* Do not edit `tests/transcript.jsonl` to make a failing check pass.
* Do not make `tests/records.rs` shorter by trusting the file size. Play it
  back; a file of the right length that no player opens is the bug this plugin
  exists to avoid.
