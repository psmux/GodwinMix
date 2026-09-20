# Test a plugin

`gmx plugin test` runs your plugin against a real core and tells you what is
wrong in the words of the thing that is wrong. It is the same suite the plugin
index runs, so passing here is passing there.

    gmx plugin test ./my-plugin

    ok   manifest               bars v0.1.0: 1 provide(s), 0 tool(s), every path and schema in place
    ok   spawn                  hello in 11 ms (the limit is 5 s), api 1, transport container
    ok   playing                reached PLAYING within the timeout
    ok   video caps             video/x-raw, format=(string)I420, width=(int)1280, height=(int)720, ...
    ok   video buffers          91 buffers, none out of order
    ok   audio buffers          not declared, not expected
    ok   stop                   the pipeline is in NULL and the kind let go
    ok   configure              14 example(s), every one answered
    ok   kill                   killed mid stream, back in 43 frames, longest interval 8.2 ms (limit 34 ms)
    ok   footprint              plugin cpu 6%, rss 48 MB; core cpu 2%, rss 210 MB (container)

    bars/source is conformant

A full run takes about a minute. Two flags change that:

    gmx plugin test ./my-plugin --quick     # about fifteen seconds
    gmx plugin test ./my-plugin --offline   # about a second, and no core at all

`--quick` skips the kill test and the footprint, the two that take time. Run it
while you are working and the full one before you publish.

`--offline` replays a recorded conversation against your binary with no core,
no sockets and no clock. It is what to put in your own CI, because it runs on
any machine in seconds.

The test core it spawns is a 1280x720x30 canvas with no outputs and no
multiview, and a supervisor whose rebuild backoff is one second so the kill test
is quick. `godwinmix --test-core` runs the same suite against the built in
kinds and against every plugin installed on the machine.

## What each check means, and what to do when it fails

### manifest

Your `gmx-plugin.toml`, every JSON Schema it points at, and every `SKILL.md`.
Failures name the key path:

    FAIL manifest    provides[0].settings: 'schemas/source.json' does not exist.

Every problem is reported at once, so you fix the whole file in one pass rather
than one error per run. This check needs no process and runs first, so a typo
costs you a second and not a minute.

### spawn

Your process starts and sends `initialize` within five seconds.

    FAIL spawn   the plugin did not send `initialize` within 5 s. It is killed.

Three usual causes. The process is not starting at all: run the command the
report prints by hand and read the error. It is buffering its stderr, so the
line is written but not flushed: flush after every line, always (`flush=True` in
Python, `process.stderr.write` then nothing in Node, `>&2` in shell is
unbuffered already). Or it is doing work before saying hello: say hello first
and open the camera afterwards, because the handshake is how the core learns you
are alive.

The report prints how long it took, so you can watch that number when you add a
dependency. Fifty milliseconds is a Python interpreter starting; five seconds is
a problem.

### playing, video caps, audio caps

Your media reaches the core at exactly the canvas contract: I420 (or AYUV with
alpha), the canvas size, the canvas frame rate, and for audio F32LE at 48 kHz
in stereo.

    FAIL video caps   wanted video/x-raw, format=(string)I420, width=(int)1280 ...
                      got video/x-raw, format=(string)I420, width=(int)1920 ...

You sent frames at your own size instead of the canvas's. The canvas is in the
handshake answer: read `canvas.width`, `canvas.height` and `canvas.fps` from it
and produce that, every time, because it can differ between mixers and can
change between runs.

A failure to reach PLAYING at all usually means the container you are writing is
not one `decodebin` opens. Streamable Matroska with raw I420 is the cheapest
thing that works; `matroskamux streamable=true` is the whole recipe.

### video buffers, audio buffers

In three seconds, at least ninety percent of the frames the canvas rate implies,
with timestamps that never go backwards. The missing tenth is the start up you
are allowed.

    FAIL video buffers   61 buffers in 3s, wanted at least 81

You are not pacing. A source must produce at the canvas rate, not as fast as it
can and not as slow as the work takes: sleep to the next frame boundary rather
than sleeping a fixed interval, or the error accumulates. The SDKs have a pacer
for exactly this.

    FAIL video buffers   3 buffers went backwards in time

Your timestamps are not monotonic. Count from your own start in nanoseconds; the
core retimes onto programme running time and does not mind where you begin, but
it cannot help if a frame claims to be earlier than the one before it.

Audio is measured in milliseconds of media rather than in buffers, because a
container is demuxed into whatever the muxer chose and a perfectly conformant
source can deliver 133 buffers where a count wanted 270.

### configure

Every example in your settings schema, sent as `configure`. The answer must be
`{applied: true}` or `{applied: false, restart_required: true, reason}`. Never a
crash.

    FAIL configure   the plugin exited while being configured.

Saying "I cannot do that while running" is correct and costs one freeze frame.
Crashing is not, and costs the operator their source.

