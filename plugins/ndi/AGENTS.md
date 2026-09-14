# AGENTS.md

For a coding agent changing this plugin. Read this before editing anything.

## What this is

Three provides in one process image. `GMX_PROVIDE` says which this process is,
and `main` picks the handler from it:

| Provide | Kind | Module |
|---|---|---|
| `ndi/source` | source | `src/media.rs` (`Receiver`) |
| `ndi/output` | output | `src/media.rs` (`Announcer`) |
| `ndi/discover` | device | `src/senders.rs`, plus `list_senders` in `src/main.rs` |

`src/library.rs` is the part that matters most: it `dlopen`s the NDI runtime to
find out whether it is there.

## Build and test

```sh
cargo test -p gmx-ndi                        # says so and skips where NDI is absent
cargo clippy -p gmx-ndi --all-targets        # add no warning that was not there
dev/harness/stage-plugins.sh
gmx plugin test plugins/ndi --offline --provide discover
plugins/ndi/bin/gmx-ndi                      # run by hand: says what it can see
```

## The rules that matter, in order

1. **Never link NDI.** Not a build script, not a `-sys` crate, not a feature
   flag. The runtime's licence forbids redistribution and the trademark belongs
   to Vizrt. This plugin must build and run on a machine that has never seen
   NDI. `libloading` is in `Cargo.toml` for exactly this and for nothing else.
2. **Keep the attribution.** `README.md` carries the trademark notice and the
   download page. Both stay.
3. **An absent runtime is a refusal, not a crash.** `library::missing()` is the
   one message, it names the download page and the reason it is not shipped, and
   every path that needs the runtime goes through `check_ready_for_ndi`. The
   device provide is the exception and stays up: it answers `discover` with an
   empty list, because a machine with no NDI on it is not broken.
4. **A test that cannot run says so and skips.** A check that cannot run is not
   a check that passed. Every test here prints a line before returning early.
5. **stdout is media** in `ndi/source`. A `println!` corrupts the stream. Log
   through the `Reporter`, which writes to stderr. `main` printing to stderr
   when the plugin is run by hand is deliberate and is the only exception.
6. **An output READS its media.** `start.params.media` is a FIFO the core has
   already begun muxing the programme into.
7. **Never block a streaming thread or the bus handler.** The bus watch is in
   `netkit::pipe` on its own thread.
8. **Use the device provider, not an mDNS client.** GStreamer's NDI plugin
   already listens for `_ndi._tcp`. A second mDNS responder would fight Avahi or
   Bonjour for the port, and would be a second implementation of the same
   discovery to keep correct.

## What is deliberately not here

* No `unixfd` transport. It would remove the copy per frame and it is the
  obvious next step, but it cannot be tested on a machine with no NDI runtime,
  and an untested transport is worse than one memcpy.
* No NDI receiver written against the C API. GStreamer's `ndisrc` and `ndisink`
  already do it, and they do the SpeedHQ decode which is the only part that
  actually needs the runtime.
* No re-encode on the output. It decodes, because `ndisink` wants raw and
  nothing on the wire is in a form NDI takes. That decode is the protocol's
  price and the README says so.
