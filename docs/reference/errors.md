# Errors and the button they offer

Every refusal has one shape, on `/rpc`, on `/api/v1` and to a plugin:

```json
{ "code": -32001,
  "message": "command sources are switched off. They run a command line on the mixer's machine, ...",
  "data": { "retryable": true, "method": "source.add",
            "action": { "kind": "set-config", "label": "Allow command sources",
                        "key": "security.allow_exec_sources", "value": true, "applies": "live" } } }
```

The message names the state and the next step, and reads on its own. `data`
is always an object and always carries `retryable`. The codes, their HTTP
statuses and whether a retry can help are generated into
[protocol.md](../../protocol.md#errors) and `protocol.json` under `errors`.

## `data` members a client acts on

| Member | When | What to do with it |
|---|---|---|
| `retryable` | always | whether sending the same call again can ever work |
| `retry_after_ms` | a hold, a rate limit, a full queue | wait that long, then send it again |
| `valid` | an unknown id (-32004) | the ids that would have worked |
| `needed`, `held` | a missing scope (-32002) | the scope to ask for, and what the token has |
| `confirm_token`, `expires_in_ms` | a destructive call on a token set to `confirm = "required"` (-32020) | send the same call again with `confirm` set to the token |
| `action` | the next step is something a client can do | offer it as a button; see below |

## `data.action`

When the way past a refusal is something a client can do for the person,
the core says so as an object rather than as a sentence to parse. The same
object can ride on an `alert` event, as `action` beside `severity` and
`message`.

| `kind` | Fields | What the button does |
|---|---|---|
| `set-config` | `key`, `value`, `applies` | `config.set {values: {key: value}}`. When `applies` is `restart`, the answer lists the key in `needs_restart` and the mixer has to start again |
| `install-plugin` | `name` | `plugin.add` with the name, looked up in the marketplaces |
| `enable-plugin` | `name` | `plugin.enable {name}` |
| `open` | `dialog`, `panel`, `key` | show that part of the client. `dialog: "settings"` with `key` means the setting of that name |
| `retry` | `after_ms` | the same call again, after the wait |
| `restart` | none | `core.restart`, when `core.info` says `restart.possible` |

Every action has a `label`, the button's text, short and in the imperative.
A client that does not know a `kind` shows the message alone; the message
never depends on the button to make sense. The kinds are listed in
`protocol.json` under `actions`, and the Rust type is `ErrorAction` in
`crates/godwinmix-protocol/src/action.rs`.

## Which refusals carry one

| Refusal | Code | `action` |
|---|---|---|
| `source.add` of an `exec:` source while they are off | -32001 | `set-config` `security.allow_exec_sources` `true`, live |
| a take inside `safety.min_hold_ms`, from a person's token | -32003 | `set-config` `safety.min_hold_ms` `0`, live. An agent's token gets no action |
| `snapshot.get` with snapshots off | -32001 | `set-config` `snapshot.enabled` `true`, restart |
| `snapshot.get`, or an MJPEG route, with the multiview off | -32001, HTTP 404 | `set-config` `multiview.enabled` `true`, restart |
| any `node.*` method, or a source placed on a node, with the node bridge off | -32001 | `set-config` `nodes.listen` `"0.0.0.0:8443"`, restart |
| a source whose plugin is not installed | -32001 | `install-plugin` |
| a source whose plugin is installed and switched off | -32001 | `enable-plugin` |
| a web page source with no browser sidecar and no `wpesrc` | -32001 | `open` `settings`, key `browser.sidecar` |
| an upload when the media folder cannot be made | -32603 | `open` `settings`, key `media.dir` |
| a call that waited on a mixer thread held by a named command | -32001 | `restart` |
| the `alert` sent when the mixer failed while handling a command | event | `restart` |

The MJPEG routes answer HTTP rather than JSON-RPC, so there the action sits
at the top of the body: `{"error": "...", "action": {...}}`.

## In the web UI

An error toast shows the caller's heading, then the core's message, then the
action's button. Pressing a `set-config` button calls `config.set`; a live key
then sends the refused call again, and a restart key says it is saved and
offers Restart now where `core.info` says a restart is possible. "Try again"
appears when the core named a wait, disabled until the wait is over. A
refusal for a missing confirmation opens a yes or no in the page and, on a
yes, sends the call again with the token, so the call site gets its answer.
The Alerts panel shows an alert's button beside it.

A method name is not shown to a person. It goes to the browser console at
debug level with the error.
