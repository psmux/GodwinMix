# Use the mixer from an AI agent

`godwinmix mcp` is a Model Context Protocol server over stdio. It is a thin
client for the mixer's HTTP API, not a second control path, so the agent and
the operator's browser see the same state and the same refusals.

## One command

The mixer has to be running and reachable. The MCP server is the same binary.

Claude Code:

```sh
claude mcp add godwinmix -- godwinmix mcp --url http://localhost:8080
```

Codex CLI, in `~/.codex/config.toml`:

```toml
[mcp_servers.godwinmix]
command = "godwinmix"
args = ["mcp", "--url", "http://localhost:8080"]
```

Cursor, in `.cursor/mcp.json` (Claude Desktop uses the same shape in
`claude_desktop_config.json`):

```json
{
  "mcpServers": {
    "godwinmix": {
      "command": "godwinmix",
      "args": ["mcp", "--url", "http://localhost:8080"],
      "env": { "GODWINMIX_TOKEN": "a-long-random-string" }
    }
  }
}
```

Drop the token when the control port is open. `--token` works too, but an
environment variable keeps the secret out of a file people commit.

Check it without a client:

```sh
printf '{"jsonrpc":"2.0","id":1,"method":"tools/list"}\n' | godwinmix mcp | jq '.result.tools[].name'
```

## Two profiles

A tool list is charged for on every single call, so there are two of them.

| | Tools | About |
|---|---|---|
| `--profile standard` (default) | 12 | roughly 2,600 tokens |
| `--profile minimal` | 5 | roughly 1,200 tokens, for a small context |

```sh
godwinmix mcp --profile minimal          # or GODWINMIX_MCP_PROFILE=minimal
```

`minimal` is `agent_state`, `take`, `add_source`, `list_sources` and
`search_tools`. `standard` adds `status`, `snapshot`, `remove_source`,
`revert`, `go_live`, `list_outputs` and `add_output`.

Everything else, about twenty tools covering outputs, media, ad breaks,
seeking, audio and codecs, is still callable by name. `search_tools` finds one
by what you want to do:

```
search_tools {"query": "stop sending to youtube"}
  -> remove_output, with its description and input schema, ready to call
```

The hot list never changes shape at runtime. Adding a source, a plugin or a
node does not alter it, because plugin tools live behind search. That is what
keeps a prompt cache valid: changing any tool definition invalidates the whole
cache, and a mixer that rebuilt its tools on every scene change would pay full
price on every call. A test asserts it.

A token can also carry `profile = "minimal"` in the `[[tokens]]` table, which
`core.info` reports back, so a credential can say which surface it was issued
for.

## Install the skills first

Two skills ship with the mixer. One is for running a show, one is for building
on it, and both are written for the tool that will read them.

```sh
gmx skill install --for claude          # or codex, or gemini
gmx skill install --for claude --print  # see what it would write first
```

`godwinmix-operate` covers the state document, the take, the safety rules that
will refuse you, what a look costs and what to do when something is wrong.
`godwinmix-develop` covers the manifest, the contract and the test loop.

## Start with `agent_state`

`agent_state` is the document written for a model: a few hundred bytes,
whatever the mixer is doing.

```json
{
  "program": "cam1",
  "program_motion": 0.31,
  "uptime_secs": 5412,
  "sources": [
    {"id": "cam1", "name": "Stage", "state": "live", "motion": 0.31},
    {"id": "score", "name": "Scoreboard", "state": "live", "superimposed": true,
     "no_audio": true, "motion": 0.0},
    {"id": "guest", "state": "connecting", "no_video": true, "no_audio": true}
  ],
  "outputs": [{"id": "youtube", "state": "live", "reconnects": 1}],
  "snapshot": "/api/v1/snapshot/{id}"
}
```

`program` is what is on air, or `null` for the slate. Only a source whose
`state` is `live` can be taken. `motion` is 0.0 to 1.0 and says how much the
picture changed between the last two frames: a slide sits near 0, a talking
head around 0.1 to 0.3, a panning camera higher. That is how an agent tells a
live camera from a frozen or black one without looking at a picture, at about
a 75th of the cost of a frame.

A source that is working says nothing about its video or its sound. One that
has lost either says `no_video` or `no_audio`. That is what keeps sixteen
sources inside 1,200 bytes.

`agent_state {"response_format": "detailed"}` adds the audio peak per source,
the encoder in use, the last five takes, the safety limits in force and the
current telemetry reading. Ask for it when something is wrong, not every time.
The whole document, both formats and their sizes, is in
[`docs/reference/agent-state.md`](../reference/agent-state.md).

## Numbers every tick, pictures on demand

`event/telemetry` carries, up to ten times a second and in under 200 bytes, the
shot change score, the black ratio, a freeze flag, short term and integrated
loudness, a silence flag and which sources are live. It is computed by cheap
probes on the raw programme frames and it runs only while somebody is
subscribed.

Over `/rpc`:

```json
{"method": "core.subscribe",
 "params": {"ext": {"telemetry": {"hz": 2},
                    "agent": {"black": 0.98, "freeze_ms": 200, "silence_ms": 500}}}}
```

`ext.agent` pushes the whole `agent_state` document with a snapshot URL when a
threshold crosses or a take lands, so an agent stops polling. Over MCP the same
push arrives on its own connection as `notifications/gmx/agent.state`, on stdio
and on Streamable HTTP; nothing has to be subscribed to.

