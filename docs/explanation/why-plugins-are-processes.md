# Why plugins are processes

The plugin model being built puts a plugin in its own process by default,
talking to the core over a pipe, with its media crossing a shared memory or
file descriptor transport. The obvious alternative, a shared library loaded
into the core, is what OBS does and it is not what this project is doing.

The reason is one sentence: a plugin that can crash the core can stop the
broadcast, and the broadcast not stopping is the only promise this project
makes that nothing else does.

## What a loaded library costs

A shared library in the host process shares everything with it: the heap, the
signal handlers, the streaming threads, the process's life. That buys speed and
it buys a specific set of failures.

* A segfault in a plugin is a segfault in the mixer. Mid service, mid match.
* A plugin that blocks on a network read inside a GStreamer streaming thread
  stalls the pipeline that thread belongs to. If that is the programme
  pipeline, the encoder misses its deadline and viewers see it.
* A plugin built against a different version of a C++ ABI, or a different
  allocator, corrupts memory in ways that surface an hour later somewhere else.
* A plugin cannot be written in Python, or Go, or anything that will not
  produce a shared library with a C ABI. That rules out most of the people who
  would write one.
* Upgrading the core breaks every plugin at once, every major release, which is
  the complaint the research turned up about every plugin ecosystem in this
  space.

The OBS record is the evidence, not a hypothesis. A plugin crash there is an
OBS crash, and "disable your plugins" is the first line of every support thread.

## What a process costs

Honesty first: it is not free.

* A bare GStreamer process costs about 41 MB of memory before it does anything.
  Every sidecar plugin inherits that floor, and a Python one carries Python on
  top.
* Media has to cross a boundary. On Linux and macOS that is `unixfd`, which
  passes file descriptors and copies nothing. On Windows it is a pipe with a
  copy, which at 720p is about 41 MB/s.
* There is a handshake, a lifecycle and a supervisor to write, and they have to
  be right, because a plugin that dies must be restarted without a gap.

That is the bill. It is paid once, in the core, by people who are paid in
attention rather than money, and it buys the following.

## What a process buys

* **A crash costs one source.** `SIGKILL` on any plugin never stops the core
  and never drops a programme frame. The source it was feeding freezes on its
  last frame, the supervisor rebuilds it, and the programme carries on. This is
  tested on every CI run rather than asserted in a document.
* **Any language.** The protocol is JSON-RPC 2.0, newline framed, on stdin and
  stdout, UTF-8. A source plugin in under 200 lines of Python importing nothing
  outside the standard library is the test of whether that stayed true. If that
  example ever needs a dependency, the protocol has drifted.
* **A budget per plugin.** A process has an RSS and a CPU share you can read
  from outside it. `max_rss_mb` and `max_cpu_percent` per plugin become
  enforceable: on breach the core logs, emits an event, and restarts or
  disables that plugin alone, never itself.
* **The same plugin in three places.** In the core's process, beside it, or on
  another machine, without being rewritten. Once the boundary is a protocol
  rather than a function call, where the other side runs is a configuration
  line. A shared library can never be moved to another host.
* **Bisecting.** A bad plugin among 32 is found in six steps, because each one
  can be disabled independently and the core survives the experiment.

## The tiers

Not everything pays the full price. The model has three placements and the
plugin does not choose which one it gets:

1. **In process.** First party Rust code implementing the trait directly. No
   boundary, no copy, and the plugin is as trusted as the core.
2. **Sidecar.** Its own process, on this machine, media over `unixfd` where
   the platform has it and a container on a pipe where it does not. This is the
   default for anything third party.
3. **Remote.** Another machine, media over SRT or RTP, control over a
   WebSocket bridge.

The same code runs in all three. The operator moves it with a line of config
when they find out where it belongs.

## Where the boundary is not

Nothing in this arrangement is allowed to put a plugin in the path of the
programme's own frames. A source plugin feeds a source pipeline, which is
already isolated behind a proxy boundary. A plugin cannot block the compositor,
because the compositor is `force-live` and produces on schedule from whatever
its pads currently say. The design does not depend on plugins behaving.

That is the distinction worth keeping in mind: isolation here is not about
trusting the plugin author less. It is about a broadcast being a thing that
cannot be paused and tried again.

## Further reading

* [Why the programme never stops](why-the-programme-never-stops.md), which is
  the constraint this follows from.
* [Your first plugin](../tutorials/your-first-plugin.md) for what exists today
  and what does not.
