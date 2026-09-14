# Plugin lifecycle

What happens to a plugin process from the moment the core starts it to the
moment it is gone: the states it can be in, what the core may call in each, the
environment it is given, the transports it can carry media on, and the budgets
it is held to.

The protocol itself is in [plugin-protocol.md](plugin-protocol.md) and the
manifest in [plugin-manifest.md](plugin-manifest.md). This page is about the
process.

## Two shapes of instance

| Shape | Kinds | How many | Who starts it |
|---|---|---|---|
| per instance | `source`, `output`, `filter` | one per `source.add`, `output.add`, `filter.add` | the operator, through a command |
| singleton | `service`, `device`, `transition` | one per plugin, named `<plugin>-<provide>` | the core, at startup and on `plugin.add` |

A singleton has no media contract: it opens no socket, starts no pipeline and
is never asked to `start`. There is one mDNS browser per plugin, not one per
camera; one OSC bridge, not one per message; one wipe, not one per take. The
supervisor owns them, on a thread of its own, and holds them to the same
lifecycle, the same restart backoff and the same budgets as a source.

A singleton that declares no `transports` is right rather than broken: the
handshake asks for one only from a provide that carries media.

## The states

```
              +----------+  initialize ok  +---------+   start    +---------+
   spawn ---> | starting | --------------> | ready   | ---------> | running | <-----+
              +----------+                 +---------+            +---------+       |
                   |                            ^                  |   |   |        |
     timeout, bad api, exit                     |  start           |   |   | health says degraded
                   |                            |                  |   |   v        |
                   v                       +----+----+    stop     |   | +----------+
              +----------+                 | stopped | <-----------+   | | degraded |
              | failed   | <---------------+---------+                 | +----------+
              +----------+  exit, crash                                |
                   ^                       no buffers for stall_timeout|
                   |                                                   v
                   |         restart (in place or rebuild)        +---------+
                   +--------------------------------------------- | stalled |
                             3 free, then 30 s doubling to 300 s, +---------+
                             cleared on first frame or on removal
```

`configure` never changes the state. `shutdown` is legal from every state and
leads to the process exiting.

A singleton never leaves `ready`, because `start` belongs to media and it has
none. That is why `render` is legal in `ready` as well as in `running`: a
transition is asked for its curves from the only state it is ever in.

There is one more value `event/plugin.state` can carry, `over-budget`. It is not
a state in the diagram: it is a report about a running instance, and what
happens next is the `on_over_budget` policy below.

## What the core may call, per state

| State | The core may call | The plugin may send |
|---|---|---|
| `starting` | nothing; it is waiting for `initialize` | `initialize` |
| `ready` | `configure`, `start`, `health`, `discover`, `render`, `tool.call`, `shutdown` | `log`, `event`, core methods over `GMX_RPC` |
| `running` | `configure`, `stop`, `health`, `seek`, `position`, `keyframe`, `audio.set`, `render`, `tool.call`, `shutdown` | `log`, `event`, `media.report`, `health.changed` |
| `stalled`, `degraded` | as `running` | as `running` |
| `stopped` | `configure`, `start`, `health`, `shutdown` | `log` |
| `failed` | nothing; the supervisor decides what happens | nothing is read |

A call made in the wrong state is refused with `-32001` and a message naming the
state and the event to wait for, rather than being written into a pipe nobody is
reading.

`configure` before `start` is legal and is how the first `params` arrive after
the handshake when they change before a source goes live.

## Services, devices and transitions

### A service

Control plane only. It gets `GMX_RPC`, the WebSocket URL of the core's own
`/rpc`, and a token scoped to itself, and calls core methods like any other
client. An AI director, a scheduler, a tally sender and an OSC bridge are all
this kind, and none of them needs a frame.

Its `[[tools]]` are reachable over `tool.call`:

```bash
gmx call tool.call '{"name": "director/pick_shot", "arguments": {"hint": "wide"}}'
```

The name is `<plugin>/<tool>`, or the bare tool name when only one plugin has
it, or `<plugin>/<provide>/<tool>` when a plugin has two instances that both
answer and you mean a particular one. Tools are declared once per plugin, so
the service takes the call by default.

### A device

Finds things the core could add as sources, and says when they arrive and
leave. Two ways, and a device may use either or both:

* **Polled.** `device.discover` fans out over every device and merges what they
  answer, each candidate's `params` ready for `source.add`. Devices share the
  timeout, which is capped at 4.5 seconds so the call stays inside the five
  second ceiling every method is held to.
* **Pushed.** The plugin raises an `event` when something turns up, and the
  core adds a source for it without anybody asking:

```json
{"jsonrpc": "2.0", "method": "event", "params": {
  "name": "source.appeared",
  "params": {"id": "guest", "type": "ingest/rtmp", "name": "live/guest",
             "params": {"uri": "rtmp://127.0.0.1:1936/live/guest"}}}}
```

and, when it goes:

```json
{"jsonrpc": "2.0", "method": "event", "params": {
  "name": "source.gone", "params": {"id": "guest"}}}
```