If the check says the schema gives no examples, add `examples` to each property.
They are worth having for their own sake: a schema with examples is a schema an
agent can fill in, and it is what this check exercises.

### stop

After `stop` and `shutdown` your process exits within eight seconds and leaves
no child processes, no open descriptors and no temporary directories.

A plugin that starts a helper must kill it. A shell plugin wants
`trap 'kill $PID' EXIT INT TERM`. If yours does not exit on `shutdown`, the core
kills the whole process group after eight seconds, which works but means every
stop takes eight seconds.

### kill

The core kills your process mid stream and watches what your media does. What
has to be true is that it comes back within the restart backoff.

    ok   kill   killed mid stream, back in 43 frames, away for 380 ms at this source's own
                end (the programme's own interval is not measured here; see check_kill)

    FAIL kill   the picture did not come back within 12 s of the process being killed

The number it reports is the gap at your own media end, and it is normally a
second or so: your process was replaced, so of course your frames stopped. The
rule that matters, that the programme's frame interval never reaches 34
milliseconds, is about the picture the audience sees, and the compositor's
freeze frame is what keeps it true while you are away. This harness builds one
source with no compositor and no encoder behind it, so it cannot measure that
yet; a test core that can is the next thing to build here.

A picture that never comes back is your problem. If restarting in place does
not work for you, drop `restart-in-place` from your capabilities and the
supervisor rebuilds you from nothing instead, which is slower and always
works.

Reproduce it by hand against a running mixer with `gmx chaos kill <instance>`.

### footprint

What your plugin costs and what the core added for the transport you chose.
Nothing fails here. There is no number a screen capture at 1080p60 and a clock
that draws text once a second both belong under.

It is here so you see the figure before you publish, and so you see it change
when you add a dependency. If the core's added cpu surprises you, that is the
decode the `container` transport pays for; `unixfd` costs it nothing at all, on
Linux and macOS.

## The offline test

    gmx plugin test ./my-plugin --offline

This reads `tests/transcript.jsonl` in your plugin, writes its `core` lines to
your process's stdin, and checks its stderr against the `plugin` lines, in
order, as a subset. One JSON object per line, each with a single key:

    {"plugin": {"method": "initialize", "params": {"plugin": "bars", "api": 1}}}
    {"core":   {"jsonrpc": "2.0", "id": 0, "result": {"core": "godwinmix", ...}}}
    {"plugin": {"method": "initialized"}}

A `plugin` line matches if the real line contains everything it names; extra
keys are fine and `"*"` matches any value, which is how a timestamp or a
generated id is allowed to vary. Lines starting `#` or `//` are comments.

Bytes in, bytes out, no sockets, no clock. `gmx plugin new` writes a starting
transcript that walks the whole handshake, and extending it as you add methods
is the cheapest test you will write.

## In your own CI

The templates ship a workflow that runs the offline test on every push, which
needs nothing but your language's runtime. Add the full harness on a runner that
has GStreamer:

    gmx plugin test . --offline      # any runner, seconds
    gmx plugin test . --quick        # a runner with GStreamer, fifteen seconds
    gmx plugin test .                # before a release, a minute

## Next

* [Install a plugin](install-a-plugin.md).
* [The plugin lifecycle](../reference/plugin-lifecycle.md).
* [Debug a show](debug-a-show.md), for when the problem is not the plugin.

## A WebAssembly plugin

A tier W plugin has no process, so the checks that are about one are not run.
What is left runs in this process against the real wasmtime host: the manifest,
the load and the handshake, every example in the settings schema through
`configure`, and the contract of whichever kind it is.

```
gmx plugin test plugins/min-hold
```

```
  ok   manifest               min-hold v0.2.0: 1 provide(s), 1 tool(s), every path and schema in place
  ok   component              loaded and hand shook in 1767 ms; tools: hold_state; hooks: take.before, take.after
  ok   media                  a service provide at the `wasm` placement carries no media, so checks 2, 3 and 6 do not apply
  ok   configure              4 settings examples were taken
  ok   service                health is ok; 2 hooks answered: take.before, take.after
```

`--offline` works the same way and replays the same transcript file, through
the component rather than down a pipe. `initialize` does not appear in a
component's transcript: a component hand shakes when it is loaded, and the
replay does that before the first line.

Both need a core built with `--features wasm`. `gmx doctor` says whether yours
is, on the `wasm host` line. See
[write a WASM plugin](write-a-wasm-plugin.md).

The Rust SDK catches unwinding panics from plugin callbacks. A panicking
`start` receives a `PLUGIN_DIED` error immediately after unwinding, with the
method name and `restart_required: true`. Cached health becomes `failing`, and
the worker refuses further callbacks against the partially updated instance.
Restart the plugin before retrying and inspect its crash report. Shutdown is
still acknowledged so the host can dispose of the failed process. Panics built
with `panic = "abort"` and process crashes instead follow the host's process
exit recovery path.
