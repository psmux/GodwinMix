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

## Start with `agent_state`

`agent_state` is the document written for a model: a few hundred tokens,
whatever the mixer is doing.

```json
{
  "program": "cam1",
  "uptime_secs": 5412,
  "sources": [
    {"id": "cam1", "name": "Stage", "state": "live", "superimposed": false,
     "has_video": true, "has_audio": true, "video_idle_ms": 0, "motion": 0.31},
    {"id": "score", "name": "Scoreboard", "state": "live", "superimposed": true,
     "has_video": true, "has_audio": false, "video_idle_ms": 0, "motion": 0.0},
    {"id": "guest", "name": "Guest", "state": "connecting", "superimposed": false,
     "has_video": false, "has_audio": false, "video_idle_ms": 0, "motion": 0.0}
  ],
  "outputs": [{"id": "youtube", "state": "live", "reconnects": 1}],
  "program_motion": 0.31,
  "backend": "nvidia",
  "snapshots": {"sheet": "/api/snapshot/sheet.jpg", "program": "/api/snapshot/program.jpg"}
}
```

`program` is what is on air, or `null` for the slate. Only a source whose
`state` is `live` can be taken. `motion` is 0.0 to 1.0 and says how much the
picture changed between the last two frames: a slide sits near 0, a talking
head around 0.1 to 0.3, a panning camera higher. That is how an agent tells a
live camera from a frozen or black one without looking at a picture, at about
a 75th of the cost of a frame.

Reach for `snapshot` when a number is not enough. `{"id": "sheet", "width":
640}` is every source and the programme in one image; `{"id": "cam1"}` is one
cell. Smaller is much cheaper to look at, and 320 across is enough to answer
"is anyone in the shot".

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
tokens outright. Give the agent the rehearsal token and it cannot go on air by
accident; give it the live one and nothing else changes.

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

* `docs/how-to/control-the-mixer.md`: the HTTP and WebSocket surface underneath.
* `docs/agents.md`: the longer walk through a director loop, with a worked example.
* `protocol.md`: every method, event and type.