A plugin that names its event after itself (`ingest.publisher`) and says which
half it means in an `action` field (`connected`, `left`) is read the same way,
because that is the shape an event stream takes when one name carries a whole
lifecycle.

The `id` is the device's own, if it gives one, so its tools can find what it
asked for; otherwise the core makes a slug of the name and avoids collisions
with a numeric suffix, exactly as `source.add` does for a person. Only a source
a device added can be taken away by one: an operator's own camera is not a
device's to remove.

The core drains what a plugin says four times a second. Measured with the test
fixture, a publisher arriving became a live source in **303 ms**, against the
five seconds the roadmap asks for.

### A transition

Asked for its curves once per frame of a take, before the take starts. See
[transitions](transitions.md) for the `render` contract and what the core does
with the answers.

## Reloading

`plugin.reload` reads the plugin's directory again and then swaps its running
instances, one at a time:

1. The new instance is built from the fresh launch plan **before** the old one
   is stopped, so a plugin whose new version will not even launch never takes
   the old one down.
2. The old one is stopped and gone.
3. The new one starts and hands shakes.
4. If it will not, the previous version's launch plan is built again and
   started, and the call answers `-32001` naming the instance that failed and
   saying the previous one is running. A bad reload is a no change, not an
   outage.

One at a time so that a plugin with a service and a device keeps the other one
answering while the first is replaced. The picture is covered throughout: a
singleton draws nothing, and a source belonging to the same plugin keeps its
slot and its last frame under the mixer's own freeze frame.

`configure` answering `{applied: false, restart_required: true}` is the
documented reason to call it; error `-32012` says so by name.

## Starting up

1. The core builds the command line from `[run]`, picking the one key that
   applies to this platform.
2. It spawns the process in its own process group, with stdin piped (the
   control channel in), stdout piped (media, in container mode) and stderr
   piped (the control channel out). The working directory is the plugin's own
   root, so a relative path in your code means what you meant.
3. It reads stderr line by line. Every line that is a JSON-RPC object is a
   protocol message; every line that is not goes to the core's log at `info`,
   tagged with the instance.
4. The plugin sends `initialize`. It has five seconds. A process that has not
   sent it by then is killed and the reason reaches `event/plugin.state`.
5. The core checks the `api` number against its own range, picks a transport
   from the ones the plugin declared, creates the media address if the
   transport needs one, and answers.
6. The plugin sends `initialized`. The instance is `ready`.

A handshake that fails is the one case where a plugin is killed rather than
restarted: a process that cannot say hello will not say it on the second try
either.

## The environment

Every plugin process is given these, on top of everything the core inherited:

| Variable | What it is |
|---|---|
| `GMX_PLUGIN` | the plugin's name, `ndi` |
| `GMX_PROVIDE` | the provide id within it, `source` |
| `GMX_INSTANCE` | the instance id, `cam1`. A device or service singleton is named after its provide |
| `GMX_API_LEVEL` | the core's `api_level` |
| `GMX_PLUGIN_ROOT` | the absolute path of the plugin's directory |
| `GMX_TOKEN` | a token scoped `plugin:<name>`, for this instance and its tools |
| `GMX_RPC` | the WebSocket URL of the core's `/rpc`, for a plugin that calls core methods outside its stdio channel. Empty on an embedded core with no server |
| `GMX_MEDIA` | after the handshake, the socket or FIFO address; empty in container mode |

Three more are read by the core rather than set by it, when they are present:
`GMX_PYTHON` and `GMX_NODE` name an interpreter to use instead of the one on
PATH, and `GMX_TEMPLATES` tells `gmx plugin new` where the templates are.

## Runtimes

One key of `[run]` wins per platform.

| Key | What `gmx plugin add` does | What the core runs |
|---|---|---|
| `bin` | verifies the file exists for this platform and keeps its executable bit | the binary, argv exactly as declared, no extra arguments |
| `python` | makes `.venv` under the plugin directory with `uv venv` if `uv` is there, else `python -m venv`; installs `pyproject.toml` or `requirements.txt` | `.venv/bin/python <entry>`, or `python3` when there is no venv |
| `node` | `npm ci --omit=dev` when there is a lockfile | `node <entry>` |
| `shell` | keeps the executable bit | `sh <entry>` on Unix; refused on Windows unless there is a `bin` entry too |
| `[build]` | git sources only: runs `build.command` and expects `build.output` | as `bin` |

A plugin with no asset for this machine is refused by name, with the platforms
it does ship listed.

## Transports

Negotiated at the handshake from the list in the provide's manifest, in the
core's order of preference. A transport this build cannot open is skipped rather
than chosen and then failed.

| Transport | Elements | Cost | Where |
|---|---|---|---|
| `unixfd` | `unixfdsink` in the plugin, `unixfdsrc` in the core | zero copy | Linux and macOS, with `gst-plugins-bad` |
| `shm` | `shmsink` and `shmsrc` | one copy a frame | Linux and macOS, with `gst-plugins-bad` |
| `container` | a pipe: `fdsrc` on Unix, a reader thread and `appsrc` on Windows, into `decodebin` | a demux, and a decode if you encoded | everywhere |

