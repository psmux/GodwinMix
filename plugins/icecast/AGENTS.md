# AGENTS.md

For a coding agent changing this plugin.

`icecast/output` (`src/send.rs`, `src/handlers.rs`) and `icecast/source`
(`src/radio.rs`, `src/station.rs`). One binary; `GMX_PROVIDE` says which.

* The output is the one place that encodes sound, because Icecast players want
  MP3 or Ogg and the programme is AAC. Never decode the picture: it goes to a
  fakesink at the demuxer.
* The source does not decode: the station's sound crosses to the core in
  Matroska as it came.
* Choose streams by the demuxer's pad name or `query_caps`: pads can appear
  before their caps are set.

```sh
cargo test -p gmx-icecast
cargo clippy -p gmx-icecast --all-targets
```
