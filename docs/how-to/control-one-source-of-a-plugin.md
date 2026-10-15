# Control one source of a plugin

A source plugin runs one process per source. A playlist plugin with three
playlists on air has three processes, and "skip" has to say which playlist.
`tool.call` takes the source id for that:

```sh
curl -s -X POST http://127.0.0.1:8080/api/v1/tool/call \
  -H "Authorization: Bearer $TOKEN" -H 'content-type: application/json' \
  -d '{"name": "playout/next", "source": "lobby", "arguments": {"source": "lobby"}}'
```

The call goes to the process behind `lobby` and nowhere else, and the answer
is that process's, in MCP's shape (`content`, `structuredContent`). Over
`/rpc` it is the same method with the same params:

```json
{"jsonrpc": "2.0", "id": 7, "method": "tool.call",
 "params": {"name": "playout/state", "source": "lobby", "arguments": {"source": "lobby"}}}
```

A panel calls it the same way, with `client.call("tool.call", {...})`.

## From an agent or the CLI

An MCP client sends a tool's arguments and nothing else, so it cannot fill in
a top level `source`. A plugin whose source tools take `source` among their
arguments, as the playout tools do, is reached anyway: when no service or
device of any plugin has the tool, the core sends the call to the source
named in `arguments.source`. The MCP server lists every installed plugin's
tools as `gmx_<plugin>_<tool>`, so from a shell:

```sh
gmx tool gmx_playout_next '{"source": "lobby"}'
```

## When it is refused

| Refusal | Code | Why |
|---|---|---|
| `no source called nope` | -32004 | the id is wrong; the message lists the sources there are |
| `source cam1 is a builtin source, not a playout one` | -32602 | the tool's plugin and the source's plugin differ; `data.plugin` names the source's |
| `source lobby did not answer next` | -32001 | the process is not running or refused; the message carries its reason, and `source.get` says whether it is up |

A plugin's own token (`GMX_TOKEN`, scoped `plugin:<name>`) reaches its own
sources' tools and no other plugin's.

See [plugin lifecycle](../reference/plugin-lifecycle.md#a-sources-tools) for
where this sits among the other ways a plugin is called.