Windows gets `container`. A plugin that declares only `unixfd` and `shm` is
refused there with a message naming `container` as the way forward, rather than
failing obscurely at start.

The socket transports carry one stream each, so a plugin with both video and
audio is given a base address and uses `<base>.video` and `<base>.audio`. The
base is in `GMX_MEDIA` and in the handshake answer's `media`.

An output's media goes the other way, and stdin is already the control channel:
a pipe carries bytes in one direction, and JSON lines and a Matroska stream
cannot share one. So an output reads the programme from the address in
`GMX_MEDIA`, which for the container transport is a FIFO the core makes beside
the instance's sockets. That makes sidecar outputs Unix only for now; a first
party output works everywhere.

Everything an instance is given lives in one directory under the core's runtime
directory, and that directory is removed when the instance goes. That is what
makes `plugin.add` then `plugin.remove` leave no sockets behind.

## Framing

* UTF-8, one JSON object per line, terminated by `\n`.
* At most 4 MiB a line. A longer one is error `-32011`, the channel closes and
  the instance goes to `failed`. Large data goes on a media transport or in a
  file, never on the control channel.
* Both directions may have several requests in flight. Ids are per direction:
  the core's ids and the plugin's ids are separate spaces and may collide
  without ambiguity.
* Keep answering `health` while a slow `start` or `configure` is pending. It is
  how the supervisor tells a slow plugin from a dead one.
* A line on stderr that is not a JSON object is logged at `info`, tagged with
  the instance. Print freely.

## Health

Polled once a second for a plugin that declared the `health` capability, with a
900 millisecond deadline: a plugin that cannot answer inside that is the thing
the poll is looking for. A plugin that did not declare it is judged on its
buffers alone, and is never asked.

What the plugin says is combined with what the core observes. A plugin that
thinks it is fine and is producing nothing is not fine.

`configure_log` arrives as a notification when an operator moves a log level
with `log.set {instance, level}`, so the plugin's own output follows the level
the operator asked for rather than only the core's view of it.

## Restarting

Three restarts are free. After that the wait is 30 seconds, doubling to a
ceiling of 300. The count is cleared on the first frame after a restart and when
the instance is removed, so a source that comes back and works is not punished
for having failed an hour ago.

What a restart does depends on what the plugin declared:

* With `restart-in-place`, the pipeline stays and the process behind it is
  replaced. Quicker, and what a plugin should declare if it can.
* Without it, the whole source is rebuilt from nothing. Slower and always
  works.

The freeze frame covers the gap either way. The programme's frame interval must
never exceed 34 milliseconds while it happens, and the harness measures exactly
that.

## Stopping

`stop`, then `shutdown`, then the process group is killed after eight seconds.
Every step is allowed to fail; the last one exists because the others can.

The kill is to the process group, not to the process, because a plugin that
started a helper leaves it orphaned otherwise. When the core is PID 1, as it is
in a container, it also reaps orphans once a second so that a plugin that leaks
children cannot fill the process table.

After a stop: no child processes, no open descriptors, no sockets, no temporary
directories. There is a test that counts each of those before and after, for a
source and for a singleton.

The singleton half is worth spelling out because it was the half that leaked. A
per instance id is whatever the operator called it and a singleton's is
`<plugin>-<provide>`, which does not begin with the plugin's name, so a removal
that pruned the registry by name prefix left the singleton's pid and its budget
watch behind. `plugin.remove` now stops every instance before the directory
goes and prunes by the rows themselves.

## Budgets

Optional, per plugin, in the operator's config:

    [plugins.ndi]
    max_rss_mb = 512
    max_cpu_percent = 60
    on_over_budget = "restart"     # or "disable", or "alert"

The sampler reads every instance's cpu and resident size once a second, cheaply:
`/proc` on Linux, one `ps` for the whole set on other Unixes, `tasklist` on
Windows, which reports memory only and leaves cpu blank rather than inventing
one.

A breach has to hold for three consecutive samples before it counts. On a
breach the core logs it, emits `event/plugin.state {state: "over-budget"}` with
the number and the limit in the message, and does what the policy says, to that
instance alone:

* `restart`: stop and start it. The freeze frame covers the gap.
* `disable`: stop it and leave it stopped. The show carries on without it.
* `alert`: say so and do nothing. The default, because a programme that keeps
  running is the safe state.

The core never restarts itself for a plugin's breach.

## The numbers

`plugin.list` and `plugin.stats` carry, per instance: `cpu_percent`,
`rss_bytes`, `media_latency_ms`, `buffers_dropped` and `restarts`, refreshed
every second and read from a table rather than measured inside the call.

## Errors

| Code | Meaning | Retryable |
|---|---|---|
| -32001 | not in a state that allows this | yes, after the named event |
| -32004 | no such plugin, provide or instance | no |
| -32005 | the plugin did not declare that placement | no |
| -32010 | the plugin died during the call | yes, once the supervisor restarts it |
| -32011 | a line was over 4 MiB | no |
| -32012 | `configure` needs a restart; call `plugin.reload` | no |

Every message names the current state and the next step. A call that times out
is indeterminate, never failed: the work is not cancelled, and reading the state
back is always better than assuming.
