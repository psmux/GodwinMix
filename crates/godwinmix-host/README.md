# godwinmix-host

The tier 2 plugin host. Nothing is written yet: this crate exists so the layout
is settled before the code arrives.

A tier 2 plugin is a separate process that the core starts and supervises. It
speaks the same JSON-RPC control protocol as every other client, over a pipe
rather than a socket, and it hands media across the process boundary through a
transport the two agree on at handshake. A `SIGKILL` on a plugin costs one
source and never a programme frame.

## What lands here

In the order 03 section 5 builds it:

* `manifest`: reading and validating `gmx-plugin.toml`, and the capability set
  a plugin declares.
* `handshake`: the version and capability exchange that picks a transport and
  refuses a plugin built against an incompatible `api_level`.
* `transport`: unixfd on Linux and macOS, a container on a pipe everywhere,
  which is the fallback that always works.
* `loader`: start, supervise, restart with backoff, hold to the RSS and CPU
  budget in `[plugins.<name>]`, and kill.

## Why it is a crate of its own

Two reasons, both from 09 section 4 item 2. The engine has to be embeddable
without a plugin loader linked in, so the loader cannot live in
`godwinmix-core`. And a plugin author writing their own host process wants this
without the engine, so it cannot live in the `godwinmix` binary either.

It depends on `godwinmix-protocol` and nothing else so far.

Licensed under Apache-2.0.
