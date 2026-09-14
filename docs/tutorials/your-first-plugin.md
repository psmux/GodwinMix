# Your first plugin

**This tutorial cannot be followed yet.** The command it is built around,
`gmx plugin new`, does not exist. This page is here so that the shape of the
path is public while it is being built, and so that the promise it makes can be
argued with before it ships rather than after.

What exists today, and what you can build against right now, is at the bottom.

## What it will be

One command to a plugin that works, then edit it into the plugin you wanted:

```sh
gmx plugin new --kind source --lang python my-cam
gmx plugin test ./my-cam          # the conformance harness, 7 checks
gmx plugin add ./my-cam
gmx source add cam "my-cam/source"
```

At that point colour bars from the template are on the multiview. Then you open
`main.py`, change it to read the camera you actually care about, and
`gmx plugin reload my-cam`.

The target is under 15 minutes from nothing to a source on the multiview for
the Python template and under 30 for Rust, measured by running it in CI on a
clean container and printing the measured time on this page. No plugin
ecosystem publishes that number today. Printing one that is not measured would
be worse than printing none, so this page will carry the number when the CI job
that produces it exists.

Five languages are planned for templates: Rust, Python, Node, Go and shell.
Each template carries a manifest, a stub that emits colour bars at the canvas's
caps, a settings schema, a `SKILL.md` and an `AGENTS.md` for a coding agent
extending it, a CI file that runs the harness, and a README that says what to
change.

## The rules it will be built to

These are settled, and they constrain everything above.

* **The protocol is implementable from a standard library.** JSON-RPC 2.0,
  newline framed, on stdin and stdout, UTF-8. `examples/zero-dep-source.py` will
  be a source plugin in under 200 lines importing nothing outside Python's
  standard library. If that example ever needs a dependency, the protocol has
  drifted and the protocol is wrong.
* **A crash costs one source.** `SIGKILL` on any plugin never stops the core
  and never drops a programme frame. The harness proves it on every plugin on
  every CI run, rather than asserting it in a document.
* **A plugin carries its own cost, visibly.** `gmx plugin test` prints the
  plugin's CPU and memory and the core's added cost for the transport it chose,
  on the machine it ran on.
* **The same plugin runs in three places.** In the core's process, as a sidecar
  beside it, or on another machine, without being rewritten. Where it runs is a
  line of configuration, not a port.

## What you can build against today

The plugin loader does not exist, but two extension points do, and both are
stable enough to build on.

### An `exec:` source

Any process that writes a container to its stdout is a source. MPEG-TS is the
usual choice. The mixer demuxes and decodes it through the same hardware aware
path as everything else, so an `exec:` source is GPU accelerated on a machine
with a GPU and falls back to software on one without, with no change to the
command.

```toml
[security]
allow_exec_sources = true   # off by default: this is arbitrary code execution
```

```sh
gmx ctl source add bars 'exec:gst-launch-1.0 -q videotestsrc ! x264enc ! mpegtsmux ! fdsink'
```

[docs/reference/sources.md](../reference/sources.md) has the detail, including
what the mixer does when your process dies (it restarts it, with backoff) and
what it does with the process group (kills it, so nothing is orphaned).

### The browser sidecar protocol

`godwinmix-browser` is a separate process that hands raw frames and PCM to the
mixer over a pipe, configured under `[browser]`. It is the prototype the plugin
protocol is being generalised from, and it is documented in
[docs/reference/web-page-sources.md](../reference/web-page-sources.md).

## Tell us where you got stuck

If you came to this page expecting to write a plugin and left without one, that
is worth recording. Add it to [the friction log](../friction-log.md) or open an
issue. The measured time to first plugin is the score this project keeps, and a
stuck moment nobody wrote down cannot be fixed.
