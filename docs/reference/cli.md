# The command line


The daemon is headless and controlled entirely over HTTP. `godwinmix ctl` is a
thin client for that same API, so scripting it does not mean assembling JSON by
hand. Point it elsewhere with `--url` or `GODWINMIX_URL`, and pass `--token`
when the mixer has one. `gmx` is the same binary under a shorter name, so
`gmx ctl status` and `godwinmix ctl status` are one command.

```sh
godwinmix ctl status
godwinmix ctl take cam2                 # or: take   (with no id, cuts to black)
godwinmix ctl source add hls1 https://host/stream.m3u8 --name "Roof camera"
godwinmix ctl source remove hls1
godwinmix ctl output add youtube rtmp://a.rtmp.youtube.com/live2/KEY --policy cdn
godwinmix ctl output list
godwinmix ctl ad /srv/ads/spot.mp4 --return-to cam1
godwinmix ctl media
godwinmix ctl golive https://example.com/event/42 --rtmp rtmp://a.rtmp.youtube.com/live2/KEY --superimpose auto
```

`golive` is `POST /api/golive`: the page becomes a web source, the destination
is added if given, and the mixer takes the page to programme by itself once it
is live. It is the call a customer's backend makes behind a "Go Live" button.

`godwinmix mcp` is the same client dressed as an MCP server over stdio, for a
model rather than a shell:

```sh
godwinmix mcp --url http://127.0.0.1:8080 --token TOKEN
```

Requests answer with the mixer's own reason for refusing rather than a bare
status code:

```
$ godwinmix ctl source add cam1 rtmp://host/live/x
Error: 400 Bad Request: source cam1 already exists
```

Nothing here needs the desktop app; it is only a window onto the same API.

### Token

Set `[control] token = "..."` in the config, or `GODWINMIX_TOKEN` in the
environment, and every `/api/*` route and `/ws` demand
`Authorization: Bearer <token>`. A GET, which is what a WebSocket upgrade is,
may send `?token=<token>` instead, because a browser cannot put a header on a
socket. Missing or wrong is a 401 with a one line JSON body. With no token
configured nothing changes and the port is open, as it always was.

The UI at `/` asks for the token once when it meets a 401 and keeps it in the
browser's localStorage. `ctl` takes `--token` or reads `GODWINMIX_TOKEN`:

```sh
GODWINMIX_TOKEN=change-me godwinmix ctl status
curl -H 'Authorization: Bearer change-me' http://mixer:8080/api/status
```

### Go live in one call

`POST /api/golive` is the "Go Live" button: a customer's page asks their
backend, the backend makes one request, and the page is on air as soon as it
renders. It adds the URL as a web source (id from the host unless `id` is
given; a source that already shows that URL is reused), adds an output for
`rtmp` unless one already sends there, and takes the source to programme the
moment it is live, waiting up to a minute before it gives up with a log line.
The reply is an immediate 202.

```sh
godwinmix ctl golive https://example.com/live-game --rtmp rtmp://a.rtmp.youtube.com/live2/KEY
curl -X POST localhost:8080/api/golive -H 'content-type: application/json' \
  -H 'Authorization: Bearer change-me' \
  -d '{"url": "https://example.com/live-game", "rtmp": "rtmp://a.rtmp.youtube.com/live2/KEY"}'
# {"source":"example-com","output":"a-rtmp-youtube-com","state":"connecting"}
```

`superimpose` defaults to `"auto"` here, the opposite of `/api/sources`,
because the caller is a machine and the saving is a CPU core. Pass `"off"`
to render the whole page in the browser.


## Plugins

`gmx plugin new` and `gmx plugin test` need no running mixer. Everything else
is a thin client of the `plugin.*` methods, which is the same contract the web
UI and an agent use.

| Command | Does |
|---|---|
| `gmx plugin new <name> --kind source --lang rust\|python\|node\|go\|shell` | write a plugin from a template, with every placeholder filled in |
| `gmx plugin test <dir> [--quick] [--offline]` | run the conformance harness; `--quick` skips the two slow checks, `--offline` replays a transcript with no core |
| `gmx plugin add <dir>` | install from a local directory, while live |
| `gmx plugin update <name> <dir>` | reinstall from a directory, keeping the settings |
| `gmx plugin reload <name>` | read the directory again and swap the running instances one at a time |
| `gmx plugin remove <name>` | uninstall, unwinding every registration |
| `gmx plugin enable\|disable <name>` | turn one on or off without uninstalling it |
| `gmx plugin list [--json]` | every plugin, with what each instance costs |
| `gmx plugin describe <name> [--json]` | manifest, settings schemas and skill descriptions |
| `gmx plugin stats [--json]` | per instance cpu, memory, latency, dropped buffers and restarts |
| `gmx plugin bisect --check "<command>"` | binary search the enabled plugins for the one that breaks a check |

`--url` and `--token` take the mixer's address and credential, or the
`GODWINMIX_URL` and `GODWINMIX_TOKEN` environment variables.

See [install a plugin](../how-to/install-a-plugin.md) and
[test a plugin](../how-to/test-a-plugin.md).

## Chaos

Break something on purpose, to see that the programme survives it. Both
subcommands need the `admin` scope, and a core that is not in rehearsal refuses
them unless `--i-am-sure` is given.

| Command | Does |
|---|---|
| `gmx chaos kill <instance>` | SIGKILL a plugin's process, the way a segfault would |
| `gmx chaos stall <instance> --secs 12` | SIGSTOP it for a while, then resume: a camera unplugged rather than a plugin crashed |

The supervisor should cover either with the freeze frame and rebuild, and the
programme's frame interval should never exceed 34 ms while it does. That is
what the harness's kill check measures automatically; these commands are for
reproducing it against a real mixer.

Stalling needs `SIGSTOP`, so it is Linux and macOS only. Killing works
everywhere.