```sh
gmx mcp --http 127.0.0.1:8765     # POST /mcp for calls, GET /mcp for the push
```

### What each of them costs

```sh
gmx agent cost
```

| What | Bytes | Tokens |
|---|---|---|
| `agent_state` at 2 sources | 228 | 57 |
| `agent_state` at 6 sources | 396 | 99 |
| `agent_state` at 16 sources | 823 | 206 |
| `event/telemetry` at 8 sources | under 200 | about 50 |
| a snapshot at 320 by 180 | about 9 kB | 84 |
| a snapshot at 640 by 360 | about 15 kB | 299 |
| a snapshot at 1280 by 720 | about 40 kB | 1,196 |

One look at 1280 costs about twelve reads of the state document. At a look
every thirty seconds a three hour show is 360 looks: about 430,000 tokens at
1280 wide, or 30,000 at 320.

Look when a number tells you to, not on a timer. Presence questions ("is
anyone in the shot") survive the smallest size with under five percent loss;
reading a lower third does not, and 1280 is for those and nothing else.

## The rules that will refuse a take

The core holds every caller, human or agent, to a minimum shot length, a rate
limit and the ITU-R BT.1702-3 flash guard, and it watches the token that made
the last take for silence. A refusal is `-32003` with `data.retry_after_ms`
and a message that says how long is left.

```
take {"source": "cam2"}
  -> the shot on air has been up for 1200 ms and this core holds a shot for
     8000 ms, so there are 6800 ms left. Wait 6800 ms and take again, or use a
     token whose safety.min_hold_ms is lower.
     {"rule":"min_hold","retry_after_ms":6800,"retryable":true}
```

An agent's token may make those numbers harder and never easier, so an agent
told to raise its own limits finds that it cannot. The whole table is in
[`docs/reference/safety.md`](../reference/safety.md).

`revert` undoes the last take and is not held by the minimum hold, because a
revert that has to wait out the hold is not a revert.

## Calls that take longer than five seconds

No method blocks for more than five seconds. One whose work would answers at
once with `{task_id, poll_interval_ms}` and the work carries on; `task_get`
reads the outcome. A call that timed out is **indeterminate**, never failed.
See [`docs/reference/tasks.md`](../reference/tasks.md).

## What the annotations mean

Every tool declares `readOnlyHint`, `destructiveHint` and `idempotentHint`,
and the server enforces the same properties independently. They are generated
from the flags the server checks, so they cannot be a lie.

* `readOnlyHint: true` means the call cannot change anything. The server
  refuses a mutating call from a read only token before it runs.
* `destructiveHint: true` means it accepts `dry_run: true`, which answers the
  diff it would make against the live state and changes nothing. On a token
  with `confirm = "required"` it is refused once with a confirm token that
  the repeat has to carry.
* `idempotentHint: true` means calling it twice with the same arguments leaves
  the same state.

Every mutating tool accepts `idempotency_key`. The answer is kept for 24 hours
and a retry returns it with `replayed: true`, so a call that timed out can be
sent again without doubling a take or a source. Use it: eight identical runs
succeed far less often than one.

## Rehearsal

An agent behaves differently when it believes a show is real, and it guesses
wrong most of the time, so the guess must not matter. The credential decides.

```sh
godwinmix --rehearsal --config rehearsal.toml
```

A rehearsal core refuses `output.add`, so nothing reaches a real destination,
and accepts only tokens marked `rehearsal = true`. A live core refuses those
tokens outright, with a message naming the fix. Give the agent the rehearsal
token and it cannot go on air by accident; give it the live one and nothing
else changes. The agent is never told which kind of core it is on, and does
not need to be.

The refusal is the same shape as every other, so a rehearsal that works is a
live show that works:

```
add_output {"id": "yt", "url": "rtmp://…"}
  -> this core was started with --rehearsal and will not add an output, so
     nothing here reaches a real destination. Everything else works. Start a
     core without --rehearsal to go on air.
```

## What refusals look like

Every refusal names the current state and the next step, and comes back as a
tool result rather than a protocol error, so the agent reads it and tries
something else:

```
take {"source": "cam9"}
  -> there is no source 'cam9'. The only source: camera-1. Use one of those.
     {"id":"cam9","kind":"source","valid":["camera-1"],"retryable":false}
```

The `data` object rides along, so `valid` can be acted on without parsing
English.

## What an agent must not do

* Do not poll `status` in a loop. `agent_state` is the cheap read, and a
  client that wants to watch opens `/rpc` and subscribes.
* Do not take a source whose `state` is not `live`. It will be refused, and
  the refusal names what to wait for.
* Do not use `remove_source` on the source that is on air without checking
  first. It cuts the programme to the slate. Ask it with `dry_run` first.
* Do not invent ids. Every unknown id is answered with the ids that exist.

## See also

* [`docs/reference/agent-state.md`](../reference/agent-state.md): both formats, every field, the sizes.
* [`docs/reference/safety.md`](../reference/safety.md): the minimum hold, the rate limit, the flash guard, the operator watchdog.
* [`docs/reference/tasks.md`](../reference/tasks.md): `task.get`, `task.cancel` and the handle pattern.
* `docs/how-to/control-the-mixer.md`: the HTTP and WebSocket surface underneath.
* `docs/agents.md`: the longer walk through a director loop, with a worked example.
* `protocol.md`: every method, event and type.
